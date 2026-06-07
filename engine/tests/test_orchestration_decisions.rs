use serde_json::{json, Value};

use engine::storage::local_product_store::LocalProductStore;
use engine::workflow::dynamic_controller::{
    DynamicControllerConfig, DynamicWorkflowController,
};
use engine::node_executor::{NoopNodeExecutor, FailNodeExecutor};

fn setup_store_with_run() -> (LocalProductStore, String) {
    let store = LocalProductStore::new(":memory:").expect("in-memory store");

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
                    "created_at": "2026-06-07T00:00:00Z",
                    "updated_at": "2026-06-07T00:00:00Z",
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

#[test]
fn test_decision_recorded_on_first_tick() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    let decisions = ctrl.decisions();
    assert!(!decisions.is_empty(), "controller should emit decisions");

    let stored = store.get_decisions_for_run(&run_id, 100).unwrap();
    assert!(!stored.is_empty(), "store should persist decisions");

    let first = &stored[0];
    assert_eq!(first.run_id, run_id);
    assert!(!first.decision_id.is_empty());
    assert!(!first.action.is_empty());
    assert!(!first.selected_executor.is_empty());
    assert!(first.confidence_score >= 0.0 && first.confidence_score <= 1.0);
    assert!(!first.created_at.is_empty());
}

#[test]
fn test_decision_schema_version() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    let stored = store.get_decisions_for_run(&run_id, 100).unwrap();
    let first = &stored[0];
    let value = first.to_value();
    let schema_version = value["schema_version"].as_str().unwrap_or("");
    assert!(
        schema_version.starts_with("orchestration_decision"),
        "schema_version should start with orchestration_decision, got: {}",
        schema_version
    );
}

#[test]
fn test_decision_emitted_for_max_ticks() {
    let store = LocalProductStore::new(":memory:").expect("in-memory store");
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
                    "created_at": "2026-06-07T00:00:00Z",
                    "updated_at": "2026-06-07T00:00:00Z",
                    "nodes": [
                        {"node_id": "n1", "task_type": "analyze", "status": "pending"}
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
    let run_id = run.get("run_id").and_then(Value::as_str).unwrap().to_string();

    let config = DynamicControllerConfig {
        max_ticks_per_run: 0,
        auto_fix_on_failure: false,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);
    let executor = NoopNodeExecutor;

    ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    let decisions = store.get_decisions_for_run(&run_id, 100).unwrap();
    assert!(!decisions.is_empty(), "should have a decision for max_ticks");

    let blocked = &decisions[0];
    assert_eq!(blocked.action, "no_action");
    assert!(blocked.blocked_reason.is_some());
    assert!(blocked.confidence_score < 0.5, "blocked decision should have low confidence");
}

#[test]
fn test_decision_emitted_for_terminal_run() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    for _ in 0..20 {
        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        if !result.should_continue {
            break;
        }
    }

    ctrl.tick(&store, &run_id, "test", &executor).expect("tick on terminal");

    let decisions = store.get_decisions_for_run(&run_id, 100).unwrap();
    let terminal_decisions: Vec<_> = decisions
        .iter()
        .filter(|d| d.action == "run_completed" || d.action == "run_failed")
        .collect();
    assert!(
        !terminal_decisions.is_empty(),
        "should have terminal decision"
    );
}

#[test]
fn test_decision_with_fail_executor() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = FailNodeExecutor::default();

    ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    let decisions = store.get_decisions_for_run(&run_id, 100).unwrap();
    assert!(!decisions.is_empty());

    let first = &decisions[0];
    assert_eq!(first.action, "execute_node");
    assert_eq!(first.run_id, run_id);
}

#[test]
fn test_decision_input_signals() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    let decisions = store.get_decisions_for_run(&run_id, 100).unwrap();
    let first = &decisions[0];
    let signals = &first.input_signals;
    assert!(signals.is_object());
    assert_eq!(signals["run_id"].as_str().unwrap(), run_id);
    assert!(signals.get("run_status").is_some());
}

#[test]
fn test_decision_log_stats() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    for _ in 0..5 {
        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        if !result.should_continue {
            break;
        }
    }

    let stats = store.decision_log_stats().unwrap();
    assert!(stats.total_decisions > 0);
    assert!(stats.avg_confidence >= 0.0 && stats.avg_confidence <= 1.0);
    assert!(!stats.by_action.is_null());
}

#[test]
fn test_decision_search() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    let all = store.search_decisions(100, 0, None).unwrap();
    assert!(!all.is_empty());

    let filtered = store
        .search_decisions(100, 0, Some("execute_node"))
        .unwrap();
    assert!(
        filtered.iter().all(|d| d.action.contains("execute")),
        "search should filter by action"
    );
}

#[test]
fn test_decision_confidence_score_range() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    for _ in 0..10 {
        let _ = ctrl.tick(&store, &run_id, "test", &executor);
    }

    let decisions = store.get_decisions_for_run(&run_id, 100).unwrap();
    for d in &decisions {
        assert!(
            d.confidence_score >= 0.0 && d.confidence_score <= 1.0,
            "confidence_score {} should be in [0, 1]",
            d.confidence_score
        );
        assert!(
            ["high", "medium", "low"].contains(&d.confidence.as_str()),
            "confidence should be high/medium/low, got: {}",
            d.confidence
        );
    }
}

#[test]
fn test_decision_mutation_limit_emits_decision() {
    let (store, run_id) = setup_store_with_run();
    let config = DynamicControllerConfig {
        max_mutations_per_run: 0,
        auto_fix_on_failure: true,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);
    let executor = NoopNodeExecutor;

    ctrl.tick(&store, &run_id, "test", &executor).expect("tick");

    let decisions = store.get_decisions_for_run(&run_id, 100).unwrap();
    assert!(
        decisions.iter().any(|d| d.action == "request_approval" || d.blocked_reason.is_some()),
        "mutation limit should emit a decision with blocked reason"
    );
}

#[test]
fn test_multiple_ticks_produce_sequential_decisions() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    let mut tick_count = 0u64;
    for _ in 0..10 {
        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        tick_count += 1;
        if !result.should_continue {
            break;
        }
    }

    assert!(tick_count >= 2, "should tick at least twice for 2-node graph");

    let decisions = store.get_decisions_for_run(&run_id, 100).unwrap();
    assert!(
        decisions.len() >= tick_count as usize,
        "should have at least one decision per tick, got {} decisions for {} ticks",
        decisions.len(),
        tick_count
    );
}

#[test]
fn test_controller_includes_decisions_in_result() {
    let (store, run_id) = setup_store_with_run();
    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
    let executor = NoopNodeExecutor;

    for _ in 0..20 {
        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        if !result.should_continue {
            break;
        }
    }

    let decisions = ctrl.decisions();
    assert!(
        decisions.len() >= 2,
        "controller should accumulate decisions across ticks"
    );
    for d in decisions {
        assert!(d.is_object());
        assert!(d.get("decision_id").is_some());
        assert!(d.get("action").is_some());
        assert!(d.get("confidence").is_some());
    }
}
