use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

fn make_bundle(dispatch_id: &str) -> Value {
    json!({
        "record": {
            "dispatch_id": dispatch_id,
            "created_at": "2026-05-30T12:00:00Z",
            "final_status": "completed",
        },
        "analysis": {"risk_level": "low"},
        "decision": {
            "selected_tier": "balanced_worker",
            "budget_reservation": {"reserved_cost": 0.01},
        },
        "execution_result": {
            "executor_type": "noop",
            "input_tokens": 10,
            "output_tokens": 5,
        },
        "evaluation_result": {"status": "pass"},
    })
}

// --- audit_log correctness after mutation operations ---

#[test]
fn audit_log_entries_after_dispatch_record() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle = make_bundle("aud-disp-001");
    store
        .record_dispatch("test request", "api", &bundle, "test-actor")
        .unwrap();

    let events = store.audit_events(10).unwrap();
    assert!(!events.is_empty(), "expected at least one audit event");

    let dispatch_event = events
        .iter()
        .find(|e| e["action"] == "dispatch.record")
        .expect("dispatch.record audit event not found");
    assert_eq!(dispatch_event["actor"], "test-actor");
    assert_eq!(dispatch_event["resource"].as_str().unwrap(), "aud-disp-001");
}

#[test]
fn audit_log_entries_after_config_update() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    store
        .set_config_value("test.key", json!("test-value"), "config-actor")
        .unwrap();

    let events = store.audit_events(10).unwrap();
    let config_event = events
        .iter()
        .find(|e| e["action"] == "config.update")
        .expect("config.update audit event not found");
    assert_eq!(config_event["actor"], "config-actor");
    assert_eq!(config_event["resource"], "test.key");
    assert_eq!(config_event["details"]["key"], "test.key");
}

#[test]
fn audit_log_entries_after_api_key_create_and_revoke() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    store
        .record_api_key_metadata(
            "key-001",
            "user-1",
            "member",
            &["dispatch:read".to_string()],
            "admin-actor",
        )
        .unwrap();

    let events = store.audit_events(10).unwrap();
    let create_event = events
        .iter()
        .find(|e| e["action"] == "api_key.record_metadata")
        .expect("api_key.record_metadata audit event not found");
    assert_eq!(create_event["actor"], "admin-actor");
    assert_eq!(create_event["resource"], "key-001");

    store
        .revoke_api_key_metadata("key-001", "admin-actor")
        .unwrap();

    let events = store.audit_events(10).unwrap();
    let revoke_event = events
        .iter()
        .find(|e| e["action"] == "team.key.revoked")
        .expect("team.key.revoked audit event not found");
    assert_eq!(revoke_event["resource"], "key-001");
}

// --- audit log ordering ---

#[test]
fn audit_log_ordering_monotonic() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    for i in 0..5 {
        let bundle = make_bundle(&format!("ord-{i}"));
        store
            .record_dispatch(&format!("req-{i}"), "api", &bundle, "actor")
            .unwrap();
    }

    let events = store.audit_events(100).unwrap();
    assert_eq!(events.len(), 5, "expected 5 audit events");

    let ids: Vec<i64> = events
        .iter()
        .map(|e| e["audit_id"].as_i64().unwrap())
        .collect();

    for window in ids.windows(2) {
        assert!(
            window[0] > window[1],
            "audit_id ordering violated: {:?}",
            ids
        );
    }

    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "duplicate audit_ids found");
}

// --- audit log persistence across reopen ---

#[test]
fn audit_log_persists_across_reopen() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    {
        let store = LocalProductStore::new(&db_path).unwrap();
        for i in 0..3 {
            let bundle = make_bundle(&format!("persist-{i}"));
            store
                .record_dispatch(&format!("req-{i}"), "api", &bundle, "actor")
                .unwrap();
        }
        store.checkpoint_wal().unwrap();
    }

    let store2 = LocalProductStore::new(&db_path).unwrap();
    let events = store2.audit_events(100).unwrap();
    assert_eq!(events.len(), 3, "audit events lost after reopen");

    let dispatch_ids: Vec<&str> = events
        .iter()
        .filter_map(|e| e["resource"].as_str())
        .collect();
    for i in 0..3 {
        assert!(
            dispatch_ids.contains(&format!("persist-{i}").as_str()),
            "missing dispatch {i} in audit log"
        );
    }
}

// --- concurrent audit writes ---

#[test]
fn concurrent_audit_writes_non_corrupted() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());
    let thread_count = 4;
    let writes_per_thread = 25;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|t| {
            let store = store.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                for i in 0..writes_per_thread {
                    let bundle = make_bundle(&format!("conc-t{t}-{i}"));
                    store
                        .record_dispatch(
                            &format!("request-conc-t{t}-{i}"),
                            "api",
                            &bundle,
                            &format!("actor-{t}"),
                        )
                        .unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let total = thread_count * writes_per_thread;
    let events = store.audit_events(200).unwrap();
    let dispatch_events: Vec<_> = events
        .iter()
        .filter(|e| e["action"] == "dispatch.record")
        .collect();
    assert_eq!(
        dispatch_events.len(),
        total,
        "expected {total} dispatch.record audit events, got {}",
        dispatch_events.len()
    );

    for e in &dispatch_events {
        assert!(e["audit_id"].is_i64());
        assert!(e["created_at"].is_string());
        assert!(e["actor"].is_string());
        assert!(e["resource"].is_string());
    }

    let integrity = store.check_integrity().unwrap();
    assert_eq!(integrity.status, "ok");
}

// --- integrity report audit_log row count ---

#[test]
fn check_integrity_reports_audit_log_row_count() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    for i in 0..5 {
        let bundle = make_bundle(&format!("integ-{i}"));
        store
            .record_dispatch(&format!("req-{i}"), "api", &bundle, "actor")
            .unwrap();
    }

    let integrity = store.check_integrity().unwrap();
    assert_eq!(integrity.status, "ok");

    let audit_table = integrity
        .tables
        .iter()
        .find(|t| t.name == "audit_log")
        .expect("audit_log table missing from integrity report");
    assert_eq!(
        audit_table.row_count, 5,
        "expected 5 audit_log rows, got {}",
        audit_table.row_count
    );
}
