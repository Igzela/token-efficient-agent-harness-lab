use engine::provider::audit::{
    ProviderAuditEvent, ProviderAuditRecorder, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION,
};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use std::sync::Arc;
use tempfile::tempdir;

fn make_event(event_id: &str, dispatch_id: &str, event_type: &str) -> ProviderAuditEvent {
    ProviderAuditEvent {
        schema_version: PROVIDER_AUDIT_EVENT_SCHEMA_VERSION.to_string(),
        event_id: event_id.to_string(),
        dispatch_id: dispatch_id.to_string(),
        provider_id: "test-provider".to_string(),
        event_type: event_type.to_string(),
        input_token_count: Some(100),
        output_token_count: Some(50),
        cost: Some(0.0025),
        currency: Some("USD".to_string()),
        latency_ms: Some(42),
        error_domain: None,
        redaction_status: "not_applicable".to_string(),
        created_at: "2026-05-29T12:00:00Z".to_string(),
    }
}

fn make_bundle_with_usage(
    dispatch_id: &str,
    executor_type: &str,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    estimated_cost: Option<f64>,
    latency_ms: Option<i64>,
) -> Value {
    json!({
        "record": {
            "dispatch_id": dispatch_id,
            "created_at": "2026-05-29T12:00:00Z",
            "final_status": "completed",
        },
        "analysis": {"risk_level": "low"},
        "decision": {
            "selected_tier": "balanced_worker",
            "budget_reservation": {"reserved_cost": 0.01},
        },
        "execution_result": {
            "executor_type": executor_type,
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "estimated_cost": estimated_cost,
            "latency_ms": latency_ms,
        },
        "evaluation_result": {"status": "pass"},
    })
}

// --- provider_audit_events tests ---

#[test]
fn record_provider_audit_event_persists() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let event = make_event("evt-001", "disp-001", "response_received");

    store.record_provider_audit_event(&event).unwrap();

    let events = store.provider_audit_events(100).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_id"], "evt-001");
    assert_eq!(events[0]["dispatch_id"], "disp-001");
    assert_eq!(events[0]["provider_id"], "test-provider");
    assert_eq!(events[0]["event_type"], "response_received");
    assert_eq!(events[0]["input_token_count"], 100);
    assert_eq!(events[0]["output_token_count"], 50);
    assert_eq!(events[0]["cost"], 0.0025);
    assert_eq!(events[0]["currency"], "USD");
    assert_eq!(events[0]["latency_ms"], 42);
    assert_eq!(events[0]["redaction_status"], "not_applicable");
}

#[test]
fn record_provider_audit_event_idempotent() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let event = make_event("evt-001", "disp-001", "response_received");

    store.record_provider_audit_event(&event).unwrap();
    store.record_provider_audit_event(&event).unwrap();

    let events = store.provider_audit_events(100).unwrap();
    assert_eq!(events.len(), 1);
}

#[test]
fn provider_audit_events_respects_limit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    for i in 0..5 {
        let event = make_event(&format!("evt-{i:03}"), "disp-001", "response_received");
        store.record_provider_audit_event(&event).unwrap();
    }

    let events = store.provider_audit_events(3).unwrap();
    assert_eq!(events.len(), 3);
}

#[test]
fn provider_audit_events_for_dispatch_filters() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    store
        .record_provider_audit_event(&make_event("evt-001", "disp-001", "response_received"))
        .unwrap();
    store
        .record_provider_audit_event(&make_event("evt-002", "disp-002", "response_received"))
        .unwrap();
    store
        .record_provider_audit_event(&make_event("evt-003", "disp-001", "error"))
        .unwrap();

    let d1_events = store
        .provider_audit_events_for_dispatch("disp-001")
        .unwrap();
    assert_eq!(d1_events.len(), 2);
    assert!(d1_events.iter().all(|e| e["dispatch_id"] == "disp-001"));

    let d2_events = store
        .provider_audit_events_for_dispatch("disp-002")
        .unwrap();
    assert_eq!(d2_events.len(), 1);

    let d3_events = store
        .provider_audit_events_for_dispatch("disp-999")
        .unwrap();
    assert_eq!(d3_events.len(), 0);
}

#[test]
fn provider_audit_event_with_null_optional_fields() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let event = ProviderAuditEvent {
        schema_version: PROVIDER_AUDIT_EVENT_SCHEMA_VERSION.to_string(),
        event_id: "evt-min".to_string(),
        dispatch_id: "disp-001".to_string(),
        provider_id: "p1".to_string(),
        event_type: "request_sent".to_string(),
        input_token_count: None,
        output_token_count: None,
        cost: None,
        currency: None,
        latency_ms: None,
        error_domain: None,
        redaction_status: "not_applicable".to_string(),
        created_at: "2026-05-29T00:00:00Z".to_string(),
    };

    store.record_provider_audit_event(&event).unwrap();

    let events = store.provider_audit_events(10).unwrap();
    assert_eq!(events.len(), 1);
    assert!(events[0]["input_token_count"].is_null());
    assert!(events[0]["output_token_count"].is_null());
    assert!(events[0]["cost"].is_null());
    assert!(events[0]["currency"].is_null());
    assert!(events[0]["latency_ms"].is_null());
    assert!(events[0]["error_domain"].is_null());
}

#[test]
fn provider_audit_event_with_error_domain() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let event = ProviderAuditEvent {
        schema_version: PROVIDER_AUDIT_EVENT_SCHEMA_VERSION.to_string(),
        event_id: "evt-err".to_string(),
        dispatch_id: "disp-001".to_string(),
        provider_id: "p1".to_string(),
        event_type: "error".to_string(),
        input_token_count: None,
        output_token_count: None,
        cost: None,
        currency: None,
        latency_ms: None,
        error_domain: Some("provider_rate_limit".to_string()),
        redaction_status: "not_applicable".to_string(),
        created_at: "2026-05-29T00:00:00Z".to_string(),
    };

    store.record_provider_audit_event(&event).unwrap();

    let events = store.provider_audit_events(10).unwrap();
    assert_eq!(events[0]["error_domain"], "provider_rate_limit");
}

// --- ProviderAuditRecorder with store persistence ---

#[test]
fn recorder_with_store_persists_events() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());
    let recorder = ProviderAuditRecorder::with_store(store.clone());

    recorder.create_and_record("disp-001", "p1", "request_sent", None);
    let extra = json!({"input_token_count": 200, "output_token_count": 100, "cost": 0.005});
    recorder.create_and_record("disp-001", "p1", "response_received", Some(&extra));

    assert_eq!(recorder.count(), 2);

    let persisted = store.provider_audit_events(100).unwrap();
    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0]["event_type"], "response_received");
    assert_eq!(persisted[0]["input_token_count"], 200);
    assert_eq!(persisted[1]["event_type"], "request_sent");
}

#[test]
fn recorder_without_store_does_not_persist() {
    let recorder = ProviderAuditRecorder::new();
    recorder.create_and_record("disp-001", "p1", "request_sent", None);

    assert_eq!(recorder.count(), 1);
}

#[test]
fn recorder_persists_error_events() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());
    let recorder = ProviderAuditRecorder::with_store(store.clone());

    let extra = json!({"error_domain": "provider_timeout"});
    recorder.create_and_record("disp-001", "p1", "error", Some(&extra));

    let persisted = store.provider_audit_events(10).unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0]["error_domain"], "provider_timeout");
    assert_eq!(persisted[0]["event_type"], "error");
}

// --- dispatch_history new columns ---

#[test]
fn dispatch_history_records_usage_data() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let bundle = make_bundle_with_usage(
        "disp-001",
        "provider",
        Some(150),
        Some(75),
        Some(0.003),
        Some(250),
    );

    let result = store
        .record_dispatch("hello", "api", &bundle, "test-actor")
        .unwrap();

    assert_eq!(result["input_tokens"], 150);
    assert_eq!(result["output_tokens"], 75);
    assert_eq!(result["estimated_cost_usd"], 0.003);
    assert_eq!(result["executor_type"], "provider");
    assert_eq!(result["latency_ms"], 250);
}

#[test]
fn dispatch_history_lists_usage_data() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let bundle = make_bundle_with_usage(
        "disp-001",
        "provider",
        Some(150),
        Some(75),
        Some(0.003),
        Some(250),
    );

    store
        .record_dispatch("hello", "api", &bundle, "test-actor")
        .unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["input_tokens"], 150);
    assert_eq!(dispatches[0]["output_tokens"], 75);
    assert_eq!(dispatches[0]["estimated_cost_usd"], 0.003);
    assert_eq!(dispatches[0]["executor_type"], "provider");
    assert_eq!(dispatches[0]["latency_ms"], 250);
}

#[test]
fn dispatch_history_defaults_executor_type_to_noop() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let bundle = make_bundle_with_usage("disp-001", "noop", None, None, None, None);

    store
        .record_dispatch("hello", "api", &bundle, "test-actor")
        .unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["executor_type"], "noop");
    assert!(dispatches[0]["input_tokens"].is_null());
    assert!(dispatches[0]["output_tokens"].is_null());
    assert!(dispatches[0]["estimated_cost_usd"].is_null());
    assert!(dispatches[0]["latency_ms"].is_null());
}

#[test]
fn dispatch_history_usage_fields_from_bundle_execution_result() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle = json!({
        "record": {
            "dispatch_id": "disp-002",
            "created_at": "2026-05-29T12:00:00Z",
            "final_status": "completed",
        },
        "analysis": {"risk_level": "low"},
        "decision": {
            "selected_tier": "strong_planner",
            "budget_reservation": {"reserved_cost": 0.05},
        },
        "execution_result": {
            "executor_type": "provider",
            "input_tokens": 500,
            "output_tokens": 200,
            "estimated_cost": 0.015,
            "latency_ms": 1200,
        },
        "evaluation_result": {"status": "pass"},
    });

    store
        .record_dispatch("code review", "api", &bundle, "actor")
        .unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    let d = &dispatches[0];
    assert_eq!(d["input_tokens"], 500);
    assert_eq!(d["output_tokens"], 200);
    assert_eq!(d["estimated_cost_usd"], 0.015);
    assert_eq!(d["executor_type"], "provider");
    assert_eq!(d["latency_ms"], 1200);
}

#[test]
fn dispatch_history_missing_execution_result_uses_defaults() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle = json!({
        "record": {
            "dispatch_id": "disp-003",
            "created_at": "2026-05-29T12:00:00Z",
            "final_status": "not_executed",
        },
        "analysis": {"risk_level": "low"},
        "decision": {
            "selected_tier": "balanced_worker",
            "budget_reservation": {"reserved_cost": 0.0},
        },
        "execution_result": {},
        "evaluation_result": {"status": "pass"},
    });

    store
        .record_dispatch("test", "api", &bundle, "actor")
        .unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    let d = &dispatches[0];
    assert_eq!(d["executor_type"], "noop");
    assert!(d["input_tokens"].is_null());
    assert!(d["output_tokens"].is_null());
    assert!(d["estimated_cost_usd"].is_null());
    assert!(d["latency_ms"].is_null());
}
