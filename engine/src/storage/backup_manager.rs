use rusqlite::Connection;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_key_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoreResult {
    pub success: bool,
    pub records_restored: i64,
    pub errors: Vec<String>,
    pub duration_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupVerification {
    pub backup_id: String,
    pub success: bool,
    pub checksum_ok: bool,
    pub integrity_ok: bool,
    pub records_checked: i64,
    pub size_bytes: u64,
    pub backup_path: String,
    pub target_path: Option<String>,
    pub restore_would_overwrite: bool,
    pub dry_run: bool,
    pub errors: Vec<String>,
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
        self.create_backup_with_encryption(source_path, label, backup_id, now, None)
    }

    pub fn create_backup_with_encryption(
        &self,
        source_path: &Path,
        label: &str,
        backup_id: &str,
        now: &str,
        encryption_key: Option<&str>,
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

        let encryption_key_hash = encryption_key.map(|key| {
            let mut hasher = Sha256::new();
            hasher.update(key.as_bytes());
            hex::encode(hasher.finalize())
        });

        Ok(BackupRecord {
            backup_id: backup_id.to_string(),
            created_at: now.to_string(),
            size_bytes: size,
            label: label.to_string(),
            source_path: source_path.display().to_string(),
            backup_path: dest.display().to_string(),
            checksum,
            encryption_key_hash,
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

    pub fn restore_backup_with_verify(
        &self,
        backup_id: &str,
        target_path: &Path,
        now: f64,
    ) -> Result<RestoreResult, String> {
        let mut result = self.restore_backup(backup_id, target_path, now)?;
        if !result.success {
            return Ok(result);
        }

        match Connection::open(target_path) {
            Ok(conn) => {
                let integrity: String = conn
                    .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                    .unwrap_or_else(|_| "error".to_string());
                if integrity != "ok" {
                    result
                        .errors
                        .push(format!("integrity check failed: {integrity}"));
                    result.success = false;
                    return Ok(result);
                }

                let tables = [
                    "dispatch_history",
                    "local_config",
                    "team_members",
                    "api_key_metadata",
                    "audit_log",
                    "provider_audit_events",
                ];
                let mut total: i64 = 0;
                for table in &tables {
                    let sql = format!("SELECT COUNT(*) FROM {table}");
                    if let Ok(count) = conn.query_row(&sql, [], |row| row.get::<_, i64>(0)) {
                        total += count;
                    }
                }
                result.records_restored = total;
            }
            Err(e) => {
                result
                    .errors
                    .push(format!("post-restore verification failed: {e}"));
                result.success = false;
            }
        }

        Ok(result)
    }

    pub fn verify_backup(&self, backup_id: &str) -> Result<BackupVerification, String> {
        self.verify_backup_for_target(backup_id, None, false)
    }

    pub fn restore_dry_run(
        &self,
        backup_id: &str,
        target_path: &Path,
    ) -> Result<BackupVerification, String> {
        self.verify_backup_for_target(backup_id, Some(target_path), true)
    }

    fn verify_backup_for_target(
        &self,
        backup_id: &str,
        target_path: Option<&Path>,
        dry_run: bool,
    ) -> Result<BackupVerification, String> {
        let backup = self
            .get_backup(backup_id)?
            .ok_or_else(|| format!("backup not found: {backup_id}"))?;
        let backup_path = Path::new(&backup.backup_path);
        let mut errors = Vec::new();

        let mut checksum_ok = false;
        if backup_path.exists() {
            match compute_checksum(backup_path) {
                Ok(actual) if actual == backup.checksum => checksum_ok = true,
                Ok(actual) => errors.push(format!(
                    "checksum mismatch: expected {}, got {}",
                    backup.checksum, actual
                )),
                Err(e) => errors.push(format!("checksum failed: {e}")),
            }
        } else {
            errors.push(format!("backup file not found: {}", backup.backup_path));
        }

        let mut integrity_ok = false;
        let mut records_checked = 0;
        if backup_path.exists() {
            match Connection::open(backup_path) {
                Ok(conn) => {
                    let integrity: String = conn
                        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                        .unwrap_or_else(|_| "error".to_string());
                    if integrity == "ok" {
                        integrity_ok = true;
                    } else {
                        errors.push(format!("integrity check failed: {integrity}"));
                    }

                    for table in [
                        "dispatch_history",
                        "local_config",
                        "team_members",
                        "api_key_metadata",
                        "audit_log",
                        "provider_audit_events",
                    ] {
                        let sql = format!("SELECT COUNT(*) FROM {table}");
                        if let Ok(count) = conn.query_row(&sql, [], |row| row.get::<_, i64>(0)) {
                            records_checked += count;
                        }
                    }
                }
                Err(e) => errors.push(format!("backup open failed: {e}")),
            }
        }

        Ok(BackupVerification {
            backup_id: backup.backup_id,
            success: errors.is_empty() && checksum_ok && integrity_ok,
            checksum_ok,
            integrity_ok,
            records_checked,
            size_bytes: backup.size_bytes,
            backup_path: backup.backup_path,
            target_path: target_path.map(|p| p.display().to_string()),
            restore_would_overwrite: target_path.map(|p| p.exists()).unwrap_or(false),
            dry_run,
            errors,
        })
    }

    pub fn verify_encryption_key(
        &self,
        backup_id: &str,
        current_key: Option<&str>,
    ) -> Result<bool, String> {
        let backup = self
            .get_backup(backup_id)?
            .ok_or_else(|| format!("backup not found: {backup_id}"))?;

        match &backup.encryption_key_hash {
            None => Ok(true),
            Some(stored_hash) => match current_key {
                None => Ok(false),
                Some(key) => {
                    let mut hasher = Sha256::new();
                    hasher.update(key.as_bytes());
                    let computed = hex::encode(hasher.finalize());
                    Ok(computed == *stored_hash)
                }
            },
        }
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

    pub fn prune_backups(&self, retain_count: usize) -> Result<Vec<String>, String> {
        let mut backups = self.list_backups()?;
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let mut deleted = Vec::new();
        for record in backups.iter().skip(retain_count) {
            self.delete_backup(&record.backup_id)?;
            deleted.push(record.backup_id.clone());
        }
        Ok(deleted)
    }

    pub fn backup_stats(&self) -> serde_json::Value {
        let backups = self.list_backups().unwrap_or_default();
        let count = backups.len();
        let total_size_bytes: u64 = backups.iter().map(|b| b.size_bytes).sum();
        let oldest_created_at = backups
            .iter()
            .min_by(|a, b| a.created_at.cmp(&b.created_at))
            .map(|b| serde_json::Value::String(b.created_at.clone()))
            .unwrap_or(serde_json::Value::Null);
        let newest_created_at = backups
            .iter()
            .max_by(|a, b| a.created_at.cmp(&b.created_at))
            .map(|b| serde_json::Value::String(b.created_at.clone()))
            .unwrap_or(serde_json::Value::Null);
        serde_json::json!({
            "count": count,
            "total_size_bytes": total_size_bytes,
            "oldest_created_at": oldest_created_at,
            "newest_created_at": newest_created_at,
        })
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
