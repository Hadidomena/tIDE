use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolchainStatus {
    SuperDirtReady,
    HeadlessOnly,
    Broken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProbeStatus {
    Ok,
    Warn,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub name: String,
    pub status: ProbeStatus,
    pub detail: String,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectReport {
    pub status: ToolchainStatus,
    pub probes: Vec<ProbeResult>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Overrides {
    pub ghc_path: Option<String>,
    pub sclang_path: Option<String>,
}

fn probe(
    name: &str,
    status: ProbeStatus,
    detail: impl Into<String>,
    hint: impl Into<String>,
) -> ProbeResult {
    ProbeResult {
        name: name.to_string(),
        status,
        detail: trim_detail(detail.into()),
        hint: hint.into(),
    }
}

fn trim_detail(s: String) -> String {
    let mut joined = s
        .trim()
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .take(3)
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" | ");
    if joined.chars().count() > 220 {
        joined = format!("{}…", joined.chars().take(220).collect::<String>());
    }
    joined
}

fn locate(override_path: Option<&str>, name: &str, fallback_dirs: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = override_path.filter(|s| !s.trim().is_empty()) {
        let pb = PathBuf::from(p);
        return pb.is_file().then_some(pb);
    }
    if let Some(p) = which(name) {
        return Some(p);
    }
    find_in(fallback_dirs, name)
}

fn exe_name(base: &str) -> OsString {
    if cfg!(windows) {
        format!("{base}.exe").into()
    } else {
        base.into()
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe_name(name)))
        .find(|candidate| candidate.is_file())
}

fn find_in(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(exe_name(name)))
        .find(|candidate| candidate.is_file())
}

fn sibling_binary(known: &Path, sibling: &str) -> PathBuf {
    known
        .parent()
        .map(|dir| dir.join(exe_name(sibling)))
        .unwrap_or_else(|| PathBuf::from(exe_name(sibling)))
}

struct RunOutcome {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run(program: &Path, args: &[&str], timeout: Duration) -> Result<RunOutcome, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run {}: {e}", program.display()))?;
    let outcome = wait_timeout(&mut child, timeout);
    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut handle) = child.stdout.take() {
        let _ = handle.read_to_string(&mut stdout);
    }
    if let Some(mut handle) = child.stderr.take() {
        let _ = handle.read_to_string(&mut stderr);
    }
    match outcome {
        Ok(Some(code)) => Ok(RunOutcome {
            code: Some(code),
            stdout,
            stderr,
        }),
        Ok(None) => Err(format!("{} produced no exit status", program.display())),
        Err(message) => Err(message),
    }
}

fn wait_timeout(child: &mut Child, timeout: Duration) -> Result<Option<i32>, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("timed out after {}s", timeout.as_secs()));
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.code()),
            Ok(None) => thread::sleep(Duration::from_millis(30)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
}

const TIDAL_PROBE_MARKER: &str = "TIDE-PROBE-OK";

const HINT_GHC: &str =
    "Install GHC via ghcup (https://www.haskell.org/ghcup/) or `brew install ghc`.";
const HINT_TIDAL: &str = "Install Tidal into the user GHC environment: `cabal update && cabal install --lib tidal` \
                          (`brew install cmake` first — tidal-link needs it). `ghc-pkg list` will NOT show it: \
                          cabal --lib installs via a GHC environment file.";
const HINT_SCLANG: &str = "Install SuperCollider: `brew install --cask supercollider` (macOS) or \
                           https://supercollider.github.io/download.";
const HINT_QUARK: &str = "Open the SuperCollider IDE → Quarks → install `SuperDirt` (musikinformatik/SuperDirt on Codeberg).";
const HINT_SAMPLES: &str = "Installing the SuperDirt quark should download Dirt-Samples (~400 MB). If absent, re-run \
                           the Quark installer or clone https://codeberg.org/musikinformatik/SuperDirt";

fn ghc_dirs(home: &Path) -> Vec<PathBuf> {
    vec![home.join(".ghcup").join("bin")]
}

fn sclang_dirs(home: &Path) -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/Applications/SuperCollider.app/Contents/MacOS"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ]
    } else if cfg!(windows) {
        vec![PathBuf::from(r"C:\Program Files\SuperCollider")]
    } else {
        vec![
            PathBuf::from("/usr/bin"),
            PathBuf::from("/usr/local/bin"),
            home.join("bin"),
        ]
    }
}

fn sclang_support_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("SuperCollider")
    } else if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Local"))
            .join("SuperCollider")
    } else {
        home.join(".local").join("share").join("SuperCollider")
    }
}

fn quark_root(home: &Path) -> PathBuf {
    sclang_support_dir(home).join("downloaded-quarks")
}

pub fn find_sclang(override_path: Option<&str>) -> Option<PathBuf> {
    let home = home_dir();
    locate(override_path, "sclang", &sclang_dirs(&home))
}

/// Dirt-Samples root (one directory per sample group). The engine boot script feeds this to
/// `~dirt.loadSoundFiles` — SuperDirt's `start` does NOT load samples itself.
pub fn find_dirt_samples() -> Option<PathBuf> {
    let dir = quark_root(&home_dir()).join("Dirt-Samples");
    dir.is_dir().then_some(dir)
}

pub(crate) fn parse_ghc_version(output: &str) -> Option<String> {
    let idx = output.find("version ")?;
    let candidate = output[idx + "version ".len()..].split_whitespace().next()?;
    let cleaned = candidate.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.');
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

fn parse_plist_version(bytes: &[u8]) -> Option<String> {
    const KEY: &[u8] = b"CFBundleShortVersionString";
    let at = find_subslice(bytes, KEY)? + KEY.len();
    let rest = bytes.get(at..)?;
    let open = find_subslice(rest, b"<string>")?;
    let after = rest.get(open + b"<string>".len()..)?;
    let close = find_subslice(after, b"</string>")?;
    std::str::from_utf8(after.get(..close)?)
        .ok()
        .map(|s| s.trim().to_string())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn bundle_version(binary: &Path) -> Option<String> {
    let contents = binary.parent()?.parent()?;
    let bytes = std::fs::read(contents.join("Info.plist")).ok()?;
    parse_plist_version(&bytes)
}

fn ghc_probe(overrides: &Overrides, home: &Path) -> ProbeResult {
    let name = "ghc";
    match locate(overrides.ghc_path.as_deref(), "ghc", &ghc_dirs(home)) {
        None => {
            let detail = if overrides.ghc_path.is_some() {
                "configured override is not an existing file".to_string()
            } else {
                "not found in PATH or ~/.ghcup/bin".to_string()
            };
            probe(name, ProbeStatus::Missing, detail, HINT_GHC)
        }
        Some(path) => match run(&path, &["--version"], Duration::from_secs(10)) {
            Ok(outcome) => {
                let version = parse_ghc_version(&outcome.stdout)
                    .unwrap_or_else(|| "found on disk".to_string());
                probe(
                    name,
                    ProbeStatus::Ok,
                    format!("{version} — {}", path.display()),
                    "",
                )
            }
            Err(message) => probe(
                name,
                ProbeStatus::Missing,
                format!("found at {} but cannot run: {message}", path.display()),
                HINT_GHC,
            ),
        },
    }
}

fn tidal_probe(overrides: &Overrides, home: &Path) -> ProbeResult {
    let name = "tidal";
    let Some(ghc) = locate(overrides.ghc_path.as_deref(), "ghc", &ghc_dirs(home)) else {
        return probe(
            name,
            ProbeStatus::Missing,
            "GHC missing — tidal check skipped",
            HINT_GHC,
        );
    };
    let ghci = sibling_binary(&ghc, "ghci");
    if !ghci.is_file() {
        return probe(
            name,
            ProbeStatus::Missing,
            format!("no ghci found next to {}", ghc.display()),
            HINT_TIDAL,
        );
    }
    let expr = format!("putStrLn (\"{TIDAL_PROBE_MARKER} \" ++ tidal_version)");
    let outcome = run(
        &ghci,
        &[
            "-ignore-dot-ghci",
            "-package",
            "tidal",
            "-e",
            "import Sound.Tidal.Context",
            "-e",
            &expr,
        ],
        Duration::from_secs(90),
    );
    match outcome {
        Ok(r) if r.code == Some(0) && r.stdout.contains(TIDAL_PROBE_MARKER) => {
            let version = r
                .stdout
                .lines()
                .find(|line| line.contains(TIDAL_PROBE_MARKER))
                .unwrap_or(TIDAL_PROBE_MARKER)
                .replace(TIDAL_PROBE_MARKER, "tidal")
                .trim()
                .to_string();
            probe(name, ProbeStatus::Ok, version, "")
        }
        Ok(r) => probe(
            name,
            ProbeStatus::Missing,
            if r.stderr.trim().is_empty() {
                r.stdout
            } else {
                r.stderr
            },
            HINT_TIDAL,
        ),
        Err(message) => probe(name, ProbeStatus::Missing, message, HINT_TIDAL),
    }
}

fn sclang_probe(overrides: &Overrides, _home: &Path) -> ProbeResult {
    let name = "sclang";
    let Some(path) = locate(
        overrides.sclang_path.as_deref(),
        "sclang",
        &sclang_dirs(_home),
    ) else {
        let detail = if overrides.sclang_path.is_some() {
            "configured override is not an existing file".to_string()
        } else {
            "not found in PATH or standard install locations".to_string()
        };
        return probe(name, ProbeStatus::Missing, detail, HINT_SCLANG);
    };
    match run(&path, &["--version"], Duration::from_secs(10)) {
        Ok(_) => {
            let detail = match bundle_version(&path) {
                Some(version) => format!("{version} — {}", path.display()),
                None => path.display().to_string(),
            };
            probe(name, ProbeStatus::Ok, detail, "")
        }
        Err(message) => probe(
            name,
            ProbeStatus::Missing,
            format!("found at {} but cannot run: {message}", path.display()),
            HINT_SCLANG,
        ),
    }
}

fn quark_probe(_overrides: &Overrides, home: &Path) -> ProbeResult {
    let name = "superdirt-quark";
    let dir = quark_root(home).join("SuperDirt");
    if dir.is_dir() {
        probe(name, ProbeStatus::Ok, dir.display().to_string(), "")
    } else {
        probe(
            name,
            ProbeStatus::Missing,
            "quark not installed",
            HINT_QUARK,
        )
    }
}

fn samples_probe(_overrides: &Overrides, home: &Path) -> ProbeResult {
    let name = "dirt-samples";
    let dir = quark_root(home).join("Dirt-Samples");
    if dir.is_dir() {
        let groups = dir
            .read_dir()
            .map(|entries| entries.flatten().count())
            .unwrap_or_default();
        probe(
            name,
            ProbeStatus::Ok,
            format!("{groups} sample groups — {}", dir.display()),
            "",
        )
    } else {
        probe(
            name,
            ProbeStatus::Missing,
            "sample pack not found",
            HINT_SAMPLES,
        )
    }
}

type ProbeFn = fn(&Overrides, &Path) -> ProbeResult;

pub fn detect(overrides: &Overrides, home: &Path) -> DetectReport {
    let specs: [(&str, ProbeFn); 5] = [
        ("ghc", ghc_probe),
        ("tidal", tidal_probe),
        ("sclang", sclang_probe),
        ("superdirt-quark", quark_probe),
        ("dirt-samples", samples_probe),
    ];
    let (tx, rx) = mpsc::channel::<(usize, ProbeResult)>();
    thread::scope(|scope| {
        for (index, (_name, run_probe)) in specs.iter().enumerate() {
            let tx = tx.clone();
            scope.spawn(move || {
                let result = run_probe(overrides, home);
                let _ = tx.send((index, result));
            });
        }
    });
    drop(tx);
    let mut slots: Vec<Option<ProbeResult>> = (0..specs.len()).map(|_| None).collect();
    for (index, result) in rx {
        slots[index] = Some(result);
    }
    let probes: Vec<ProbeResult> = slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            slot.unwrap_or_else(|| probe(specs[index].0, ProbeStatus::Warn, "probe crashed", ""))
        })
        .collect();
    let status = derive_status(&probes);
    DetectReport { status, probes }
}

fn status_of(probes: &[ProbeResult], name: &str) -> ProbeStatus {
    probes
        .iter()
        .find(|p| p.name == name)
        .map(|p| p.status)
        .unwrap_or(ProbeStatus::Missing)
}

pub fn derive_status(probes: &[ProbeResult]) -> ToolchainStatus {
    let required_ok = [ProbeStatus::Ok, ProbeStatus::Ok]
        == [status_of(probes, "ghc"), status_of(probes, "tidal")];
    if !required_ok {
        return ToolchainStatus::Broken;
    }
    let superdirt_ok = ["sclang", "superdirt-quark", "dirt-samples"]
        .iter()
        .all(|name| status_of(probes, name) == ProbeStatus::Ok);
    if superdirt_ok {
        ToolchainStatus::SuperDirtReady
    } else {
        ToolchainStatus::HeadlessOnly
    }
}

pub fn detect_with_override_json(overrides_json: Option<&str>) -> DetectReport {
    let home = home_dir();
    let (overrides, note) = match overrides_json {
        None | Some("") => (Overrides::default(), None),
        Some(raw) => match serde_json::from_str::<Overrides>(raw) {
            Ok(parsed) => (parsed, None),
            Err(e) => (
                Overrides::default(),
                Some(probe(
                    "overrides",
                    ProbeStatus::Warn,
                    format!("override JSON ignored: {e}"),
                    r#"expected {"ghcPath":"…","sclangPath":"…"}"#,
                )),
            ),
        },
    };
    let mut report = detect(&overrides, &home);
    if let Some(probe) = note {
        report.probes.push(probe);
    }
    report
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[tauri::command]
pub async fn toolchain_detect(overrides: Option<String>) -> Result<DetectReport, String> {
    tauri::async_runtime::spawn_blocking(move || detect_with_override_json(overrides.as_deref()))
        .await
        .map_err(|e| format!("toolchain_detect failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str, status: ProbeStatus) -> ProbeResult {
        probe(name, status, "", "")
    }

    #[test]
    fn parses_ghc_version_line() {
        assert_eq!(
            parse_ghc_version("The Glorious Glasgow Haskell Compilation System, version 9.10.3")
                .as_deref(),
            Some("9.10.3")
        );
        assert_eq!(parse_ghc_version("no version here"), None);
    }

    #[test]
    fn parses_plist_version_from_xml() {
        let plist = b"<plist><key>CFBundleShortVersionString</key><string>3.14.1</string></plist>";
        assert_eq!(parse_plist_version(plist).as_deref(), Some("3.14.1"));
        assert_eq!(parse_plist_version(b"nonsense"), None);
    }

    #[test]
    fn derives_status_from_matrix() {
        let full = vec![
            named("ghc", ProbeStatus::Ok),
            named("tidal", ProbeStatus::Ok),
            named("sclang", ProbeStatus::Ok),
            named("superdirt-quark", ProbeStatus::Ok),
            named("dirt-samples", ProbeStatus::Ok),
        ];
        assert_eq!(derive_status(&full), ToolchainStatus::SuperDirtReady);

        let headless: Vec<ProbeResult> = full
            .iter()
            .filter(|p| !["sclang", "superdirt-quark", "dirt-samples"].contains(&p.name.as_str()))
            .cloned()
            .collect();
        assert_eq!(derive_status(&headless), ToolchainStatus::HeadlessOnly);

        let no_samples = vec![
            named("ghc", ProbeStatus::Ok),
            named("tidal", ProbeStatus::Ok),
            named("sclang", ProbeStatus::Ok),
            named("superdirt-quark", ProbeStatus::Ok),
            named("dirt-samples", ProbeStatus::Missing),
        ];
        assert_eq!(derive_status(&no_samples), ToolchainStatus::HeadlessOnly);

        let no_tidal = vec![
            named("ghc", ProbeStatus::Ok),
            named("tidal", ProbeStatus::Missing),
            named("sclang", ProbeStatus::Ok),
        ];
        assert_eq!(derive_status(&no_tidal), ToolchainStatus::Broken);

        let empty: Vec<ProbeResult> = vec![];
        assert_eq!(derive_status(&empty), ToolchainStatus::Broken);
    }

    #[test]
    fn strict_override_never_falls_back() {
        assert!(locate(Some("/definitely/not/ghc"), "ghc", &[]).is_none());
    }

    #[test]
    fn overrides_json_is_camel_case() {
        let parsed: Overrides = serde_json::from_str(r#"{"ghcPath":"/x/ghc"}"#).unwrap();
        assert_eq!(parsed.ghc_path.as_deref(), Some("/x/ghc"));
        assert_eq!(parsed.sclang_path, None);
    }

    #[test]
    #[ignore = "runs ghci probes (~10s); requires a machine with GHC + Tidal"]
    fn detects_this_machine() {
        let report = detect_with_override_json(None);
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        assert!(
            report.status != ToolchainStatus::Broken,
            "ghc+tidal present on this machine yet status={:?} probes={:?}",
            report.status,
            report
                .probes
                .iter()
                .map(|p| (&p.name, p.status))
                .collect::<Vec<_>>()
        );
    }
}
