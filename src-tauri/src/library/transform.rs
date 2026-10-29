use std::path::{Path, PathBuf};

use serde::Serialize;

use tauri::{AppHandle, Emitter, Manager};

use super::dsp::{self, NormalizeMode, Op};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransformMeta {
    pub name: String,
    pub file_path: String,
    pub hash: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub file_size: u64,
    pub source_path: String,
    pub operation: String,
    pub parameters_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TransformEvent {
    Progress { phase: String },
    Done { meta: TransformMeta },
    Error { message: String },
}

#[tauri::command]
pub async fn sample_transform(
    source_path: String,
    spec: Op,
    bits_per_sample: Option<u16>,
    app: AppHandle,
) -> Result<TransformMeta, String> {
    let bits = bits_per_sample.unwrap_or(16);
    if !matches!(bits, 16 | 24) {
        return Err(format!("bitsPerSample must be 16 or 24, got {bits}"));
    }

    let source = PathBuf::from(&source_path);
    if !source.is_file() {
        return Err(format!("source not found: {source_path}"));
    }

    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("sample");
    let name = derived_name(stem, &spec);
    let operation = op_name(&spec).to_string();
    let parameters_json = serde_json::to_string(&spec).unwrap_or_else(|_| "{}".to_string());

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    let out_dir = derived_dir(&data_dir).join(&name);
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("create {}: {e}", out_dir.display()))?;
    let out = out_dir.join(format!("{name}.wav"));

    let emitter = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<TransformMeta, String> {
        let _ = emitter.emit(
            "transform://event",
            &TransformEvent::Progress {
                phase: "decode".to_string(),
            },
        );
        let mut decoded = dsp::decode(&source)?;
        dsp::apply(&mut decoded, spec)?;
        let _ = emitter.emit(
            "transform://event",
            &TransformEvent::Progress {
                phase: "encode".to_string(),
            },
        );
        let file_size = dsp::encode_wav(&out, &decoded, bits)?;
        Ok(TransformMeta {
            name: name.clone(),
            file_path: out.display().to_string(),
            hash: dsp::hash(&decoded),
            duration_ms: decoded.duration_ms(),
            sample_rate: decoded.sample_rate,
            channels: decoded.channels,
            file_size,
            source_path: source.display().to_string(),
            operation,
            parameters_json,
        })
    })
    .await
    .map_err(|e| format!("transform job failed: {e}"))?;

    match result {
        Ok(meta) => {
            let _ = app.emit(
                "transform://event",
                &TransformEvent::Done { meta: meta.clone() },
            );
            Ok(meta)
        }
        Err(message) => {
            let _ = app.emit(
                "transform://event",
                &TransformEvent::Error {
                    message: message.clone(),
                },
            );
            Err(message)
        }
    }
}

pub fn derived_name(stem: &str, op: &Op) -> String {
    sanitize(&format!("{}-{}", stem, op_suffix(op)))
}

pub fn derived_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("samples").join("derived")
}

pub fn reload_flag_path(data_dir: &Path) -> PathBuf {
    data_dir.join("engine").join("reload.txt")
}

#[tauri::command]
pub fn sample_reload(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    let flag = reload_flag_path(&data_dir);
    std::fs::write(&flag, "reload")
        .map_err(|e| format!("cannot write reload flag {}: {e}", flag.display()))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirtGroup {
    pub name: String,
    pub files: Vec<String>,
}

#[tauri::command]
pub fn dirt_samples_groups() -> Result<Vec<DirtGroup>, String> {
    let root = crate::toolchain::find_dirt_samples()
        .ok_or_else(|| "Dirt-Samples not found — install the SuperDirt quark".to_string())?;
    let mut groups = Vec::new();
    for entry in std::fs::read_dir(&root).map_err(|e| format!("read {}: {e}", root.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let mut files = Vec::new();
        for f in std::fs::read_dir(entry.path()).map_err(|e| e.to_string())? {
            let f = f.map_err(|e| e.to_string())?;
            if f.file_type().map(|t| t.is_file()).unwrap_or(false) && is_audio(&f.path()) {
                files.push(f.path().display().to_string());
            }
        }
        files.sort();
        groups.push(DirtGroup { name, files });
    }
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(groups)
}

fn is_audio(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "wav" | "aif" | "aiff" | "aifc" | "flac" | "mp3" | "ogg" | "caf"
    )
}

fn op_name(op: &Op) -> &'static str {
    match op {
        Op::Reverse => "reverse",
        Op::Trim { .. } => "trim",
        Op::Normalize { .. } => "normalize",
        Op::Gain { .. } => "gain",
        Op::Fade { .. } => "fade",
    }
}

fn op_suffix(op: &Op) -> String {
    match op {
        Op::Reverse => "reverse".to_string(),
        Op::Trim { start_ms, end_ms } => format!("trim-{start_ms}-{end_ms}"),
        Op::Normalize {
            mode: NormalizeMode::Peak,
        } => "normalize-peak".to_string(),
        Op::Normalize {
            mode: NormalizeMode::Rms,
        } => "normalize-rms".to_string(),
        Op::Gain { db } => format!("gain-{}", db_token(*db)),
        Op::Fade { in_ms, out_ms } => format!("fade-{in_ms}-{out_ms}"),
    }
}

fn db_token(db: f32) -> String {
    let mut s = format!("{db:.2}");
    if s.ends_with(".00") {
        s.truncate(s.len() - 3);
    } else if s.ends_with('0') {
        s.truncate(s.len() - 1);
    }
    s.replace('-', "m").replace('.', "-")
}

fn sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_sep = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
            prev_sep = false;
        } else if !prev_sep {
            out.push('-');
            prev_sep = true;
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_normalizes_to_superdirt_convention() {
        assert_eq!(sanitize("Kick 01!Snare.wav"), "kick-01-snare-wav");
        assert_eq!(sanitize("  !!  leading/trailing -- "), "leading-trailing");
        assert_eq!(sanitize("Already_ok-01"), "already_ok-01");
    }

    #[test]
    fn derived_names_are_distinct_and_valid() {
        let stem = "Kick 01";
        let cases = vec![
            (Op::Reverse, "kick-01-reverse"),
            (
                Op::Trim {
                    start_ms: 100,
                    end_ms: 200,
                },
                "kick-01-trim-100-200",
            ),
            (
                Op::Normalize {
                    mode: NormalizeMode::Peak,
                },
                "kick-01-normalize-peak",
            ),
            (
                Op::Normalize {
                    mode: NormalizeMode::Rms,
                },
                "kick-01-normalize-rms",
            ),
            (Op::Gain { db: 6.0 }, "kick-01-gain-6"),
            (Op::Gain { db: -3.0 }, "kick-01-gain-m3"),
            (Op::Gain { db: 0.5 }, "kick-01-gain-0-5"),
            (
                Op::Fade {
                    in_ms: 100,
                    out_ms: 200,
                },
                "kick-01-fade-100-200",
            ),
        ];
        let mut seen = std::collections::HashSet::new();
        for (op, expected) in cases {
            let name = derived_name(stem, &op);
            assert_eq!(name, expected);
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "invalid chars in {name}"
            );
            assert!(seen.insert(name.clone()), "duplicate name {name}");
        }
    }

    #[test]
    fn op_serializes_to_tagged_json() {
        let json = serde_json::to_value(Op::Trim {
            start_ms: 100,
            end_ms: 200,
        })
        .unwrap();
        assert_eq!(json["op"], "trim");
        assert_eq!(json["startMs"], 100);
        assert_eq!(json["endMs"], 200);

        let json = serde_json::to_value(Op::Reverse).unwrap();
        assert_eq!(json["op"], "reverse");
    }

    #[test]
    fn op_deserializes_from_camel_case_json() {
        let op: Op = serde_json::from_str(r#"{"op":"trim","startMs":100,"endMs":200}"#).unwrap();
        assert_eq!(
            op,
            Op::Trim {
                start_ms: 100,
                end_ms: 200,
            }
        );

        let op: Op = serde_json::from_str(r#"{"op":"normalize","mode":"rms"}"#).unwrap();
        assert_eq!(
            op,
            Op::Normalize {
                mode: NormalizeMode::Rms,
            }
        );

        let op: Op = serde_json::from_str(r#"{"op":"reverse"}"#).unwrap();
        assert_eq!(op, Op::Reverse);
    }

    #[test]
    fn db_token_is_unambiguous() {
        assert_eq!(db_token(6.0), "6");
        assert_eq!(db_token(-3.0), "m3");
        assert_eq!(db_token(0.5), "0-5");
        assert_eq!(db_token(-0.5), "m0-5");
    }

    #[test]
    fn is_audio_accepts_supported_extensions() {
        assert!(is_audio(Path::new("a.wav")));
        assert!(is_audio(Path::new("a.FLAC")));
        assert!(is_audio(Path::new("a.mp3")));
        assert!(is_audio(Path::new("a.ogg")));
        assert!(is_audio(Path::new("a.caf")));
        assert!(is_audio(Path::new("a.aiff")));
        assert!(!is_audio(Path::new("README.md")));
        assert!(!is_audio(Path::new("a.txt")));
        assert!(!is_audio(Path::new("noext")));
    }
}
