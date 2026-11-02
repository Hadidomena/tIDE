pub mod dsp;
pub mod transform;

use std::path::{Path, PathBuf};

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
}
