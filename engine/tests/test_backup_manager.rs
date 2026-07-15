use engine::storage::backup_manager::BackupManager;
use engine::storage::local_product_store::{DurableMemoryCreate, LocalProductStore, MemoryScope};
use serde_json::json;
use std::path::Path;
use tempfile::TempDir;

fn setup() -> (TempDir, BackupManager) {
    let tmp = TempDir::new().unwrap();
    let backup_dir = tmp.path().join("backups");
    let mgr = BackupManager::new(&backup_dir).unwrap();
    (tmp, mgr)
}

fn create_test_db(path: &Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE plans (id TEXT PRIMARY KEY, data TEXT);
         INSERT INTO plans (id, data) VALUES ('p1', '{\"test\": true}');",
    )
    .unwrap();
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

#[test]
fn online_backup_restores_durable_memory_while_source_connection_is_open() {
    let (tmp, mgr) = setup();
    let db_path = tmp.path().join("control-plane.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    let memory = store
        .create_durable_memory(
            &DurableMemoryCreate {
                scope: MemoryScope {
                    tenant_id: "local".to_string(),
                    workspace_id: "backup-workspace".to_string(),
                    agent_id: None,
                    task_id: None,
                },
                run_id: None,
                source_id: "backup-source".to_string(),
                source_sha256: "77".repeat(32),
                conflict_key: "backup-fact".to_string(),
                content: json!({"fact":"survives online backup"}),
                confidence: 1.0,
                fresh_until: None,
                expires_at: None,
                supersedes_memory_id: None,
            },
            "backup-test",
        )
        .unwrap();
    let memory_id = memory["memory_id"].as_str().unwrap().to_string();

    let record = mgr
        .create_backup(
            &db_path,
            "online durable memory",
            "durable-memory",
            "2026-07-14T00:00:00Z",
        )
        .unwrap();
    mgr.save_metadata(&[record]).unwrap();
    let verification = mgr.verify_backup("durable-memory").unwrap();
    assert!(verification.success, "{:?}", verification.errors);
    assert!(verification.records_checked > 0);

    let restored_path = tmp.path().join("restored-control-plane.db");
    let restored = mgr
        .restore_backup_with_verify("durable-memory", &restored_path, 1.0)
        .unwrap();
    assert!(restored.success, "{:?}", restored.errors);
    drop(store);
    let reopened = LocalProductStore::new(&restored_path).unwrap();
    let history = reopened.inspect_durable_memory(&memory_id).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["content"]["fact"], "survives online backup");
}
