use std::path::{Path, PathBuf};

pub struct Kernel {
    event_log_path: PathBuf,
}

impl Kernel {
    pub fn new(event_log_path: &Path) -> Self {
        Self {
            event_log_path: event_log_path.to_path_buf(),
        }
    }

    pub fn event_log_path(&self) -> &Path {
        &self.event_log_path
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.event_log_path.exists() {
            return Err("event log does not exist".to_string());
        }
        let content = std::fs::read_to_string(&self.event_log_path)
            .map_err(|e| format!("cannot read event log: {}", e))?;
        for (i, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            serde_json::from_str::<serde_json::Value>(line)
                .map_err(|e| format!("invalid JSON on line {}: {}", i + 1, e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn kernel_new() {
        let k = Kernel::new(Path::new("/tmp/events.jsonl"));
        assert_eq!(k.event_log_path(), Path::new("/tmp/events.jsonl"));
    }

    #[test]
    fn validate_missing_file() {
        assert!(Kernel::new(Path::new("/nonexistent")).validate().is_err());
    }

    #[test]
    fn validate_valid_jsonl() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("events.jsonl");
        std::fs::write(&path, "{\"id\":\"e1\"}\n{\"id\":\"e2\"}\n").unwrap();
        assert!(Kernel::new(&path).validate().is_ok());
    }

    #[test]
    fn validate_invalid_json() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("events.jsonl");
        std::fs::write(&path, "{\"id\":\"e1\"}\nnot json\n").unwrap();
        assert!(Kernel::new(&path).validate().is_err());
    }

    #[test]
    fn validate_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("events.jsonl");
        std::fs::write(&path, "").unwrap();
        assert!(Kernel::new(&path).validate().is_ok());
    }
}
