use engine::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
use engine::provider::audit::{
    ProviderAuditEvent, ProviderAuditRecorder, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION,
};
use engine::read_only_planner::ReadOnlyPlanner;
use engine::storage::local_product_store::{DurableMemoryCreate, LocalProductStore, MemoryScope};
use engine::tool_policy_executor::ToolPolicyNodeExecutor;
use serde_json::{json, Value};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use tempfile::tempdir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct ContextEchoExecutor;

impl NodeExecutor for ContextEchoExecutor {
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let output = input
            .node_metadata
            .get("context_injection")
            .cloned()
            .unwrap_or(Value::Null)
            .to_string();
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "context_echo".to_string(),
            output: Some(output),
            error_domain: None,
            error_message: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            estimated_cost: Some(0.0),
            latency_ms: Some(0),
        }
    }

    fn executor_type_name(&self) -> &str {
        "context_echo"
    }
}

struct AgentContextEchoExecutor;

impl NodeExecutor for AgentContextEchoExecutor {
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        ContextEchoExecutor.execute_node(input)
    }

    fn executor_type_name(&self) -> &str {
        "agent_step"
    }
}

struct ContextPreservingExecutor;

impl NodeExecutor for ContextPreservingExecutor {
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let has_context = input.node_metadata.get("context_injection").is_some();
        let original_input = input.node_metadata.get("input").cloned();
        let output = json!({
            "has_context_injection": has_context,
            "original_input": original_input,
        })
        .to_string();
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "preserving".to_string(),
            output: Some(output),
            error_domain: None,
            error_message: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            estimated_cost: Some(0.0),
            latency_ms: Some(0),
        }
    }

    fn executor_type_name(&self) -> &str {
        "preserving"
    }
}

struct LargeOutputExecutor;

impl NodeExecutor for LargeOutputExecutor {
    fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "large_output".to_string(),
            output: Some("x".repeat(200)),
            error_domain: None,
            error_message: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            estimated_cost: Some(0.0),
            latency_ms: Some(0),
        }
    }

    fn executor_type_name(&self) -> &str {
        "large_output"
    }
}

struct PromptEchoExecutor;

impl NodeExecutor for PromptEchoExecutor {
    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "prompt_echo".to_string(),
            output: input
                .node_metadata
                .get("prompt")
                .and_then(Value::as_str)
                .map(str::to_string),
            error_domain: None,
            error_message: None,
            input_tokens: Some(0),
            output_tokens: Some(0),
            estimated_cost: Some(0.0),
            latency_ms: Some(0),
        }
    }

    fn executor_type_name(&self) -> &str {
        "prompt_echo"
    }
}

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
fn provider_audit_events_have_stable_same_timestamp_ordering() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    for event_id in ["evt-a", "evt-c", "evt-b"] {
        store
            .record_provider_audit_event(&make_event(event_id, "disp-stable", "response_received"))
            .unwrap();
    }
    let first = store
        .provider_audit_events_for_dispatch("disp-stable")
        .unwrap();
    let second = store
        .provider_audit_events_for_dispatch("disp-stable")
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(
        first
            .iter()
            .map(|event| event["event_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["evt-c", "evt-b", "evt-a"]
    );
}

#[test]
fn daily_provider_audit_cost_counts_only_completed_responses_for_date() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let mut first = make_event("evt-cost-1", "disp-001", "response_received");
    first.cost = Some(0.125);
    first.created_at = "2026-07-10T01:00:00Z".to_string();
    store.record_provider_audit_event(&first).unwrap();

    let mut second = make_event("evt-cost-2", "disp-002", "response_received");
    second.cost = Some(0.375);
    second.created_at = "2026-07-10T23:59:59Z".to_string();
    store.record_provider_audit_event(&second).unwrap();

    let mut request = make_event("evt-reserved", "disp-003", "request_sent");
    request.cost = Some(99.0);
    request.created_at = "2026-07-10T12:00:00Z".to_string();
    store.record_provider_audit_event(&request).unwrap();

    let mut other_day = make_event("evt-other-day", "disp-004", "response_received");
    other_day.cost = Some(10.0);
    other_day.created_at = "2026-07-09T23:59:59Z".to_string();
    store.record_provider_audit_event(&other_day).unwrap();

    assert_eq!(
        store.daily_provider_audit_cost_usd("2026-07-10").unwrap(),
        0.5
    );
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
fn independent_recorders_do_not_collide_in_persistent_store() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());

    ProviderAuditRecorder::with_store(store.clone()).create_and_record(
        "disp-first",
        "p1",
        "request_sent",
        None,
    );
    ProviderAuditRecorder::with_store(store.clone()).create_and_record(
        "disp-second",
        "p1",
        "request_sent",
        None,
    );

    let persisted = store.provider_audit_events(10).unwrap();
    assert_eq!(persisted.len(), 2);
    assert_ne!(persisted[0]["event_id"], persisted[1]["event_id"]);
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
fn dispatch_history_owner_provenance_survives_restart_and_deduplicates_imports() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("owner.db");
    let store = LocalProductStore::new(&path).unwrap();
    let bundle = make_bundle_with_usage(
        "owner-dispatch",
        "provider",
        Some(10),
        Some(5),
        Some(0.01),
        Some(20),
    );
    store
        .record_dispatch("{}", "test", &bundle, "actor")
        .unwrap();

    let ids = vec!["owner-dispatch".to_string(), "owner-dispatch".to_string()];
    let first = store
        .trusted_replay_eligibility_request(
            &ids,
            "2026-05-29T12:01:00Z",
            300,
            engine::feedback::ReplayEvidenceScope::default(),
        )
        .unwrap();
    drop(store);
    let restarted = LocalProductStore::new(&path).unwrap();
    let second = restarted
        .trusted_replay_eligibility_request(
            &["owner-dispatch".to_string()],
            "2026-05-29T12:01:00Z",
            300,
            engine::feedback::ReplayEvidenceScope::default(),
        )
        .unwrap();
    assert_eq!(first.traces.len(), 1);
    assert_eq!(first.traces[0].trace, second.traces[0].trace);
}

#[test]
fn dispatch_history_owner_tampering_and_missing_binding_are_refused() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("owner-tamper.db");
    {
        let store = LocalProductStore::new(&path).unwrap();
        store
            .record_dispatch(
                "{}",
                "test",
                &make_bundle_with_usage(
                    "tamper-dispatch",
                    "provider",
                    Some(10),
                    Some(5),
                    Some(0.01),
                    Some(20),
                ),
                "actor",
            )
            .unwrap();
    }
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE dispatch_history SET bundle_json = ?1 WHERE dispatch_id = ?2",
            rusqlite::params!["{\"tampered\":true}", "tamper-dispatch"],
        )
        .unwrap();
    drop(connection);
    let store = LocalProductStore::new(&path).unwrap();
    let error = store
        .trusted_replay_eligibility_request(
            &["tamper-dispatch".to_string()],
            "2026-05-29T12:01:00Z",
            300,
            engine::feedback::ReplayEvidenceScope::default(),
        )
        .unwrap_err();
    assert!(error.contains("untrusted_trace_source"));
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE dispatch_history SET trace_content_sha256 = NULL WHERE dispatch_id = ?1",
            rusqlite::params!["tamper-dispatch"],
        )
        .unwrap();
    drop(connection);
    let store = LocalProductStore::new(&path).unwrap();
    let error = store
        .trusted_replay_eligibility_request(
            &["tamper-dispatch".to_string()],
            "2026-05-29T12:01:00Z",
            300,
            engine::feedback::ReplayEvidenceScope::default(),
        )
        .unwrap_err();
    assert!(error.contains("untrusted_trace_source"));
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
fn dispatch_history_search_filters_and_paginates() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    store
        .record_dispatch(
            "Alpha parser work",
            "api",
            &make_bundle_with_usage("disp-alpha", "noop", None, None, None, None),
            "actor",
        )
        .unwrap();
    store
        .record_dispatch(
            "Beta docs review",
            "dashboard",
            &make_bundle_with_usage("disp-beta", "noop", None, None, None, None),
            "actor",
        )
        .unwrap();

    let raw_matches = store.search_dispatches(10, 0, Some("alpha")).unwrap();
    assert_eq!(raw_matches.len(), 1);
    assert_eq!(raw_matches[0]["dispatch_id"], "disp-alpha");

    let source_matches = store.search_dispatches(10, 0, Some("DASHBOARD")).unwrap();
    assert_eq!(source_matches.len(), 1);
    assert_eq!(source_matches[0]["dispatch_id"], "disp-beta");

    let wildcard_matches = store.search_dispatches(10, 0, Some("%")).unwrap();
    assert!(wildcard_matches.is_empty());

    let paged_matches = store.search_dispatches(1, 1, Some("disp")).unwrap();
    assert_eq!(paged_matches.len(), 1);
    assert_eq!(paged_matches[0]["dispatch_id"], "disp-alpha");
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

fn make_workflow_plan(ids: &engine::storage::local_product_store::WorkflowPlanIds) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-0001", "task_domain": "docs"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "nodes": [],
            "edges": [],
        },
        "boundaries": {
            "execution": "disabled",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled",
        },
    })
}

fn make_workflow_plan_with_nodes(
    ids: &engine::storage::local_product_store::WorkflowPlanIds,
) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-0001", "task_domain": "docs"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-06-05T00:00:00Z",
            "updated_at": "2026-06-05T00:00:00Z",
            "nodes": [
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-a",
                    "workflow_id": ids.workflow_id,
                    "task_type": "analysis",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": [],
                    "output_ref": null,
                    "budget": 0.1,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                },
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-b",
                    "workflow_id": ids.workflow_id,
                    "task_type": "docs",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": ["node-a"],
                    "output_ref": null,
                    "budget": 0.2,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                }
            ],
            "edges": [
                {
                    "schema_version": "workflow_edge.v1",
                    "edge_id": "edge-a-b",
                    "from_node_id": "node-a",
                    "to_node_id": "node-b",
                    "edge_type": "dependency"
                }
            ],
            "started_at": null,
            "completed_at": null,
            "result": null
        },
        "boundaries": {
            "execution": "disabled",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled",
        },
    })
}

fn make_workflow_plan_three_nodes(
    ids: &engine::storage::local_product_store::WorkflowPlanIds,
) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-0001", "task_domain": "docs"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-06-05T00:00:00Z",
            "updated_at": "2026-06-05T00:00:00Z",
            "nodes": [
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-a",
                    "workflow_id": ids.workflow_id,
                    "task_type": "analysis",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": [],
                    "output_ref": null,
                    "budget": 0.1,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                },
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-b",
                    "workflow_id": ids.workflow_id,
                    "task_type": "docs",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": [],
                    "output_ref": null,
                    "budget": 0.1,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                },
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-c",
                    "workflow_id": ids.workflow_id,
                    "task_type": "docs",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": ["node-a", "node-b"],
                    "output_ref": null,
                    "budget": 0.2,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                }
            ],
            "edges": [
                {
                    "schema_version": "workflow_edge.v1",
                    "edge_id": "edge-a-c",
                    "from_node_id": "node-a",
                    "to_node_id": "node-c",
                    "edge_type": "dependency"
                },
                {
                    "schema_version": "workflow_edge.v1",
                    "edge_id": "edge-b-c",
                    "from_node_id": "node-b",
                    "to_node_id": "node-c",
                    "edge_type": "dependency"
                }
            ],
            "started_at": null,
            "completed_at": null,
            "result": null
        },
        "boundaries": {
            "execution": "disabled",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled",
        },
    })
}

fn make_workflow_plan_no_predecessor(
    ids: &engine::storage::local_product_store::WorkflowPlanIds,
) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-0001", "task_domain": "docs"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-06-05T00:00:00Z",
            "updated_at": "2026-06-05T00:00:00Z",
            "nodes": [
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-x",
                    "workflow_id": ids.workflow_id,
                    "task_type": "analysis",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": [],
                    "output_ref": null,
                    "budget": 0.1,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                }
            ],
            "edges": [],
            "started_at": null,
            "completed_at": null,
            "result": null
        },
        "boundaries": {
            "execution": "disabled",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled",
        },
    })
}

fn make_agent_step_memory_plan(
    ids: &engine::storage::local_product_store::WorkflowPlanIds,
) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-0001", "task_domain": "agent"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-07-08T00:00:00Z",
            "updated_at": "2026-07-08T00:00:00Z",
            "nodes": [
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "agent-node-memory",
                    "workflow_id": ids.workflow_id,
                    "task_type": "agent_step",
                    "agent_id": "agent-memory",
                    "assigned_agent_id": "agent-memory",
                    "agent_role": "implementer",
                    "agent_objective": "bounded memory context fixture",
                    "profile_id": "bounded",
                    "capability_profile": ["code"],
                    "decision_source": "fixture",
                    "max_actions": 1,
                    "status": "pending",
                    "input_refs": [],
                    "output_ref": null,
                    "budget": 0.1,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-07-08T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                }
            ],
            "edges": [],
            "started_at": null,
            "completed_at": null,
            "result": null
        },
        "boundaries": {
            "execution_authority": "rust_scheduler_only",
            "provider_calls": "default_off",
            "target_repository_writes": "disabled",
            "runtime_workers": "env_gated_supervised",
        },
    })
}

fn make_workflow_plan_with_field_mapping(
    ids: &engine::storage::local_product_store::WorkflowPlanIds,
) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-0001", "task_domain": "docs"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-06-05T00:00:00Z",
            "updated_at": "2026-06-05T00:00:00Z",
            "nodes": [
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-a",
                    "workflow_id": ids.workflow_id,
                    "task_type": "analysis",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": [],
                    "output_ref": null,
                    "budget": 0.1,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                },
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-b",
                    "workflow_id": ids.workflow_id,
                    "task_type": "docs",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": ["node-a"],
                    "output_ref": null,
                    "budget": 0.2,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                }
            ],
            "edges": [
                {
                    "schema_version": "workflow_edge.v1",
                    "edge_id": "edge-a-b",
                    "from_node_id": "node-a",
                    "to_node_id": "node-b",
                    "edge_type": "dependency",
                    "field_mapping": {"value": "analysis_result"}
                }
            ],
            "started_at": null,
            "completed_at": null,
            "result": null
        },
        "boundaries": {
            "execution": "disabled",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled",
        },
    })
}

#[test]
fn workflow_plans_create_list_get_and_audit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let created = store
        .create_workflow_plan("Plan docs only", "api", "actor", |ids, _created_at| {
            Ok(make_workflow_plan(ids))
        })
        .unwrap();

    assert_eq!(created["plan_id"], "plan-0001");
    assert_eq!(created["status"], "planned_read_only");
    assert_eq!(created["workflow_id"], "wf-plan-0001");
    assert_eq!(created["dispatch_id"], "plan-dispatch-0001");
    assert_eq!(created["boundaries"]["execution"], "disabled");

    let listed = store.search_workflow_plans(10, 0, None).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["plan_id"], "plan-0001");

    let fetched = store.get_workflow_plan("plan-0001").unwrap().unwrap();
    assert_eq!(fetched["raw_request"], "Plan docs only");
    assert_eq!(fetched["graph"]["workflow_id"], "wf-plan-0001");

    let audit = store.audit_events(10).unwrap();
    assert!(audit
        .iter()
        .any(|event| event["action"] == "workflow_plan.create"));
}

#[test]
fn workflow_plans_persist_read_only_advisory_metadata() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let planner = ReadOnlyPlanner::new();

    let created = store
        .create_workflow_plan("Plan docs only", "api", "actor", |ids, created_at| {
            planner.create_plan(ids, "Plan docs only", "api", created_at)
        })
        .unwrap();

    assert_eq!(created["advisory"]["schema_version"], "plan_advisory.v1");
    assert_eq!(created["advisory"]["mode"], "recommendation_only");
    assert_eq!(
        created["advisory"]["decision"]["target_repository_writes"],
        "disabled"
    );
    assert_eq!(
        created["advisory"]["retry"]["provider_invocation"],
        "not_invoked"
    );

    let fetched = store.get_workflow_plan("plan-0001").unwrap().unwrap();
    assert_eq!(fetched["advisory"]["schema_version"], "plan_advisory.v1");
    assert_eq!(
        fetched["advisory"]["routing"]["adaptive_routing_available"],
        false
    );
}

#[test]
fn workflow_plan_search_filters_and_paginates() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    store
        .create_workflow_plan("Alpha workflow plan", "api", "actor", |ids, _created_at| {
            Ok(make_workflow_plan(ids))
        })
        .unwrap();
    store
        .create_workflow_plan(
            "Beta routing proposal",
            "dashboard",
            "actor",
            |ids, _created_at| Ok(make_workflow_plan(ids)),
        )
        .unwrap();

    let alpha = store.search_workflow_plans(10, 0, Some("alpha")).unwrap();
    assert_eq!(alpha.len(), 1);
    assert_eq!(alpha[0]["plan_id"], "plan-0001");

    let source = store
        .search_workflow_plans(10, 0, Some("DASHBOARD"))
        .unwrap();
    assert_eq!(source.len(), 1);
    assert_eq!(source[0]["plan_id"], "plan-0002");

    let paged = store.search_workflow_plans(1, 1, Some("plan")).unwrap();
    assert_eq!(paged.len(), 1);
    assert_eq!(paged[0]["plan_id"], "plan-0001");
}

#[test]
fn workflow_runs_create_from_plan_persists_nodes_edges_events_and_approvals() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let plan = store
        .create_workflow_plan("Plan run state", "api", "actor", |ids, _created_at| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();

    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();
    assert_eq!(run["run_id"], "run-0001");
    assert_eq!(run["plan_id"], "plan-0001");
    assert_eq!(run["workflow_id"], "wf-plan-0001");
    assert_eq!(run["status"], "created");
    assert_eq!(run["boundaries"]["execution_authority"], "disabled");
    assert_eq!(run["boundaries"]["runtime_workers"], "env_gated_supervised");
    assert!(run.get("execution_result").is_none());
    assert_eq!(run["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(run["edges"].as_array().unwrap().len(), 1);

    let listed = store
        .search_workflow_runs(10, 0, Some("plan-0001"))
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0]["run_id"], "run-0001");

    let fetched = store.get_workflow_run("run-0001").unwrap().unwrap();
    assert_eq!(fetched["nodes"][1]["node_id"], "node-b");

    let event = store
        .append_workflow_run_event(
            "run-0001",
            Some("node-a"),
            "node_status_observed",
            &json!({"status": "ready"}),
            "actor",
        )
        .unwrap();
    assert_eq!(event["event_id"], "workflow-event-0002");
    assert_eq!(event["event_type"], "node_status_observed");

    let approval = store
        .record_workflow_run_approval(
            "run-0001",
            "node-a",
            "approved",
            "reviewer",
            Some("metadata only"),
            None,
            None,
            None,
            None,
        )
        .unwrap();
    assert_eq!(approval["approval_id"], "workflow-approval-0001");
    assert_eq!(approval["decision"], "approved");

    let events = store.workflow_run_events("run-0001", 10).unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event_type"], "workflow_run.created");
    assert_eq!(events[1]["event_type"], "node_status_observed");

    let approvals = store.workflow_run_approvals("run-0001", 10).unwrap();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0]["actor"], "reviewer");
}

#[test]
fn workflow_tick_inherits_plan_raw_request_as_prompt() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let raw_request = "请在 README 中增加中文快速开始";
    let plan = store
        .create_workflow_plan(raw_request, "api", "actor", |ids, _created_at| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let tick = store
        .tick_with_executor("run-0001", "actor", 0, &PromptEchoExecutor)
        .unwrap();

    assert_eq!(tick["result"]["output"].as_str(), Some(raw_request));
}

#[test]
fn workflow_tick_command_override_does_not_inject_plan_prompt() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let plan = store
        .create_workflow_plan("plan prompt", "api", "actor", |ids, _created_at| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let tick = store
        .tick_with_executor_and_command(
            "run-0001",
            "actor",
            0,
            &PromptEchoExecutor,
            Some("explicit command"),
        )
        .unwrap();

    assert!(tick["result"]["output"].is_null());
}

#[test]
fn workflow_tick_injects_completed_predecessor_context_into_metadata() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let plan = store
        .create_workflow_plan(
            "Plan context assembly",
            "api",
            "actor",
            |ids, _created_at| Ok(make_workflow_plan_with_nodes(ids)),
        )
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    let first = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(first["node_id"], "node-a");

    let second = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(second["node_id"], "node-b");
    let output = second["result"]["output"].as_str().unwrap();
    let injection: Value = serde_json::from_str(output).unwrap();
    assert_eq!(injection["schema_version"], "context_injection.v1");
    assert_eq!(injection["target_node_id"], "node-b");
    assert_eq!(injection["sources"][0]["from_node_id"], "node-a");
    assert_eq!(injection["injection_surface"], "node_metadata_only");
}

#[test]
fn workflow_run_resume_and_cancel_are_metadata_only() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let plan = store
        .create_workflow_plan("Plan run actions", "api", "actor", |ids, _created_at| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let resumed = store
        .request_workflow_run_resume("run-0001", "operator", Some("manual resume"))
        .unwrap();
    assert_eq!(resumed["status"], "running");
    assert_eq!(
        resumed["boundaries"]["resume_execution_authority"],
        "disabled"
    );
    assert!(resumed.get("execution_result").is_none());

    let cancelled = store
        .request_workflow_run_cancel("run-0001", "operator", Some("stop metadata run"))
        .unwrap();
    assert_eq!(cancelled["status"], "cancelled");
    assert_eq!(
        cancelled["boundaries"]["cancel_execution_authority"],
        "disabled"
    );
    assert!(cancelled.get("execution_result").is_none());

    let events = store.workflow_run_events("run-0001", 10).unwrap();
    assert!(events
        .iter()
        .any(|event| event["event_type"] == "workflow_resume_requested"));
    assert!(events
        .iter()
        .any(|event| event["event_type"] == "workflow_cancel_requested"));
}

#[test]
fn supervised_patch_workspace_records_metadata_only_boundary_evidence() {
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let workspace_path = workspace_root.path().join("workspaces").join("ws-001");

    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-0001",
                "run_id": "run-0001",
                "target_id": "target-001",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_path.to_string_lossy(),
                "source_revision": "abc123",
                "source_tree_hash": "tree123",
            }),
            "operator",
        )
        .unwrap();

    assert_eq!(workspace["schema_version"], "supervised_patch_workspace.v1");
    assert_eq!(workspace["workspace_id"], "patch-workspace-0001");
    assert_eq!(workspace["status"], "requested");
    assert_eq!(workspace["metadata_only"], true);
    assert_eq!(workspace["execution_authority"], "disabled");
    assert_eq!(
        workspace["boundary"]["target_repository_writes"],
        "disabled"
    );
    assert_eq!(
        workspace["boundary"]["workspace_directory_creation"],
        "not_performed"
    );
    assert_eq!(
        workspace["boundary"]["registered_git_worktree"],
        "forbidden"
    );

    let listed = store.supervised_patch_workspaces(10).unwrap();
    assert_eq!(listed.len(), 1);
    let fetched = store
        .get_supervised_patch_workspace("patch-workspace-0001")
        .unwrap()
        .unwrap();
    assert_eq!(fetched["target_id"], "target-001");

    let stats = store.stats().unwrap();
    assert_eq!(stats["supervised_patch_workspaces"], 1);
    assert_eq!(stats["supervised_patch_artifacts"], 0);

    let audit = store.audit_events(10).unwrap();
    assert!(audit
        .iter()
        .any(|event| event["action"] == "supervised_patch.workspace_record"));
}

#[test]
fn supervised_patch_workspace_rejects_registered_target_paths() {
    let target_dir = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let workspace_path = target_dir
        .path()
        .join(".agent-control-plane")
        .join("ws-001");

    let result = store.record_supervised_patch_workspace(
        &json!({
            "run_id": "run-0001",
            "target_id": "target-001",
            "target_repo_path": target_dir.path().to_string_lossy(),
            "workspace_path": workspace_path.to_string_lossy(),
            "source_revision": "abc123",
        }),
        "operator",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("outside registered target repository"));
    assert_eq!(store.supervised_patch_workspaces(10).unwrap().len(), 0);
}

#[test]
fn supervised_patch_workspace_rejects_path_like_workspace_ids() {
    let target_dir = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let result = store.create_workspace_directory("../escape", target_dir.path().to_str().unwrap());

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("workspace_id"));
}

#[cfg(unix)]
#[test]
fn supervised_patch_workspace_copy_skips_symlink_escape() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let target_dir = dir.path().join("target");
    let outside_dir = dir.path().join("outside");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::create_dir_all(&outside_dir).unwrap();
    std::fs::write(target_dir.join("safe.txt"), "safe").unwrap();
    std::fs::write(outside_dir.join("secret.txt"), "api_key = should_not_copy").unwrap();
    std::os::unix::fs::symlink(&outside_dir, target_dir.join("escape_link")).unwrap();

    let workspace_path = store
        .create_workspace_directory("ws-symlink", target_dir.to_str().unwrap())
        .unwrap();

    let workspace = std::path::Path::new(&workspace_path);
    assert!(workspace.join("safe.txt").exists());
    assert!(!workspace.join("escape_link").exists());
    assert!(!workspace.join("escape_link/secret.txt").exists());
}

#[test]
fn supervised_patch_artifact_records_metadata_without_apply_authority() {
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let workspace_path = workspace_root.path().join("workspaces").join("ws-001");
    store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-0001",
                "run_id": "run-0001",
                "target_id": "target-001",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_path.to_string_lossy(),
                "source_revision": "abc123",
            }),
            "operator",
        )
        .unwrap();

    let artifact = store
        .record_supervised_patch_artifact(
            &json!({
                "workspace_id": "patch-workspace-0001",
                "patch_hash": "sha256-patch",
                "changed_files": ["src/lib.rs", "README.md"],
                "redaction_status": "redacted",
                "storage_refs": {"patch": "app-owned://patches/patch-artifact-0001"},
            }),
            "operator",
        )
        .unwrap();

    assert_eq!(artifact["schema_version"], "supervised_patch_artifact.v1");
    assert_eq!(artifact["artifact_id"], "patch-artifact-0001");
    assert_eq!(artifact["workspace_id"], "patch-workspace-0001");
    assert_eq!(artifact["metadata_only"], true);
    assert_eq!(artifact["execution_authority"], "disabled");
    assert_eq!(artifact["patch_apply_authority"], "disabled");
    assert_eq!(artifact["artifact_file_created"], false);
    assert_eq!(artifact["changed_files"].as_array().unwrap().len(), 2);

    let fetched = store
        .get_supervised_patch_artifact("patch-artifact-0001")
        .unwrap()
        .unwrap();
    assert_eq!(fetched["patch_hash"], "sha256-patch");
    assert_eq!(store.supervised_patch_artifacts(10).unwrap().len(), 1);

    let stats = store.stats().unwrap();
    assert_eq!(stats["supervised_patch_workspaces"], 1);
    assert_eq!(stats["supervised_patch_artifacts"], 1);
}

#[test]
fn supervised_patch_artifact_rejects_unsafe_changed_files() {
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let workspace_path = workspace_root.path().join("workspaces").join("ws-001");
    store
        .record_supervised_patch_workspace(
            &json!({
                "run_id": "run-0001",
                "target_id": "target-001",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_path.to_string_lossy(),
                "source_revision": "abc123",
            }),
            "operator",
        )
        .unwrap();

    let result = store.record_supervised_patch_artifact(
        &json!({
            "workspace_id": "patch-workspace-0001",
            "patch_hash": "sha256-patch",
            "changed_files": ["../secret.txt"],
        }),
        "operator",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("changed file must be normalized"));
    assert_eq!(store.supervised_patch_artifacts(10).unwrap().len(), 0);

    let backslash_result = store.record_supervised_patch_artifact(
        &json!({
            "workspace_id": "patch-workspace-0001",
            "patch_hash": "sha256-patch",
            "changed_files": ["src\\lib.rs"],
        }),
        "operator",
    );

    assert!(backslash_result.is_err());
    assert!(backslash_result
        .unwrap_err()
        .contains("changed file must use forward slashes"));
    assert_eq!(store.supervised_patch_artifacts(10).unwrap().len(), 0);
}

#[test]
fn supervised_patch_artifact_rejects_secret_review_diff() {
    let target_dir = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let workspace_path = workspace_root.path().join("workspaces").join("ws-001");
    store
        .record_supervised_patch_workspace(
            &json!({
                "run_id": "run-0001",
                "target_id": "target-001",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_path.to_string_lossy(),
                "source_revision": "abc123",
            }),
            "operator",
        )
        .unwrap();

    let result = store.record_supervised_patch_artifact(
        &json!({
            "workspace_id": "patch-workspace-0001",
            "patch_hash": "sha256-patch",
            "changed_files": ["leak.txt"],
            "redaction_status": "failed",
            "review_diff": "+api_key = sk-should-not-store",
        }),
        "operator",
    );

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("review_diff"));
    assert_eq!(store.supervised_patch_artifacts(10).unwrap().len(), 0);
}

// --- cost_summary v2 tests ---

#[test]
fn cost_summary_empty_store() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let summary = store.cost_summary().unwrap();

    assert_eq!(summary["schema_version"], "local_cost_summary.v2");
    assert_eq!(summary["currency"], "USD");
    assert_eq!(summary["dispatch_count"], 0);
    assert_eq!(summary["total_reserved_cost"], 0.0);
    assert_eq!(summary["total_estimated_cost_usd"], 0.0);
    assert_eq!(summary["total_input_tokens"], 0);
    assert_eq!(summary["total_output_tokens"], 0);
    assert_eq!(summary["estimated_cost_available"], false);
    assert_eq!(summary["cost_utilization"], 0.0);
    assert_eq!(summary["by_tier"].as_array().unwrap().len(), 0);
    assert_eq!(summary["daily"].as_array().unwrap().len(), 0);
}

#[test]
fn cost_summary_aggregates_reserved_and_estimated() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle1 = make_bundle_with_usage(
        "d1",
        "provider",
        Some(100),
        Some(50),
        Some(0.003),
        Some(100),
    );
    let bundle2 = make_bundle_with_usage(
        "d2",
        "provider",
        Some(200),
        Some(80),
        Some(0.005),
        Some(200),
    );
    store
        .record_dispatch("req1", "api", &bundle1, "actor")
        .unwrap();
    store
        .record_dispatch("req2", "api", &bundle2, "actor")
        .unwrap();

    let summary = store.cost_summary().unwrap();

    assert_eq!(summary["dispatch_count"], 2);
    assert_eq!(summary["total_reserved_cost"], 0.02);
    assert_eq!(summary["total_estimated_cost_usd"], 0.008);
    assert_eq!(summary["total_input_tokens"], 300);
    assert_eq!(summary["total_output_tokens"], 130);
    assert_eq!(summary["estimated_cost_available"], true);
    assert!((summary["cost_utilization"].as_f64().unwrap() - 0.4).abs() < 0.001);
}

#[test]
fn cost_summary_distinguishes_token_usage_without_estimated_cost() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let bundle = make_bundle_with_usage("d1", "provider", Some(100), Some(50), None, Some(100));
    store
        .record_dispatch("req1", "api", &bundle, "actor")
        .unwrap();

    let summary = store.cost_summary().unwrap();

    assert_eq!(summary["dispatch_count"], 1);
    assert_eq!(summary["total_input_tokens"], 100);
    assert_eq!(summary["total_output_tokens"], 50);
    assert_eq!(summary["total_estimated_cost_usd"], 0.0);
    assert_eq!(summary["estimated_cost_available"], false);
}

#[test]
fn cost_summary_groups_by_tier() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle_cheap = json!({
        "record": {"dispatch_id": "d1", "created_at": "2026-05-29T12:00:00Z", "final_status": "completed"},
        "analysis": {"risk_level": "low"},
        "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.001}},
        "execution_result": {"executor_type": "provider", "input_tokens": 50, "output_tokens": 20, "estimated_cost": 0.0005},
        "evaluation_result": {"status": "pass"},
    });
    let bundle_strong = json!({
        "record": {"dispatch_id": "d2", "created_at": "2026-05-29T12:01:00Z", "final_status": "completed"},
        "analysis": {"risk_level": "medium"},
        "decision": {"selected_tier": "strong_planner", "budget_reservation": {"reserved_cost": 0.05}},
        "execution_result": {"executor_type": "provider", "input_tokens": 500, "output_tokens": 200, "estimated_cost": 0.015},
        "evaluation_result": {"status": "pass"},
    });
    store
        .record_dispatch("req1", "api", &bundle_cheap, "actor")
        .unwrap();
    store
        .record_dispatch("req2", "api", &bundle_strong, "actor")
        .unwrap();

    let summary = store.cost_summary().unwrap();
    let tiers = summary["by_tier"].as_array().unwrap();
    assert_eq!(tiers.len(), 2);

    let cheap = &tiers[0];
    assert_eq!(cheap["selected_tier"], "cheap_executor");
    assert_eq!(cheap["dispatch_count"], 1);
    assert_eq!(cheap["reserved_cost"], 0.001);
    assert_eq!(cheap["estimated_cost_usd"], 0.0005);
    assert_eq!(cheap["input_tokens"], 50);
    assert_eq!(cheap["output_tokens"], 20);

    let strong = &tiers[1];
    assert_eq!(strong["selected_tier"], "strong_planner");
    assert_eq!(strong["dispatch_count"], 1);
    assert_eq!(strong["reserved_cost"], 0.05);
    assert_eq!(strong["estimated_cost_usd"], 0.015);
}

#[test]
fn cost_summary_daily_breakdown() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle = make_bundle_with_usage("d1", "noop", None, None, None, None);
    store
        .record_dispatch("req1", "api", &bundle, "actor")
        .unwrap();

    let summary = store.cost_summary().unwrap();
    let daily = summary["daily"].as_array().unwrap();
    assert_eq!(daily.len(), 1);
    assert_eq!(daily[0]["date"], "2026-05-29");
    assert_eq!(daily[0]["dispatch_count"], 1);
}

// --- dispatch_cost_details tests ---

#[test]
fn dispatch_cost_details_returns_per_dispatch_rows() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle = make_bundle_with_usage(
        "d1",
        "provider",
        Some(150),
        Some(75),
        Some(0.003),
        Some(250),
    );
    store
        .record_dispatch("hello", "api", &bundle, "actor")
        .unwrap();

    let details = store.dispatch_cost_details(10).unwrap();
    assert_eq!(details["schema_version"], "local_dispatch_cost_detail.v1");
    let dispatches = details["dispatches"].as_array().unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["dispatch_id"], "d1");
    assert_eq!(dispatches[0]["reserved_cost"], 0.01);
    assert_eq!(dispatches[0]["input_tokens"], 150);
    assert_eq!(dispatches[0]["output_tokens"], 75);
    assert_eq!(dispatches[0]["estimated_cost_usd"], 0.003);
    assert_eq!(dispatches[0]["executor_type"], "provider");
    assert_eq!(dispatches[0]["latency_ms"], 250);
}

#[test]
fn dispatch_cost_details_respects_limit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    for i in 0..5 {
        let bundle = make_bundle_with_usage(&format!("d{}", i), "noop", None, None, None, None);
        store
            .record_dispatch(&format!("req{}", i), "api", &bundle, "actor")
            .unwrap();
    }

    let details = store.dispatch_cost_details(3).unwrap();
    let dispatches = details["dispatches"].as_array().unwrap();
    assert_eq!(dispatches.len(), 3);
}

#[test]
fn dispatch_cost_details_empty_store() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let details = store.dispatch_cost_details(50).unwrap();
    assert_eq!(details["schema_version"], "local_dispatch_cost_detail.v1");
    assert_eq!(details["dispatches"].as_array().unwrap().len(), 0);
}

// --- get_dispatch tests ---

#[test]
fn get_dispatch_returns_dispatch_for_existing_id() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle = make_bundle_with_usage("d1", "noop", Some(100), Some(50), Some(0.001), Some(42));
    store
        .record_dispatch("request-1", "api", &bundle, "actor")
        .unwrap();

    let result = store.get_dispatch("d1").unwrap();
    assert!(result.is_some());
    let dispatch = result.unwrap();
    assert_eq!(dispatch["dispatch_id"], "d1");
    assert_eq!(dispatch["raw_request"], "request-1");
    assert_eq!(dispatch["request_source"], "api");
    assert_eq!(dispatch["executor_type"], "noop");
    assert_eq!(dispatch["input_tokens"], 100);
    assert_eq!(dispatch["output_tokens"], 50);
    assert!((dispatch["estimated_cost_usd"].as_f64().unwrap() - 0.001).abs() < 1e-10);
    assert_eq!(dispatch["latency_ms"], 42);
    assert!(dispatch["bundle"].is_object());
}

#[test]
fn get_dispatch_returns_none_for_missing_id() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let result = store.get_dispatch("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn get_dispatch_returns_latest_when_duplicate_ids() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let bundle1 = make_bundle_with_usage("d1", "noop", Some(10), Some(5), Some(0.0005), Some(10));
    store
        .record_dispatch("request-1", "api", &bundle1, "actor")
        .unwrap();

    let bundle2 = make_bundle_with_usage(
        "d1",
        "provider",
        Some(200),
        Some(100),
        Some(0.01),
        Some(200),
    );
    store
        .record_dispatch("request-2", "api", &bundle2, "actor")
        .unwrap();

    let result = store.get_dispatch("d1").unwrap().unwrap();
    assert_eq!(result["raw_request"], "request-2");
    assert_eq!(result["executor_type"], "provider");
    assert_eq!(result["input_tokens"], 200);
}

// --- SQLite contention / concurrent-write tests ---

#[test]
fn concurrent_provider_cost_reservations_do_not_exceed_daily_cap() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("provider-budget.db");
    let store_a = Arc::new(LocalProductStore::new(&db_path).unwrap());
    let store_b = Arc::new(LocalProductStore::new(&db_path).unwrap());
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = [
        ("reservation-a", Arc::clone(&store_a)),
        ("reservation-b", Arc::clone(&store_b)),
    ]
    .into_iter()
    .map(|(event_id, store)| {
        let barrier = Arc::clone(&barrier);
        thread::spawn(move || {
            let mut event = make_event(event_id, event_id, "request_reserved");
            event.cost = Some(0.6);
            event.created_at = "2026-05-29T12:00:00Z".to_string();
            barrier.wait();
            store.reserve_provider_audit_cost(&event, 1.0, 1.0)
        })
    })
    .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    assert!(results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .all(|error| error.contains("daily cost cap exceeded")));
    let reservations = store_a.provider_audit_events(10).unwrap();
    assert_eq!(
        reservations
            .iter()
            .filter(|event| event["event_type"] == "request_reserved")
            .count(),
        1
    );
}

#[test]
fn provider_cost_reservation_retry_is_idempotent_and_conflicts_fail_closed() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("provider-budget-retry.db")).unwrap();
    let mut event = make_event("stable-reservation", "stable-dispatch", "request_reserved");
    event.cost = Some(0.25);
    event.created_at = "2026-05-29T12:00:00Z".to_string();

    store.reserve_provider_audit_cost(&event, 0.5, 1.0).unwrap();
    store.reserve_provider_audit_cost(&event, 0.5, 1.0).unwrap();
    assert_eq!(store.provider_audit_events(10).unwrap().len(), 1);

    let mut conflicting = event.clone();
    conflicting.cost = Some(0.3);
    assert!(store
        .reserve_provider_audit_cost(&conflicting, 0.5, 1.0)
        .unwrap_err()
        .contains("identity conflicts"));
    assert_eq!(store.provider_audit_events(10).unwrap().len(), 1);
}

#[test]
fn concurrent_tool_policy_updates_require_current_hash() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("tool-policy.db")).unwrap());
    let initial = store
        .configure_tool_capability("setup", "echo", "initial", None, None, false, "low", None)
        .unwrap();
    let expected = initial["resource_sha256"].as_str().unwrap().to_string();
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|description| {
            let store = store.clone();
            let barrier = barrier.clone();
            let expected = expected.clone();
            thread::spawn(move || {
                barrier.wait();
                store.configure_tool_capability(
                    description,
                    "echo",
                    description,
                    None,
                    None,
                    true,
                    "medium",
                    Some(&expected),
                )
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    assert!(results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .all(|error| error.contains("changed concurrently")));

    let current = store.read_tool_capability_policy("echo").unwrap().unwrap();
    let winner = results
        .iter()
        .find_map(|result| result.as_ref().ok())
        .unwrap();
    assert_eq!(current["resource_sha256"], winner["resource_sha256"]);
}

#[test]
fn sqlite_implicit_tool_receipt_is_atomic_across_connections() {
    struct CountingEffectExecutor(Arc<std::sync::atomic::AtomicUsize>);

    impl NodeExecutor for CountingEffectExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "command".to_string(),
                output: Some("bounded effect".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
            }
        }

        fn executor_type_name(&self) -> &str {
            "command"
        }
    }

    let dir = tempdir().unwrap();
    let path = dir.path().join("implicit-tool-receipt.db");
    let setup = LocalProductStore::new(&path).unwrap();
    let plan = setup
        .create_workflow_plan("implicit tool receipt", "test", "test", |ids, _| {
            Ok(json!({
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "graph": {
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "nodes": [{
                        "node_id": "tool-node",
                        "task_type": "command",
                        "status": "pending",
                        "profile_id": "tool-profile",
                        "command": "fixture-tool bounded"
                    }],
                    "edges": []
                },
                "analysis": {},
                "boundaries": {"execution_authority": "bounded_trusted_local"}
            }))
        })
        .unwrap();
    let run = setup
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap().to_string();
    let workflow_id = plan["workflow_id"].as_str().unwrap().to_string();
    setup
        .configure_tool_capability(
            "test",
            "fixture-tool",
            "bounded fixture",
            None,
            None,
            false,
            "low",
            None,
        )
        .unwrap();
    setup
        .configure_tool_allowlist("test", "tool-profile", &["fixture-tool".to_string()], None)
        .unwrap();
    drop(setup);

    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let inner: Arc<dyn NodeExecutor> = Arc::new(CountingEffectExecutor(calls.clone()));
    let store_a = Arc::new(LocalProductStore::new(&path).unwrap());
    let store_b = Arc::new(LocalProductStore::new(&path).unwrap());
    let barrier = Arc::new(Barrier::new(2));
    let input = NodeExecutionInput {
        node_id: "tool-node".to_string(),
        task_type: "command".to_string(),
        run_id: run_id.clone(),
        workflow_id,
        node_metadata: json!({
            "profile_id": "tool-profile",
            "command": "fixture-tool bounded"
        }),
    };
    let handles = [store_a.clone(), store_b.clone()]
        .into_iter()
        .map(|store| {
            let barrier = barrier.clone();
            let inner = inner.clone();
            let input = input.clone();
            thread::spawn(move || {
                let executor = ToolPolicyNodeExecutor::command(inner, store);
                barrier.wait();
                executor.execute_node(&input)
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        results
            .iter()
            .filter(|result| result.status == "completed")
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| {
                result.error_domain.as_deref() == Some("tool_execution_outcome_unknown")
            })
            .count(),
        1
    );
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let receipt = store_a
        .inspect_tool_execution_authorization(&run_id, "tool-node")
        .unwrap()
        .unwrap();
    assert_eq!(receipt["status"], "consumed");
    assert_eq!(receipt["resolved_by"], "tool-policy:implicit");
    let implicit_claims = store_a
        .audit_events(100)
        .unwrap()
        .into_iter()
        .filter(|event| {
            event["action"] == "tool_execution.implicit_receipt_claimed"
                && event["resource"] == run_id
        })
        .count();
    assert_eq!(implicit_claims, 1);
}

#[test]
fn tool_policy_hook_limit_rolls_back_rejected_hook() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tool-hook-limit.db")).unwrap();
    for index in 0..32 {
        store
            .configure_tool_hook(
                "setup",
                &format!("hook-{index:02}"),
                "pre_execution",
                None,
                None,
                "log",
                None,
                true,
                None,
            )
            .unwrap();
    }

    let error = store
        .configure_tool_hook(
            "setup",
            "hook-over-limit",
            "pre_execution",
            None,
            None,
            "log",
            None,
            true,
            None,
        )
        .expect_err("the thirty-third enabled hook must fail closed");
    assert!(error.contains("enabled tool hook count exceeds"));
    assert!(store
        .read_tool_hook_policy("hook-over-limit")
        .unwrap()
        .is_none());
}

#[test]
fn workflow_run_reuses_consistent_agent_state_and_rejects_conflicting_identity() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("agent-state-reuse.db")).unwrap();
    let make_plan = |role: &str, request: &str| {
        store
            .create_workflow_plan(request, "test", "test", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [
                            {
                                "node_id": "agent-step-1",
                                "task_type": "agent_step",
                                "status": "pending",
                                "agent_id": "agent-shared",
                                "agent_role": "implementer",
                                "agent_objective": "shared objective",
                                "profile_id": "bounded",
                                "capability_profile": ["code"]
                            },
                            {
                                "node_id": "agent-step-2",
                                "task_type": "agent_step",
                                "status": "pending",
                                "agent_id": "agent-shared",
                                "agent_role": role,
                                "agent_objective": "shared objective",
                                "profile_id": "bounded",
                                "capability_profile": ["code"]
                            }
                        ],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "rust_scheduler_only"}
                }))
            })
            .unwrap()
    };

    let consistent = make_plan("implementer", "consistent shared agent");
    let run = store
        .create_workflow_run_from_plan(consistent["plan_id"].as_str().unwrap(), "test")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();
    assert_eq!(store.list_agent_state_by_run(run_id).unwrap().len(), 1);

    let conflicting = make_plan("reviewer", "conflicting shared agent");
    let runs_before = store.list_workflow_runs_with_offset(100, 0).unwrap().len();
    let error = store
        .create_workflow_run_from_plan(conflicting["plan_id"].as_str().unwrap(), "test")
        .expect_err("conflicting agent identity must roll back run creation");
    assert!(error.contains("conflicting state identity"));
    assert_eq!(
        store.list_workflow_runs_with_offset(100, 0).unwrap().len(),
        runs_before
    );
}

#[test]
fn sqlite_agent_global_cap_is_atomic_across_store_connections() {
    struct HoldingAgentExecutor {
        entered: std::sync::mpsc::Sender<()>,
        release: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }
    impl engine::node_executor::NodeExecutor for HoldingAgentExecutor {
        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn execute_node(
            &self,
            _input: &engine::node_executor::NodeExecutionInput,
        ) -> engine::node_executor::NodeExecutionOutput {
            self.entered.send(()).unwrap();
            let (lock, condition) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = condition.wait(released).unwrap();
            }
            engine::node_executor::NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("held fixture completed".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
            }
        }
    }
    struct CountingAgentExecutor(Arc<std::sync::atomic::AtomicUsize>);
    impl engine::node_executor::NodeExecutor for CountingAgentExecutor {
        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn execute_node(
            &self,
            _input: &engine::node_executor::NodeExecutionInput,
        ) -> engine::node_executor::NodeExecutionOutput {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            engine::node_executor::NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("unexpected second execution".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
            }
        }
    }

    let dir = tempdir().unwrap();
    let path = dir.path().join("agent-global-cap.db");
    let setup = LocalProductStore::new(&path).unwrap();
    let create_run = |agent_id: &str| {
        let plan = setup
            .create_workflow_plan(agent_id, "test", "test", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [{
                            "node_id": format!("node-{agent_id}"),
                            "task_type": "agent_step",
                            "status": "pending",
                            "agent_id": agent_id,
                            "agent_role": "worker",
                            "agent_objective": "bounded concurrency",
                            "profile_id": "bounded",
                            "capability_profile": ["work"]
                        }],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "rust_scheduler_only"}
                }))
            })
            .unwrap();
        setup
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
            .unwrap()["run_id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let run_a = create_run("agent-cap-a");
    let run_b = create_run("agent-cap-b");
    drop(setup);

    let store_a = LocalProductStore::new(&path).unwrap();
    let store_b = LocalProductStore::new(&path).unwrap();
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let release_for_thread = release.clone();
    let first = thread::spawn(move || {
        store_a.tick_with_executor_with_agent_caps(
            &run_a,
            "test",
            0,
            &HoldingAgentExecutor {
                entered: entered_tx,
                release: release_for_thread,
            },
            1,
            1,
        )
    });
    entered_rx.recv().unwrap();

    let second_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let second = store_b
        .tick_with_executor_with_agent_caps(
            &run_b,
            "test",
            0,
            &CountingAgentExecutor(second_calls.clone()),
            1,
            1,
        )
        .unwrap();
    assert_eq!(second["action"], "no_ready_node");
    assert_eq!(second_calls.load(std::sync::atomic::Ordering::SeqCst), 0);

    let (lock, condition) = &*release;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    assert_eq!(
        first.join().unwrap().unwrap()["result"]["status"],
        "completed"
    );
}

#[test]
fn sqlite_stale_worker_cannot_overwrite_reclaimed_attempt() {
    struct HoldingCommandExecutor {
        entered: std::sync::mpsc::Sender<()>,
        release: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }

    impl NodeExecutor for HoldingCommandExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.entered.send(()).unwrap();
            let (lock, condition) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = condition.wait(released).unwrap();
            }
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "command".to_string(),
                output: Some("stale attempt output".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
            }
        }

        fn executor_type_name(&self) -> &str {
            "command"
        }
    }

    struct ReclaimedCommandExecutor;

    impl NodeExecutor for ReclaimedCommandExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "command".to_string(),
                output: Some("reclaimed attempt output".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
            }
        }

        fn executor_type_name(&self) -> &str {
            "command"
        }
    }

    let dir = tempdir().unwrap();
    let path = dir.path().join("stale-worker-cas.db");
    let clock = Arc::new(std::sync::Mutex::new("2026-07-14T00:00:00Z".to_string()));
    let setup_clock = clock.clone();
    let setup =
        LocalProductStore::new_with_clock(&path, move || setup_clock.lock().unwrap().clone())
            .unwrap();
    let plan = setup
        .create_workflow_plan("stale worker CAS", "test", "test", |ids, _| {
            Ok(json!({
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "graph": {
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "nodes": [{
                        "node_id": "command-node",
                        "task_type": "command",
                        "status": "pending",
                        "command": "true"
                    }],
                    "edges": []
                },
                "analysis": {},
                "boundaries": {"execution_authority": "bounded_trusted_local"}
            }))
        })
        .unwrap();
    let run_id = setup
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
        .unwrap()["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    drop(setup);

    let first_clock = clock.clone();
    let first_store =
        LocalProductStore::new_with_clock(&path, move || first_clock.lock().unwrap().clone())
            .unwrap();
    let second_clock = clock.clone();
    let second_store = Arc::new(
        LocalProductStore::new_with_clock(&path, move || second_clock.lock().unwrap().clone())
            .unwrap(),
    );
    let recovery_clock = clock.clone();
    let recovery_store = Arc::new(
        LocalProductStore::new_with_clock(&path, move || recovery_clock.lock().unwrap().clone())
            .unwrap(),
    );
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let first_release = release.clone();
    let first_run_id = run_id.clone();
    let first = thread::spawn(move || {
        first_store.tick_with_executor(
            &first_run_id,
            "old-worker",
            0,
            &HoldingCommandExecutor {
                entered: entered_tx,
                release: first_release,
            },
        )
    });
    entered_rx.recv().unwrap();

    *clock.lock().unwrap() = "2026-07-14T00:00:02Z".to_string();
    let recovery_barrier = Arc::new(Barrier::new(2));
    let recoveries = [second_store.clone(), recovery_store]
        .into_iter()
        .map(|store| {
            let barrier = recovery_barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                store.recover_stale_leases(1_000).unwrap()
            })
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(recoveries.iter().sum::<i64>(), 1);
    assert!(recoveries.iter().all(|count| *count <= 1));
    let reclaimed = second_store
        .tick_with_executor(&run_id, "new-worker", 0, &ReclaimedCommandExecutor)
        .unwrap();
    assert_eq!(reclaimed["action"], "node_executed");
    assert_eq!(reclaimed["attempt"], 2);
    assert_eq!(reclaimed["result"]["output"], "reclaimed attempt output");

    let (lock, condition) = &*release;
    *lock.lock().unwrap() = true;
    condition.notify_all();
    let stale = first.join().unwrap().unwrap();
    assert_eq!(stale["action"], "stale_completion_ignored");
    assert_eq!(stale["attempt"], 1);

    let persisted = second_store.get_workflow_run(&run_id).unwrap().unwrap();
    let node = persisted["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == "command-node")
        .unwrap();
    assert_eq!(node["attempt_count"], 2);
    assert_eq!(node["status"], "completed");
    assert_eq!(node["result"]["output"], "reclaimed attempt output");
    let audit = second_store.audit_events(100).unwrap();
    assert_eq!(
        audit
            .iter()
            .filter(|event| {
                event["action"] == "workflow_node.stale_completion_ignored"
                    && event["resource"] == "command-node"
            })
            .count(),
        1
    );
    let terminal = audit
        .iter()
        .find(|event| event["action"] == "workflow_run.completed" && event["resource"] == run_id)
        .expect("executable run terminal audit");
    assert_eq!(terminal["details"]["metadata_only"], false);
    assert_eq!(
        terminal["details"]["execution_authority"],
        "bounded_trusted_local"
    );
}

#[test]
fn sqlite_stale_lease_recovery_rolls_back_when_audit_fails() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("stale-recovery-audit-rollback.db");
    let store =
        LocalProductStore::new_with_clock(&path, || "2026-07-14T00:00:02Z".to_string()).unwrap();
    let plan = store
        .create_workflow_plan("stale audit rollback", "test", "test", |ids, _| {
            Ok(json!({
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "graph": {
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "nodes": [{
                        "node_id": "audit-rollback-node",
                        "task_type": "agent_step",
                        "status": "pending",
                        "agent_id": "audit-rollback-agent",
                        "assigned_agent_id": "audit-rollback-agent",
                        "agent_role": "worker",
                        "agent_objective": "prove atomic stale recovery audit",
                        "profile_id": "bounded",
                        "capability_profile": ["work"],
                        "decision_source": "fixture",
                        "max_actions": 1
                    }],
                    "edges": []
                },
                "analysis": {},
                "boundaries": {"execution_authority": "rust_scheduler_only"}
            }))
        })
        .unwrap();
    let run_id = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
        .unwrap()["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        store
            .set_pending_node_to_running_for_test("2026-07-14T00:00:00Z")
            .unwrap(),
        1
    );

    let fault = rusqlite::Connection::open(&path).unwrap();
    fault
        .execute_batch(
            "CREATE TRIGGER fail_stale_recovery_audit
             BEFORE INSERT ON audit_log
             WHEN NEW.action = 'agent_step.lease_expired'
                  AND NEW.resource = 'audit-rollback-node'
             BEGIN
                 SELECT RAISE(ABORT, 'fixture agent lease audit failure');
             END;",
        )
        .unwrap();
    let error = store
        .recover_stale_leases(1_000)
        .expect_err("audit failure must roll back recovery");
    assert!(error.contains("fixture agent lease audit failure"));
    let after_failure = store.get_workflow_run(&run_id).unwrap().unwrap();
    let failed_node = after_failure["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == "audit-rollback-node")
        .unwrap();
    assert_eq!(failed_node["db_status"], "running");
    assert_eq!(failed_node["leased_at"], "2026-07-14T00:00:00Z");
    assert!(store.audit_events(100).unwrap().iter().all(|event| {
        event["action"] != "workflow_node.stale_lease_recovered"
            && event["action"] != "agent_step.lease_expired"
    }));

    fault
        .execute_batch("DROP TRIGGER fail_stale_recovery_audit;")
        .unwrap();
    assert_eq!(store.recover_stale_leases(1_000).unwrap(), 1);
    let recovered = store.get_workflow_run(&run_id).unwrap().unwrap();
    let recovered_node = recovered["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == "audit-rollback-node")
        .unwrap();
    assert_eq!(recovered_node["db_status"], "pending");
    assert!(recovered_node.get("leased_at").is_none());
    assert_eq!(
        store
            .audit_events(100)
            .unwrap()
            .iter()
            .filter(|event| {
                event["action"] == "workflow_node.stale_lease_recovered"
                    && event["resource"] == "audit-rollback-node"
            })
            .count(),
        1
    );
    assert_eq!(
        store
            .audit_events(100)
            .unwrap()
            .iter()
            .filter(|event| {
                event["action"] == "agent_step.lease_expired"
                    && event["resource"] == "audit-rollback-node"
            })
            .count(),
        1
    );
}

#[test]
fn concurrent_dispatch_writes_from_multiple_threads() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());
    let thread_count = 8;
    let writes_per_thread = 20;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|t| {
            let store = store.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                for i in 0..writes_per_thread {
                    let dispatch_id = format!("disp-t{t}-{i}");
                    let bundle = make_bundle_with_usage(
                        &dispatch_id,
                        "noop",
                        Some((t * 100 + i) as i64),
                        None,
                        None,
                        None,
                    );
                    store
                        .record_dispatch(
                            &format!("request-{dispatch_id}"),
                            "api",
                            &bundle,
                            "test-actor",
                        )
                        .unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let dispatches = store.list_dispatches(1000).unwrap();
    assert_eq!(dispatches.len(), thread_count * writes_per_thread);
}

#[test]
fn concurrent_reads_during_writes() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());

    let bundle = make_bundle_with_usage("seed", "noop", None, None, None, None);
    store
        .record_dispatch("seed-request", "api", &bundle, "actor")
        .unwrap();

    let thread_count = 6;
    let barrier = Arc::new(Barrier::new(thread_count));
    let store_clone = store.clone();

    let writer = {
        let store = store.clone();
        let barrier = barrier.clone();
        thread::spawn(move || {
            barrier.wait();
            for i in 0..30 {
                let bundle =
                    make_bundle_with_usage(&format!("w-{i}"), "noop", None, None, None, None);
                store
                    .record_dispatch(&format!("w-req-{i}"), "api", &bundle, "actor")
                    .unwrap();
            }
        })
    };

    let readers: Vec<_> = (0..(thread_count - 1))
        .map(|_| {
            let store = store_clone.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                for _ in 0..10 {
                    let _ = store.list_dispatches(100);
                    let _ = store.cost_summary();
                    let _ = store.get_dispatch("seed");
                    thread::yield_now();
                }
            })
        })
        .collect();

    writer.join().unwrap();
    for r in readers {
        r.join().unwrap();
    }

    let dispatches = store.list_dispatches(1000).unwrap();
    assert_eq!(dispatches.len(), 31);
}

#[test]
fn concurrent_provider_audit_events() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());
    let thread_count = 6;
    let events_per_thread = 15;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|t| {
            let store = store.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                for i in 0..events_per_thread {
                    let event = make_event(
                        &format!("evt-t{t}-{i}"),
                        &format!("disp-{t}"),
                        "response_received",
                    );
                    store.record_provider_audit_event(&event).unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let events = store.provider_audit_events(1000).unwrap();
    assert_eq!(events.len(), thread_count * events_per_thread);
}

#[test]
fn no_deadlock_under_rapid_lock_contention() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());
    let thread_count = 12;
    let ops_per_thread = 50;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|t| {
            let store = store.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                for i in 0..ops_per_thread {
                    if i % 3 == 0 {
                        let bundle = make_bundle_with_usage(
                            &format!("deadlock-{t}-{i}"),
                            "noop",
                            None,
                            None,
                            None,
                            None,
                        );
                        let _ =
                            store.record_dispatch(&format!("req-{t}-{i}"), "api", &bundle, "actor");
                    } else if i % 3 == 1 {
                        let _ = store.list_dispatches(100);
                    } else {
                        let _ = store.cost_summary();
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let dispatches = store.list_dispatches(10000).unwrap();
    let expected_writes =
        thread_count * (ops_per_thread / 3 + if ops_per_thread % 3 > 0 { 1 } else { 0 });
    assert!(
        dispatches.len() <= expected_writes,
        "Got {} dispatches, expected at most {}",
        dispatches.len(),
        expected_writes,
    );
}

#[test]
fn data_integrity_after_concurrent_writes() {
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
                    let dispatch_id = format!("integ-t{t}-{i}");
                    let bundle = make_bundle_with_usage(
                        &dispatch_id,
                        "provider",
                        Some(100),
                        Some(50),
                        Some(0.001),
                        Some(100),
                    );
                    store
                        .record_dispatch(&format!("request-{dispatch_id}"), "api", &bundle, "actor")
                        .unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let dispatches = store.list_dispatches(1000).unwrap();
    assert_eq!(dispatches.len(), thread_count * writes_per_thread);

    let summary = store.cost_summary().unwrap();
    assert_eq!(summary["dispatch_count"], thread_count * writes_per_thread);
    assert_eq!(
        summary["total_input_tokens"],
        (thread_count * writes_per_thread * 100) as i64
    );
    assert_eq!(
        summary["total_output_tokens"],
        (thread_count * writes_per_thread * 50) as i64
    );

    for d in &dispatches {
        assert_eq!(d["executor_type"], "provider");
        assert_eq!(d["input_tokens"], 100);
        assert_eq!(d["output_tokens"], 50);
        assert!(d["dispatch_id"].as_str().unwrap().starts_with("integ-t"));
    }

    let integrity = store.check_integrity().unwrap();
    assert_eq!(integrity.status, "ok");
}

#[test]
fn concurrent_dispatch_read_by_id() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());

    for i in 0..10 {
        let bundle = make_bundle_with_usage(&format!("d{i}"), "noop", Some(i), None, None, None);
        store
            .record_dispatch(&format!("req-{i}"), "api", &bundle, "actor")
            .unwrap();
    }

    let thread_count = 6;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|_t| {
            let store = store.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                for i in 0..10 {
                    let result = store.get_dispatch(&format!("d{i}")).unwrap();
                    assert!(result.is_some());
                    let dispatch = result.unwrap();
                    assert_eq!(dispatch["dispatch_id"], format!("d{i}"));
                    assert_eq!(dispatch["input_tokens"], i as i64);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_new_with_clock_deterministic_timestamps() {
    let dir = tempdir().unwrap();
    let fixed_time = "2030-01-01T00:00:00Z";
    let clock = || fixed_time.to_string();
    let store = LocalProductStore::new_with_clock(dir.path().join("clock.db"), clock).unwrap();

    let bundle = json!({
        "record": {"dispatch_id": "clock-test", "final_status": "ok"},
        "decision": {"selected_tier": "noop", "budget_reservation": {"reserved_cost": 0.0}},
        "analysis": {"risk_level": "low"}
    });
    let result = store
        .record_dispatch("{}", "cli", &bundle, "tester")
        .unwrap();
    assert_eq!(result["created_at"], fixed_time);

    let audit = store
        .append_audit("tester", "test.action", "res", &json!({}))
        .unwrap();
    assert_eq!(audit["created_at"], fixed_time);

    let export = store.export_snapshot("noop", false).unwrap();
    assert_eq!(export["generated_at"], fixed_time);
}

#[test]
fn test_list_api_key_metadata() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("keys.db")).unwrap();

    // Empty list initially
    let keys = store.list_api_key_metadata(100).unwrap();
    assert!(keys.is_empty());

    // Record two keys
    store
        .record_api_key_metadata(
            "key_1",
            "user_a",
            "admin",
            &["dispatch:read".to_string()],
            "admin",
        )
        .unwrap();
    store
        .record_api_key_metadata(
            "key_2",
            "user_b",
            "readonly",
            &["team:read".to_string(), "audit:read".to_string()],
            "admin",
        )
        .unwrap();

    let keys = store.list_api_key_metadata(100).unwrap();
    assert_eq!(keys.len(), 2);
    // Ordered by created_at DESC (both have same timestamp, so order is insertion order)
    let key_ids: Vec<&str> = keys.iter().map(|k| k["key_id"].as_str().unwrap()).collect();
    assert!(key_ids.contains(&"key_1"));
    assert!(key_ids.contains(&"key_2"));

    // Verify fields present
    for key in &keys {
        assert!(key["user_id"].as_str().is_some());
        assert!(key["role"].as_str().is_some());
        assert!(key["scopes"].as_array().is_some());
        assert!(key["created_at"].as_str().is_some());
        assert!(key["created_by"].as_str().is_some());
    }

    // Limit works
    let keys = store.list_api_key_metadata(1).unwrap();
    assert_eq!(keys.len(), 1);
}

// ---------------------------------------------------------------------------
// GA-1: Artifact Ignore + Persisted Diff
// ---------------------------------------------------------------------------

fn setup_workspace_with_target(
    store: &LocalProductStore,
    dir: &tempfile::TempDir,
    run_id: &str,
    target_files: Vec<(&str, &str)>,
) -> (String, String) {
    let target_dir = dir.path().join(format!("target_{run_id}"));
    std::fs::create_dir_all(&target_dir).unwrap();
    for (name, content) in &target_files {
        let path = target_dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let ws_path = store
        .create_workspace_directory(run_id, target_dir.to_str().unwrap())
        .unwrap();
    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-ga1",
                "run_id": run_id,
                "target_id": "ga1-target",
                "target_repo_path": target_dir.to_string_lossy(),
                "workspace_path": &ws_path,
                "source_revision": "abc123",
                "status": "workspace_created",
            }),
            "operator",
        )
        .unwrap();
    let ws_id = workspace["workspace_id"].as_str().unwrap().to_string();
    (ws_path, ws_id)
}

#[test]
fn ga1_capture_excludes_target_dir() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let (ws_path, ws_id) = setup_workspace_with_target(
        &store,
        &dir,
        "run-ga1-target",
        vec![
            ("src/lib.rs", "fn main() {}\n"),
            ("Cargo.toml", "[package]\n"),
        ],
    );

    // Simulate build: add target/ with artifacts
    let target_build = std::path::PathBuf::from(&ws_path).join("target");
    std::fs::create_dir_all(target_build.join("debug")).unwrap();
    std::fs::write(target_build.join("debug").join("app"), "binary\x00data").unwrap();

    // Also add a real source change
    std::fs::write(
        std::path::PathBuf::from(&ws_path)
            .join("src")
            .join("lib.rs"),
        "fn main() { println!(\"hello\"); }\n",
    )
    .unwrap();

    let artifact = store.capture_patch(&ws_id, "operator").unwrap();
    let changed: Vec<&str> = artifact["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    // Core assertion: target/ build artifacts must not appear in patch
    assert!(
        changed.iter().all(|f| !f.contains("target/")),
        "target/ should be excluded from changed_files: {:?}",
        changed
    );
    // Verify the patch is non-empty (at least the source change was detected)
    assert!(
        !changed.is_empty(),
        "changed_files should not be empty after modifying src/lib.rs"
    );
}

#[test]
fn ga1_capture_excludes_node_modules() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let (ws_path, ws_id) = setup_workspace_with_target(
        &store,
        &dir,
        "run-ga1-nm",
        vec![("index.js", "console.log('hi');\n")],
    );

    // Add node_modules
    let nm = std::path::PathBuf::from(&ws_path).join("node_modules");
    std::fs::create_dir_all(&nm).unwrap();
    std::fs::write(nm.join("pkg.js"), "module.exports = {};").unwrap();

    // Add real change
    std::fs::write(
        std::path::PathBuf::from(&ws_path).join("index.js"),
        "console.log('hello');\n",
    )
    .unwrap();

    let artifact = store.capture_patch(&ws_id, "operator").unwrap();
    let changed: Vec<&str> = artifact["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    assert!(
        changed.iter().all(|f| !f.contains("node_modules")),
        "node_modules should be excluded: {:?}",
        changed
    );
}

#[test]
fn ga1_capture_excludes_binary_files() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let (ws_path, ws_id) = setup_workspace_with_target(
        &store,
        &dir,
        "run-ga1-bin",
        vec![("src/main.rs", "fn main() {}\n")],
    );

    // Add a binary file
    std::fs::write(
        std::path::PathBuf::from(&ws_path).join("image.png"),
        b"\x89PNG\r\n\x1a\n\x00binary\x00data",
    )
    .unwrap();

    // Add a real change
    std::fs::write(
        std::path::PathBuf::from(&ws_path)
            .join("src")
            .join("main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();

    let artifact = store.capture_patch(&ws_id, "operator").unwrap();
    let changed: Vec<&str> = artifact["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    assert!(
        changed.iter().all(|f| !f.contains("image.png")),
        "binary files should be excluded: {:?}",
        changed
    );
}

#[test]
fn ga1_capture_persists_review_diff() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let (ws_path, ws_id) = setup_workspace_with_target(
        &store,
        &dir,
        "run-ga1-diff",
        vec![("src/lib.rs", "fn add(a: i32, b: i32) -> i32 { a + b }\n")],
    );

    // Add a new file and modify existing
    std::fs::write(
        std::path::PathBuf::from(&ws_path)
            .join("src")
            .join("greeting.rs"),
        "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n",
    )
    .unwrap();
    std::fs::write(
        std::path::PathBuf::from(&ws_path)
            .join("src")
            .join("lib.rs"),
        "fn add(a: i32, b: i32) -> i32 { a + b }\npub mod greeting;\n",
    )
    .unwrap();

    let artifact = store.capture_patch(&ws_id, "operator").unwrap();

    let review_diff = artifact["review_diff"].as_str().unwrap();
    assert!(
        !review_diff.is_empty(),
        "review_diff should be persisted and non-empty"
    );
    assert!(
        review_diff.contains("+++ b/src/greeting.rs"),
        "review_diff should contain added file header"
    );
    assert!(
        review_diff.contains("pub fn greet"),
        "review_diff should contain added file content"
    );
    assert!(
        review_diff.contains("--- a/src/lib.rs"),
        "review_diff should contain modified file header"
    );
}

#[test]
fn ga1_review_diff_survives_artifact_read() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let (ws_path, ws_id) = setup_workspace_with_target(
        &store,
        &dir,
        "run-ga1-persist",
        vec![("README.md", "# Test\n")],
    );

    std::fs::write(
        std::path::PathBuf::from(&ws_path).join("README.md"),
        "# Test Project\n\nUpdated.\n",
    )
    .unwrap();

    let artifact = store.capture_patch(&ws_id, "operator").unwrap();
    let art_id = artifact["artifact_id"].as_str().unwrap();

    // Read artifact back from store
    let stored = store
        .get_supervised_patch_artifact(art_id)
        .unwrap()
        .unwrap();
    let stored_diff = stored["review_diff"].as_str().unwrap();
    assert!(
        stored_diff.contains("--- a/README.md"),
        "persisted artifact should contain review_diff"
    );
}

#[test]
fn supervised_patch_integrity_hash_binds_file_content() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let (workspace_path, workspace_id) = setup_workspace_with_target(
        &store,
        &dir,
        "run-content-integrity",
        vec![("README.md", "base\n")],
    );
    let readme = std::path::PathBuf::from(&workspace_path).join("README.md");
    std::fs::write(&readme, "approved content\n").unwrap();
    let artifact = store.capture_patch(&workspace_id, "operator").unwrap();
    let artifact_id = artifact["artifact_id"].as_str().unwrap();

    let before = store.validate_artifact_integrity(artifact_id).unwrap();
    assert_eq!(before["integrity_ok"], true);

    std::fs::write(&readme, "tampered content\n").unwrap();
    let after = store.validate_artifact_integrity(artifact_id).unwrap();
    assert_eq!(after["integrity_ok"], false);
    assert!(after["checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| { check["check"] == "patch_hash_unchanged" && check["passed"] == false }));
}

#[test]
fn supervised_patch_capture_binds_recorded_command_verification() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let (workspace_path, workspace_id) = setup_workspace_with_target(
        &store,
        &dir,
        "run-command-verification",
        vec![("README.md", "base\n")],
    );
    let evidence = json!({
        "schema_version": "workspace_verification.v1",
        "status": "evidence_recorded",
        "command": ["cargo", "test"],
        "result_status": "completed",
        "attempt": 1,
    });

    let workspace = store
        .record_workspace_verification(&workspace_id, &evidence, "operator")
        .unwrap();
    assert_eq!(workspace["verification"], evidence);
    assert_eq!(
        workspace["verification_execution_authority"],
        "allowlisted_commands"
    );

    std::fs::write(
        std::path::PathBuf::from(&workspace_path).join("README.md"),
        "verified change\n",
    )
    .unwrap();
    let artifact = store.capture_patch(&workspace_id, "operator").unwrap();
    assert_eq!(
        artifact["evidence_bundle"]["verification"]["command"],
        json!(["cargo", "test"])
    );
    assert_eq!(
        artifact["evidence_bundle"]["verification"]["status"],
        "evidence_recorded"
    );
}

#[test]
fn ga1_capture_blocks_secret_content_from_artifact_and_response() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let (ws_path, ws_id) = setup_workspace_with_target(
        &store,
        &dir,
        "run-ga1-secret",
        vec![("README.md", "# Test\n")],
    );

    std::fs::write(
        std::path::PathBuf::from(&ws_path).join("leak.txt"),
        "api_key = sk-should-not-appear\n",
    )
    .unwrap();

    let artifact = store.capture_patch(&ws_id, "operator").unwrap();
    let rendered = artifact.to_string();

    assert_eq!(artifact["redaction_status"], "failed");
    assert!(artifact["review_diff"]
        .as_str()
        .unwrap()
        .contains("suppressed"));
    assert!(!rendered.contains("sk-should-not-appear"));

    let stored = store
        .get_supervised_patch_artifact(artifact["artifact_id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let stored_rendered = stored.to_string();
    assert!(!stored_rendered.contains("sk-should-not-appear"));
}

#[test]
fn context_assembly_disabled_env_no_injection() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("ACP_CONTEXT_ASSEMBLY_ENABLED", "0");
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_disabled.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx disabled", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    let first = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(first["node_id"], "node-a");

    let second = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(second["node_id"], "node-b");
    let output = second["result"]["output"].as_str().unwrap();
    let injection: Value = serde_json::from_str(output).unwrap();
    assert!(
        injection.is_null(),
        "context_injection should be Null when disabled"
    );

    std::env::remove_var("ACP_CONTEXT_ASSEMBLY_ENABLED");
}

#[test]
fn context_assembly_multiple_predecessors_all_included() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_multi.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx multi", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_three_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();

    let third = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(third["node_id"], "node-c");
    let output = third["result"]["output"].as_str().unwrap();
    let injection: Value = serde_json::from_str(output).unwrap();
    assert_eq!(injection["schema_version"], "context_injection.v1");

    let sources = injection["sources"].as_array().unwrap();
    assert_eq!(
        sources.len(),
        2,
        "should have 2 sources from two predecessors"
    );
    let from_ids: Vec<&str> = sources
        .iter()
        .map(|s| s["from_node_id"].as_str().unwrap())
        .collect();
    assert!(from_ids.contains(&"node-a"), "should include node-a");
    assert!(from_ids.contains(&"node-b"), "should include node-b");
}

#[test]
fn context_assembly_over_budget_truncation() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("ACP_CONTEXT_ASSEMBLY_MAX_TOKENS", "5");
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_trunc.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx trunc", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let large_executor = LargeOutputExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &large_executor)
        .unwrap();

    let echo = ContextEchoExecutor;
    let second = store
        .tick_with_executor("run-0001", "actor", 0, &echo)
        .unwrap();
    assert_eq!(second["node_id"], "node-b");
    let output = second["result"]["output"].as_str().unwrap();
    let injection: Value = serde_json::from_str(output).unwrap();

    let source = &injection["sources"][0];
    assert_eq!(source["truncated"], true, "source should be truncated");
    let included = source["included_tokens"].as_u64().unwrap();
    let estimated = source["estimated_tokens"].as_u64().unwrap();
    assert!(
        included < estimated,
        "included_tokens ({included}) should be less than estimated_tokens ({estimated})"
    );
    assert_eq!(
        injection["truncated"], true,
        "top-level truncated should be true"
    );

    std::env::remove_var("ACP_CONTEXT_ASSEMBLY_MAX_TOKENS");
}

#[test]
fn context_assembly_no_predecessor_no_injection() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_nopred.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx nopred", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_no_predecessor(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    let tick = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(tick["node_id"], "node-x");
    let output = tick["result"]["output"].as_str().unwrap();
    let injection: Value = serde_json::from_str(output).unwrap();
    assert!(
        injection.is_null(),
        "no predecessors means no context_injection"
    );
}

#[test]
fn context_assembly_injects_agent_memory_for_agent_step_metadata_only() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("ACP_CONTEXT_ASSEMBLY_ENABLED", "1");
    std::env::set_var("ACP_CONTEXT_ASSEMBLY_MAX_TOKENS", "32");
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_agent_memory.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx agent memory", "api", "actor", |ids, _| {
            Ok(make_agent_step_memory_plan(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();
    store
        .create_durable_memory(
            &DurableMemoryCreate {
                scope: MemoryScope {
                    tenant_id: "local".into(),
                    workspace_id: "local".into(),
                    agent_id: Some("agent-memory".into()),
                    task_id: None,
                },
                run_id: Some("run-before-0001".into()),
                source_id: "source-before-0001".into(),
                source_sha256: "a".repeat(64),
                conflict_key: "agent-node-memory-fact".into(),
                content: json!({"text":"agent node memory durable fact"}),
                confidence: 0.9,
                fresh_until: None,
                expires_at: None,
                supersedes_memory_id: None,
            },
            "actor",
        )
        .unwrap();
    store
        .update_agent_state(
            "agent-memory",
            "run-0001",
            Some("idle"),
            None,
            Some("raw objective must not leak"),
            Some(&json!({
                "memory_digest": {
                    "source_refs": [
                        "agent_state:run-0001:agent-memory:scratchpad_summary",
                        "/home/igzela/private/repo.rs"
                    ],
                    "expiry_policy": "forever",
                    "conflict_resolution": "append_raw",
                    "summary": "remember bounded progress with sk-test-secret-token"
                }
            })),
        )
        .unwrap();

    let tick = store
        .tick_with_executor("run-0001", "actor", 0, &AgentContextEchoExecutor)
        .unwrap();
    assert_eq!(tick["node_id"], "agent-node-memory");
    let output = tick["result"]["output"].as_str().unwrap();
    let injection: Value = serde_json::from_str(output).unwrap();
    assert_eq!(injection["schema_version"], "context_injection.v1");
    assert_eq!(injection["target_node_id"], "agent-node-memory");
    assert_eq!(injection["injection_surface"], "node_metadata_only");
    assert!(injection["sources"].as_array().unwrap().is_empty());
    assert_eq!(
        injection["memory_context"]["memory_digest"]["source_refs"],
        json!(["agent_state:run-0001:agent-memory:scratchpad_summary"])
    );
    assert!(
        injection["memory_context"]["included_tokens"]
            .as_i64()
            .unwrap()
            <= 32
    );
    assert_eq!(
        injection["memory_context"]["retrieved_references"][0]["source_id"], "source-before-0001",
        "unexpected scheduler memory injection: {injection}"
    );
    assert_eq!(
        injection["memory_context"]["retrieved_references"][0]["source_sha256"],
        "a".repeat(64)
    );
    assert_eq!(
        injection["total_estimated_tokens"],
        injection["memory_context"]["estimated_tokens"]
    );
    let rendered = injection.to_string();
    assert!(!rendered.contains("raw objective"));
    assert!(!rendered.contains("/home/igzela"));
    assert!(!rendered.contains("sk-test-secret-token"));

    std::env::remove_var("ACP_CONTEXT_ASSEMBLY_ENABLED");
    std::env::remove_var("ACP_CONTEXT_ASSEMBLY_MAX_TOKENS");
}

#[test]
fn agent_step_rejects_non_agent_executor_at_store_tick_boundary() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("agent_executor_guard.db")).unwrap();
    let plan = store
        .create_workflow_plan("agent executor guard", "api", "actor", |ids, _| {
            Ok(make_agent_step_memory_plan(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let tick = store
        .tick_with_executor(
            "run-0001",
            "actor",
            0,
            &engine::node_executor::NoopNodeExecutor,
        )
        .unwrap();

    assert_eq!(tick["result"]["status"], "failed");
    assert_eq!(tick["result"]["error_domain"], "reserved_executor_mismatch");
}

#[test]
fn context_assembly_shares_budget_between_predecessors_and_agent_memory() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("ACP_CONTEXT_ASSEMBLY_ENABLED", "1");
    std::env::set_var("ACP_CONTEXT_ASSEMBLY_MAX_TOKENS", "4");
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_agent_memory_budget.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx shared budget", "api", "actor", |ids, _| {
            let mut plan = make_workflow_plan_with_nodes(ids);
            let node = plan["graph"]["nodes"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|node| node["node_id"] == "node-b")
                .unwrap()
                .as_object_mut()
                .unwrap();
            node.insert("task_type".to_string(), json!("agent_step"));
            node.insert("agent_id".to_string(), json!("agent-memory"));
            node.insert("assigned_agent_id".to_string(), json!("agent-memory"));
            node.insert("agent_role".to_string(), json!("implementer"));
            node.insert(
                "agent_objective".to_string(),
                json!("bounded shared context fixture"),
            );
            node.insert("profile_id".to_string(), json!("bounded"));
            node.insert("capability_profile".to_string(), json!(["code"]));
            node.insert("decision_source".to_string(), json!("fixture"));
            node.insert("max_actions".to_string(), json!(1));
            plan["boundaries"] = json!({
                "execution_authority": "rust_scheduler_only",
                "provider_calls": "default_off",
                "target_repository_writes": "disabled",
                "runtime_workers": "env_gated_supervised"
            });
            Ok(plan)
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();
    store
        .update_agent_state(
            "agent-memory",
            "run-0001",
            Some("idle"),
            None,
            None,
            Some(&json!({
                "memory_digest": {
                    "source_refs": ["agent_state:run-0001:agent-memory:scratchpad_summary"],
                    "summary": "0123456789abcdef0123456789abcdef"
                }
            })),
        )
        .unwrap();

    store
        .tick_with_executor("run-0001", "actor", 0, &LargeOutputExecutor)
        .unwrap();
    let tick = store
        .tick_with_executor("run-0001", "actor", 0, &AgentContextEchoExecutor)
        .unwrap();
    let injection: Value =
        serde_json::from_str(tick["result"]["output"].as_str().unwrap()).unwrap();
    let predecessor_tokens: u64 = injection["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["included_tokens"].as_u64().unwrap())
        .sum();
    let memory_tokens = injection["memory_context"]["included_tokens"]
        .as_u64()
        .unwrap();
    assert!(
        predecessor_tokens + memory_tokens <= 4,
        "combined context must stay within the shared budget"
    );

    std::env::remove_var("ACP_CONTEXT_ASSEMBLY_ENABLED");
    std::env::remove_var("ACP_CONTEXT_ASSEMBLY_MAX_TOKENS");
}

#[test]
fn context_assembly_preserves_existing_input() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_preserve.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx preserve", "api", "actor", |ids, _| {
            let mut plan = make_workflow_plan_with_nodes(ids);
            let node_b = plan["graph"]["nodes"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|n| n["node_id"] == "node-b")
                .unwrap();
            node_b.as_object_mut().unwrap().insert(
                "input".to_string(),
                json!({"existing_key": "existing_value"}),
            );
            Ok(plan)
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();

    let preserving = ContextPreservingExecutor;
    let second = store
        .tick_with_executor("run-0001", "actor", 0, &preserving)
        .unwrap();
    assert_eq!(second["node_id"], "node-b");
    let output = second["result"]["output"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(output).unwrap();
    assert_eq!(parsed["has_context_injection"], true);
    assert_eq!(
        parsed["original_input"],
        json!({"existing_key": "existing_value"}),
        "existing input should not be overwritten by context injection"
    );
}

#[test]
fn context_assembly_edge_field_mapping() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_mapping.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx mapping", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_with_field_mapping(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();

    let second = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(second["node_id"], "node-b");
    let output = second["result"]["output"].as_str().unwrap();
    let injection: Value = serde_json::from_str(output).unwrap();
    let source = &injection["sources"][0];
    let decisions = source["mapping_decisions"].as_array().unwrap();
    assert!(
        decisions
            .iter()
            .any(|d| d.as_str().unwrap().contains("analysis_result")),
        "mapping_decisions should reference the field_mapping, got: {decisions:?}"
    );
}

#[test]
fn context_assembly_missing_mapping_fallback() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_nomapping.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx nomapping", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();

    let second = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(second["node_id"], "node-b");
    let output = second["result"]["output"].as_str().unwrap();
    let injection: Value = serde_json::from_str(output).unwrap();
    let source = &injection["sources"][0];
    let decisions = source["mapping_decisions"].as_array().unwrap();
    assert_eq!(
        decisions[0].as_str().unwrap(),
        "default_passthrough",
        "no field_mapping should yield default_passthrough"
    );
    assert!(
        !source["output"].is_null(),
        "passthrough should include full predecessor output"
    );
}

#[test]
fn context_assembly_failed_predecessor_not_injected() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_failpred.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx failpred", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_three_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();

    let fail = engine::node_executor::FailNodeExecutor::default();
    store
        .tick_with_executor("run-0001", "actor", 0, &fail)
        .unwrap();

    let result = store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    assert_eq!(
        result["action"].as_str().unwrap(),
        "no_ready_node",
        "node-c should not be ready when predecessor node-b failed"
    );
}

#[test]
fn context_assembly_does_not_alter_provider_cli_fields() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_nocli.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx nocli", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();

    let preserving = ContextPreservingExecutor;
    let second = store
        .tick_with_executor("run-0001", "actor", 0, &preserving)
        .unwrap();
    let output = second["result"]["output"].as_str().unwrap();
    let parsed: Value = serde_json::from_str(output).unwrap();
    assert_eq!(parsed["has_context_injection"], true);

    let run = store.get_workflow_run("run-0001").unwrap().unwrap();
    let node_b = run["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["node_id"] == "node-b")
        .unwrap();
    let meta_keys: Vec<&str> = node_b
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    for forbidden in &["provider_type", "cli_binary", "auth_token", "api_key"] {
        assert!(
            !meta_keys.contains(forbidden),
            "node_metadata should not contain {forbidden}, found keys: {meta_keys:?}"
        );
    }
}

#[test]
fn context_assembly_persisted_in_node_metadata() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("ctx_persist.db")).unwrap();
    let plan = store
        .create_workflow_plan("ctx persist", "api", "actor", |ids, _| {
            Ok(make_workflow_plan_with_nodes(ids))
        })
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();

    let run = store.get_workflow_run("run-0001").unwrap().unwrap();
    let node_b = run["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["node_id"] == "node-b")
        .unwrap();
    let output = node_b["result"]["output"]
        .as_str()
        .expect("node-b result should have output field");
    let injection: Value = serde_json::from_str(output).unwrap();
    assert_eq!(
        injection["schema_version"], "context_injection.v1",
        "context_injection should be persisted in node-b result output"
    );
    assert_eq!(injection["target_node_id"], "node-b");
    assert_eq!(injection["sources"][0]["from_node_id"], "node-a");
}

#[test]
fn context_assembly_persisted_in_node_json_directly() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let plan = store
        .create_workflow_plan(
            "Plan persistence check",
            "api",
            "actor",
            |ids, _created_at| Ok(make_workflow_plan_with_nodes(ids)),
        )
        .unwrap();
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();

    let executor = ContextEchoExecutor;
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();
    store
        .tick_with_executor("run-0001", "actor", 0, &executor)
        .unwrap();

    let run = store.get_workflow_run("run-0001").unwrap().unwrap();
    let node_b = run["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["node_id"] == "node-b")
        .unwrap();

    let ci = node_b
        .get("context_injection")
        .expect("node-b node_json must have context_injection field persisted");
    assert_eq!(ci["schema_version"], "context_injection.v1");
    assert_eq!(ci["target_node_id"], "node-b");
    assert_eq!(ci["sources"][0]["from_node_id"], "node-a");
    assert_eq!(ci["injection_surface"], "node_metadata_only");
}
