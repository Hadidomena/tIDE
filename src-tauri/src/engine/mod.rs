use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::session::SessionEvent;

pub const OSC_PORT: u16 = 57120;
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const MARKER_PLACEHOLDER: &str = "__TIDE_MARKER__";
const SAMPLES_PLACEHOLDER: &str = "__TIDE_SAMPLES__";
const IN_DEVICE_PLACEHOLDER: &str = "__TIDE_IN_DEVICE__";
const OUT_DEVICE_PLACEHOLDER: &str = "__TIDE_OUT_DEVICE__";
const BOOT_SCRIPT: &str = include_str!("../../resources/boot-superdirt.scd");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    SuperDirt,
    Headless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineState {
    Starting,
    Live,
    Down,
}

#[derive(Debug, Clone, Default)]
pub struct AudioDevices {
    pub input: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineStatus {
    pub engine: Option<Engine>,
    pub state: EngineState,
    pub detail: String,
}

impl Default for EngineStatus {
    fn default() -> Self {
        Self {
            engine: None,
            state: EngineState::Down,
            detail: "no engine started".to_string(),
        }
    }
}

enum Cmd {
    Start(Engine, PathBuf, Option<String>, AudioDevices),
    Stop,
}

struct Running {
    engine: Engine,
    child: Arc<Mutex<Child>>,
    lines: Receiver<String>,
}

pub struct EngineLauncher {
    cmd_tx: Sender<Cmd>,
    status: Arc<Mutex<EngineStatus>>,
}

impl EngineLauncher {
    pub fn new(sink: Sender<SessionEvent>) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let status: Arc<Mutex<EngineStatus>> = Arc::new(Mutex::new(EngineStatus::default()));
        let worker_status = Arc::clone(&status);
        thread::spawn(move || worker_loop(cmd_rx, sink, worker_status));
        Self { cmd_tx, status }
    }

    #[cfg(test)]
    pub fn for_test() -> (Self, Receiver<SessionEvent>) {
        let (sink, rx) = mpsc::channel();
        (Self::new(sink), rx)
    }

    pub fn status(&self) -> EngineStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn start(
        &self,
        engine: Engine,
        data_dir: PathBuf,
        sclang_override: Option<String>,
        devices: AudioDevices,
    ) -> Result<(), String> {
        set_status(
            &self.status,
            Some(engine),
            EngineState::Starting,
            "starting",
        );
        self.cmd_tx
            .send(Cmd::Start(engine, data_dir, sclang_override, devices))
            .map_err(|_| "engine worker gone".to_string())
    }

    pub fn stop(&self) {
        if self.status().state == EngineState::Down {
            return;
        }
        let _ = self.cmd_tx.send(Cmd::Stop);
        let deadline = Instant::now() + Duration::from_secs(10);
        while self.status().state != EngineState::Down && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
    }
}

fn emit_engine(evt: &Sender<SessionEvent>, engine: Engine, state: EngineState, detail: &str) {
    let _ = evt.send(SessionEvent::Engine {
        engine,
        status: state,
        detail: detail.to_string(),
    });
}

fn emit_output(evt: &Sender<SessionEvent>, text: String) {
    let _ = evt.send(SessionEvent::Output {
        stream: "engine",
        text,
    });
}

fn set_status(
    status: &Arc<Mutex<EngineStatus>>,
    engine: Option<Engine>,
    state: EngineState,
    detail: &str,
) {
    *status.lock().unwrap() = EngineStatus {
        engine,
        state,
        detail: detail.to_string(),
    };
}

fn worker_loop(rx: Receiver<Cmd>, evt: Sender<SessionEvent>, status: Arc<Mutex<EngineStatus>>) {
    let mut running: Option<Running> = None;

    loop {
        if let Some(run) = running.as_ref() {
            drain(run, &evt);
            if let Some(detail) = exited(&run.child) {
                kill_group(&run.child);
                drain(run, &evt);
                emit_engine(&evt, run.engine, EngineState::Down, &detail);
                set_status(&status, Some(run.engine), EngineState::Down, &detail);
                running = None;
            }
        }

        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(Cmd::Start(engine, dir, sclang_override, devices)) => {
                if let Some(old) = running.take() {
                    shutdown(old, &evt, &status, "restarted");
                }
                if let Ok(started) =
                    start_engine(engine, &dir, sclang_override, devices, &rx, &evt, &status)
                {
                    running = started;
                }
            }
            Ok(Cmd::Stop) => {
                if let Some(run) = running.take() {
                    shutdown(run, &evt, &status, "stopped");
                } else {
                    let current = status.lock().unwrap().engine;
                    emit_engine(
                        &evt,
                        current.unwrap_or(Engine::Headless),
                        EngineState::Down,
                        "stopped",
                    );
                    set_status(&status, current, EngineState::Down, "stopped");
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                if let Some(run) = running.take() {
                    shutdown(run, &evt, &status, "app exit");
                }
                break;
            }
        }
    }
}

fn start_engine(
    engine: Engine,
    data_dir: &Path,
    sclang_override: Option<String>,
    devices: AudioDevices,
    rx: &Receiver<Cmd>,
    evt: &Sender<SessionEvent>,
    status: &Arc<Mutex<EngineStatus>>,
) -> Result<Option<Running>, String> {
    set_status(status, Some(engine), EngineState::Starting, "starting");
    emit_engine(evt, engine, EngineState::Starting, "starting");

    match engine {
        Engine::Headless => {
            let detail = "headless — no synth process, eval only (silent)";
            emit_output(evt, format!("tIDE: {detail}\n"));
            set_status(status, Some(engine), EngineState::Live, detail);
            emit_engine(evt, engine, EngineState::Live, detail);
            Ok(None)
        }
        Engine::SuperDirt => {
            let custom_devices = devices.input.is_some() || devices.output.is_some();
            match start_superdirt(data_dir, sclang_override.clone(), devices, rx, evt, status) {
                Ok(running) => Ok(Some(running)),
                Err(reason) if custom_devices => {
                    emit_output(
                        evt,
                        "tIDE: selected audio device failed — retrying with system defaults\n"
                            .to_string(),
                    );
                    match start_superdirt(
                        data_dir,
                        sclang_override,
                        AudioDevices::default(),
                        rx,
                        evt,
                        status,
                    ) {
                        Ok(running) => Ok(Some(running)),
                        Err(retry) => fail(
                            evt,
                            status,
                            format!("{reason}; retry with system defaults also failed: {retry}"),
                        )
                        .map(Some),
                    }
                }
                Err(reason) => fail(evt, status, reason).map(Some),
            }
        }
    }
}

fn fail(
    evt: &Sender<SessionEvent>,
    status: &Arc<Mutex<EngineStatus>>,
    detail: String,
) -> Result<Running, String> {
    emit_output(evt, format!("tIDE: engine failed — {detail}\n"));
    emit_engine(evt, Engine::SuperDirt, EngineState::Down, &detail);
    set_status(status, Some(Engine::SuperDirt), EngineState::Down, &detail);
    Err(detail)
}

fn start_superdirt(
    data_dir: &Path,
    sclang_override: Option<String>,
    devices: AudioDevices,
    rx: &Receiver<Cmd>,
    evt: &Sender<SessionEvent>,
    status: &Arc<Mutex<EngineStatus>>,
) -> Result<Running, String> {
    let engine_dir = data_dir.join("engine");
    if let Err(e) = std::fs::create_dir_all(&engine_dir) {
        return Err(format!("cannot create {}: {e}", engine_dir.display()));
    }
    let marker = engine_dir.join("ready.txt");
    let _ = std::fs::remove_file(&marker);
    let samples = crate::toolchain::find_dirt_samples();
    let script = engine_dir.join("boot-superdirt.scd");
    if let Err(e) = std::fs::write(
        &script,
        render_boot_script(&marker, samples.as_deref(), &devices),
    ) {
        return Err(format!("cannot write boot script: {e}"));
    }

    if port_held(OSC_PORT) {
        let reaped = reap_stale_engines(&script);
        if reaped > 0 {
            emit_output(
                evt,
                format!("tIDE: cleaned up {reaped} stale engine process(es) from a previous run\n"),
            );
            let deadline = Instant::now() + Duration::from_secs(5);
            while port_held(OSC_PORT) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
            }
        }
        if port_held(OSC_PORT) {
            return Err(format!(
                "OSC port {OSC_PORT} is already in use — another sclang/SuperCollider is running. \
                 Quit it (`pkill sclang`) and restart the session, or use the headless engine."
            ));
        }
    }

    let Some(sclang) = crate::toolchain::find_sclang(sclang_override.as_deref()) else {
        return Err(
            "sclang not found — install SuperCollider or set a sclang path override".to_string(),
        );
    };

    let mut command = Command::new(&sclang);
    command
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let spawned = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            return Err(format!("cannot spawn {}: {e}", sclang.display()));
        }
    };

    let mut child = spawned;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (line_tx, lines) = mpsc::channel::<String>();
    if let Some(stdout) = stdout {
        spawn_reader(stdout, line_tx.clone());
    }
    if let Some(stderr) = stderr {
        spawn_reader(stderr, line_tx);
    }
    emit_output(
        evt,
        format!(
            "tIDE: starting sclang (pid {}) — {}\n",
            child.id(),
            sclang.display()
        ),
    );

    let child = Arc::new(Mutex::new(child));
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        drain_output(&lines, evt);
        if let Ok(content) = std::fs::read_to_string(&marker) {
            let content = content.trim();
            if content == "ready" {
                if port_held(OSC_PORT) {
                    let detail = format!(
                        "superdirt live — sclang pid {}, OSC {OSC_PORT}",
                        child.lock().unwrap().id()
                    );
                    set_status(status, Some(Engine::SuperDirt), EngineState::Live, &detail);
                    emit_engine(evt, Engine::SuperDirt, EngineState::Live, &detail);
                    return Ok(Running {
                        engine: Engine::SuperDirt,
                        child,
                        lines,
                    });
                }
            } else if let Some(reason) = content.strip_prefix("failed:") {
                kill_group(&child);
                return Err(format!("boot script failed: {}", reason.trim()));
            }
        }
        if let Some(detail) = exited(&child) {
            kill_group(&child);
            return Err(detail);
        }
        match rx.try_recv() {
            Ok(Cmd::Stop) => {
                kill_group(&child);
                return Err("stopped during startup".to_string());
            }
            Err(TryRecvError::Disconnected) => {
                kill_group(&child);
                return Err("app exit during startup".to_string());
            }
            _ => {}
        }
        if Instant::now() >= deadline {
            kill_group(&child);
            return Err(format!(
                "no readiness marker after {}s — is the audio device available? (see engine output)",
                READY_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn shutdown(
    run: Running,
    evt: &Sender<SessionEvent>,
    status: &Arc<Mutex<EngineStatus>>,
    why: &str,
) {
    kill_group(&run.child);
    drain(&run, evt);
    emit_engine(evt, run.engine, EngineState::Down, why);
    set_status(status, Some(run.engine), EngineState::Down, why);
}

fn drain(run: &Running, evt: &Sender<SessionEvent>) {
    drain_output(&run.lines, evt);
}

fn drain_output(lines: &Receiver<String>, evt: &Sender<SessionEvent>) {
    const BATCH: usize = 32 * 1024;
    let mut batch = String::new();
    while let Ok(text) = lines.try_recv() {
        batch.push_str(&text);
        if batch.len() >= BATCH {
            emit_output(evt, std::mem::take(&mut batch));
        }
    }
    if !batch.is_empty() {
        emit_output(evt, batch);
    }
}

fn exited(child: &Arc<Mutex<Child>>) -> Option<String> {
    match child.lock().unwrap().try_wait() {
        Ok(Some(status)) => Some(format!("sclang exited ({status})")),
        Ok(None) => None,
        Err(e) => Some(format!("sclang wait failed: {e}")),
    }
}

fn spawn_reader<R: Read + Send + 'static>(mut read: R, tx: Sender<String>) {
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match read.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    if tx.send(text).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

#[cfg(unix)]
fn kill_group(child: &Arc<Mutex<Child>>) {
    let mut guard = child.lock().unwrap();
    let pgid = guard.id() as i32;
    let _ = unsafe { libc::kill(-pgid, libc::SIGTERM) };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match guard.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            _ => break,
        }
    }
    let _ = unsafe { libc::kill(-pgid, libc::SIGKILL) };
    let _ = guard.kill();
    let _ = guard.wait();
}

#[cfg(not(unix))]
fn kill_group(child: &Arc<Mutex<Child>>) {
    let mut guard = child.lock().unwrap();
    let pid = guard.id().to_string();
    let _ = Command::new("taskkill")
        .args(["/PID", &pid, "/T", "/F"])
        .output();
    let _ = guard.kill();
    let _ = guard.wait();
}

#[cfg(unix)]
fn reap_stale_engines(script: &Path) -> usize {
    let Ok(output) = Command::new("pgrep")
        .args(["-f", &script.to_string_lossy()])
        .output()
    else {
        return 0;
    };
    let mut signalled = 0;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Ok(pid) = line.trim().parse::<i32>() {
            let _ = unsafe { libc::kill(-pid, libc::SIGTERM) };
            let _ = unsafe { libc::kill(pid, libc::SIGTERM) };
            signalled += 1;
        }
    }
    if signalled > 0 {
        thread::sleep(Duration::from_millis(500));
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(pid) = line.trim().parse::<i32>() {
                let _ = unsafe { libc::kill(-pid, libc::SIGKILL) };
            }
        }
    }
    signalled
}

#[cfg(not(unix))]
fn reap_stale_engines(_script: &Path) -> usize {
    0
}

fn port_held(port: u16) -> bool {
    match std::net::UdpSocket::bind(("127.0.0.1", port)) {
        Ok(socket) => {
            drop(socket);
            false
        }
        Err(_) => true,
    }
}

fn render_boot_script(marker: &Path, samples: Option<&Path>, devices: &AudioDevices) -> String {
    let samples = samples
        .map(|dir| escape_sc_string(&dir.display().to_string()))
        .unwrap_or_default();
    let input = devices
        .input
        .as_deref()
        .map(escape_sc_string)
        .unwrap_or_default();
    let output = devices
        .output
        .as_deref()
        .map(escape_sc_string)
        .unwrap_or_default();
    BOOT_SCRIPT
        .replace(
            MARKER_PLACEHOLDER,
            &escape_sc_string(&marker.display().to_string()),
        )
        .replace(SAMPLES_PLACEHOLDER, &samples)
        .replace(IN_DEVICE_PLACEHOLDER, &input)
        .replace(OUT_DEVICE_PLACEHOLDER, &output)
}

fn escape_sc_string(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_script_embeds_escaped_paths() {
        let script = render_boot_script(
            Path::new(r#"C:\Users\me\"tIDE"\ready.txt"#),
            Some(Path::new(r#"C:\Users\me\samples"#)),
            &AudioDevices {
                input: Some("Mic \"Pro\"".to_string()),
                output: Some("Speakers".to_string()),
            },
        );
        assert!(!script.contains(MARKER_PLACEHOLDER));
        assert!(!script.contains(SAMPLES_PLACEHOLDER));
        assert!(script.contains(r#""C:\\Users\\me\\\"tIDE\"\\ready.txt""#));
        assert!(script.contains(r#""C:\\Users\\me\\samples""#));
        assert!(script.contains("SuperDirt(2, s)"));
        assert!(script.contains("var inDevice = \"Mic \\\"Pro\\\"\";"));
        assert!(script.contains("var outDevice = \"Speakers\";"));
        assert!(script.contains("s.options.outDevice = outDevice;"));
        assert!(script.contains("loadSoundFiles"));
        assert!(script.contains("start(57120, 0 ! 16)"));
        assert!(script.contains("SynthDef(\\chip"));
        assert!(script.contains("SynthDef(\\chipbass"));
        assert!(script.contains("numInputBusChannels = 0"));
        assert!(script.contains("numBuffers = 1024 * 256"));
        assert!(script.contains("maxNodes = 1024 * 32"));

        // Without a samples dir the script must still be valid and explicit about silence.
        let headless_script =
            render_boot_script(Path::new("/tmp/ready.txt"), None, &AudioDevices::default());
        assert!(!headless_script.contains(SAMPLES_PLACEHOLDER));
        assert!(headless_script.contains("no Dirt-Samples directory configured"));
        assert!(headless_script.contains("var inDevice = \"\";"));
        assert!(headless_script.contains("var outDevice = \"\";"));
    }

    #[test]
    fn port_probe_detects_bind_conflicts() {
        let socket = std::net::UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        let port = socket.local_addr().unwrap().port();
        assert!(port_held(port), "a bound socket must read as held");
        drop(socket);
        assert!(!port_held(port), "a released port must read as free");
    }

    #[test]
    fn engine_events_share_the_session_envelope() {
        let event = SessionEvent::Engine {
            engine: Engine::SuperDirt,
            status: EngineState::Live,
            detail: "test".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["kind"], "engine");
        assert_eq!(json["engine"], "superdirt");
        assert_eq!(json["status"], "live");
        assert_eq!(json["detail"], "test");
    }

    #[test]
    #[ignore = "requires SuperCollider (sclang) — boots a real engine (~10 s)"]
    fn superdirt_engine_lifecycle() {
        let (launcher, rx) = EngineLauncher::for_test();
        let dir = std::env::temp_dir().join(format!(
            "tide-engine-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        launcher
            .start(
                Engine::SuperDirt,
                dir.clone(),
                None,
                AudioDevices::default(),
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut live = false;
        let mut log = String::new();
        while Instant::now() < deadline && !live {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(SessionEvent::Output { text, .. }) => log.push_str(&text),
                Ok(SessionEvent::Engine {
                    status: EngineState::Live,
                    ..
                }) => live = true,
                _ => {}
            }
        }
        assert!(
            live,
            "engine never reached Live; status {:?}\nengine output:\n{log}",
            launcher.status()
        );
        assert!(port_held(OSC_PORT), "OSC {OSC_PORT} not held after Live");

        let script = dir.join("engine").join("boot-superdirt.scd");
        let out = Command::new("pgrep")
            .args(["-f", script.to_str().unwrap()])
            .output()
            .expect("pgrep");
        let pid = String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        assert!(!pid.is_empty(), "could not find the sclang pid via pgrep");
        let _ = Command::new("kill").args(["-9", &pid]).status();

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && launcher.status().state != EngineState::Down {
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            launcher.status().state,
            EngineState::Down,
            "no Down after external kill"
        );

        thread::sleep(Duration::from_millis(700));
        assert!(
            !port_held(OSC_PORT),
            "OSC {OSC_PORT} still held — scsynth was orphaned"
        );
        launcher.stop();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
