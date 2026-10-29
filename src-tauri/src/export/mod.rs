use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::library::dsp;

pub fn exports_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("exports")
}

pub fn record_flag_path(data_dir: &Path) -> PathBuf {
    data_dir.join("engine").join("record.txt")
}

pub fn stop_flag_path(data_dir: &Path) -> PathBuf {
    data_dir.join("engine").join("stoprecord.txt")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStatus {
    pub recording: bool,
    pub path: String,
    pub format: String,
    pub duration_secs: f64,
}

const FORMATS: [&str; 3] = ["float", "int16", "int24"];

#[tauri::command]
pub fn export_start(
    duration_secs: Option<f64>,
    format: Option<String>,
    app: AppHandle,
) -> Result<ExportStatus, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    start_in(&data_dir, duration_secs, format)
}

#[tauri::command]
pub fn export_stop(app: AppHandle) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    stop_in(&data_dir)
}

#[tauri::command]
pub async fn export_finalize(
    wav_path: String,
    target: String,
    delete_source: Option<bool>,
    app: AppHandle,
) -> Result<ExportStatus, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    let delete_source = delete_source.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || {
        finalize_in(&data_dir, &wav_path, &target, delete_source)
    })
    .await
    .map_err(|e| format!("export finalize job failed: {e}"))?
}

pub fn start_in(
    data_dir: &Path,
    duration_secs: Option<f64>,
    format: Option<String>,
) -> Result<ExportStatus, String> {
    let format = format.unwrap_or_else(|| "int16".to_string());
    if !FORMATS.contains(&format.as_str()) {
        return Err(format!(
            "unsupported format '{format}' (use float|int16|int24)"
        ));
    }
    let duration = duration_secs
        .filter(|d| d.is_finite() && *d > 0.0)
        .unwrap_or(-1.0);

    let dir = exports_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join(format!("tIDE-{}.wav", utc_stamp(SystemTime::now())));

    let flag = record_flag_path(data_dir);
    ensure_parent(&flag)?;
    write_atomic(
        &flag,
        &format!("{}\n{}\n{}\n", path.display(), duration, format),
    )?;

    Ok(ExportStatus {
        recording: true,
        path: path.display().to_string(),
        format,
        duration_secs: duration,
    })
}

pub fn stop_in(data_dir: &Path) -> Result<(), String> {
    let flag = stop_flag_path(data_dir);
    ensure_parent(&flag)?;
    write_atomic(&flag, "stop")
}

pub fn finalize_in(
    data_dir: &Path,
    wav_path: &str,
    target: &str,
    delete_source: bool,
) -> Result<ExportStatus, String> {
    let path = Path::new(wav_path);
    if !path.is_file() {
        return Err(format!("recording not found: {wav_path}"));
    }
    let exports = exports_dir(data_dir);
    if !is_within(&exports, path) {
        return Err(format!(
            "refusing to finalize a file outside {}: {wav_path}",
            exports.display()
        ));
    }

    let target = target.to_ascii_lowercase();
    let decoded = wait_for_wav(wav_path)?;
    let duration_secs = decoded.duration_ms() as f64 / 1000.0;

    if target != "flac" {
        return Ok(ExportStatus {
            recording: false,
            path: wav_path.to_string(),
            format: "wav".to_string(),
            duration_secs,
        });
    }

    let out = path.with_extension("flac");
    let _ = dsp::encode_flac(&out, &decoded, 16)?;
    if delete_source && out != path {
        let _ = std::fs::remove_file(path);
    }
    Ok(ExportStatus {
        recording: false,
        path: out.display().to_string(),
        format: "flac".to_string(),
        duration_secs,
    })
}

fn is_within(dir: &Path, path: &Path) -> bool {
    let (Ok(dir), Ok(path)) = (dir.canonicalize(), path.canonicalize()) else {
        return false;
    };
    path.starts_with(dir)
}

fn wait_for_wav(wav_path: &str) -> Result<dsp::Decoded, String> {
    let path = Path::new(wav_path);
    if !path.is_file() {
        return Err(format!("recording not found: {wav_path}"));
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let last = match dsp::decode(path) {
            Ok(decoded) if decoded.frames() > 0 => return Ok(decoded),
            Ok(_) => "no frames".to_string(),
            Err(e) => e,
        };
        if Instant::now() >= deadline {
            return Err(format!("recording did not finalize in time ({last})"));
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    Ok(())
}

fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    let _ = std::fs::remove_file(path); // rename-over isn't portable (Windows)
    std::fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", tmp.display()))
}

fn utc_stamp(now: SystemTime) -> String {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let secs_of_day = (secs % 86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    let (hh, mm, ss) = (
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    );
    format!("{year:04}{month:02}{day:02}-{hh:02}{mm:02}{ss:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (y + i64::from(m <= 2), m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tide-export-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn paths_are_rooted_at_data_dir() {
        let data = Path::new("/tmp/tide");
        assert_eq!(exports_dir(data), Path::new("/tmp/tide/exports"));
        assert_eq!(
            record_flag_path(data),
            Path::new("/tmp/tide/engine/record.txt")
        );
        assert_eq!(
            stop_flag_path(data),
            Path::new("/tmp/tide/engine/stoprecord.txt")
        );
    }

    #[test]
    fn utc_stamp_matches_known_dates() {
        assert_eq!(utc_stamp(UNIX_EPOCH), "19700101-000000");
        assert_eq!(
            utc_stamp(UNIX_EPOCH + Duration::from_secs(946_684_800)),
            "20000101-000000"
        );
        assert_eq!(
            utc_stamp(UNIX_EPOCH + Duration::from_secs(1_709_164_800)),
            "20240229-000000"
        );
        assert_eq!(
            utc_stamp(UNIX_EPOCH + Duration::from_secs(946_684_800 + 3_661)),
            "20000101-010101"
        );
    }

    #[test]
    fn start_writes_a_line_based_request() {
        let data = temp_dir("start");
        let status = start_in(&data, Some(2.5), Some("int24".to_string())).unwrap();
        assert!(status.recording);
        assert_eq!(status.format, "int24");
        assert!((status.duration_secs - 2.5).abs() < 1e-9);
        assert!(status.path.ends_with(".wav"));

        let raw = std::fs::read_to_string(record_flag_path(&data)).unwrap();
        let lines: Vec<&str> = raw.lines().collect();
        assert_eq!(
            lines.len(),
            3,
            "request must be path/duration/format: {raw:?}"
        );
        assert_eq!(lines[0], status.path);
        assert_eq!(lines[1], "2.5");
        assert_eq!(lines[2], "int24");

        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn start_defaults_to_manual_int16() {
        let data = temp_dir("defaults");
        let status = start_in(&data, None, None).unwrap();
        assert_eq!(status.format, "int16");
        assert_eq!(status.duration_secs, -1.0);
        let raw = std::fs::read_to_string(record_flag_path(&data)).unwrap();
        assert!(raw.contains("\n-1\nint16\n"), "{raw:?}");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn start_rejects_unknown_format() {
        let data = temp_dir("badfmt");
        assert!(start_in(&data, None, Some("mp3".to_string())).is_err());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn stop_writes_the_stop_flag() {
        let data = temp_dir("stop");
        stop_in(&data).unwrap();
        assert!(stop_flag_path(&data).is_file());
        let _ = std::fs::remove_dir_all(&data);
    }

    fn write_test_wav(path: &Path) {
        let decoded = dsp::Decoded {
            samples: (0..441).map(|i| (i as f32 / 44.1).sin() * 0.5).collect(),
            sample_rate: 44100,
            channels: 1,
        };
        dsp::encode_wav(path, &decoded, 16).unwrap();
    }

    fn exports_with_wav(tag: &str) -> (PathBuf, PathBuf) {
        let data = temp_dir(tag);
        let exports = exports_dir(&data);
        std::fs::create_dir_all(&exports).unwrap();
        let wav = exports.join("take.wav");
        write_test_wav(&wav);
        (data, wav)
    }

    #[test]
    fn finalize_flac_converts_and_removes_source() {
        let (data, wav) = exports_with_wav("flac");
        let status = finalize_in(&data, wav.to_str().unwrap(), "flac", true).unwrap();
        assert_eq!(status.format, "flac");
        assert!(status.path.ends_with(".flac"));
        assert!(Path::new(&status.path).is_file(), "flac output missing");
        assert!(!wav.exists(), "source wav should be removed");

        let decoded = dsp::decode(Path::new(&status.path)).expect("decode the flac");
        assert_eq!(decoded.channels, 1);
        assert!(decoded.frames() > 0);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn finalize_wav_keeps_the_file() {
        let (data, wav) = exports_with_wav("keepwav");
        let status = finalize_in(&data, wav.to_str().unwrap(), "wav", true).unwrap();
        assert_eq!(status.format, "wav");
        assert_eq!(status.path, wav.display().to_string());
        assert!(wav.is_file(), "wav must be kept for a wav target");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn finalize_rejects_paths_outside_exports() {
        let data = temp_dir("outside");
        let outside = data.join("not-a-recording.wav");
        write_test_wav(&outside);

        assert!(finalize_in(&data, outside.to_str().unwrap(), "flac", true).is_err());
        assert!(outside.is_file(), "must not touch files outside exports/");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn finalize_does_not_delete_a_flac_input() {
        let (data, wav) = exports_with_wav("flacinput");
        let flac = wav.with_extension("flac");
        let decoded = dsp::decode(&wav).unwrap();
        dsp::encode_flac(&flac, &decoded, 16).unwrap();

        let status = finalize_in(&data, flac.to_str().unwrap(), "flac", true).unwrap();
        assert_eq!(status.path, flac.display().to_string());
        assert!(flac.is_file(), "must not delete its own output");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn finalize_missing_file_errors() {
        let data = temp_dir("missing");
        assert!(finalize_in(&data, "/nonexistent/tide-take.wav", "flac", false).is_err());
        let _ = std::fs::remove_dir_all(&data);
    }
}
