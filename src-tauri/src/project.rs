use serde::Serialize;
use std::fs;
use std::path::Path;

const MAX_READ_BYTES: u64 = 8 * 1024 * 1024; // 8 MB is plenty for pattern files

#[derive(Serialize)]
pub struct OpenedFile {
    pub path: String,
    pub content: String,
}

fn require_absolute(path: &str) -> Result<&Path, String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("only absolute paths are accepted (pick via dialog)".to_string());
    }
    Ok(p)
}

#[tauri::command]
pub fn project_open(path: String) -> Result<OpenedFile, String> {
    let p = require_absolute(&path)?;
    let meta = fs::metadata(p).map_err(|e| format!("stat failed: {e}"))?;
    if meta.len() > MAX_READ_BYTES {
        return Err(format!("file too large ({} bytes)", meta.len()));
    }
    let content = fs::read_to_string(p).map_err(|e| format!("read failed: {e}"))?;
    Ok(OpenedFile { path, content })
}

#[tauri::command]
pub fn project_save(path: String, content: String) -> Result<(), String> {
    let p = require_absolute(&path)?;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir failed: {e}"))?;
    }
    fs::write(p, content).map_err(|e| format!("write failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_paths() {
        assert!(project_open("main.tidal".into()).is_err());
        assert!(project_save("rel/p.tidal".into(), "x".into()).is_err());
    }

    #[test]
    fn roundtrip_creates_parents() {
        let dir = std::env::temp_dir().join(format!("tide-project-{}", std::process::id()));
        let file = dir.join("deep").join("main.tidal");
        let path = file.to_str().unwrap().to_string();

        project_save(path.clone(), "d1 $ s \"bd\"".into()).expect("save");
        let opened = project_open(path).expect("open");
        assert_eq!(opened.content, "d1 $ s \"bd\"");

        fs::remove_dir_all(&dir).ok();
    }
}
