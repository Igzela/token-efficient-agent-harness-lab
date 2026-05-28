use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const BACKUP_MANAGER_SCHEMA_VERSION: &str = "backup_manager.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupRecord {
    pub backup_id: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub label: String,
    pub source_path: String,
    pub backup_path: String,
    pub checksum: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoreResult {
    pub success: bool,
    pub records_restored: i64,
    pub errors: Vec<String>,
    pub duration_ms: f64,
}

pub struct BackupManager {
    backup_dir: PathBuf,
    _lock: Mutex<()>,
}

impl BackupManager {
    pub fn new(backup_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(backup_dir).map_err(|e| e.to_string())?;
        Ok(Self {
            backup_dir: backup_dir.to_path_buf(),
            _lock: Mutex::new(()),
        })
    }

    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    pub fn create_backup(
        &self,
        source_path: &Path,
        label: &str,
        backup_id: &str,
        now: &str,
    ) -> Result<BackupRecord, String> {
        if !source_path.exists() {
            return Err(format!("source file not found: {}", source_path.display()));
        }

        let dest = self.backup_dir.join(format!("{backup_id}.db"));
        fs::copy(source_path, &dest).map_err(|e| e.to_string())?;

        // Checkpoint WAL if present
        let wal_path = source_path.with_extension("db-wal");
        if wal_path.exists() {
            let _ = fs::copy(
                &wal_path,
                self.backup_dir.join(format!("{backup_id}.db-wal")),
            );
        }

        let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);

        let checksum = compute_checksum(&dest)?;

        Ok(BackupRecord {
            backup_id: backup_id.to_string(),
            created_at: now.to_string(),
            size_bytes: size,
            label: label.to_string(),
            source_path: source_path.display().to_string(),
            backup_path: dest.display().to_string(),
            checksum,
        })
    }

    pub fn list_backups(&self) -> Result<Vec<BackupRecord>, String> {
        let meta_path = self.backup_dir.join("backup_metadata.json");
        if !meta_path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
        let records: Vec<BackupRecord> = serde_json::from_str(&data).map_err(|e| e.to_string())?;
        Ok(records)
    }

    pub fn get_backup(&self, backup_id: &str) -> Result<Option<BackupRecord>, String> {
        let backups = self.list_backups()?;
        Ok(backups.into_iter().find(|b| b.backup_id == backup_id))
    }

    pub fn restore_backup(
        &self,
        backup_id: &str,
        target_path: &Path,
        now: f64,
    ) -> Result<RestoreResult, String> {
        let start = now;
        let mut errors = Vec::new();

        let backup = self
            .get_backup(backup_id)?
            .ok_or_else(|| format!("backup not found: {backup_id}"))?;

        let backup_path = Path::new(&backup.backup_path);
        if !backup_path.exists() {
            return Err(format!("backup file not found: {}", backup.backup_path));
        }

        // Verify checksum
        let actual_checksum = compute_checksum(backup_path)?;
        if actual_checksum != backup.checksum {
            return Err(format!(
                "checksum mismatch: expected {}, got {}",
                backup.checksum, actual_checksum
            ));
        }

        // Copy to temp first, then atomic rename
        let tmp_path = target_path.with_extension("db.restore_tmp");
        if let Err(e) = fs::copy(backup_path, &tmp_path) {
            errors.push(format!("copy failed: {e}"));
            return Ok(RestoreResult {
                success: false,
                records_restored: 0,
                errors,
                duration_ms: (now - start) * 1000.0,
            });
        }

        // Atomic rename
        if let Err(e) = fs::rename(&tmp_path, target_path) {
            let _ = fs::remove_file(&tmp_path);
            errors.push(format!("rename failed: {e}"));
            return Ok(RestoreResult {
                success: false,
                records_restored: 0,
                errors,
                duration_ms: (now - start) * 1000.0,
            });
        }

        Ok(RestoreResult {
            success: true,
            records_restored: 0,
            errors,
            duration_ms: (now - start) * 1000.0,
        })
    }

    pub fn delete_backup(&self, backup_id: &str) -> Result<bool, String> {
        let backup_path = self.backup_dir.join(format!("{backup_id}.db"));
        if backup_path.exists() {
            fs::remove_file(&backup_path).map_err(|e| e.to_string())?;
        }

        // Remove from metadata
        let meta_path = self.backup_dir.join("backup_metadata.json");
        if meta_path.exists() {
            let data = fs::read_to_string(&meta_path).map_err(|e| e.to_string())?;
            let mut records: Vec<BackupRecord> =
                serde_json::from_str(&data).map_err(|e| e.to_string())?;
            let before = records.len();
            records.retain(|r| r.backup_id != backup_id);
            if records.len() != before {
                let json = serde_json::to_string_pretty(&records).map_err(|e| e.to_string())?;
                fs::write(&meta_path, json).map_err(|e| e.to_string())?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn save_metadata(&self, records: &[BackupRecord]) -> Result<(), String> {
        let meta_path = self.backup_dir.join("backup_metadata.json");
        let json = serde_json::to_string_pretty(records).map_err(|e| e.to_string())?;
        let tmp_path = meta_path.with_extension("json.tmp");
        fs::write(&tmp_path, &json).map_err(|e| e.to_string())?;
        fs::rename(&tmp_path, &meta_path).map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn compute_checksum(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    Ok(hex::encode(hasher.finalize()))
}
