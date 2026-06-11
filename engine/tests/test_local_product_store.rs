use engine::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
use engine::provider::audit::{
    ProviderAuditEvent, ProviderAuditRecorder, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION,
};
use engine::read_only_planner::ReadOnlyPlanner;
use engine::storage::local_product_store::LocalProductStore;
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
    assert_eq!(run["boundaries"]["runtime_workers"], "disabled");
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
