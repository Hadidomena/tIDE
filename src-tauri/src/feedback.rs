use std::io::ErrorKind;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;

pub const HIT_ADDRESS: &str = "/tide/hit";
pub const ORBITS: usize = 16;
const TICK: Duration = Duration::from_millis(50); // 20 Hz snapshots
const IDLE_CUTOFF_SECS: f64 = 2.0;
const MIN_CPS: f64 = 0.05;
const MAX_CPS: f64 = 32.0;
const CPS_SMOOTHING: f64 = 0.2;

#[derive(Debug, Clone, PartialEq)]
pub enum OscArg {
    Int(i32),
    Float(f32),
    Str(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OscMessage {
    pub address: String,
    pub args: Vec<OscArg>,
}

impl OscArg {
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            OscArg::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match self {
            OscArg::Float(v) => Some(*v),
            OscArg::Int(v) => Some(*v as f32),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            OscArg::Str(v) => Some(v.as_str()),
            _ => None,
        }
    }
}

/// One parsed event from SuperDirt.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub orbit: u8,
    pub cycle: f64,
    pub latency: f32,
    pub sample: String,
    pub note: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrbitHit {
    pub orbit: u8,
    pub sample: String,
    pub hits: u32,
    pub cycle: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSnapshot {
    pub cycle: f64,
    pub cps: f64,
    pub latency: f32,
    pub orbits: Vec<OrbitHit>,
}

#[derive(Clone)]
struct OrbitState {
    sample: String,
    cycle: f64,
}

#[derive(Clone)]
pub struct PlaybackState {
    cycle: f64,
    cps: f64,
    latency: f32,
    orbits: Vec<Option<OrbitState>>,
    pending: Vec<u32>,
    last: Option<(f64, f64)>,
    seen_hit: bool,
    last_hit_at: f64,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self::new()
    }
}

impl PlaybackState {
    pub fn new() -> Self {
        Self {
            cycle: 0.0,
            cps: 0.0,
            latency: 0.0,
            orbits: (0..ORBITS).map(|_| None).collect(),
            pending: vec![0; ORBITS],
            last: None,
            seen_hit: false,
            last_hit_at: 0.0,
        }
    }

    pub fn ingest(&mut self, hit: &Hit, now: f64) {
        let orbit = hit.orbit as usize;
        if orbit >= ORBITS {
            return;
        }
        self.cycle = hit.cycle;
        self.latency = hit.latency;
        self.orbits[orbit] = Some(OrbitState {
            sample: hit.sample.clone(),
            cycle: hit.cycle,
        });
        self.pending[orbit] += 1;
        self.seen_hit = true;
        self.last_hit_at = now;

        if let Some((last_cycle, last_at)) = self.last {
            let dt = now - last_at;
            let dc = hit.cycle - last_cycle;
            if dt > 1e-3 && dc >= 0.0 {
                let cps = dc / dt;
                if (MIN_CPS..=MAX_CPS).contains(&cps) {
                    self.cps = if self.cps <= 0.0 {
                        cps
                    } else {
                        self.cps * (1.0 - CPS_SMOOTHING) + cps * CPS_SMOOTHING
                    };
                }
            }
        }
        self.last = Some((hit.cycle, now));
    }

    pub fn cps(&self) -> f64 {
        self.cps
    }

    pub fn cycle(&self) -> f64 {
        self.cycle
    }

    pub fn snapshot(&mut self, now: f64) -> Option<PlaybackSnapshot> {
        if !self.seen_hit || now - self.last_hit_at > IDLE_CUTOFF_SECS {
            return None;
        }
        let mut orbits = Vec::new();
        for (index, count) in self.pending.iter_mut().enumerate() {
            if *count == 0 {
                continue;
            }
            if let Some(state) = self.orbits[index].as_ref() {
                orbits.push(OrbitHit {
                    orbit: index as u8,
                    sample: state.sample.clone(),
                    hits: *count,
                    cycle: state.cycle,
                });
            }
            *count = 0;
        }
        Some(PlaybackSnapshot {
            cycle: self.cycle,
            cps: self.cps,
            latency: self.latency,
            orbits,
        })
    }
}

pub struct Feedback {
    port: u16,
    state: Arc<Mutex<PlaybackState>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Feedback {
    pub fn start() -> Result<(Self, Receiver<PlaybackSnapshot>), String> {
        let socket = UdpSocket::bind(("127.0.0.1", 0))
            .map_err(|e| format!("cannot bind the feedback socket: {e}"))?;
        let port = socket
            .local_addr()
            .map_err(|e| format!("cannot read the feedback port: {e}"))?
            .port();
        socket
            .set_read_timeout(Some(TICK))
            .map_err(|e| format!("cannot set the feedback timeout: {e}"))?;

        let state = Arc::new(Mutex::new(PlaybackState::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();

        let worker_state = Arc::clone(&state);
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            run_loop(socket, worker_state, worker_stop, tx);
        });

        Ok((
            Self {
                port,
                state,
                stop,
                worker: Some(worker),
            },
            rx,
        ))
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn state(&self) -> PlaybackState {
        self.state.lock().unwrap().clone()
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for Feedback {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run_loop(
    socket: UdpSocket,
    state: Arc<Mutex<PlaybackState>>,
    stop: Arc<AtomicBool>,
    tx: mpsc::Sender<PlaybackSnapshot>,
) {
    let started = Instant::now();
    let mut buf = [0u8; 2048];
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let now = started.elapsed().as_secs_f64();
        if let Some(snapshot) = state.lock().unwrap().snapshot(now)
            && tx.send(snapshot).is_err()
        {
            break;
        }
        match socket.recv_from(&mut buf) {
            Ok((size, _)) => {
                if let Some(hit) = parse_packet(&buf[..size]) {
                    let now = started.elapsed().as_secs_f64();
                    state.lock().unwrap().ingest(&hit, now);
                }
            }
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
}

pub fn parse_packet(packet: &[u8]) -> Option<Hit> {
    let message = parse_osc(packet).ok()?;
    if message.address != HIT_ADDRESS {
        return None;
    }
    let orbit = message.args.first()?.as_i32()?;
    if !(0..ORBITS as i32).contains(&orbit) {
        return None;
    }
    let cycle = message.args.get(1)?.as_str()?.parse::<f64>().ok()?;
    if !cycle.is_finite() {
        return None;
    }
    let latency = message.args.get(2)?.as_f32()?;
    let sample = message.args.get(3)?.as_str()?.to_string();
    let note = message
        .args
        .get(4)
        .and_then(|arg| arg.as_f32())
        .unwrap_or(-1.0);
    Some(Hit {
        orbit: orbit as u8,
        cycle,
        latency,
        sample,
        note,
    })
}

pub fn parse_osc(packet: &[u8]) -> Result<OscMessage, String> {
    if packet.is_empty() {
        return Err("empty OSC packet".to_string());
    }
    if packet.starts_with(b"#bundle") {
        return Err("OSC bundles are not supported".to_string());
    }
    let mut cursor = 0usize;
    let address = read_string(packet, &mut cursor)?;
    if !address.starts_with('/') {
        return Err(format!("not an OSC address: {address:?}"));
    }
    let tags = read_string(packet, &mut cursor)?;
    let tags = tags
        .strip_prefix(',')
        .ok_or_else(|| "missing OSC type tags".to_string())?;

    let mut args = Vec::with_capacity(tags.len());
    for tag in tags.chars() {
        match tag {
            'i' => args.push(OscArg::Int(read_i32(packet, &mut cursor)?)),
            'f' => args.push(OscArg::Float(read_f32(packet, &mut cursor)?)),
            's' => args.push(OscArg::Str(read_string(packet, &mut cursor)?)),
            other => return Err(format!("unsupported OSC type tag {other:?}")),
        }
    }
    Ok(OscMessage { address, args })
}

fn read_string(packet: &[u8], cursor: &mut usize) -> Result<String, String> {
    let start = *cursor;
    let Some(len) = packet[start..].iter().position(|b| *b == 0) else {
        return Err("unterminated OSC string".to_string());
    };
    let raw = &packet[start..start + len];
    let text = std::str::from_utf8(raw)
        .map_err(|e| format!("OSC string is not utf-8: {e}"))?
        .to_string();
    *cursor = start + pad4(len + 1);
    if *cursor > packet.len() {
        return Err("OSC string runs past the packet".to_string());
    }
    Ok(text)
}

fn read_i32(packet: &[u8], cursor: &mut usize) -> Result<i32, String> {
    let bytes = read_array::<4>(packet, cursor)?;
    Ok(i32::from_be_bytes(bytes))
}

fn read_f32(packet: &[u8], cursor: &mut usize) -> Result<f32, String> {
    let bytes = read_array::<4>(packet, cursor)?;
    Ok(f32::from_be_bytes(bytes))
}

fn read_array<const N: usize>(packet: &[u8], cursor: &mut usize) -> Result<[u8; N], String> {
    let end = *cursor + N;
    if end > packet.len() {
        return Err("OSC argument runs past the packet".to_string());
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&packet[*cursor..end]);
    *cursor = end;
    Ok(out)
}

fn pad4(len: usize) -> usize {
    len.div_ceil(4) * 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::RecvTimeoutError;

    fn packet(address: &str, args: &[OscArg]) -> Vec<u8> {
        let mut out = Vec::new();
        write_string(&mut out, address);
        let tags: String = args
            .iter()
            .map(|arg| match arg {
                OscArg::Int(_) => 'i',
                OscArg::Float(_) => 'f',
                OscArg::Str(_) => 's',
            })
            .collect();
        write_string(&mut out, &format!(",{tags}"));
        for arg in args {
            match arg {
                OscArg::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
                OscArg::Float(v) => out.extend_from_slice(&v.to_be_bytes()),
                OscArg::Str(v) => write_string(&mut out, v),
            }
        }
        out
    }

    fn write_string(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(text.as_bytes());
        out.push(0);
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }

    fn hit(orbit: u8, cycle: f64, sample: &str) -> Hit {
        Hit {
            orbit,
            cycle,
            latency: 0.2,
            sample: sample.to_string(),
            note: -1.0,
        }
    }

    #[test]
    fn parses_int_float_and_string_arguments() {
        let raw = packet(
            "/tide/hit",
            &[
                OscArg::Int(3),
                OscArg::Str("42.25".to_string()),
                OscArg::Float(0.2),
                OscArg::Str("bd".to_string()),
                OscArg::Float(-1.0),
            ],
        );
        let message = parse_osc(&raw).expect("parse");
        assert_eq!(message.address, "/tide/hit");
        assert_eq!(message.args[0].as_i32(), Some(3));
        assert_eq!(message.args[1].as_str(), Some("42.25"));
        assert_eq!(message.args[3].as_str(), Some("bd"));
    }

    #[test]
    fn parses_a_hit_packet() {
        let raw = packet(
            "/tide/hit",
            &[
                OscArg::Int(0),
                OscArg::Str("42.25".to_string()),
                OscArg::Float(0.2),
                OscArg::Str("bd".to_string()),
                OscArg::Float(60.0),
            ],
        );
        let hit = parse_packet(&raw).expect("hit");
        assert_eq!(hit.orbit, 0);
        assert_eq!(hit.cycle, 42.25);
        assert_eq!(hit.sample, "bd");
        assert_eq!(hit.note, 60.0);
    }

    #[test]
    fn ignores_foreign_addresses_and_bad_args() {
        let foreign = packet("/dirt/play", &[OscArg::Int(0)]);
        assert!(parse_packet(&foreign).is_none());

        let bad_orbit = packet(
            "/tide/hit",
            &[
                OscArg::Int(99),
                OscArg::Str("1".to_string()),
                OscArg::Float(0.0),
                OscArg::Str("bd".to_string()),
            ],
        );
        assert!(parse_packet(&bad_orbit).is_none());

        let bad_cycle = packet(
            "/tide/hit",
            &[
                OscArg::Int(0),
                OscArg::Str("not-a-number".to_string()),
                OscArg::Float(0.0),
                OscArg::Str("bd".to_string()),
            ],
        );
        assert!(parse_packet(&bad_cycle).is_none());
    }

    #[test]
    fn rejects_truncated_and_unsupported_packets() {
        assert!(parse_osc(&[]).is_err());
        assert!(parse_osc(b"#bundle\0\0\0\0").is_err());
        let raw = packet("/tide/hit", &[OscArg::Int(1)]);
        assert!(parse_osc(&raw[..raw.len() - 2]).is_err());
        assert!(parse_osc(b"no-address\0\0,,").is_err());
    }

    #[test]
    fn cps_converges_from_cycle_deltas() {
        let mut state = PlaybackState::new();
        // 1 cycle every 2 s → 0.5 cps.
        for i in 0..12 {
            state.ingest(&hit(0, i as f64, "bd"), i as f64 * 2.0);
        }
        assert!((state.cps() - 0.5).abs() < 0.01, "cps {}", state.cps());
    }

    #[test]
    fn reset_cycles_snaps_without_polluting_cps() {
        let mut state = PlaybackState::new();
        for i in 0..12 {
            state.ingest(&hit(0, 100.0 + i as f64, "bd"), i as f64);
        }
        let before = state.cps();
        state.ingest(&hit(0, 0.0, "bd"), 12.5);
        assert_eq!(state.cycle(), 0.0);
        assert!(
            (state.cps() - before).abs() < 1e-9,
            "cps must not average a reset"
        );
    }

    #[test]
    fn snapshot_reports_pending_hits_then_clears_them() {
        let mut state = PlaybackState::new();
        state.ingest(&hit(1, 4.0, "sn"), 1.0);
        state.ingest(&hit(1, 4.5, "sn"), 1.2);
        let snapshot = state.snapshot(1.3).expect("snapshot");
        assert_eq!(snapshot.cycle, 4.5);
        assert_eq!(snapshot.orbits.len(), 1);
        assert_eq!(snapshot.orbits[0].orbit, 1);
        assert_eq!(snapshot.orbits[0].hits, 2);
        assert_eq!(snapshot.orbits[0].sample, "sn");

        let next = state.snapshot(1.4).expect("still live");
        assert!(next.orbits.is_empty(), "pending hits must be drained");
    }

    #[test]
    fn snapshot_goes_quiet_when_hits_stop() {
        let mut state = PlaybackState::new();
        assert!(state.snapshot(0.0).is_none(), "nothing seen yet");
        state.ingest(&hit(0, 1.0, "bd"), 0.0);
        assert!(state.snapshot(1.0).is_some());
        assert!(
            state.snapshot(IDLE_CUTOFF_SECS + 0.5).is_none(),
            "a quiet engine must not stream stale cycles"
        );
    }

    #[test]
    fn socket_round_trip_produces_a_snapshot() {
        let (feedback, rx) = Feedback::start().expect("start feedback");
        assert!(feedback.port() > 0);

        let sender = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let raw = packet(
            "/tide/hit",
            &[
                OscArg::Int(2),
                OscArg::Str("7.5".to_string()),
                OscArg::Float(0.2),
                OscArg::Str("hh".to_string()),
                OscArg::Float(-1.0),
            ],
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut snapshot = None;
        while Instant::now() < deadline && snapshot.is_none() {
            sender
                .send_to(&raw, ("127.0.0.1", feedback.port()))
                .expect("send hit");
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(candidate) if !candidate.orbits.is_empty() => snapshot = Some(candidate),
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => panic!("feedback thread died"),
            }
        }
        let snapshot = snapshot.expect("no snapshot after a hit");
        assert_eq!(snapshot.cycle, 7.5);
        assert_eq!(snapshot.orbits[0].orbit, 2);
        assert_eq!(snapshot.orbits[0].sample, "hh");
        assert_eq!(feedback.state().cycle(), 7.5);
        feedback.stop();
    }
}
