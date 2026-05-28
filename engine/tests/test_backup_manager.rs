use engine::storage::backup_manager::BackupManager;
use std::path::Path;
use tempfile::TempDir;

fn setup() -> (TempDir, BackupManager) {
    let tmp = TempDir::new().unwrap();
    let backup_dir = tmp.path().join("backups");
    let mgr = BackupManager::new(&backup_dir).unwrap();
    (tmp, mgr)
}

fn create_test_db(path: &Path) {
    let store = engine::storage::durable_store::DurableStore::new(path.to_str().unwrap()).unwrap();
    store
        .save_plan(
            "p1",
            &serde_json::json!({"test": true}),
            None,
            Some("2025-01-01"),
            false,
        )
        .unwrap();
    store.close().unwrap();
}

#[test]
fn test_new_backup_manager() {
    let (_tmp, mgr) = setup();
    assert!(mgr.backup_dir().exists());
}

#[test]
fn test_create_backup() {
    let (tmp, mgr) = setup();
    let db_path = tmp.path().join("test.db");
    create_test_db(&db_path);

    let record = mgr
        .create_backup(&db_path, "test backup", "b1", "2025-01-01T00:00:00Z")
        .unwrap();
    assert_eq!(record.backup_id, "b1");
    assert_eq!(record.label, "test backup");
    assert!(record.size_bytes > 0);
    assert!(!record.checksum.is_empty());
}

#[test]
fn test_create_backup_source_not_found() {
    let (_tmp, mgr) = setup();
    let result = mgr.create_backup(Path::new("/nonexistent/db.db"), "test", "b1", "2025-01-01");
    assert!(result.is_err());
}

#[test]
fn test_list_and_get_backup() {
    let (tmp, mgr) = setup();
    let db_path = tmp.path().join("test.db");
    create_test_db(&db_path);

    let record = mgr
        .create_backup(&db_path, "backup 1", "b1", "2025-01-01")
        .unwrap();
    mgr.save_metadata(&[record]).unwrap();

    let backups = mgr.list_backups().unwrap();
    assert_eq!(backups.len(), 1);

    let found = mgr.get_backup("b1").unwrap().unwrap();
    assert_eq!(found.label, "backup 1");

    assert!(mgr.get_backup("nonexistent").unwrap().is_none());
}

#[test]
fn test_delete_backup() {
    let (tmp, mgr) = setup();
    let db_path = tmp.path().join("test.db");
    create_test_db(&db_path);

    let record = mgr
        .create_backup(&db_path, "backup 1", "b1", "2025-01-01")
        .unwrap();
    mgr.save_metadata(&[record]).unwrap();

    assert!(mgr.delete_backup("b1").unwrap());
    assert!(mgr.get_backup("b1").unwrap().is_none());
    assert!(!mgr.delete_backup("nonexistent").unwrap());
}

#[test]
fn test_restore_backup() {
    let (tmp, mgr) = setup();
    let db_path = tmp.path().join("test.db");
    create_test_db(&db_path);

    let record = mgr
        .create_backup(&db_path, "backup 1", "b1", "2025-01-01")
        .unwrap();
    mgr.save_metadata(&[record]).unwrap();

    let restore_path = tmp.path().join("restored.db");
    let result = mgr.restore_backup("b1", &restore_path, 1000.0).unwrap();
    assert!(result.success);
    assert!(result.errors.is_empty());
    assert!(restore_path.exists());
}

#[test]
fn test_restore_nonexistent_backup() {
    let (_tmp, mgr) = setup();
    let result = mgr.restore_backup("nonexistent", Path::new("/tmp/test.db"), 1000.0);
    assert!(result.is_err());
}

#[test]
fn test_checksum_integrity() {
    let (tmp, mgr) = setup();
    let db_path = tmp.path().join("test.db");
    create_test_db(&db_path);

    let record = mgr
        .create_backup(&db_path, "backup", "b1", "2025-01-01")
        .unwrap();
    // Checksum should be deterministic
    let record2 = mgr
        .create_backup(&db_path, "backup", "b2", "2025-01-01")
        .unwrap();
    assert_eq!(record.checksum, record2.checksum);
}
