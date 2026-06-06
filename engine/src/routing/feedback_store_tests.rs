use serde_json::{json, Value};

use crate::node_executor::{FailNodeExecutor, NoopNodeExecutor};
use crate::storage::local_product_store::LocalProductStore;

fn new_store() -> LocalProductStore {
    LocalProductStore::new(":memory:").expect("in-memory store")
}

fn setup_run_with_nodes(store: &LocalProductStore) -> String {
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
    run.get("run_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string()
}

// ---------------------------------------------------------------------------
// Test 1: test_insert_and_retrieve_feedback
// ---------------------------------------------------------------------------

#[test]
fn test_insert_and_retrieve_feedback() {
    let store = new_store();

    let record = store
        .insert_scheduler_feedback(
            "run-001",
            Some("node-001"),
            "noop",
            "analyze/execute",
            true,
            150,
            0,
            0.95,
            0.0,
            None,
        )
        .expect("insert feedback");

    assert_eq!(record.run_id, "run-001");
    assert_eq!(record.node_id, Some("node-001".to_string()));
    assert_eq!(record.executor_type, "noop");
    assert_eq!(record.task_group, "analyze/execute");
    assert_eq!(record.task_domain, "analyze");
    assert_eq!(record.task_intent, "execute");
    assert!(record.success);
    assert_eq!(record.latency_ms, 150);
    assert_eq!(record.retry_count, 0);
    assert!((record.quality_score - 0.95).abs() < f64::EPSILON);
    assert!((record.cost).abs() < f64::EPSILON);
    assert!(record.error_domain.is_none());
    assert!(!record.feedback_id.is_empty());
    assert!(!record.created_at.is_empty());
}

// ---------------------------------------------------------------------------
// Test 2: test_feedback_for_run_isolation
// ---------------------------------------------------------------------------

#[test]
fn test_feedback_for_run_isolation() {
    let store = new_store();

    store
        .insert_scheduler_feedback(
            "run-A",
            Some("n1"),
            "noop",
            "test/run",
            true,
            100,
            0,
            0.8,
            0.0,
            None,
        )
        .expect("insert A");

    store
        .insert_scheduler_feedback(
            "run-B",
            Some("n2"),
            "command",
            "test/run",
            false,
            200,
            1,
            0.3,
            0.0,
            Some("command_exit_nonzero"),
        )
        .expect("insert B");

    store
        .insert_scheduler_feedback(
            "run-A",
            Some("n3"),
            "noop",
            "test/run",
            true,
            50,
            0,
            0.9,
            0.0,
            None,
        )
        .expect("insert A2");

    let a_records = store.get_feedback_for_run("run-A").expect("get A");
    assert_eq!(a_records.len(), 2);
    assert!(a_records.iter().all(|r| r.run_id == "run-A"));

    let b_records = store.get_feedback_for_run("run-B").expect("get B");
    assert_eq!(b_records.len(), 1);
    assert_eq!(b_records[0].run_id, "run-B");
}

// ---------------------------------------------------------------------------
// Test 3: test_feedback_stats_success_rate
// ---------------------------------------------------------------------------

#[test]
fn test_feedback_stats_success_rate() {
    let store = new_store();

    // 3 successes, 1 failure -> 75% success rate
    for i in 0..3 {
        store
            .insert_scheduler_feedback(
                &format!("run-{i}"),
                Some(&format!("n-{i}")),
                "noop",
                "test/stats",
                true,
                100,
                0,
                0.8,
                0.0,
                None,
            )
            .expect("insert success");
    }
    store
        .insert_scheduler_feedback(
            "run-fail",
            Some("n-fail"),
            "noop",
            "test/stats",
            false,
            500,
            2,
            0.1,
            0.0,
            Some("test_failure"),
        )
        .expect("insert failure");

    let stats = store.get_feedback_stats("test/stats").expect("stats");
    assert_eq!(stats.total_records, 4);
    assert!((stats.success_rate - 0.75).abs() < f64::EPSILON);
    assert!(stats.avg_latency_ms > 0.0);
    assert!(stats.avg_quality > 0.0);
}

// ---------------------------------------------------------------------------
// Test 4: test_feedback_stats_by_executor_type
// ---------------------------------------------------------------------------

#[test]
fn test_feedback_stats_by_executor_type() {
    let store = new_store();

    store
        .insert_scheduler_feedback(
            "run-1",
            Some("n1"),
            "noop",
            "test/executor",
            true,
            100,
            0,
            0.9,
            0.0,
            None,
        )
        .expect("insert noop");

    store
        .insert_scheduler_feedback(
            "run-2",
            Some("n2"),
            "command",
            "test/executor",
            true,
            200,
            0,
            0.7,
            0.0,
            None,
        )
        .expect("insert command");

    store
        .insert_scheduler_feedback(
            "run-3",
            Some("n3"),
            "command",
            "test/executor",
            false,
            300,
            1,
            0.2,
            0.0,
            Some("command_timeout"),
        )
        .expect("insert command fail");

    let stats = store.get_feedback_stats("test/executor").expect("stats");
    assert_eq!(stats.total_records, 3);

    let by_executor = stats
        .by_executor_type
        .as_array()
        .expect("by_executor_type should be array");
    assert_eq!(by_executor.len(), 2);

    let noop_entry = by_executor
        .iter()
        .find(|e| e["executor_type"] == "noop")
        .expect("noop entry");
    assert_eq!(noop_entry["count"], 1);
    assert_eq!(noop_entry["success_count"], 1);
    assert!((noop_entry["success_rate"].as_f64().unwrap() - 1.0).abs() < f64::EPSILON);

    let command_entry = by_executor
        .iter()
        .find(|e| e["executor_type"] == "command")
        .expect("command entry");
    assert_eq!(command_entry["count"], 2);
    assert_eq!(command_entry["success_count"], 1);
    assert!((command_entry["success_rate"].as_f64().unwrap() - 0.5).abs() < f64::EPSILON);
}

// ---------------------------------------------------------------------------
// Test 5: test_suggest_executor_type_returns_best
// ---------------------------------------------------------------------------

#[test]
fn test_suggest_executor_type_returns_best() {
    let store = new_store();

    // noop: 2/2 success
    store
        .insert_scheduler_feedback(
            "run-1",
            Some("n1"),
            "noop",
            "test/suggest",
            true,
            50,
            0,
            0.9,
            0.0,
            None,
        )
        .expect("insert");
    store
        .insert_scheduler_feedback(
            "run-2",
            Some("n2"),
            "noop",
            "test/suggest",
            true,
            60,
            0,
            0.85,
            0.0,
            None,
        )
        .expect("insert");

    // command: 1/2 success
    store
        .insert_scheduler_feedback(
            "run-3",
            Some("n3"),
            "command",
            "test/suggest",
            true,
            200,
            0,
            0.7,
            0.0,
            None,
        )
        .expect("insert");
    store
        .insert_scheduler_feedback(
            "run-4",
            Some("n4"),
            "command",
            "test/suggest",
            false,
            300,
            1,
            0.2,
            0.0,
            Some("command_timeout"),
        )
        .expect("insert");

    let suggestion = store.suggest_executor_type("test/suggest");
    assert_eq!(suggestion, Some("noop".to_string()));
}

// ---------------------------------------------------------------------------
// Test 6: test_suggest_executor_type_empty_returns_none
// ---------------------------------------------------------------------------

#[test]
fn test_suggest_executor_type_empty_returns_none() {
    let store = new_store();

    let suggestion = store.suggest_executor_type("nonexistent/task_group");
    assert!(suggestion.is_none());
}

// ---------------------------------------------------------------------------
// Test 7: test_feedback_recorded_after_tick
// ---------------------------------------------------------------------------

#[test]
fn test_feedback_recorded_after_tick() {
    let store = new_store();
    let run_id = setup_run_with_nodes(&store);

    store
        .tick_with_executor(&run_id, "test", 0, &NoopNodeExecutor)
        .expect("tick");

    let feedback = store.get_feedback_for_run(&run_id).expect("get feedback");
    assert!(
        !feedback.is_empty(),
        "feedback should be recorded after tick"
    );

    let first = &feedback[0];
    assert_eq!(first.run_id, run_id);
    assert_eq!(first.executor_type, "noop");
    assert!(first.success);
}

// ---------------------------------------------------------------------------
// Test 8: test_feedback_recorded_on_retry
// ---------------------------------------------------------------------------

#[test]
fn test_feedback_recorded_on_retry() {
    let store = new_store();
    let run_id = setup_run_with_nodes(&store);

    // Tick with a fail executor and max_retries=1
    store
        .tick_with_executor(&run_id, "test", 1, &FailNodeExecutor::default())
        .expect("tick");

    let feedback = store.get_feedback_for_run(&run_id).expect("get feedback");
    assert!(!feedback.is_empty(), "feedback should be recorded on retry");

    let first = &feedback[0];
    assert!(!first.success, "failed node should have success=false");
    assert_eq!(first.executor_type, "fail");
    assert_eq!(first.retry_count, 1);
}

// ---------------------------------------------------------------------------
// Test 9: test_controller_uses_suggested_executor
// ---------------------------------------------------------------------------

#[test]
fn test_controller_uses_suggested_executor() {
    let store = new_store();

    // Seed feedback so that "stub" has 100% success for analyze/execute with more records
    for i in 0..3 {
        store
            .insert_scheduler_feedback(
                &format!("run-seed-{i}"),
                Some(&format!("n-seed-{i}")),
                "stub",
                "analyze/execute",
                true,
                100,
                0,
                0.9,
                0.0,
                None,
            )
            .expect("seed feedback");
    }

    // Create a run for the controller
    let run_id = setup_run_with_nodes(&store);

    let config = crate::workflow::dynamic_controller::DynamicControllerConfig {
        record_feedback: true,
        auto_fix_on_failure: false,
        ..Default::default()
    };
    let mut ctrl = crate::workflow::dynamic_controller::DynamicWorkflowController::new(config);
    let executor = NoopNodeExecutor;

    let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    // The suggestion should reflect the seeded feedback
    assert!(
        result.suggested_executor_type.is_some(),
        "controller should return suggested_executor_type when record_feedback=true"
    );
    assert_eq!(
        result.suggested_executor_type,
        Some("stub".to_string()),
        "should suggest stub executor based on seeded feedback"
    );
}

// ---------------------------------------------------------------------------
// Test 10: test_feedback_persists_across_ticks
// ---------------------------------------------------------------------------

#[test]
fn test_feedback_persists_across_ticks() {
    let store = new_store();
    let run_id = setup_run_with_nodes(&store);

    // First tick (executes n1, task_type=analyze)
    store
        .tick_with_executor(&run_id, "test", 0, &NoopNodeExecutor)
        .expect("tick 1");

    let feedback_after_1 = store
        .get_feedback_for_run(&run_id)
        .expect("feedback after tick 1");
    assert_eq!(feedback_after_1.len(), 1, "1 feedback record after 1 tick");
    assert_eq!(feedback_after_1[0].task_group, "analyze/execute");

    // Second tick (executes n2, task_type=execute)
    store
        .tick_with_executor(&run_id, "test", 0, &NoopNodeExecutor)
        .expect("tick 2");

    let feedback_after_2 = store
        .get_feedback_for_run(&run_id)
        .expect("feedback after tick 2");
    assert_eq!(
        feedback_after_2.len(),
        2,
        "2 feedback records after 2 ticks"
    );

    // Verify different task_groups are recorded
    let analyze_feedback = store
        .get_feedback_for_task_group("analyze/execute", 100)
        .expect("analyze feedback");
    assert_eq!(analyze_feedback.len(), 1, "1 analyze/execute feedback");

    let execute_feedback = store
        .get_feedback_for_task_group("execute/execute", 100)
        .expect("execute feedback");
    assert_eq!(execute_feedback.len(), 1, "1 execute/execute feedback");
}
