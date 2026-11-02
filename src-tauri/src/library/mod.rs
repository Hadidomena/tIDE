pub mod dsp;
pub mod import;
pub mod transform;

use std::path::{Path, PathBuf};

pub const AUDIO_EXTENSIONS: [&str; 8] = ["wav", "aif", "aiff", "aifc", "flac", "mp3", "ogg", "caf"];

pub fn is_audio(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    AUDIO_EXTENSIONS.contains(&ext.as_str())
}

pub fn sanitize_name(raw: &str) -> String {
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

pub fn samples_root(data_dir: &Path) -> PathBuf {
    data_dir.join("samples")
}

pub fn custom_dir(data_dir: &Path) -> PathBuf {
    samples_root(data_dir).join("custom")
}

pub fn derived_dir(data_dir: &Path) -> PathBuf {
    samples_root(data_dir).join("derived")
}

pub fn ensure_custom_dir(data_dir: &Path) -> Result<PathBuf, String> {
    let dir = custom_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
}

pub fn delete_in(data_dir: &Path, source_type: &str, name: &str) -> Result<(), String> {
    let root = match source_type {
        "custom" => custom_dir(data_dir),
        "derived" => derived_dir(data_dir),
        other => {
            return Err(format!(
                "only 'custom' and 'derived' samples are managed (got '{other}')"
            ));
        }
    };
    if name.is_empty() || sanitize_name(name) != name {
        return Err(format!("invalid sample name: '{name}'"));
    }
    let dir = root.join(name);
    if !dir.is_dir() {
        return Err(format!("no {source_type} sample named {name}"));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("delete {}: {e}", dir.display()))
}

#[tauri::command]
pub fn sample_delete(
    name: String,
    source_type: String,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri::Manager;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    delete_in(&data_dir, &source_type, &name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirs_share_the_samples_root() {
        let data = Path::new("/tmp/tide-data");
        assert_eq!(samples_root(data), Path::new("/tmp/tide-data/samples"));
        assert_eq!(custom_dir(data), Path::new("/tmp/tide-data/samples/custom"));
        assert_eq!(
            derived_dir(data),
            Path::new("/tmp/tide-data/samples/derived")
        );
    }

    #[test]
    fn ensure_custom_dir_is_idempotent() {
        let data = std::env::temp_dir().join(format!(
            "tide-library-dirs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let first = ensure_custom_dir(&data).expect("create custom dir");
        assert!(first.is_dir());
        let second = ensure_custom_dir(&data).expect("second call must not fail");
        assert_eq!(first, second);
        assert!(!derived_dir(&data).exists(), "derived is created lazily");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn sanitize_name_normalizes_to_superdirt_convention() {
        assert_eq!(sanitize_name("Kick 01!Snare.wav"), "kick-01-snare-wav");
        assert_eq!(
            sanitize_name("  !!  leading/trailing -- "),
            "leading-trailing"
        );
        assert_eq!(sanitize_name("Already_ok-01"), "already_ok-01");
        assert_eq!(sanitize_name("###"), "");
    }

    #[test]
    fn is_audio_accepts_supported_extensions() {
        assert!(is_audio(Path::new("a.wav")));
        assert!(is_audio(Path::new("a.FLAC")));
        assert!(is_audio(Path::new("a.mp3")));
        assert!(is_audio(Path::new("a.ogg")));
        assert!(is_audio(Path::new("a.caf")));
        assert!(is_audio(Path::new("a.aiff")));
        assert!(is_audio(Path::new("a.aifc")));
        assert!(!is_audio(Path::new("README.md")));
        assert!(!is_audio(Path::new("a.txt")));
        assert!(!is_audio(Path::new("noext")));
    }

    fn temp_data(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tide-library-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn seeded(data: &Path) -> (PathBuf, PathBuf) {
        let custom = custom_dir(data).join("kick-01");
        let derived = derived_dir(data).join("kick-01-reverse");
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::create_dir_all(&derived).unwrap();
        std::fs::write(custom.join("kick-01.wav"), b"x").unwrap();
        std::fs::write(derived.join("kick-01-reverse.wav"), b"x").unwrap();
        (custom, derived)
    }

    #[test]
    fn delete_in_removes_only_the_named_tree() {
        let data = temp_data("delete");
        let (custom, derived) = seeded(&data);

        delete_in(&data, "custom", "kick-01").expect("delete custom");
        assert!(!custom.exists());
        assert!(derived.is_dir(), "derived must survive a custom delete");

        delete_in(&data, "derived", "kick-01-reverse").expect("delete derived");
        assert!(!derived.exists());
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn delete_in_rejects_traversal_and_foreign_trees() {
        let data = temp_data("delete-guard");
        seeded(&data);

        assert!(delete_in(&data, "custom", "../derived/kick-01-reverse").is_err());
        assert!(delete_in(&data, "custom", "kick-01/../kick-01-reverse").is_err());
        assert!(delete_in(&data, "custom", "..").is_err());
        assert!(delete_in(&data, "custom", "").is_err());
        assert!(delete_in(&data, "community", "kick-01").is_err());
        assert!(delete_in(&data, "custom", "nope").is_err());
        assert!(
            custom_dir(&data).join("kick-01").is_dir(),
            "nothing deleted"
        );
        let _ = std::fs::remove_dir_all(&data);
    }
}
