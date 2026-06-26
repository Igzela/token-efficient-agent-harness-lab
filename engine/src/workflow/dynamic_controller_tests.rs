use serde_json::{json, Value};

use super::dynamic_controller::*;
use crate::node_executor::{FailNodeExecutor, NoopNodeExecutor};
use crate::storage::local_product_store::LocalProductStore;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_store() -> LocalProductStore {
    LocalProductStore::new(":memory:").expect("in-memory store")
}

/// Create a plan with 2 nodes (n1 -> n2) and return (store, run_id).
fn setup_two_node_run() -> (LocalProductStore, String) {
    let store = new_store();
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "n1", "task_type": "analyze", "status": "pending"},
                        {"node_id": "n2", "task_type": "execute", "status": "pending"}
                    ],
                    "edges": [
                        {"edge_id": "e1", "from_node_id": "n1", "to_node_id": "n2", "edge_type": "dependency"}
                    ]
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run
        .get("run_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    (store, run_id)
}

/// Create a single-node plan (no edges) and return (store, run_id).
fn setup_single_node_run() -> (LocalProductStore, String) {
    let store = new_store();
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "only", "task_type": "analyze", "status": "pending"}
                    ],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run
        .get("run_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    (store, run_id)
}

#[allow(dead_code)]
fn default_config() -> DynamicControllerConfig {
    DynamicControllerConfig::default()
}

fn no_auto_fix_config() -> DynamicControllerConfig {
    DynamicControllerConfig {
        auto_fix_on_failure: false,
        ..Default::default()
    }
}

fn run_status(store: &LocalProductStore, run_id: &str) -> String {
    store
        .get_workflow_run(run_id)
        .expect("get run")
        .expect("run exists")
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn run_nodes(store: &LocalProductStore, run_id: &str) -> Vec<Value> {
    store
        .get_workflow_run(run_id)
        .expect("get run")
        .expect("run exists")
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn find_node<'a>(nodes: &'a [Value], node_id: &str) -> Option<&'a Value> {
    nodes
        .iter()
        .find(|n| n.get("node_id").and_then(Value::as_str) == Some(node_id))
}

fn node_db_status(node: &Value) -> &str {
    node.get("db_status")
        .and_then(Value::as_str)
        .or_else(|| node.get("status").and_then(Value::as_str))
        .unwrap_or("unknown")
}

fn run_events(store: &LocalProductStore, run_id: &str) -> Vec<Value> {
    store
        .get_workflow_run(run_id)
        .expect("get run")
        .expect("run exists")
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

#[test]
fn test_controller_tick_executes_ready_node() {
    let (store, run_id) = setup_two_node_run();
    let mut ctrl = DynamicWorkflowController::new(no_auto_fix_config());
    let executor = NoopNodeExecutor;

    let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    assert!(
        result.actions.iter().any(
            |a| matches!(a, ControllerAction::NodeExecuted { node_id, .. } if node_id == "n1")
        ),
        "first tick should execute n1"
    );

    let nodes = run_nodes(&store, &run_id);
    let n1 = find_node(&nodes, "n1").expect("n1 exists");
    assert_eq!(node_db_status(n1), "completed");
}

#[test]
fn test_controller_tick_returns_no_action_when_no_ready_nodes() {
    let store = new_store();
    // Two independent nodes, both already completed via separate ticks
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "a", "task_type": "t", "status": "pending"},
                        {"node_id": "b", "task_type": "t", "status": "pending"}
                    ],
                    "edges": [
                        {"edge_id": "e1", "from_node_id": "a", "to_node_id": "b", "edge_type": "dependency"}
                    ]
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Tick both nodes to completion using the store's tick directly
    store
        .tick_with_executor(run_id, "test", 0, &NoopNodeExecutor)
        .unwrap();
    store
        .tick_with_executor(run_id, "test", 0, &NoopNodeExecutor)
        .unwrap();

    // Run should now be completed (terminal)
    let mut ctrl = DynamicWorkflowController::new(no_auto_fix_config());
    let executor = NoopNodeExecutor;

    let result = ctrl.tick(&store, run_id, "test", &executor).expect("tick");
    assert!(!result.should_continue);
    assert!(result.actions.iter().any(|a| matches!(
        a,
        ControllerAction::NoAction { reason } if reason.contains("terminal")
    )));
}

#[test]
fn test_controller_completes_run_when_all_nodes_done() {
    let (store, run_id) = setup_two_node_run();
    let mut ctrl = DynamicWorkflowController::new(no_auto_fix_config());
    let executor = NoopNodeExecutor;

    // Tick until the run is terminal
    for _ in 0..10 {
        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        if !result.should_continue {
            assert_eq!(
                result.run_status, "completed",
                "run should be completed, got: {}",
                result.run_status
            );
            assert!(result
                .actions
                .iter()
                .any(|a| matches!(a, ControllerAction::RunCompleted)));

            // Verify store state
            let status = run_status(&store, &run_id);
            assert_eq!(status, "completed");
            return;
        }
    }
    panic!("run did not complete within 10 ticks");
}

#[test]
fn test_controller_fails_run_on_node_failure() {
    let store = new_store();
    // Single node with max_retries=0 so it fails immediately
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "fragile", "task_type": "test", "status": "pending"}
                    ],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    let config = DynamicControllerConfig {
        auto_fix_on_failure: false,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);
    let fail_executor = FailNodeExecutor::default();

    // With max_retries=0 passed to tick, the node should fail and the run should
    // transition to failed. The controller uses 0 for max_retries in
    // tick_with_executor_and_command.
    let result = ctrl
        .tick(&store, run_id, "test", &fail_executor)
        .expect("tick");

    // The run should be failed (terminal)
    assert_eq!(result.run_status, "failed");
    assert!(!result.should_continue);
    assert!(result.actions.iter().any(
        |a| matches!(a, ControllerAction::NodeExecuted { node_id, .. } if node_id == "fragile")
    ));
}

#[test]
fn test_controller_creates_fix_nodes_on_failure() {
    let store = new_store();
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "fragile", "task_type": "test", "status": "pending"},
                        {"node_id": "leaf", "task_type": "observe", "status": "pending"}
                    ],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Manually mark "fragile" as failed to simulate a node failure,
    // then run the controller tick which should create fix nodes.
    store
        .update_workflow_node_status(run_id, "fragile", "failed", "test", "simulated failure")
        .unwrap();

    let config = DynamicControllerConfig {
        auto_fix_on_failure: true,
        max_mutations_per_run: 10,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);
    let executor = NoopNodeExecutor;

    let result = ctrl.tick(&store, run_id, "test", &executor).expect("tick");

    // Should have a GraphMutated action for the auto-fix
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, ControllerAction::GraphMutated { mutation_type, .. } if mutation_type == "add_node")),
        "expected GraphMutated action, got: {:?}",
        result.actions
    );

    // Verify fix node and test node were created
    let nodes = run_nodes(&store, run_id);
    assert!(
        find_node(&nodes, "fix-fragile").is_some(),
        "fix-fragile node should exist"
    );
    assert!(
        find_node(&nodes, "test-fix-fragile").is_some(),
        "test-fix-fragile node should exist"
    );
}

#[test]
fn test_controller_records_mutation_events() {
    let store = new_store();
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "broken", "task_type": "test", "status": "pending"}
                    ],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .update_workflow_node_status(run_id, "broken", "failed", "test", "simulated failure")
        .unwrap();

    let config = DynamicControllerConfig {
        auto_fix_on_failure: true,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);
    let executor = NoopNodeExecutor;

    ctrl.tick(&store, run_id, "test", &executor).expect("tick");

    let events = run_events(&store, run_id);
    let mutation_events: Vec<&Value> = events
        .iter()
        .filter(|e| {
            e.get("event_type")
                .and_then(Value::as_str)
                .map(|t| t.starts_with("dag.mutation."))
                .unwrap_or(false)
        })
        .collect();

    assert!(
        !mutation_events.is_empty(),
        "expected dag.mutation.* events to be recorded"
    );

    // Verify at least one node_added event exists
    assert!(
        mutation_events
            .iter()
            .any(|e| e.get("event_type").and_then(Value::as_str) == Some("dag.mutation.node_added")),
        "expected dag.mutation.node_added event"
    );
}

#[test]
fn test_controller_respects_mutation_limit() {
    let store = new_store();
    // Create a plan with multiple nodes that will all fail
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "f1", "task_type": "t", "status": "pending"},
                        {"node_id": "f2", "task_type": "t", "status": "pending"},
                        {"node_id": "f3", "task_type": "t", "status": "pending"}
                    ],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Mark all nodes as failed
    for nid in &["f1", "f2", "f3"] {
        store
            .update_workflow_node_status(run_id, nid, "failed", "test", "simulated failure")
            .unwrap();
    }

    // Set max_mutations_per_run = 4: one fix node's batch (4 proposals) fits,
    // but the second fix node's batch (4 more) would exceed the limit (4 >= 4).
    let config = DynamicControllerConfig {
        auto_fix_on_failure: true,
        max_mutations_per_run: 4,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);
    let executor = NoopNodeExecutor;

    let _result = ctrl.tick(&store, run_id, "test", &executor).expect("tick");

    // Exactly 4 mutations from the first fix batch, not 12 (4*3)
    assert!(
        ctrl.mutations_applied_total() <= 4,
        "total mutations should respect the limit, got: {}",
        ctrl.mutations_applied_total()
    );

    // Not all failed nodes should have fix nodes (only the first one processed)
    let nodes = run_nodes(&store, run_id);
    let has_fix_f1 = find_node(&nodes, "fix-f1").is_some();
    let has_fix_f2 = find_node(&nodes, "fix-f2").is_some();
    let has_fix_f3 = find_node(&nodes, "fix-f3").is_some();
    let fix_count = [has_fix_f1, has_fix_f2, has_fix_f3]
        .iter()
        .filter(|&&b| b)
        .count();
    assert!(
        fix_count < 3,
        "not all fix nodes should exist due to mutation limit, got {} fixes",
        fix_count
    );
}

#[test]
fn test_controller_returns_should_continue_while_pending() {
    let (store, run_id) = setup_two_node_run();
    let mut ctrl = DynamicWorkflowController::new(no_auto_fix_config());
    let executor = NoopNodeExecutor;

    // First tick: executes n1, n2 is still pending (blocked by dependency)
    let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
    assert!(
        result.should_continue,
        "should_continue should be true when n2 is still pending"
    );
}

#[test]
fn test_controller_should_continue_false_when_done() {
    let (store, run_id) = setup_single_node_run();
    let mut ctrl = DynamicWorkflowController::new(no_auto_fix_config());
    let executor = NoopNodeExecutor;

    // Tick until done (single node -> completes in 1 tick)
    let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
    assert!(
        !result.should_continue,
        "single node run should be done after one tick"
    );
    assert_eq!(result.run_status, "completed");
}

#[test]
fn test_controller_mutation_produces_valid_dag() {
    let store = new_store();
    let plan = store
        .create_workflow_plan("test-req", "test", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "main", "task_type": "test", "status": "pending"}
                    ],
                    "edges": []
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("create plan");

    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test")
        .expect("create run");
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .update_workflow_node_status(run_id, "main", "failed", "test", "simulated failure")
        .unwrap();

    let config = DynamicControllerConfig {
        auto_fix_on_failure: true,
        max_mutations_per_run: 20,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);
    let executor = NoopNodeExecutor;

    ctrl.tick(&store, run_id, "test", &executor).expect("tick");

    // Verify DAG validity: no duplicate node_ids, all edge endpoints exist
    let nodes = run_nodes(&store, run_id);
    let run_data = store
        .get_workflow_run(run_id)
        .expect("get run")
        .expect("run exists");
    let edges = run_data
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // No duplicate node_ids
    let mut node_ids: Vec<&str> = nodes
        .iter()
        .map(|n| n.get("node_id").and_then(Value::as_str).unwrap())
        .collect();
    let before_dedup = node_ids.len();
    node_ids.sort();
    node_ids.dedup();
    assert_eq!(
        node_ids.len(),
        before_dedup,
        "no duplicate node_ids allowed"
    );

    // All edge endpoints reference existing nodes
    let node_id_set: Vec<&str> = nodes
        .iter()
        .map(|n| n.get("node_id").and_then(Value::as_str).unwrap())
        .collect();
    for edge in &edges {
        let from = edge
            .get("from_node_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let to = edge.get("to_node_id").and_then(Value::as_str).unwrap_or("");
        if !from.is_empty() {
            assert!(
                node_id_set.contains(&from),
                "edge references nonexistent from_node: {}",
                from
            );
        }
        if !to.is_empty() {
            assert!(
                node_id_set.contains(&to),
                "edge references nonexistent to_node: {}",
                to
            );
        }
    }

    // No duplicate edge_ids
    let mut edge_ids: Vec<&str> = edges
        .iter()
        .map(|e| e.get("edge_id").and_then(Value::as_str).unwrap())
        .collect();
    let before_edge_dedup = edge_ids.len();
    edge_ids.sort();
    edge_ids.dedup();
    assert_eq!(
        edge_ids.len(),
        before_edge_dedup,
        "no duplicate edge_ids allowed"
    );

    // Integrity check passes
    let report = store.check_integrity().expect("integrity check");
    assert_eq!(report.status, "ok", "DAG should pass integrity check");
}
