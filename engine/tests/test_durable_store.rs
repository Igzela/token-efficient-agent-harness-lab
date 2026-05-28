use engine::storage::durable_store::DurableStore;

#[test]
fn test_new_memory_store() {
    let store = DurableStore::new_memory().unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats["plans"], 0);
    assert_eq!(stats["repos"], 0);
    assert_eq!(stats["events"], 0);
}

#[test]
fn test_save_and_get_plan() {
    let store = DurableStore::new_memory().unwrap();
    let data = serde_json::json!({"name": "test", "schema_version": "plan.v1"});
    let record = store
        .save_plan("p1", &data, None, Some("2025-01-01T00:00:00Z"), false)
        .unwrap();
    assert_eq!(record.record_id, "p1");

    let fetched = store.get_plan("p1").unwrap().unwrap();
    assert_eq!(fetched.record_id, "p1");
    assert_eq!(fetched.data["name"], "test");
}

#[test]
fn test_get_plan_not_found() {
    let store = DurableStore::new_memory().unwrap();
    assert!(store.get_plan("nonexistent").unwrap().is_none());
}

#[test]
fn test_save_plan_duplicate_fails() {
    let store = DurableStore::new_memory().unwrap();
    let data = serde_json::json!({"name": "test"});
    store
        .save_plan("p1", &data, None, Some("2025-01-01"), false)
        .unwrap();
    let result = store.save_plan("p1", &data, None, Some("2025-01-01"), false);
    assert!(result.is_err());
}

#[test]
fn test_save_plan_upsert() {
    let store = DurableStore::new_memory().unwrap();
    let data1 = serde_json::json!({"name": "v1"});
    let data2 = serde_json::json!({"name": "v2"});
    store
        .save_plan("p1", &data1, None, Some("2025-01-01"), false)
        .unwrap();
    store
        .save_plan("p1", &data2, None, Some("2025-01-02"), true)
        .unwrap();
    let fetched = store.get_plan("p1").unwrap().unwrap();
    assert_eq!(fetched.data["name"], "v2");
}

#[test]
fn test_list_plans() {
    let store = DurableStore::new_memory().unwrap();
    store
        .save_plan(
            "p1",
            &serde_json::json!({}),
            None,
            Some("2025-01-01"),
            false,
        )
        .unwrap();
    store
        .save_plan(
            "p2",
            &serde_json::json!({}),
            None,
            Some("2025-01-02"),
            false,
        )
        .unwrap();
    let plans = store.list_plans().unwrap();
    assert_eq!(plans.len(), 2);
}

#[test]
fn test_delete_plan() {
    let store = DurableStore::new_memory().unwrap();
    store
        .save_plan(
            "p1",
            &serde_json::json!({}),
            None,
            Some("2025-01-01"),
            false,
        )
        .unwrap();
    assert!(store.delete_plan("p1").unwrap());
    assert!(store.get_plan("p1").unwrap().is_none());
    assert!(!store.delete_plan("nonexistent").unwrap());
}

#[test]
fn test_save_and_get_repo() {
    let store = DurableStore::new_memory().unwrap();
    let data = serde_json::json!({"url": "https://github.com/test"});
    store
        .save_repo("r1", &data, None, Some("2025-01-01"), false)
        .unwrap();
    let fetched = store.get_repo("r1").unwrap().unwrap();
    assert_eq!(fetched.data["url"], "https://github.com/test");
}

#[test]
fn test_save_and_get_events() {
    let store = DurableStore::new_memory().unwrap();
    let data = serde_json::json!({"event_type": "dispatch", "payload": {}});
    store
        .save_event("e1", &data, None, Some("2025-01-01"), false)
        .unwrap();
    let events = store.get_events(None, 100).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data["event_type"], "dispatch");
}

#[test]
fn test_get_events_by_type() {
    let store = DurableStore::new_memory().unwrap();
    store
        .save_event(
            "e1",
            &serde_json::json!({"event_type": "dispatch"}),
            None,
            Some("2025-01-01"),
            false,
        )
        .unwrap();
    store
        .save_event(
            "e2",
            &serde_json::json!({"event_type": "error"}),
            None,
            Some("2025-01-02"),
            false,
        )
        .unwrap();
    let dispatches = store.get_events(Some("dispatch"), 100).unwrap();
    assert_eq!(dispatches.len(), 1);
    let errors = store.get_events(Some("error"), 100).unwrap();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_migration_log() {
    let store = DurableStore::new_memory().unwrap();
    let id = store.log_migration_start("json", "sqlite").unwrap();
    assert!(id > 0);
    store.log_migration_finish(id, 42, "completed").unwrap();
    let log = store.get_migration_log().unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0]["records_migrated"], 42);
    assert_eq!(log[0]["status"], "completed");
}

#[test]
fn test_stats() {
    let store = DurableStore::new_memory().unwrap();
    store
        .save_plan(
            "p1",
            &serde_json::json!({}),
            None,
            Some("2025-01-01"),
            false,
        )
        .unwrap();
    store
        .save_repo(
            "r1",
            &serde_json::json!({}),
            None,
            Some("2025-01-01"),
            false,
        )
        .unwrap();
    store
        .save_event(
            "e1",
            &serde_json::json!({}),
            None,
            Some("2025-01-01"),
            false,
        )
        .unwrap();
    let stats = store.stats().unwrap();
    assert_eq!(stats["plans"], 1);
    assert_eq!(stats["repos"], 1);
    assert_eq!(stats["events"], 1);
}

#[test]
fn test_close_and_reopen() {
    let path = ":memory:"; // in-memory can't really reopen, but test close doesn't crash
    let store = DurableStore::new(path).unwrap();
    store
        .save_plan(
            "p1",
            &serde_json::json!({}),
            None,
            Some("2025-01-01"),
            false,
        )
        .unwrap();
    store.close().unwrap();
    // After close, operations should fail
    let result = store.get_plan("p1");
    assert!(result.is_err());
}

#[test]
fn test_schema_version_extraction() {
    let store = DurableStore::new_memory().unwrap();
    let data = serde_json::json!({"schema_version": "plan.v2", "name": "test"});
    let record = store
        .save_plan("p1", &data, None, Some("2025-01-01"), false)
        .unwrap();
    assert_eq!(record.schema_version.as_deref(), Some("plan.v2"));
}
