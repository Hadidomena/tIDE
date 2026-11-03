use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::{AUDIO_EXTENSIONS, dsp, ensure_custom_dir, is_audio, sanitize_name};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSample {
    pub name: String,
    pub file_path: String,
    pub hash: String,
    pub duration_ms: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub file_size: u64,
}

#[tauri::command]
pub async fn sample_import(
    paths: Vec<String>,
    name: Option<String>,
    app: tauri::AppHandle,
) -> Result<Vec<ImportedSample>, String> {
    use tauri::Manager;
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    tauri::async_runtime::spawn_blocking(move || import_into(&data_dir, &paths, name.as_deref()))
        .await
        .map_err(|e| format!("import job failed: {e}"))?
}

pub fn import_into(
    data_dir: &Path,
    paths: &[String],
    name: Option<&str>,
) -> Result<Vec<ImportedSample>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    if name.is_some() && paths.len() != 1 {
        return Err("a sample name can only be used with a single file".to_string());
    }
    let prepared: Vec<Prepared> = paths
        .iter()
        .map(|raw| prepare(Path::new(raw)))
        .collect::<Result<_, String>>()?;
    let root = ensure_custom_dir(data_dir)?;
    prepared
        .into_iter()
        .map(|item| copy_one(&root, item, name))
        .collect()
}

struct Prepared {
    source: PathBuf,
    decoded: dsp::Decoded,
    hash: String,
}

fn prepare(source: &Path) -> Result<Prepared, String> {
    if !source.is_file() {
        return Err(format!("not a file: {}", source.display()));
    }
    if !is_audio(source) {
        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("(none)")
            .to_ascii_lowercase();
        return Err(format!(
            "unsupported file type .{ext} — supported: {}",
            AUDIO_EXTENSIONS.join(", ")
        ));
    }
    let decoded = dsp::decode(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let hash = hash_file(source)?;
    Ok(Prepared {
        source: source.to_path_buf(),
        decoded,
        hash,
    })
}

fn copy_one(root: &Path, item: Prepared, name: Option<&str>) -> Result<ImportedSample, String> {
    let Prepared {
        source,
        decoded,
        hash,
    } = item;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let requested = name.unwrap_or(stem);
    let base = match sanitize_name(requested) {
        name if name.is_empty() => "sample".to_string(),
        name => name,
    };
    let name = unique_name(root, &base);
    let dir = root.join(&name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    let file_name = source
        .file_name()
        .ok_or_else(|| format!("no file name in {}", source.display()))?;
    let dest = dir.join(file_name);
    if let Err(e) = std::fs::copy(&source, &dest) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!(
            "copy {} -> {}: {e}",
            source.display(),
            dest.display()
        ));
    }

    let file_size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    Ok(ImportedSample {
        name,
        file_path: dest.display().to_string(),
        hash,
        duration_ms: decoded.duration_ms(),
        sample_rate: decoded.sample_rate,
        channels: decoded.channels,
        file_size,
    })
}

fn unique_name(root: &Path, base: &str) -> String {
    if !root.join(base).exists() {
        return base.to_string();
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}-{n}");
        if !root.join(&candidate).exists() {
            return candidate;
        }
        n += 1;
    }
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buf)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::custom_dir;
    use crate::library::dsp::Decoded;

    fn temp_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tide-import-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn write_wav(dir: &Path, file_name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(file_name);
        let decoded = Decoded {
            samples: (0..441).map(|i| i as f32 / 441.0 * 0.5).collect(),
            sample_rate: 44100,
            channels: 1,
        };
        dsp::encode_wav(&path, &decoded, 16).expect("encode test wav");
        path
    }

    fn bytes_hash(path: &Path) -> String {
        let bytes = std::fs::read(path).unwrap();
        blake3::hash(&bytes).to_hex().to_string()
    }

    #[test]
    fn import_copies_as_is_and_reports_metadata() {
        let src_dir = temp_root("src");
        let src = write_wav(&src_dir, "Kick 01!.wav");
        let data = temp_root("data");

        let imported = import_into(&data, &[src.display().to_string()], None).expect("import");
        assert_eq!(imported.len(), 1);
        let sample = &imported[0];

        assert_eq!(sample.name, "kick-01");
        let dest = custom_dir(&data).join(&sample.name).join("Kick 01!.wav");
        assert_eq!(Path::new(&sample.file_path), dest);
        assert!(dest.is_file(), "copied file missing at {}", dest.display());
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            std::fs::read(&src).unwrap(),
            "import must copy bytes as-is"
        );

        assert_eq!(sample.hash, bytes_hash(&src));
        assert_eq!(sample.sample_rate, 44100);
        assert_eq!(sample.channels, 1);
        assert_eq!(sample.duration_ms, 10);
        assert_eq!(sample.file_size, std::fs::metadata(&dest).unwrap().len());

        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn name_collisions_get_a_numeric_suffix() {
        let src_dir = temp_root("collide");
        let a = write_wav(&src_dir.join("a"), "kick.wav");
        let b = write_wav(&src_dir.join("b"), "kick.wav");
        let data = temp_root("collide-data");

        let imported = import_into(
            &data,
            &[a.display().to_string(), b.display().to_string()],
            None,
        )
        .expect("import both");
        let names: Vec<&str> = imported.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["kick", "kick-2"]);
        assert!(custom_dir(&data).join("kick").is_dir());
        assert!(custom_dir(&data).join("kick-2").is_dir());

        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn hash_is_stable_for_identical_bytes() {
        let a_dir = temp_root("hash-a");
        let b_dir = temp_root("hash-b");
        let a = write_wav(&a_dir, "probe.wav");
        let b = write_wav(&b_dir, "probe.wav");
        assert_eq!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
        // A different name must not change the content hash.
        let c = write_wav(&b_dir, "other.wav");
        assert_eq!(hash_file(&a).unwrap(), hash_file(&c).unwrap());
        let _ = std::fs::remove_dir_all(&a_dir);
        let _ = std::fs::remove_dir_all(&b_dir);
    }

    #[test]
    fn unsupported_extension_is_rejected_without_writing() {
        let src_dir = temp_root("reject");
        std::fs::create_dir_all(&src_dir).unwrap();
        let src = src_dir.join("notes.txt");
        std::fs::write(&src, b"not audio").unwrap();
        let data = temp_root("reject-data");

        let err = import_into(&data, &[src.display().to_string()], None).unwrap_err();
        assert!(err.contains(".txt"), "unreadable error: {err}");
        assert!(err.contains("supported: wav"), "unreadable error: {err}");
        assert!(
            !custom_dir(&data).join("notes-txt").exists(),
            "nothing may be copied for a rejected file"
        );
        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn missing_path_is_rejected() {
        let data = temp_root("missing-data");
        let err =
            import_into(&data, &["/nonexistent/tide/kick.wav".to_string()], None).unwrap_err();
        assert!(err.contains("not a file"), "unreadable error: {err}");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn empty_selection_is_a_noop() {
        let data = temp_root("empty");
        assert!(import_into(&data, &[], None).unwrap().is_empty());
        assert!(!data.exists(), "no dirs are created for an empty selection");
    }

    #[test]
    fn named_import_uses_the_given_name() {
        let src_dir = temp_root("named-src");
        let src = write_wav(&src_dir, "tIDE-20261103-101500.wav");
        let data = temp_root("named-data");

        let imported = import_into(&data, &[src.display().to_string()], Some("Bounce 01!"))
            .expect("named import");
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].name, "bounce-01");
        assert!(
            custom_dir(&data)
                .join("bounce-01")
                .join("tIDE-20261103-101500.wav")
                .is_file()
        );

        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn named_import_rejects_multiple_paths() {
        let src_dir = temp_root("named-multi-src");
        let a = write_wav(&src_dir.join("a"), "a.wav");
        let b = write_wav(&src_dir.join("b"), "b.wav");
        let data = temp_root("named-multi-data");

        let err = import_into(
            &data,
            &[a.display().to_string(), b.display().to_string()],
            Some("both"),
        )
        .unwrap_err();
        assert!(err.contains("single file"), "unreadable error: {err}");
        let _ = std::fs::remove_dir_all(&src_dir);
        let _ = std::fs::remove_dir_all(&data);
    }
}
