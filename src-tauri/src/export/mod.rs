use std::path::{Path, PathBuf};

/// Where exported recordings are written: `data_dir/exports`.
pub fn exports_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("exports")
}

pub fn record_flag_path(data_dir: &Path) -> PathBuf {
    data_dir.join("engine").join("record.txt")
}

pub fn stop_flag_path(data_dir: &Path) -> PathBuf {
    data_dir.join("engine").join("stoprecord.txt")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
