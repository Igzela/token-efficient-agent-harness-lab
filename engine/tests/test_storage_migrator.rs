use engine::storage::durable_store::DurableStore;
use engine::storage::storage_migrator::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_migrate_plans_json_to_sqlite() {
    let tmp = TempDir::new().unwrap();
    let json_path = tmp.path().join("plans.json");
    fs::write(
        &json_path,
        r#"{"plans": [{"plan_id": "p1", "name": "test", "schema_version": "plan.v1"}]}"#,
    )
    .unwrap();

    let store = DurableStore::new_memory().unwrap();
    let report = migrate_plans_json_to_sqlite(&json_path, &store, 0.0, 0.1);
    assert_eq!(report.records_migrated, 1);
    assert!(report.errors.is_empty());

    let plan = store.get_plan("p1").unwrap().unwrap();
    assert_eq!(plan.data["name"], "test");
}

#[test]
fn test_migrate_plans_missing_file() {
    let store = DurableStore::new_memory().unwrap();
    let report = migrate_plans_json_to_sqlite(
        std::path::Path::new("/nonexistent/plans.json"),
        &store,
        0.0,
        0.1,
    );
    assert_eq!(report.records_migrated, 0);
    assert!(!report.errors.is_empty());
}

#[test]
fn test_migrate_plans_missing_id() {
    let tmp = TempDir::new().unwrap();
    let json_path = tmp.path().join("plans.json");
    fs::write(&json_path, r#"{"plans": [{"name": "no-id"}]}"#).unwrap();

    let store = DurableStore::new_memory().unwrap();
    let report = migrate_plans_json_to_sqlite(&json_path, &store, 0.0, 0.1);
    assert_eq!(report.records_migrated, 0);
    assert!(report.errors.iter().any(|e| e.contains("missing plan_id")));
}

#[test]
fn test_migrate_repos_json_to_sqlite() {
    let tmp = TempDir::new().unwrap();
    let json_path = tmp.path().join("repos.json");
    fs::write(
        &json_path,
        r#"{"repos": [{"id": "r1", "url": "https://github.com/test", "schema_version": "repo.v1"}]}"#,
    )
    .unwrap();

    let store = DurableStore::new_memory().unwrap();
    let report = migrate_repos_json_to_sqlite(&json_path, &store, 0.0, 0.1);
    assert_eq!(report.records_migrated, 1);
    assert!(report.errors.is_empty());

    let repo = store.get_repo("r1").unwrap().unwrap();
    assert_eq!(repo.data["url"], "https://github.com/test");
}

#[test]
fn test_migrate_events_jsonl_to_sqlite() {
    let tmp = TempDir::new().unwrap();
    let jsonl_path = tmp.path().join("events.jsonl");
    fs::write(
        &jsonl_path,
        r#"{"event_id": "e1", "event_type": "dispatch", "schema_version": "event.v1"}
{"event_id": "e2", "event_type": "error", "schema_version": "event.v1"}"#,
    )
    .unwrap();

    let store = DurableStore::new_memory().unwrap();
    let report = migrate_events_jsonl_to_sqlite(&jsonl_path, &store, 0.0, 0.1);
    assert_eq!(report.records_migrated, 2);
    assert!(report.errors.is_empty());

    let events = store.get_events(None, 100).unwrap();
    assert_eq!(events.len(), 2);
}

#[test]
fn test_migrate_events_missing_file() {
    let store = DurableStore::new_memory().unwrap();
    let report = migrate_events_jsonl_to_sqlite(
        std::path::Path::new("/nonexistent/events.jsonl"),
        &store,
        0.0,
        0.1,
    );
    assert_eq!(report.records_migrated, 0);
}

#[test]
fn test_migrate_events_bad_json() {
    let tmp = TempDir::new().unwrap();
    let jsonl_path = tmp.path().join("events.jsonl");
    fs::write(
        &jsonl_path,
        r#"{"event_id": "e1", "event_type": "good"}
not valid json
{"event_id": "e2", "event_type": "also-good"}"#,
    )
    .unwrap();

    let store = DurableStore::new_memory().unwrap();
    let report = migrate_events_jsonl_to_sqlite(&jsonl_path, &store, 0.0, 0.1);
    assert_eq!(report.records_migrated, 2);
    assert_eq!(report.errors.len(), 1);
    assert!(!report.errors.is_empty());
}

#[test]
fn test_full_migration() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("plans.json"),
        r#"{"plans": [{"plan_id": "p1", "schema_version": "plan.v1"}]}"#,
    )
    .unwrap();
    fs::write(
        tmp.path().join("repos.json"),
        r#"{"repos": [{"id": "r1", "schema_version": "repo.v1"}]}"#,
    )
    .unwrap();
    fs::write(
        tmp.path().join("events.jsonl"),
        r#"{"event_id": "e1", "schema_version": "event.v1"}"#,
    )
    .unwrap();

    let store = DurableStore::new_memory().unwrap();
    let report = full_migration(
        &tmp.path().join("plans.json"),
        &tmp.path().join("repos.json"),
        &tmp.path().join("events.jsonl"),
        &store,
        0.0,
        0.1,
    );
    assert_eq!(report.plans.records_migrated, 1);
    assert_eq!(report.repos.records_migrated, 1);
    assert_eq!(report.events.records_migrated, 1);

    let stats = store.stats().unwrap();
    assert_eq!(stats["plans"], 1);
    assert_eq!(stats["repos"], 1);
    assert_eq!(stats["events"], 1);
}

#[test]
fn test_migration_report_serde() {
    let report = MigrationReport {
        source: "test.json".to_string(),
        target: "sqlite".to_string(),
        records_migrated: 5,
        errors: vec![],
        duration_ms: 100.0,
    };
    let json = serde_json::to_string(&report).unwrap();
    let r: MigrationReport = serde_json::from_str(&json).unwrap();
    assert_eq!(r.records_migrated, 5);
}
