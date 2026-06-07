use serde_json::{json, Value};

use super::LocalProductStore;

fn new_test_store() -> LocalProductStore {
    LocalProductStore::new(":memory:").expect("failed to create test store")
}

fn create_test_plan(store: &LocalProductStore) -> Value {
    store
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
                    "created_at": "2026-06-05T00:00:00Z",
                    "updated_at": "2026-06-05T00:00:00Z",
                    "nodes": [
                        {"node_id": "node-a", "task_type": "implementation", "status": "pending"},
                        {"node_id": "node-b", "task_type": "testing", "status": "pending"}
                    ],
                    "edges": [
                        {"edge_id": "edge-ab", "from_node_id": "node-a", "to_node_id": "node-b", "edge_type": "dependency"}
                    ]
                },
                "boundaries": {
                    "execution_authority": "disabled",
                    "target_repository_writes": "disabled",
                    "runtime_workers": "disabled",
                },
            }))
        })
        .expect("failed to create plan")
}

fn run_from_plan(store: &LocalProductStore) -> Value {
    let plan = create_test_plan(store);
    let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
    store
        .create_workflow_run_from_plan(plan_id, "test")
        .unwrap()
}

fn node_ids(run: &Value) -> Vec<String> {
    run.get("nodes")
        .and_then(Value::as_array)
        .unwrap_or(&vec![])
        .iter()
        .map(|n| {
            n.get("node_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

fn edge_ids(run: &Value) -> Vec<String> {
    run.get("edges")
        .and_then(Value::as_array)
        .unwrap_or(&vec![])
        .iter()
        .map(|e| {
            e.get("edge_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

fn mutation_events(run: &Value) -> Vec<Value> {
    run.get("events")
        .and_then(Value::as_array)
        .unwrap_or(&vec![])
        .iter()
        .filter(|e| {
            e.get("event_type")
                .and_then(Value::as_str)
                .map(|t| t.starts_with("dag.mutation."))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

fn find_event(run: &Value, event_type: &str) -> Option<Value> {
    run.get("events")
        .and_then(Value::as_array)
        .unwrap_or(&vec![])
        .iter()
        .find(|e| e.get("event_type").and_then(Value::as_str) == Some(event_type))
        .cloned()
}

#[test]
fn test_insert_workflow_node_success() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "adding review node",
        )
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let ids = node_ids(&updated);
    assert_eq!(ids.len(), 3, "expected 3 nodes after insert");
    assert!(ids.contains(&"node-c".to_string()));
}

#[test]
fn test_insert_workflow_node_duplicate_fails() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    let result = store.insert_workflow_node(
        run_id,
        &json!({"node_id": "node-a", "task_type": "implementation", "status": "pending"}),
        "test",
        "duplicate",
    );
    assert!(result.is_err(), "expected error on duplicate node");
    assert!(
        result.unwrap_err().contains("duplicate"),
        "error should mention duplicate"
    );
}

#[test]
fn test_insert_workflow_node_records_event() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "adding review node",
        )
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let event = find_event(&updated, "dag.mutation.node_added");
    assert!(event.is_some(), "expected dag.mutation.node_added event");
    let event = event.unwrap();
    assert_eq!(
        event
            .get("details")
            .and_then(|d| d.get("node_id"))
            .and_then(Value::as_str),
        Some("node-c")
    );
}

#[test]
fn test_remove_workflow_node_success() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .remove_workflow_node(run_id, "node-b", "test", "removing testing node")
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let ids = node_ids(&updated);
    assert_eq!(ids.len(), 1, "expected 1 node after removal");
    assert!(ids.contains(&"node-a".to_string()));

    let edges = edge_ids(&updated);
    assert_eq!(
        edges.len(),
        0,
        "expected 0 edges after node-b removal (edge-ab connected)"
    );
}

#[test]
fn test_remove_workflow_node_not_found_fails() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Removing a nonexistent node succeeds silently (no rows deleted) but still records event
    let result = store.remove_workflow_node(run_id, "node-nonexistent", "test", "not found");
    assert!(
        result.is_ok(),
        "removing nonexistent node is a no-op, not an error"
    );
}

#[test]
fn test_remove_workflow_node_records_event() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .remove_workflow_node(run_id, "node-b", "test", "removing testing node")
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let event = find_event(&updated, "dag.mutation.node_removed");
    assert!(event.is_some(), "expected dag.mutation.node_removed event");
    let event = event.unwrap();
    assert_eq!(
        event
            .get("details")
            .and_then(|d| d.get("node_id"))
            .and_then(Value::as_str),
        Some("node-b")
    );
}

#[test]
fn test_update_workflow_node_status_success() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .update_workflow_node_status(run_id, "node-a", "completed", "test", "done")
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let node_a = updated
        .get("nodes")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .find(|n| n.get("node_id").and_then(Value::as_str) == Some("node-a"))
        .unwrap();
    assert_eq!(
        node_a.get("status").and_then(Value::as_str),
        Some("completed")
    );
}

#[test]
fn test_update_workflow_node_status_invalid_fails() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    let result = store.update_workflow_node_status(run_id, "node-a", "bogus", "test", "invalid");
    assert!(result.is_err(), "expected error on invalid status");
    assert!(
        result.unwrap_err().contains("invalid"),
        "error should mention invalid"
    );
}

#[test]
fn test_update_workflow_node_status_not_found_fails() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    let result = store.update_workflow_node_status(
        run_id,
        "node-nonexistent",
        "completed",
        "test",
        "not found",
    );
    assert!(result.is_err(), "expected error when node not found");
}

#[test]
fn test_insert_workflow_edge_success() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Add node-c first so the edge target exists
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "setup",
        )
        .unwrap();

    store
        .insert_workflow_edge(
            run_id,
            &json!({"edge_id": "edge-ac", "from_node_id": "node-a", "to_node_id": "node-c", "edge_type": "dependency"}),
            "test",
            "adding edge",
        )
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let edges = edge_ids(&updated);
    assert_eq!(edges.len(), 2, "expected 2 edges after insert");
    assert!(edges.contains(&"edge-ac".to_string()));
}

#[test]
fn test_insert_workflow_edge_duplicate_fails() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    let result = store.insert_workflow_edge(
        run_id,
        &json!({"edge_id": "edge-ab", "from_node_id": "node-a", "to_node_id": "node-b", "edge_type": "dependency"}),
        "test",
        "duplicate",
    );
    assert!(result.is_err(), "expected error on duplicate edge");
    assert!(
        result.unwrap_err().contains("duplicate"),
        "error should mention duplicate"
    );
}

#[test]
fn test_remove_workflow_edge_success() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .remove_workflow_edge(run_id, "edge-ab", "test", "removing edge")
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let edges = edge_ids(&updated);
    assert_eq!(edges.len(), 0, "expected 0 edges after removal");
}

#[test]
fn test_remove_workflow_edge_not_found_fails() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Removing nonexistent edge is a no-op (no rows deleted)
    let result = store.remove_workflow_edge(run_id, "edge-nonexistent", "test", "not found");
    assert!(
        result.is_ok(),
        "removing nonexistent edge is a no-op, not an error"
    );
}

#[test]
fn test_rewire_workflow_edge_success() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Add node-c
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "setup",
        )
        .unwrap();

    // Rewire edge-ab: from node-c, to node-b
    store
        .rewire_workflow_edge(
            run_id,
            "edge-ab",
            Some("node-c"),
            Some("node-b"),
            "test",
            "rewiring",
        )
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let edges = updated.get("edges").and_then(Value::as_array).unwrap();
    let edge_ab = edges
        .iter()
        .find(|e| e.get("edge_id").and_then(Value::as_str) == Some("edge-ab"))
        .unwrap();
    assert_eq!(
        edge_ab.get("from_node_id").and_then(Value::as_str),
        Some("node-c")
    );
    assert_eq!(
        edge_ab.get("to_node_id").and_then(Value::as_str),
        Some("node-b")
    );
}

#[test]
fn test_rewire_workflow_edge_partial() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Add node-c
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "setup",
        )
        .unwrap();

    // Only change target, keep source (pass None for new_from)
    store
        .rewire_workflow_edge(
            run_id,
            "edge-ab",
            None,
            Some("node-c"),
            "test",
            "partial rewire",
        )
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let edges = updated.get("edges").and_then(Value::as_array).unwrap();
    let edge_ab = edges
        .iter()
        .find(|e| e.get("edge_id").and_then(Value::as_str) == Some("edge-ab"))
        .unwrap();
    assert_eq!(
        edge_ab.get("from_node_id").and_then(Value::as_str),
        Some("node-a"),
        "source should be unchanged"
    );
    assert_eq!(
        edge_ab.get("to_node_id").and_then(Value::as_str),
        Some("node-c"),
        "target should be updated"
    );
}

#[test]
fn test_replay_mutation_events_no_mutations() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    let replay = store.replay_mutation_events(run_id).unwrap();
    assert_eq!(
        replay.get("mutations_replayed").and_then(Value::as_u64),
        Some(0),
        "expected 0 mutations replayed on fresh run"
    );
}

#[test]
fn test_replay_mutation_events_with_add_and_remove() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Add node-c
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "adding node",
        )
        .unwrap();

    // Remove node-b (also removes edge-ab)
    store
        .remove_workflow_node(run_id, "node-b", "test", "removing node")
        .unwrap();

    let replay = store.replay_mutation_events(run_id).unwrap();
    assert!(
        replay
            .get("mutations_replayed")
            .and_then(Value::as_u64)
            .unwrap()
            >= 2,
        "expected at least 2 mutations replayed"
    );

    let replay_nodes = replay.get("nodes").and_then(Value::as_array).unwrap();
    let replay_node_ids: Vec<&str> = replay_nodes
        .iter()
        .map(|n| n.get("node_id").and_then(Value::as_str).unwrap())
        .collect();
    assert!(
        replay_node_ids.contains(&"node-c"),
        "replay should include added node-c"
    );
    assert!(
        !replay_node_ids.contains(&"node-b"),
        "replay should not include removed node-b"
    );
}

#[test]
fn test_replay_protects_completed_nodes() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Complete node-a via tick
    store.tick_workflow_run(run_id, "test").unwrap();

    // Add node-c after completion
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "adding after complete",
        )
        .unwrap();

    let replay = store.replay_mutation_events(run_id).unwrap();
    let protected = replay
        .get("protected_completed_nodes")
        .and_then(Value::as_array)
        .unwrap();
    let protected_ids: Vec<&str> = protected.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(
        protected_ids.contains(&"node-a"),
        "node-a should be in protected_completed_nodes after tick"
    );
}

#[test]
fn test_replay_with_edge_rewire() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Add node-c
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "setup",
        )
        .unwrap();

    // Rewire edge-ab
    store
        .rewire_workflow_edge(
            run_id,
            "edge-ab",
            Some("node-c"),
            Some("node-b"),
            "test",
            "rewiring",
        )
        .unwrap();

    let replay = store.replay_mutation_events(run_id).unwrap();
    let replay_edges = replay.get("edges").and_then(Value::as_array).unwrap();
    let edge_ab = replay_edges
        .iter()
        .find(|e| e.get("edge_id").and_then(Value::as_str) == Some("edge-ab"))
        .unwrap();
    assert_eq!(
        edge_ab.get("from_node_id").and_then(Value::as_str),
        Some("node-c"),
        "replay should reflect rewired from_node"
    );
    assert_eq!(
        edge_ab.get("to_node_id").and_then(Value::as_str),
        Some("node-b"),
        "replay should reflect rewired to_node"
    );
}

#[test]
fn test_apply_dag_mutations_batch_add_node() {
    use crate::workflow::dag_manager::types::DAGMutationProposal;
    use std::collections::HashMap;

    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    let mut payload = HashMap::new();
    payload.insert("node_id".to_string(), json!("node-c"));
    payload.insert("node_type".to_string(), json!("review"));
    payload.insert("status".to_string(), json!("pending"));

    let proposal = DAGMutationProposal {
        proposal_id: "add-node-c".to_string(),
        dag_id: run_id.to_string(),
        mutation_type: "add_node".to_string(),
        payload,
        reason: "batch add".to_string(),
        ..Default::default()
    };

    let results = store
        .apply_dag_mutations_batch(run_id, &[proposal], "test")
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].get("applied").and_then(Value::as_bool).unwrap(),
        "proposal should be applied"
    );

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let ids = node_ids(&updated);
    assert!(
        ids.contains(&"node-c".to_string()),
        "node-c should exist after batch add"
    );
}

#[test]
fn test_apply_dag_mutations_batch_records_events() {
    use crate::workflow::dag_manager::types::DAGMutationProposal;
    use std::collections::HashMap;

    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    let mut payload = HashMap::new();
    payload.insert("node_id".to_string(), json!("node-c"));
    payload.insert("node_type".to_string(), json!("review"));
    payload.insert("status".to_string(), json!("pending"));

    let proposal = DAGMutationProposal {
        proposal_id: "add-node-c".to_string(),
        dag_id: run_id.to_string(),
        mutation_type: "add_node".to_string(),
        payload,
        reason: "batch add".to_string(),
        ..Default::default()
    };

    store
        .apply_dag_mutations_batch(run_id, &[proposal], "test")
        .unwrap();

    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    let applied_events: Vec<&Value> = updated
        .get("events")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter(|e| e.get("event_type").and_then(Value::as_str) == Some("dag.mutation.applied"))
        .collect();
    assert!(
        !applied_events.is_empty(),
        "expected at least one dag.mutation.applied event"
    );
    assert_eq!(
        applied_events[0]
            .get("details")
            .and_then(|d| d.get("proposal_id"))
            .and_then(Value::as_str),
        Some("add-node-c")
    );
}

#[test]
fn test_export_import_roundtrip_with_mutations() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Mutate: add node-c, remove edge-ab
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "mutation",
        )
        .unwrap();
    store
        .remove_workflow_edge(run_id, "edge-ab", "test", "mutation")
        .unwrap();

    let exported = store.export_workflow_runs(100).unwrap();
    assert_eq!(exported.len(), 1);
    let export = &exported[0];

    // Import into a fresh store
    let store2 = new_test_store();
    let imported = store2.import_workflow_run(export).unwrap();
    assert!(imported, "import should succeed");

    let imported_run = store2.get_workflow_run(run_id).unwrap().unwrap();
    let ids = node_ids(&imported_run);
    assert!(
        ids.contains(&"node-c".to_string()),
        "imported run should have node-c"
    );
    let edges = edge_ids(&imported_run);
    assert!(edges.is_empty(), "imported run should have 0 edges");

    // Verify mutation events survived roundtrip
    let events = mutation_events(&imported_run);
    assert!(
        !events.is_empty(),
        "imported run should have mutation events"
    );
}

#[test]
fn test_integrity_after_mutations() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Perform several mutations
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "mutation",
        )
        .unwrap();
    store
        .remove_workflow_edge(run_id, "edge-ab", "test", "mutation")
        .unwrap();
    store
        .update_workflow_node_status(run_id, "node-a", "completed", "test", "mutation")
        .unwrap();

    let report = store.check_integrity().unwrap();
    assert_eq!(
        report.status, "ok",
        "integrity check should pass after mutations"
    );
}

#[test]
fn test_no_duplicate_execution_after_mutation() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Tick to execute node-a (NoopNodeExecutor completes it)
    store.tick_workflow_run(run_id, "test").unwrap();

    // Verify node-a is completed
    let run_after_tick = store.get_workflow_run(run_id).unwrap().unwrap();
    let node_a = run_after_tick
        .get("nodes")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .find(|n| n.get("node_id").and_then(Value::as_str) == Some("node-a"))
        .unwrap();
    assert_eq!(
        node_a
            .get("db_status")
            .and_then(Value::as_str)
            .or_else(|| node_a.get("status").and_then(Value::as_str)),
        Some("completed")
    );

    // Add node-c after node-a completed
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "adding after tick",
        )
        .unwrap();

    // Tick again - should not re-execute node-a
    store.tick_workflow_run(run_id, "test").unwrap();

    let run_final = store.get_workflow_run(run_id).unwrap().unwrap();
    let node_a_final = run_final
        .get("nodes")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .find(|n| n.get("node_id").and_then(Value::as_str) == Some("node-a"))
        .unwrap();
    // node-a should still be completed (not re-executed)
    assert_eq!(
        node_a_final
            .get("db_status")
            .and_then(Value::as_str)
            .or_else(|| node_a_final.get("status").and_then(Value::as_str)),
        Some("completed"),
        "node-a should remain completed, not re-executed"
    );
}

#[test]
fn test_workflow_run_detail_includes_mutation_events() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    // Perform a mutation
    store
        .insert_workflow_node(
            run_id,
            &json!({"node_id": "node-c", "task_type": "review", "status": "pending"}),
            "test",
            "mutation",
        )
        .unwrap();

    let detail = store.get_workflow_run(run_id).unwrap().unwrap();
    let events = detail.get("events").and_then(Value::as_array).unwrap();

    let mutation_event = events
        .iter()
        .find(|e| e.get("event_type").and_then(Value::as_str) == Some("dag.mutation.node_added"));
    assert!(
        mutation_event.is_some(),
        "get_workflow_run should include mutation events in the events array"
    );
}

#[test]
fn test_list_active_workflow_runs_prioritized_ordering() {
    let store = new_test_store();
    let plan1 = create_test_plan(&store);
    let pid1 = plan1.get("plan_id").and_then(Value::as_str).unwrap();
    let plan2 = create_test_plan(&store);
    let pid2 = plan2.get("plan_id").and_then(Value::as_str).unwrap();
    let plan3 = create_test_plan(&store);
    let pid3 = plan3.get("plan_id").and_then(Value::as_str).unwrap();

    store
        .create_workflow_run_with_queue_metadata(pid1, "test", 10, None, None, None)
        .unwrap();
    store
        .create_workflow_run_with_queue_metadata(pid2, "test", 1, None, None, None)
        .unwrap();
    store
        .create_workflow_run_with_queue_metadata(pid3, "test", 5, None, None, None)
        .unwrap();

    let runs = store.list_active_workflow_runs_prioritized().unwrap();
    assert_eq!(runs.len(), 3);
    // Priority 1 (highest) should come first
    assert_eq!(runs[0].get("priority").and_then(Value::as_i64), Some(1));
    assert_eq!(runs[1].get("priority").and_then(Value::as_i64), Some(5));
    assert_eq!(runs[2].get("priority").and_then(Value::as_i64), Some(10));
}

#[test]
fn test_list_active_workflow_runs_paused_go_last() {
    let store = new_test_store();
    let plan1 = create_test_plan(&store);
    let pid1 = plan1.get("plan_id").and_then(Value::as_str).unwrap();
    let plan2 = create_test_plan(&store);
    let pid2 = plan2.get("plan_id").and_then(Value::as_str).unwrap();

    let run1 = store
        .create_workflow_run_with_queue_metadata(pid1, "test", 1, None, None, None)
        .unwrap();
    store
        .create_workflow_run_with_queue_metadata(pid2, "test", 10, None, None, None)
        .unwrap();

    let run1_id = run1.get("run_id").and_then(Value::as_str).unwrap();
    store
        .update_run_pause_reason(run1_id, Some("operator_paused"))
        .unwrap();

    let runs = store.list_active_workflow_runs_prioritized().unwrap();
    assert_eq!(runs.len(), 2);
    // Non-paused priority 10 comes before paused priority 1
    assert_eq!(runs[0].get("priority").and_then(Value::as_i64), Some(10));
    assert_eq!(
        runs[1].get("pause_reason").and_then(Value::as_str),
        Some("operator_paused")
    );
}

#[test]
fn test_update_run_priority() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store.update_run_priority(run_id, 3).unwrap();
    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    assert_eq!(
        updated.get("priority").and_then(Value::as_i64),
        Some(3),
        "priority should be updated to 3"
    );
}

#[test]
fn test_update_run_pause_reason_set_and_clear() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .update_run_pause_reason(run_id, Some("pool_saturated"))
        .unwrap();
    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    assert_eq!(
        updated.get("pause_reason").and_then(Value::as_str),
        Some("pool_saturated")
    );

    store.update_run_pause_reason(run_id, None).unwrap();
    let cleared = store.get_workflow_run(run_id).unwrap().unwrap();
    assert!(
        cleared.get("pause_reason").unwrap().is_null(),
        "pause_reason should be null after clearing"
    );
}

#[test]
fn test_update_run_degrade_mode_set_and_clear() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store
        .update_run_degrade_mode(run_id, Some("reduced_concurrency"))
        .unwrap();
    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    assert_eq!(
        updated.get("degrade_mode").and_then(Value::as_str),
        Some("reduced_concurrency")
    );

    store.update_run_degrade_mode(run_id, None).unwrap();
    let cleared = store.get_workflow_run(run_id).unwrap().unwrap();
    assert!(
        cleared.get("degrade_mode").unwrap().is_null(),
        "degrade_mode should be null after clearing"
    );
}

#[test]
fn test_set_run_queue_position_set_and_clear() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

    store.set_run_queue_position(run_id, Some(7)).unwrap();
    let updated = store.get_workflow_run(run_id).unwrap().unwrap();
    assert_eq!(
        updated.get("queue_position").and_then(Value::as_i64),
        Some(7)
    );

    store.set_run_queue_position(run_id, None).unwrap();
    let cleared = store.get_workflow_run(run_id).unwrap().unwrap();
    assert!(
        cleared.get("queue_position").unwrap().is_null(),
        "queue_position should be null after clearing"
    );
}

#[test]
fn test_get_queue_status_empty() {
    let store = new_test_store();
    let status = store.get_queue_status().unwrap();
    assert_eq!(status.get("total_queued").and_then(Value::as_i64), Some(0));
    assert_eq!(status.get("total_running").and_then(Value::as_i64), Some(0));
    assert_eq!(status.get("total_paused").and_then(Value::as_i64), Some(0));
    assert_eq!(
        status.get("total_completed").and_then(Value::as_i64),
        Some(0)
    );
    assert_eq!(status.get("total_failed").and_then(Value::as_i64), Some(0));
    assert_eq!(status.get("overdue_count").and_then(Value::as_i64), Some(0));
}

#[test]
fn test_get_queue_status_with_runs() {
    let store = new_test_store();
    let plan1 = create_test_plan(&store);
    let pid1 = plan1.get("plan_id").and_then(Value::as_str).unwrap();
    let plan2 = create_test_plan(&store);
    let pid2 = plan2.get("plan_id").and_then(Value::as_str).unwrap();

    let _run1 = store.create_workflow_run_from_plan(pid1, "test").unwrap();
    let run2 = store
        .create_workflow_run_with_queue_metadata(pid2, "test", 3, None, None, None)
        .unwrap();

    // Pause run2
    let run2_id = run2.get("run_id").and_then(Value::as_str).unwrap();
    store
        .update_run_pause_reason(run2_id, Some("quota_exceeded"))
        .unwrap();

    let status = store.get_queue_status().unwrap();
    // run1 is "created" (not paused) = queued; run2 is "created" but paused
    assert_eq!(status.get("total_queued").and_then(Value::as_i64), Some(1));
    assert_eq!(status.get("total_paused").and_then(Value::as_i64), Some(1));
}

#[test]
fn test_list_tenants_with_quota_empty() {
    let store = new_test_store();
    let tenants = store.list_tenants_with_quota().unwrap();
    assert!(tenants.is_empty());
}

#[test]
fn test_list_tenants_with_quota_groups_correctly() {
    let store = new_test_store();
    let plan1 = create_test_plan(&store);
    let pid1 = plan1.get("plan_id").and_then(Value::as_str).unwrap();
    let plan2 = create_test_plan(&store);
    let pid2 = plan2.get("plan_id").and_then(Value::as_str).unwrap();
    let plan3 = create_test_plan(&store);
    let pid3 = plan3.get("plan_id").and_then(Value::as_str).unwrap();

    store
        .create_workflow_run_with_queue_metadata(pid1, "test", 3, None, None, Some("tenant-a"))
        .unwrap();
    store
        .create_workflow_run_with_queue_metadata(pid2, "test", 7, None, None, Some("tenant-a"))
        .unwrap();
    store
        .create_workflow_run_with_queue_metadata(pid3, "test", 5, None, None, Some("tenant-b"))
        .unwrap();

    let tenants = store.list_tenants_with_quota().unwrap();
    assert_eq!(tenants.len(), 2);

    let tenant_a = tenants
        .iter()
        .find(|t| t.get("tenant_id").and_then(Value::as_str) == Some("tenant-a"))
        .unwrap();
    assert_eq!(tenant_a.get("run_count").and_then(Value::as_i64), Some(2));
    let avg = tenant_a
        .get("avg_priority")
        .and_then(Value::as_f64)
        .unwrap();
    assert!(
        (avg - 5.0).abs() < 0.01,
        "avg_priority should be ~5.0, got {avg}"
    );

    let tenant_b = tenants
        .iter()
        .find(|t| t.get("tenant_id").and_then(Value::as_str) == Some("tenant-b"))
        .unwrap();
    assert_eq!(tenant_b.get("run_count").and_then(Value::as_i64), Some(1));
}

#[test]
fn test_create_workflow_run_with_queue_metadata() {
    let store = new_test_store();
    let plan = create_test_plan(&store);
    let pid = plan.get("plan_id").and_then(Value::as_str).unwrap();

    let run = store
        .create_workflow_run_with_queue_metadata(
            pid,
            "test",
            2,
            Some("2026-12-31T23:59:59Z"),
            Some(60000),
            Some("tenant-x"),
        )
        .unwrap();

    assert_eq!(run.get("priority").and_then(Value::as_i64), Some(2));
    assert_eq!(
        run.get("deadline_at").and_then(Value::as_str),
        Some("2026-12-31T23:59:59Z")
    );
    assert_eq!(run.get("sla_ms").and_then(Value::as_i64), Some(60000));
    assert_eq!(
        run.get("tenant_id").and_then(Value::as_str),
        Some("tenant-x")
    );
    assert!(run.get("pause_reason").unwrap().is_null());
    assert!(run.get("degrade_mode").unwrap().is_null());
}

#[test]
fn test_default_priority_is_five() {
    let store = new_test_store();
    let run = run_from_plan(&store);
    assert_eq!(
        run.get("priority").and_then(Value::as_i64),
        Some(5),
        "default priority should be 5"
    );
}
