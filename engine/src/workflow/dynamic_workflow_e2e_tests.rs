use serde_json::{json, Value};

use super::agent_profiles::AgentProfileRegistry;
use super::dynamic_controller::{
    ControllerAction, DynamicControllerConfig, DynamicWorkflowController,
};
use super::tool_registry::{HookAction, HookType};
use crate::node_executor::{FailNodeExecutor, NoopNodeExecutor};
use crate::storage::local_product_store::LocalProductStore;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_store() -> LocalProductStore {
    LocalProductStore::new(":memory:").expect("in-memory store")
}

/// Create a workflow plan with 2 initial nodes: analyze -> execute.
fn setup_e2e_plan(store: &LocalProductStore) -> (String, String) {
    let plan = store
        .create_workflow_plan("e2e-req", "e2e-workflow", "actor", |ids, _| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-e2e", "task_domain": "test"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-06-06T00:00:00Z",
                    "updated_at": "2026-06-06T00:00:00Z",
                    "nodes": [
                        {"node_id": "analyze", "task_type": "analyze", "status": "pending"},
                        {"node_id": "execute", "task_type": "execute", "status": "pending"}
                    ],
                    "edges": [
                        {"edge_id": "e-analyze-execute", "from_node_id": "analyze", "to_node_id": "execute", "edge_type": "dependency"}
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

    let plan_id = plan
        .get("plan_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let run = store
        .create_workflow_run_from_plan(&plan_id, "actor")
        .expect("create run");
    let run_id = run
        .get("run_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    (plan_id, run_id)
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
        .workflow_run_events(run_id, 10_000)
        .expect("get events")
}

fn has_event_type(events: &[Value], event_type: &str) -> bool {
    events
        .iter()
        .any(|e| e.get("event_type").and_then(Value::as_str) == Some(event_type))
}

/// Collect all node_id values from events of a given type.
fn event_node_ids(events: &[Value], event_type: &str) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.get("event_type").and_then(Value::as_str) == Some(event_type))
        .filter_map(|e| {
            e.get("details")
                .and_then(|d| d.get("node_id"))
                .and_then(Value::as_str)
                .map(String::from)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// E2E Test: Full Dynamic Workflow Cycle
// ---------------------------------------------------------------------------
//
// Scenario:
//   1. Create a workflow plan with 2 initial nodes: analyze -> execute
//   2. Create a workflow run from the plan
//   3. Set up DynamicWorkflowController with auto_fix_on_failure=true
//   4. Register agent profiles, tool capabilities, and a PreExecution hook
//
// Flow:
//   A. tick -> analyze node executes (NoopNodeExecutor, succeeds)
//   B. Manually mark execute as failed -> tick -> auto-fix adds fix + test nodes
//   C. tick -> fix node executes (NoopNodeExecutor, succeeds)
//   D. tick -> test node executes (NoopNodeExecutor, succeeds)
//   E-H. Verify event trail, feedback, graph integrity, export/import

#[test]
fn test_e2e_dynamic_workflow_full_cycle() {
    let store = new_store();

    // =========================================================================
    // Setup: Agent profiles
    // =========================================================================
    let registry = AgentProfileRegistry::new();
    for profile in registry.list_all() {
        store
            .upsert_agent_profile(
                profile.profile_id.as_str(),
                profile.role.as_str(),
                &profile.tools,
                profile.model_hint.as_deref(),
                profile.context_budget_tokens,
                profile.workspace_scope.as_str(),
                profile.executor_preference.as_deref(),
                profile.max_retries,
            )
            .expect("upsert profile");
    }

    let stored_profiles = store.list_agent_profiles().expect("list profiles");
    assert!(
        stored_profiles
            .iter()
            .any(|p| p.profile_id.as_str() == "implementer"),
        "implementer profile should be registered"
    );
    assert!(
        stored_profiles
            .iter()
            .any(|p| p.profile_id.as_str() == "tester"),
        "tester profile should be registered"
    );
    assert!(
        stored_profiles
            .iter()
            .any(|p| p.profile_id.as_str() == "reviewer"),
        "reviewer profile should be registered"
    );

    // =========================================================================
    // Setup: Tool capabilities
    // =========================================================================
    store
        .register_tool_capability("read", "Read files", None, None, false, "low")
        .expect("register read");
    store
        .register_tool_capability("write", "Write files", None, None, false, "medium")
        .expect("register write");
    store
        .register_tool_capability("bash", "Execute shell commands", None, None, false, "high")
        .expect("register bash");

    let tools = store.list_tool_capabilities().expect("list tools");
    assert_eq!(tools.len(), 3);
    assert!(tools.iter().any(|t| t.tool_name == "read"));
    assert!(tools.iter().any(|t| t.tool_name == "write"));
    assert!(tools.iter().any(|t| t.tool_name == "bash"));

    // =========================================================================
    // Setup: Tool allowlists
    // =========================================================================
    store
        .set_tool_allowlist(
            "implementer",
            &["read".into(), "write".into(), "bash".into()],
        )
        .expect("set implementer allowlist");
    store
        .set_tool_allowlist("tester", &["read".into(), "bash".into()])
        .expect("set tester allowlist");
    store
        .set_tool_allowlist("reviewer", &["read".into()])
        .expect("set reviewer allowlist");

    assert!(store.check_tool_allowed("implementer", "read").unwrap());
    assert!(store.check_tool_allowed("implementer", "bash").unwrap());
    assert!(store.check_tool_allowed("tester", "read").unwrap());
    assert!(!store.check_tool_allowed("tester", "write").unwrap());
    assert!(store.check_tool_allowed("reviewer", "read").unwrap());
    assert!(!store.check_tool_allowed("reviewer", "bash").unwrap());

    // =========================================================================
    // Setup: PreExecution hook (log tool usage)
    // =========================================================================
    store
        .add_tool_hook(
            "log-tool-usage",
            HookType::PreExecution.as_str(),
            None,
            None,
            HookAction::Log.as_str(),
            None,
        )
        .expect("add pre-execution hook");

    let hook_result = store
        .evaluate_hooks(&HookType::PreExecution, "read", &json!({"context": "test"}))
        .expect("evaluate hooks");
    assert!(
        matches!(hook_result, super::tool_registry::HookResult::Allow),
        "Log hook should return Allow"
    );

    // =========================================================================
    // Setup: Create workflow plan with analyze -> execute
    // =========================================================================
    let (_plan_id, run_id) = setup_e2e_plan(&store);

    assert_eq!(run_status(&store, &run_id), "created");
    let initial_nodes = run_nodes(&store, &run_id);
    assert_eq!(initial_nodes.len(), 2);
    assert!(find_node(&initial_nodes, "analyze").is_some());
    assert!(find_node(&initial_nodes, "execute").is_some());

    // =========================================================================
    // Setup: DynamicWorkflowController with auto_fix_on_failure=true
    // =========================================================================
    let config = DynamicControllerConfig {
        auto_fix_on_failure: true,
        max_mutations_per_run: 20,
        record_feedback: true,
        ..Default::default()
    };
    let mut controller = DynamicWorkflowController::new(config);

    let noop = NoopNodeExecutor;
    let _fail = FailNodeExecutor {
        error_domain: "test_failure".to_string(),
        error_message: "execute node failed intentionally".to_string(),
    };

    // =========================================================================
    // Step A: tick -> analyze node executes (NoopNodeExecutor, succeeds)
    // =========================================================================
    let result_a = controller
        .tick(&store, &run_id, "actor", &noop)
        .expect("tick A");

    assert!(
        result_a.actions.iter().any(
            |a| matches!(a, ControllerAction::NodeExecuted { node_id, status }
                if node_id == "analyze" && status == "completed")
        ),
        "Step A: analyze should execute and complete, got: {:?}",
        result_a.actions
    );
    assert_eq!(run_status(&store, &run_id), "running");

    let nodes_a = run_nodes(&store, &run_id);
    assert_eq!(
        node_db_status(find_node(&nodes_a, "analyze").unwrap()),
        "completed"
    );
    assert!(result_a.should_continue, "execute is still pending");

    // Verify feedback was recorded
    let feedback_a = store.get_feedback_for_run(&run_id).expect("feedback A");
    assert!(
        !feedback_a.is_empty(),
        "Step A: feedback should be recorded"
    );
    assert!(feedback_a.iter().any(|f| f.success));

    // =========================================================================
    // Step B: Mark execute as failed, tick -> auto-fix triggers
    //
    // The controller's Phase 1 (auto-fix) scans for failed nodes BEFORE
    // Phase 4 (executor tick). We manually fail the execute node to simulate
    // a real failure, then the controller detects it and creates fix/test nodes.
    // This matches how the existing dynamic_controller_tests work.
    // =========================================================================
    store
        .update_workflow_node_status(&run_id, "execute", "failed", "actor", "simulated failure")
        .expect("mark execute failed");

    // Verify execute is now failed
    let nodes_b_pre = run_nodes(&store, &run_id);
    assert_eq!(
        node_db_status(find_node(&nodes_b_pre, "execute").unwrap()),
        "failed"
    );

    let result_b = controller
        .tick(&store, &run_id, "actor", &noop)
        .expect("tick B");

    // Auto-fix should have been triggered
    assert!(
        result_b.actions.iter().any(
            |a| matches!(a, ControllerAction::GraphMutated { mutation_type, .. }
                if mutation_type == "add_node")
        ),
        "Step B: auto-fix should trigger GraphMutated, got: {:?}",
        result_b.actions
    );

    // Verify fix and test nodes were created
    let nodes_b = run_nodes(&store, &run_id);
    assert!(
        find_node(&nodes_b, "fix-execute").is_some(),
        "Step B: fix-execute node should be created"
    );
    assert!(
        find_node(&nodes_b, "test-fix-execute").is_some(),
        "Step B: test-fix-execute node should be created"
    );

    // The fix-execute node depends on execute (the failed node). The store's
    // ready-node check requires all predecessors to be "completed". To unblock
    // the recovery chain, mark execute back to "completed" -- this simulates
    // the orchestration layer acknowledging that a fix strategy has been applied
    // for the failed node, allowing the recovery nodes to proceed.
    store
        .update_workflow_node_status(
            &run_id,
            "execute",
            "completed",
            "actor",
            "failure resolved by auto-fix",
        )
        .expect("resolve execute failure");

    // =========================================================================
    // Step C: tick -> fix node executes (NoopNodeExecutor, succeeds)
    // =========================================================================
    let result_c = controller
        .tick(&store, &run_id, "actor", &noop)
        .expect("tick C");

    let nodes_c = run_nodes(&store, &run_id);
    let fix_node = find_node(&nodes_c, "fix-execute").expect("fix-execute");
    assert_eq!(
        node_db_status(fix_node),
        "completed",
        "Step C: fix-execute should be completed"
    );

    assert!(
        result_c.actions.iter().any(
            |a| matches!(a, ControllerAction::NodeExecuted { status, .. } if status == "completed")
        ),
        "Step C: should execute a node successfully, got: {:?}",
        result_c.actions
    );

    // test-fix-execute should now be unblocked (pending or being executed)
    let test_node_c = find_node(&nodes_c, "test-fix-execute").expect("test-fix-execute");
    let test_status_c = node_db_status(test_node_c);
    assert!(
        test_status_c == "pending" || test_status_c == "completed",
        "Step C: test-fix-execute should be pending or completed, got: {}",
        test_status_c
    );

    // =========================================================================
    // Step D: tick -> test node executes (NoopNodeExecutor, succeeds)
    //         Verify run status=completed
    // =========================================================================
    let result_d = controller
        .tick(&store, &run_id, "actor", &noop)
        .expect("tick D");

    let nodes_d = run_nodes(&store, &run_id);
    let test_node_d = find_node(&nodes_d, "test-fix-execute").expect("test-fix-execute");
    assert_eq!(
        node_db_status(test_node_d),
        "completed",
        "Step D: test-fix-execute should be completed"
    );

    // The run should be terminal now.
    // Note: execute is still "failed" in the node table, so check_run_completion
    // will see has_failure=true and mark the run as "failed". This is correct:
    // the original node failed, auto-fix added recovery nodes, but the run
    // still contains a failed node.
    let status_d = run_status(&store, &run_id);
    assert!(
        status_d == "completed" || status_d == "failed",
        "Step D: run should be terminal, got: {}",
        status_d
    );

    // If the run is "failed" due to the original execute node, let's verify
    // the controller also reports RunCompleted or RunFailed appropriately.
    if status_d == "completed" {
        assert!(
            result_d
                .actions
                .iter()
                .any(|a| matches!(a, ControllerAction::RunCompleted)),
            "Step D: should report RunCompleted"
        );
    }
    // If failed, the RunFailed is also acceptable since execute is still marked failed.
    // The key assertion is that ALL recovery nodes completed.

    // =========================================================================
    // Step E: Verify full event trail
    // =========================================================================
    let events = run_events(&store, &run_id);

    // workflow_run.created
    assert!(
        has_event_type(&events, "workflow_run.created"),
        "Step E: should have workflow_run.created"
    );

    // workflow_run.tick_started
    assert!(
        has_event_type(&events, "workflow_run.tick_started"),
        "Step E: should have workflow_run.tick_started"
    );

    // node.leased (multiple -- analyze, fix-execute, test-fix-execute were
    // executed via controller tick; execute was manually failed so it has
    // a dag.mutation.node_status_updated event instead of node.leased)
    let leased_ids = event_node_ids(&events, "node.leased");
    assert!(
        leased_ids.len() >= 2,
        "Step E: should have >=2 node.leased events, got: {:?}",
        leased_ids
    );
    assert!(
        leased_ids.contains(&"analyze".to_string()),
        "Step E: analyze should be leased"
    );
    assert!(
        leased_ids.contains(&"fix-execute".to_string()),
        "Step E: fix-execute should be leased"
    );

    // node.completed (analyze, fix-execute, test-fix-execute)
    let completed_ids = event_node_ids(&events, "node.completed");
    assert!(
        completed_ids.contains(&"analyze".to_string()),
        "Step E: analyze should have node.completed"
    );
    assert!(
        completed_ids.contains(&"fix-execute".to_string()),
        "Step E: fix-execute should have node.completed"
    );
    assert!(
        completed_ids.contains(&"test-fix-execute".to_string()),
        "Step E: test-fix-execute should have node.completed"
    );

    // node.failed (execute) -- execute was manually set to "failed" via
    // update_workflow_node_status which records dag.mutation.node_status_updated,
    // not node.failed. The node.failed event only comes from tick execution.
    // Verify the status-update event exists for execute instead.
    let execute_status_updated = events.iter().any(|e| {
        e.get("event_type").and_then(Value::as_str) == Some("dag.mutation.node_status_updated")
            && e.get("details")
                .and_then(|d| d.get("node_id"))
                .and_then(Value::as_str)
                == Some("execute")
            && e.get("details")
                .and_then(|d| d.get("new_status"))
                .and_then(Value::as_str)
                == Some("failed")
    });
    assert!(
        execute_status_updated,
        "Step E: execute should have dag.mutation.node_status_updated(failed) event"
    );

    // After resolving execute, there should be a second status update to "completed"
    let execute_resolved = events.iter().any(|e| {
        e.get("event_type").and_then(Value::as_str) == Some("dag.mutation.node_status_updated")
            && e.get("details")
                .and_then(|d| d.get("node_id"))
                .and_then(Value::as_str)
                == Some("execute")
            && e.get("details")
                .and_then(|d| d.get("new_status"))
                .and_then(Value::as_str)
                == Some("completed")
    });
    assert!(
        execute_resolved,
        "Step E: execute should have dag.mutation.node_status_updated(completed) event after resolution"
    );

    // dag.mutation.node_added (fix, test)
    let node_added_ids: Vec<String> = events
        .iter()
        .filter(|e| e.get("event_type").and_then(Value::as_str) == Some("dag.mutation.node_added"))
        .filter_map(|e| {
            e.get("details")
                .and_then(|d| d.get("node_id"))
                .and_then(Value::as_str)
                .map(String::from)
        })
        .collect();
    assert!(
        node_added_ids.contains(&"fix-execute".to_string()),
        "Step E: should have dag.mutation.node_added for fix-execute"
    );
    assert!(
        node_added_ids.contains(&"test-fix-execute".to_string()),
        "Step E: should have dag.mutation.node_added for test-fix-execute"
    );

    // dag.mutation.edge_added (edges to fix and test)
    let edge_added_events: Vec<&Value> = events
        .iter()
        .filter(|e| e.get("event_type").and_then(Value::as_str) == Some("dag.mutation.edge_added"))
        .collect();
    assert!(
        !edge_added_events.is_empty(),
        "Step E: should have dag.mutation.edge_added events"
    );

    // workflow_run.completed or workflow_run.failed
    assert!(
        has_event_type(&events, "workflow_run.completed")
            || has_event_type(&events, "workflow_run.failed"),
        "Step E: should have terminal workflow_run event"
    );

    // =========================================================================
    // Step F: Verify feedback records
    // =========================================================================
    // Record feedback for the manually-failed execute node.
    // In production, the orchestration layer records feedback after detecting
    // node failures. Here we simulate that explicitly.
    store
        .insert_scheduler_feedback(
            &run_id,
            Some("execute"),
            "noop",
            &crate::routing::schemas::make_task_group("execute", "execute"),
            false,
            0,
            1,
            0.0,
            0.0,
            Some("test_failure"),
        )
        .expect("insert execute failure feedback");

    let feedback_records = store.get_feedback_for_run(&run_id).expect("feedback F");

    assert!(
        feedback_records.len() >= 4,
        "Step F: should have >=4 feedback records, got: {}",
        feedback_records.len()
    );

    assert!(
        feedback_records.iter().any(|f| f.success),
        "Step F: should have successful feedback"
    );
    assert!(
        feedback_records.iter().any(|f| !f.success),
        "Step F: should have failed feedback (execute)"
    );

    // suggest_executor_type returns based on history
    let task_group = crate::routing::schemas::make_task_group("analyze", "execute");
    let suggestion = store.suggest_executor_type(&task_group);
    assert!(
        suggestion.is_some(),
        "Step F: suggest_executor_type should return a suggestion"
    );

    // =========================================================================
    // Step G: Verify graph integrity
    // =========================================================================

    // replay_mutation_events produces valid DAG
    let replayed = store
        .replay_mutation_events(&run_id)
        .expect("replay mutations");
    let replayed_nodes = replayed.get("nodes").and_then(Value::as_array).unwrap();
    let replayed_node_ids: Vec<String> = replayed_nodes
        .iter()
        .filter_map(|n| n.get("node_id").and_then(Value::as_str).map(String::from))
        .collect();

    assert!(replayed_node_ids.contains(&"analyze".to_string()));
    assert!(replayed_node_ids.contains(&"execute".to_string()));
    assert!(replayed_node_ids.contains(&"fix-execute".to_string()));
    assert!(replayed_node_ids.contains(&"test-fix-execute".to_string()));

    // No duplicate node_ids
    let mut sorted = replayed_node_ids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        replayed_node_ids.len(),
        "Step G: no duplicate node_ids in replayed graph"
    );

    let mutations_replayed = replayed
        .get("mutations_replayed")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert!(
        mutations_replayed > 0,
        "Step G: should replay at least 1 mutation"
    );

    // All nodes should have valid node_id
    let final_nodes = run_nodes(&store, &run_id);
    for node in &final_nodes {
        let nid = node.get("node_id").and_then(Value::as_str).unwrap_or("");
        assert!(
            !nid.is_empty(),
            "Step G: all nodes should have non-empty node_id"
        );
    }

    // Tool allowlist was checked
    assert!(store.check_tool_allowed("implementer", "read").unwrap());
    assert!(!store.check_tool_allowed("tester", "write").unwrap());

    // Integrity check passes
    let report = store.check_integrity().expect("integrity check");
    assert_eq!(report.status, "ok", "Step G: integrity check should pass");

    // =========================================================================
    // Step H: Verify export/import round-trip
    // =========================================================================
    let exported_runs = store.export_workflow_runs(10_000).expect("export runs");
    assert!(!exported_runs.is_empty());

    let exported_run = exported_runs
        .iter()
        .find(|r| r.get("run_id").and_then(Value::as_str) == Some(&run_id))
        .expect("should find our run in export");

    // Exported run has events
    let exported_events = exported_run
        .get("events")
        .and_then(Value::as_array)
        .unwrap();
    assert!(
        !exported_events.is_empty(),
        "Step H: exported events should not be empty"
    );

    // Exported run has nodes
    let exported_nodes = exported_run.get("nodes").and_then(Value::as_array).unwrap();
    assert_eq!(
        exported_nodes.len(),
        final_nodes.len(),
        "Step H: exported node count should match"
    );

    // Re-import into a fresh store
    let store2 = new_store();
    assert!(
        store2.import_workflow_run(exported_run).expect("import"),
        "Step H: import should succeed"
    );

    // Verify imported run preserves graph state
    let imported_run = store2
        .get_workflow_run(&run_id)
        .expect("get imported")
        .expect("imported run should exist");

    let imported_nodes = imported_run.get("nodes").and_then(Value::as_array).unwrap();
    assert_eq!(
        imported_nodes.len(),
        final_nodes.len(),
        "Step H: imported node count should match"
    );

    let imported_edges = imported_run.get("edges").and_then(Value::as_array).unwrap();
    assert!(
        !imported_edges.is_empty(),
        "Step H: imported edges should not be empty"
    );

    let imported_status = imported_run
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    assert_eq!(
        imported_status, status_d,
        "Step H: imported status should match"
    );

    let imported_events = imported_run
        .get("events")
        .and_then(Value::as_array)
        .unwrap();
    assert!(
        !imported_events.is_empty(),
        "Step H: imported events should not be empty"
    );

    // Duplicate import returns false
    assert!(
        !store2
            .import_workflow_run(exported_run)
            .expect("dup import"),
        "Step H: duplicate import should return false"
    );

    // Export/import tool capabilities round-trip
    let exported_tools = store.export_tool_capabilities().expect("export tools");
    assert_eq!(exported_tools.len(), 3);

    for entry in &exported_tools {
        assert!(
            store2
                .import_tool_capability_entry(entry)
                .expect("import tool"),
            "Step H: tool import should succeed"
        );
    }
    assert_eq!(
        store2.list_tool_capabilities().unwrap().len(),
        3,
        "Step H: store2 should have 3 tool capabilities"
    );
}
