pub mod process;

use crate::engine::{Engine, EngineLauncher, EngineState};
use process::{PRELUDE, PROMPT_SETUP, is_ready, prelude_prompts, wrap_code};
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, State};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const EVAL_TIMEOUT: Duration = Duration::from_secs(10);
/// Engine boot (SuperDirt: sclang + SuperDirt + marker) can be slow on cold starts — a
/// first-ever sclang launch compiles the whole class library (minutes); the launcher itself
/// gives up at its own 120 s readiness timeout.
const ENGINE_TIMEOUT: Duration = Duration::from_secs(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Stopped,
    Starting,
    Ready,
    Busy,
    Failed,
}

impl SessionState {
    fn is_live(self) -> bool {
        matches!(
            self,
            SessionState::Starting | SessionState::Ready | SessionState::Busy
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionEvent {
    State {
        state: SessionState,
    },
    Output {
        stream: &'static str,
        text: String,
    },
    Engine {
        engine: Engine,
        status: EngineState,
        detail: String,
    },
}

enum Line {
    Stdout(String),
    Stderr(String),
}

enum Cmd {
    Start(Engine, PathBuf),
    Restart(Option<Engine>, PathBuf),
    Stop,
    Eval(String),
    Death { id: u32, reason: String },
}

type SharedState = Arc<Mutex<SessionState>>;
type Events = Sender<SessionEvent>;
type Lines = Receiver<Line>;

struct Active {
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    lines: Lines,
    id: u32,
}

impl Drop for Active {
    fn drop(&mut self) {
        let _ = self.child.lock().unwrap().kill();
    }
}

pub struct Session {
    cmd_tx: Sender<Cmd>,
    evt_rx: Mutex<Option<Receiver<SessionEvent>>>,
    state: SharedState,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Blocking teardown on drop: tests panic mid-run and the app quits — neither may
        // leave ghci or a synth server behind. `stop()` is a no-op when already stopped.
        self.stop();
    }
}

impl Session {
    pub fn new() -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let state: SharedState = Arc::new(Mutex::new(SessionState::Stopped));
        let monitor_tx = cmd_tx.clone();
        let worker_evt = evt_tx.clone();
        let worker_state = Arc::clone(&state);
        // The launcher feeds the same event channel → one forwarder for the whole app.
        let engine = EngineLauncher::new(evt_tx.clone());
        thread::spawn(move || worker_loop(cmd_rx, monitor_tx, worker_evt, worker_state, engine));
        Self {
            cmd_tx,
            evt_rx: Mutex::new(Some(evt_rx)),
            state,
        }
    }

    #[cfg(test)]
    pub fn for_test() -> (Self, Receiver<SessionEvent>) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let state: SharedState = Arc::new(Mutex::new(SessionState::Stopped));
        let monitor_tx = cmd_tx.clone();
        let worker_evt = evt_tx.clone();
        let worker_state = Arc::clone(&state);
        let engine = EngineLauncher::new(evt_tx.clone());
        thread::spawn(move || worker_loop(cmd_rx, monitor_tx, worker_evt, worker_state, engine));
        (
            Self {
                cmd_tx,
                evt_rx: Mutex::new(None),
                state,
            },
            evt_rx,
        )
    }

    pub fn state(&self) -> SessionState {
        *self.state.lock().unwrap()
    }

    fn send(&self, cmd: Cmd) -> Result<(), String> {
        self.cmd_tx
            .send(cmd)
            .map_err(|_| "session worker gone".to_string())
    }

    pub fn start(&self, engine: Engine, data_dir: PathBuf) -> Result<(), String> {
        // Report Starting immediately (the command returns this state to the UI before the
        // worker has run); the worker emits the authoritative State event next.
        *self.state.lock().unwrap() = SessionState::Starting;
        if let Err(e) = self.send(Cmd::Start(engine, data_dir)) {
            *self.state.lock().unwrap() = SessionState::Stopped;
            return Err(e);
        }
        Ok(())
    }

    pub fn eval(&self, code: String) -> Result<(), String> {
        self.send(Cmd::Eval(code))
    }

    pub fn restart(&self, engine: Option<Engine>, data_dir: PathBuf) -> Result<(), String> {
        *self.state.lock().unwrap() = SessionState::Starting;
        self.send(Cmd::Restart(engine, data_dir))
    }

    pub fn stop(&self) {
        if self.cmd_tx.send(Cmd::Stop).is_err() {
            return; // worker gone — nothing to wait for
        }
        // Covers ghci teardown *and* the blocking engine stop (which alone may take ~10 s).
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.state() != SessionState::Stopped && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn attach_forwarder(&self, app: &AppHandle) {
        if let Some(rx) = self.evt_rx.lock().unwrap().take() {
            let app = app.clone();
            thread::spawn(move || {
                while let Ok(ev) = rx.recv() {
                    let _ = app.emit("session://event", &ev);
                }
            });
        }
    }
}

// ---------- commands ----------

#[tauri::command]
pub fn session_start(
    engine: Option<String>,
    session: State<'_, Session>,
    app: AppHandle,
) -> Result<SessionState, String> {
    use tauri::Manager;
    session.attach_forwarder(&app);
    if matches!(
        session.state(),
        SessionState::Stopped | SessionState::Failed
    ) {
        // The frontend resolves "auto" via the detector (step 4) and passes a concrete kind;
        // Rust stays DB-free by design.
        let engine = match engine.as_deref() {
            None | Some("superdirt") => Engine::SuperDirt,
            Some("headless") => Engine::Headless,
            Some(other) => {
                return Err(format!(
                    "unknown engine '{other}' (expected superdirt|headless)"
                ));
            }
        };
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("app data dir unavailable: {e}"))?;
        session.start(engine, data_dir)?;
    }
    Ok(session.state())
}

#[tauri::command]
pub fn session_eval(code: String, session: State<'_, Session>) -> Result<SessionState, String> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err("nothing to evaluate".to_string());
    }
    if session.state() != SessionState::Ready {
        return Err(format!("session not ready (state: {:?})", session.state()));
    }
    session.eval(code)?;
    Ok(session.state())
}

#[tauri::command]
pub fn session_restart(
    engine: Option<String>,
    session: State<'_, Session>,
    app: AppHandle,
) -> Result<SessionState, String> {
    use tauri::Manager;
    let engine = match engine.as_deref() {
        None => None,
        Some("superdirt") => Some(Engine::SuperDirt),
        Some("headless") => Some(Engine::Headless),
        Some(other) => {
            return Err(format!(
                "unknown engine '{other}' (expected superdirt|headless)"
            ));
        }
    };
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    session.restart(engine, data_dir)?;
    Ok(session.state())
}

// ---------- worker internals ----------

fn set_state(shared: &SharedState, evt: &Events, next: SessionState) {
    *shared.lock().unwrap() = next;
    let _ = evt.send(SessionEvent::State { state: next });
}

fn emit_output(evt: &Events, stream: &'static str, text: String) {
    let _ = evt.send(SessionEvent::Output { stream, text });
}

fn spawn_reader<R: Read + Send + 'static>(
    mut read: R,
    tx: mpsc::Sender<Line>,
    wrap: fn(String) -> Line,
) {
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match read.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    if tx.send(wrap(text)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

fn write_all(stdin: &mut ChildStdin, data: &str) -> Result<(), String> {
    stdin
        .write_all(data.as_bytes())
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("ghci stdin write failed: {e}"))
}

enum PumpOutcome {
    Done,
    Timeout,
    Died,
}

fn pump(
    a: &mut Active,
    evt: &Events,
    acc: &mut String,
    timeout: Duration,
    min_prompts: usize,
) -> PumpOutcome {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match a.lines.recv_timeout(remaining) {
            Ok(Line::Stdout(t)) => {
                emit_output(evt, "stdout", t.clone());
                acc.push_str(&t);
            }
            Ok(Line::Stderr(t)) => {
                emit_output(evt, "stderr", t.clone());
                acc.push_str(&t);
            }
            Err(RecvTimeoutError::Timeout) => return PumpOutcome::Timeout,
            Err(RecvTimeoutError::Disconnected) => return PumpOutcome::Died,
        }
        if is_ready(acc, min_prompts) {
            return PumpOutcome::Done;
        }
    }
}

fn kill(active: &mut Option<Active>) {
    if let Some(a) = active.take() {
        let _ = a.child.lock().unwrap().kill();
    }
}

fn launch(monitor_tx: &Sender<Cmd>, id: u32) -> Result<Active, String> {
    let spawned = Command::new("ghci")
        .args(["-ignore-dot-ghci", "-package", "tidal"])
        .env("TERM", "dumb")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!("failed to spawn ghci: {e} — is Tidal installed? (see README prerequisites)")
        })?;

    let child = Arc::new(Mutex::new(spawned));
    let stdin = child
        .lock()
        .unwrap()
        .stdin
        .take()
        .ok_or("no stdin handle")?;
    let stdout = child
        .lock()
        .unwrap()
        .stdout
        .take()
        .ok_or("no stdout handle")?;
    let stderr = child
        .lock()
        .unwrap()
        .stderr
        .take()
        .ok_or("no stderr handle")?;

    let (line_tx, line_rx) = mpsc::channel();
    spawn_reader(stdout, line_tx.clone(), Line::Stdout);
    spawn_reader(stderr, line_tx, Line::Stderr);

    {
        let monitor_tx = monitor_tx.clone();
        let watched = Arc::clone(&child);
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(200));
                match watched.lock().unwrap().try_wait() {
                    Ok(Some(status)) => {
                        let _ = monitor_tx.send(Cmd::Death {
                            id,
                            reason: status.to_string(),
                        });
                        break;
                    }
                    Ok(None) => {} // still running
                    Err(_) => break,
                }
            }
        });
    }

    Ok(Active {
        child,
        stdin,
        lines: line_rx,
        id,
    })
}

fn wait_engine(launcher: &EngineLauncher) -> Result<(), String> {
    let deadline = Instant::now() + ENGINE_TIMEOUT;
    loop {
        let status = launcher.status();
        match status.state {
            EngineState::Live => return Ok(()),
            EngineState::Down => {
                return Err(format!(
                    "engine unavailable — {}. Install SuperCollider or use the headless engine (diagnostics pane lands in M2 step 4).",
                    status.detail
                ));
            }
            EngineState::Starting => {}
        }
        if Instant::now() >= deadline {
            return Err("engine did not become live in time".to_string());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn do_start(
    engine: Engine,
    data_dir: &Path,
    launcher: &EngineLauncher,
    monitor_tx: &Sender<Cmd>,
    evt: &Events,
    id: u32,
) -> Result<Active, String> {
    // Engine first: the session reaches Ready only once audio actually runs.
    launcher
        .start(engine, data_dir.to_path_buf(), None)
        .map_err(|e| format!("engine start failed: {e}"))?;
    wait_engine(launcher)?;

    let mut a = launch(monitor_tx, id)?;
    write_all(&mut a.stdin, PROMPT_SETUP)?;
    let mut acc = String::new();
    match pump(
        &mut a,
        evt,
        &mut acc,
        STARTUP_TIMEOUT,
        PROMPT_SETUP.matches('\n').count(),
    ) {
        PumpOutcome::Done => {}
        PumpOutcome::Timeout => {
            let _ = a.child.lock().unwrap().kill();
            return Err(
                "ghci startup timed out — check that `ghci` works in a terminal".to_string(),
            );
        }
        PumpOutcome::Died => {
            let _ = a.child.lock().unwrap().kill();
            return Err("ghci closed during startup".to_string());
        }
    }

    // Tidal prelude — separate statements, one prompt echo each (step-0 verified shape).
    write_all(&mut a.stdin, PRELUDE)?;
    let mut acc = String::new();
    match pump(&mut a, evt, &mut acc, STARTUP_TIMEOUT, prelude_prompts()) {
        PumpOutcome::Done => Ok(a),
        PumpOutcome::Timeout => {
            let _ = a.child.lock().unwrap().kill();
            Err(
                "Tidal prelude timed out — is Tidal installed? (`cabal install --lib tidal`, see README)"
                    .to_string(),
            )
        }
        PumpOutcome::Died => {
            let _ = a.child.lock().unwrap().kill();
            Err("ghci closed while loading the Tidal prelude".to_string())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn begin_session(
    kind: Engine,
    data_dir: &Path,
    active: &mut Option<Active>,
    next_id: &mut u32,
    monitor_tx: &Sender<Cmd>,
    evt: &Events,
    state: &SharedState,
    engine: &EngineLauncher,
) {
    kill(active);
    engine.stop();
    set_state(state, evt, SessionState::Starting);
    match do_start(kind, data_dir, engine, monitor_tx, evt, *next_id) {
        Ok(a) => {
            *next_id += 1;
            *active = Some(a);
            set_state(state, evt, SessionState::Ready);
        }
        Err(reason) => {
            emit_output(evt, "stderr", reason);
            engine.stop(); // never leave a half-booted synth behind a failed session
            set_state(state, evt, SessionState::Failed);
        }
    }
}

fn worker_loop(
    cmd_rx: Receiver<Cmd>,
    monitor_tx: Sender<Cmd>,
    evt: Events,
    state: SharedState,
    engine: EngineLauncher,
) {
    let mut active: Option<Active> = None;
    let mut next_id: u32 = 0;
    let mut config: Option<(Engine, PathBuf)> = None;

    loop {
        let cmd = match cmd_rx.recv() {
            Ok(c) => c,
            Err(_) => {
                kill(&mut active); // session dropped (app exit)
                engine.stop();
                break;
            }
        };

        match cmd {
            Cmd::Start(kind, data_dir) => {
                config = Some((kind, data_dir.clone()));
                begin_session(
                    kind,
                    &data_dir,
                    &mut active,
                    &mut next_id,
                    &monitor_tx,
                    &evt,
                    &state,
                    &engine,
                );
            }

            Cmd::Restart(engine_kind, data_dir) => {
                let target = match engine_kind {
                    Some(kind) => {
                        config = Some((kind, data_dir.clone()));
                        Some((kind, data_dir))
                    }
                    None => config.clone(),
                };
                match target {
                    Some((kind, dir)) => begin_session(
                        kind,
                        &dir,
                        &mut active,
                        &mut next_id,
                        &monitor_tx,
                        &evt,
                        &state,
                        &engine,
                    ),
                    None => emit_output(
                        &evt,
                        "stderr",
                        "nothing to restart — start the session first".to_string(),
                    ),
                }
            }

            Cmd::Eval(code) => {
                let frame = wrap_code(&code);
                if frame.is_empty() {
                    continue;
                }
                if active.is_none() {
                    emit_output(&evt, "stderr", "no session — start it first".to_string());
                    continue;
                }
                set_state(&state, &evt, SessionState::Busy);
                let outcome = {
                    let a = active.as_mut().unwrap();
                    if let Err(e) = write_all(&mut a.stdin, &frame) {
                        Err(e)
                    } else {
                        let mut acc = String::new();
                        match pump(a, &evt, &mut acc, EVAL_TIMEOUT, 1) {
                            PumpOutcome::Done => Ok(()),
                            PumpOutcome::Timeout => {
                                Err("eval timed out waiting for prompt — restart the session"
                                    .to_string())
                            }
                            PumpOutcome::Died => Err("ghci exited".to_string()),
                        }
                    }
                };
                match outcome {
                    Ok(()) => set_state(&state, &evt, SessionState::Ready),
                    Err(msg) => {
                        emit_output(&evt, "stderr", msg);
                        kill(&mut active);
                        set_state(&state, &evt, SessionState::Failed);
                    }
                }
            }

            Cmd::Death { id, reason } => {
                let live = state.lock().unwrap().is_live();
                if live && active.as_ref().is_some_and(|a| a.id == id) {
                    emit_output(&evt, "stderr", format!("ghci exited: {reason}"));
                    kill(&mut active);
                    engine.stop();
                    set_state(&state, &evt, SessionState::Failed);
                }
            }

            Cmd::Stop => {
                kill(&mut active);
                engine.stop();
                set_state(&state, &evt, SessionState::Stopped);
            }
        }
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn wait_for_state(sess: &Session, want: SessionState, secs: u64) -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < deadline {
            if sess.state() == want {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    fn run_eval(
        sess: &Session,
        rx: &mut Receiver<SessionEvent>,
        code: &str,
    ) -> Result<String, String> {
        sess.eval(code.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let ev = rx
                .recv_timeout(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(50)),
                )
                .map_err(|_| "no Busy transition seen".to_string())?;
            if matches!(
                ev,
                SessionEvent::State {
                    state: SessionState::Busy
                }
            ) {
                break;
            }
        }
        let mut out = String::new();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let ev = rx
                .recv_timeout(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .max(Duration::from_millis(50)),
                )
                .map_err(|_| "eval never completed".to_string())?;
            match ev {
                SessionEvent::Output { text, .. } => out.push_str(&text),
                SessionEvent::State {
                    state: SessionState::Ready,
                } => return Ok(out),
                SessionEvent::State {
                    state: SessionState::Failed,
                } => return Err(format!("session failed, output so far: {out}")),
                SessionEvent::State { state } => return Err(format!("unexpected state {state:?}")),
                SessionEvent::Engine { .. } => {}
            }
        }
    }

    fn drain_events(rx: &mut Receiver<SessionEvent>) -> String {
        let mut log = String::new();
        while let Ok(ev) = rx.recv_timeout(Duration::from_millis(100)) {
            match ev {
                SessionEvent::Output { text, .. } => log.push_str(&text),
                SessionEvent::Engine { detail, status, .. } => {
                    log.push_str(&format!("[engine {status:?}] {detail}\n"));
                }
                SessionEvent::State { state } => log.push_str(&format!("[state {state:?}]\n")),
            }
        }
        log
    }

    #[test]
    #[ignore = "requires working ghci + tidal (m1-plan step 0 probe must print TIDAL-PROBE-OK)"]
    fn stateful_ghci_session() {
        let (sess, mut rx) = Session::for_test();
        sess.start(Engine::Headless, std::env::temp_dir()).unwrap();
        assert!(
            wait_for_state(&sess, SessionState::Ready, 90),
            "ghci startup timed out; events:\n{}",
            drain_events(&mut rx)
        );

        let out = run_eval(&sess, &mut rx, "1+1").unwrap();
        assert!(out.contains('2'), "expected result 2, got: {out}");

        run_eval(&sess, &mut rx, "let zz = 5*5").unwrap();
        let out = run_eval(&sess, &mut rx, "zz + 1").unwrap();
        assert!(out.contains("26"), "expected stateful reuse, got: {out}");

        let out = run_eval(&sess, &mut rx, "undefinedThing 1").unwrap();
        assert!(
            out.to_lowercase().contains("error"),
            "expected error text, got: {out}"
        );
        assert_eq!(sess.state(), SessionState::Ready);

        sess.restart(None, std::env::temp_dir()).unwrap();
        assert!(
            wait_for_state(&sess, SessionState::Ready, 60),
            "restart timed out"
        );

        sess.stop();
        assert!(wait_for_state(&sess, SessionState::Stopped, 10));
        drop(sess);
    }

    /// M2 step 3 exit criterion: the headless engine still boots the Tidal prelude —
    /// `d1..d8` and `hush` are bound without any SuperCollider on the machine. CI-safe.
    #[test]
    #[ignore = "requires working ghci + tidal (headless prelude, no audio device needed)"]
    fn headless_session_prelude() {
        let (sess, mut rx) = Session::for_test();
        sess.start(Engine::Headless, std::env::temp_dir()).unwrap();
        assert!(
            wait_for_state(&sess, SessionState::Ready, 90),
            "headless session startup timed out (prelude not verified); events:\n{}",
            drain_events(&mut rx)
        );

        let out = run_eval(&sess, &mut rx, ":t d1").unwrap();
        assert!(
            out.contains("d1 ::") && out.contains("ControlPattern"),
            "expected d1 to be bound by the prelude, got: {out}"
        );

        let out = run_eval(&sess, &mut rx, ":t hush").unwrap();
        assert!(
            out.contains("hush ::"),
            "expected hush to be bound by the prelude, got: {out}"
        );

        sess.stop();
        assert!(wait_for_state(&sess, SessionState::Stopped, 15));
    }

    /// Full stack on a real machine: SuperDirt engine + ghci session + prelude, then a
    /// pattern eval and hush. Sound itself needs ears; this proves the pipeline is error-free.
    #[test]
    #[ignore = "requires SuperCollider + ghci/tidal — full engine+session smoke (~20 s)"]
    fn superdirt_session_smoke() {
        let (sess, mut rx) = Session::for_test();
        let data_dir =
            std::env::temp_dir().join(format!("tide-session-smoke-{}", std::process::id()));
        sess.start(Engine::SuperDirt, data_dir.clone()).unwrap();
        assert!(
            wait_for_state(&sess, SessionState::Ready, 120),
            "superdirt session startup timed out; events:\n{}",
            drain_events(&mut rx)
        );

        // Owner manual test (2026-10-25): SuperDirt.start does not load samples — the boot
        // script must call loadSoundFiles with the detected Dirt-Samples dir, or every
        // `s "bd"` fails with "no synth or sample named 'bd'".
        let boot_log = drain_events(&mut rx);
        assert!(
            boot_log.contains("tIDE: loading samples from") && boot_log.contains("Dirt-Samples"),
            "boot script did not load the sample library; engine log:\n{boot_log}"
        );

        let out = run_eval(&sess, &mut rx, "d1 $ s \"bd\"").unwrap();
        assert!(
            !out.to_lowercase().contains("error"),
            "pattern eval reported an error: {out}"
        );

        run_eval(&sess, &mut rx, "hush").unwrap();

        sess.stop();
        assert!(wait_for_state(&sess, SessionState::Stopped, 20));
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}
