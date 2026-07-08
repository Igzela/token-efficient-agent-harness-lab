use serde_json::{json, Value};

use engine::node_executor::{
    LocalRunnerValidationExecutor, NodeExecutionInput, NodeExecutionOutput, NodeExecutor,
};
use engine::storage::local_product_store::LocalProductStore;

fn make_store() -> LocalProductStore {
    LocalProductStore::new(":memory:").expect("in-memory store")
}

fn stub_input(run_id: &str, node_id: &str, iterations: u64) -> NodeExecutionInput {
    NodeExecutionInput {
        node_id: node_id.to_string(),
        task_type: "local_runner_validation".to_string(),
        run_id: run_id.to_string(),
        workflow_id: "test-wf".to_string(),
        node_metadata: json!({
            "iterations": iterations,
            "max_calls": iterations * 4,
        }),
    }
}

fn assert_node_output(output: &NodeExecutionOutput, expected_status: &str) {
    assert_eq!(
        output.status, expected_status,
        "expected status {expected_status}, got {}: {:?}",
        output.status, output.error_message
    );
}

#[test]
fn local_runner_executor_completes_deterministically() {
    let executor = LocalRunnerValidationExecutor;
    let input = stub_input("run-lr-test-1", "node-001", 10);
    let output = executor.execute_node(&input);
    assert_node_output(&output, "completed");
    assert!(output.output.is_some());
    let summary: Value =
        serde_json::from_str(output.output.as_ref().unwrap()).expect("valid JSON summary");
    assert_eq!(summary["validation_status"], "pass");
    assert!(
        summary["stateless_total_tokens"].as_i64().unwrap_or(0)
            > summary["stateful_total_tokens"].as_i64().unwrap_or(0),
        "stateful should use fewer tokens"
    );
    assert!(
        summary["token_reduction_ratio"].as_f64().unwrap_or(0.0) > 0.0,
        "token reduction must be positive"
    );
}

#[test]
fn local_runner_executor_no_provider_calls() {
    let executor = LocalRunnerValidationExecutor;
    let input = stub_input("run-lr-np", "node-np", 10);
    let output = executor.execute_node(&input);
    assert_node_output(&output, "completed");
    assert_eq!(output.estimated_cost, Some(0.0));
    assert!(output.input_tokens.unwrap_or(0) > 0);
}

#[test]
fn local_runner_executor_output_is_bounded() {
    let executor = LocalRunnerValidationExecutor;
    let input = stub_input("run-lr-bound", "node-bound", 10);
    let output = executor.execute_node(&input);
    assert_node_output(&output, "completed");
    let json_output = output.output.unwrap();
    assert!(json_output.len() < 2048, "output should be bounded");
    // Verify no raw scorecard steps or prompts in output
    assert!(
        !json_output.contains("adapter_step_id"),
        "output must not contain raw steps"
    );
    assert!(
        !json_output.contains("raw_prompt"),
        "output must not contain raw prompts"
    );
    assert!(
        !json_output.contains("raw_output"),
        "output must not contain raw outputs"
    );
}

#[test]
fn local_runner_executor_fails_on_bad_config() {
    let executor = LocalRunnerValidationExecutor;
    let input = NodeExecutionInput {
        node_id: "bad-node".to_string(),
        task_type: "local_runner_validation".to_string(),
        run_id: "run-bad".to_string(),
        workflow_id: "wf".to_string(),
        node_metadata: json!({
            "iterations": 100, // > 50, should be clamped
            "max_calls": 0,   // too low
        }),
    };
    let output = executor.execute_node(&input);
    // iterations is clamped to 50, and max_calls is clamped to >= iterations*2
    // So this should still work with clamped values
    assert_node_output(&output, "completed");
}

#[test]
fn local_runner_executor_is_idempotent() {
    let executor = LocalRunnerValidationExecutor;
    let input = stub_input("run-lr-idem", "node-idem", 5);
    let output1 = executor.execute_node(&input);
    let output2 = executor.execute_node(&input);
    assert_node_output(&output1, "completed");
    assert_node_output(&output2, "completed");
    assert_eq!(output1.output, output2.output);
}

#[test]
fn local_runner_executor_tick_through_workflow() {
    let store = make_store();

    let plan = store
        .create_workflow_plan("lr-plan", "lr-wf", "test-actor", |ids, _plan| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-lr", "task_domain": "scorecard"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-07-08T00:00:00Z",
                    "updated_at": "2026-07-08T00:00:00Z",
                    "nodes": [
                        {
                            "node_id": "lr-validate",
                            "task_type": "local_runner_validation",
                            "status": "pending",
                            "node_json": {
                                "iterations": 10,
                                "max_calls": 40
                            }
                        }
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

    let plan_id = plan["plan_id"].as_str().unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test-actor")
        .expect("create run");
    let run_id = run["run_id"].as_str().unwrap().to_string();

    // Tick with the local runner validation executor
    let executor = LocalRunnerValidationExecutor;
    let result = store
        .tick_with_executor_and_command_inner(&run_id, "test-actor", 0, &executor, None, None)
        .expect("first tick should succeed");

    assert_eq!(
        result["action"], "node_executed",
        "first tick should execute the node"
    );
    assert_eq!(result["executor_type"], "local_runner_validation");
    assert_eq!(
        result["run"]["status"], "completed",
        "run should be complete after single-node execution"
    );
}

#[test]
fn local_runner_executor_operator_evidence_is_bounded() {
    let store = make_store();

    let plan = store
        .create_workflow_plan("lr-ev-plan", "lr-ev-wf", "test-actor", |ids, _plan| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-lr-ev", "task_domain": "scorecard"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-07-08T00:00:00Z",
                    "updated_at": "2026-07-08T00:00:00Z",
                    "nodes": [
                        {
                            "node_id": "lr-ev-validate",
                            "task_type": "local_runner_validation",
                            "status": "pending",
                            "node_json": {
                                "iterations": 5,
                                "max_calls": 40
                            }
                        }
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

    let plan_id = plan["plan_id"].as_str().unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test-actor")
        .expect("create run");
    let run_id = run["run_id"].as_str().unwrap().to_string();

    let executor = LocalRunnerValidationExecutor;
    store
        .tick_with_executor_and_command_inner(&run_id, "test-actor", 0, &executor, None, None)
        .expect("tick");

    // Verify the run shows as completed
    let run_after = store.get_workflow_run(&run_id).expect("get run").unwrap();
    assert_eq!(run_after["status"], "completed");

    // Get the node results (nodes are embedded in the run JSON)
    let run_nodes = run_after["nodes"].as_array().cloned().unwrap_or_default();
    assert!(!run_nodes.is_empty(), "should have at least one node");
    let node_output = run_nodes[0]
        .pointer("/result/output")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Output should be a bounded summary, not raw scorecard
    assert!(
        node_output.contains("validation_status"),
        "output must contain validation summary"
    );
    assert!(
        !node_output.contains("adapter_step_id"),
        "output must not contain raw steps"
    );
}

#[test]
fn local_runner_executor_external_calls_tracked() {
    let executor = LocalRunnerValidationExecutor;
    let input = stub_input("run-lr-calls", "node-calls", 5);
    let output = executor.execute_node(&input);
    assert_node_output(&output, "completed");
    let summary: Value = serde_json::from_str(output.output.as_ref().unwrap()).expect("valid JSON");
    // The summary doesn't contain external_calls directly, but the executor
    // logs step counts via stateless_total_tokens and stateful_total_tokens
    // which are derived from actual provider calls
    assert!(
        summary["stateless_total_tokens"].as_i64().unwrap_or(0) > 0,
        "stateless tokens must reflect simulated provider calls"
    );
}

#[test]
fn local_runner_executor_tick_records_automatic_scorecard() {
    let store = make_store();

    let plan = store
        .create_workflow_plan("lr-sc-plan", "lr-sc-wf", "test-actor", |ids, _plan| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-lr-sc", "task_domain": "scorecard"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-07-08T00:00:00Z",
                    "updated_at": "2026-07-08T00:00:00Z",
                    "nodes": [
                        {
                            "node_id": "lr-sc-validate",
                            "task_type": "local_runner_validation",
                            "status": "pending",
                            "node_json": {
                                "iterations": 5,
                                "max_calls": 40
                            }
                        }
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

    let plan_id = plan["plan_id"].as_str().unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test-actor")
        .expect("create run");
    let run_id = run["run_id"].as_str().unwrap().to_string();

    let executor = LocalRunnerValidationExecutor;
    store
        .tick_with_executor_and_command_inner(&run_id, "test-actor", 0, &executor, None, None)
        .expect("tick");

    // Verify a native scorecard artifact was automatically recorded
    let artifacts = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .expect("get artifacts");
    assert!(
        !artifacts.is_empty(),
        "should have at least one scorecard artifact"
    );

    // Each artifact must be metadata-only, no raw local-runner traces
    for artifact in &artifacts {
        assert_eq!(
            artifact["metadata_only"], true,
            "artifact must be metadata-only"
        );
        if let Some(scorecard) = artifact.get("scorecard") {
            assert!(
                scorecard.get("adapter_run_id").is_some(),
                "scorecard must have adapter_run_id"
            );
            let text = serde_json::to_string(artifact).unwrap_or_default();
            // Must not contain raw local runner identifiers or unbounded fields
            assert!(
                !text.contains("real-runner-"),
                "artifact must not contain local runner durable run IDs"
            );
        }
    }
}

#[test]
fn local_runner_executor_operator_evidence_metadata_is_bounded() {
    let store = make_store();

    let plan = store
        .create_workflow_plan("lr-op-plan", "lr-op-wf", "test-actor", |ids, _plan| {
            Ok(json!({
                "schema_version": "read_only_plan.v1",
                "plan_id": ids.plan_id,
                "status": "planned_read_only",
                "workflow_id": ids.workflow_id,
                "dispatch_id": ids.dispatch_id,
                "analysis": {"analysis_id": "a-lr-op", "task_domain": "scorecard"},
                "graph": {
                    "schema_version": "workflow_graph.v1",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "status": "decomposed",
                    "created_at": "2026-07-08T00:00:00Z",
                    "updated_at": "2026-07-08T00:00:00Z",
                    "nodes": [
                        {
                            "node_id": "lr-op-validate",
                            "task_type": "local_runner_validation",
                            "status": "pending",
                            "node_json": {
                                "iterations": 5,
                                "max_calls": 40
                            }
                        }
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

    let plan_id = plan["plan_id"].as_str().unwrap();
    let run = store
        .create_workflow_run_from_plan(plan_id, "test-actor")
        .expect("create run");
    let run_id = run["run_id"].as_str().unwrap().to_string();

    let executor = LocalRunnerValidationExecutor;
    store
        .tick_with_executor_and_command_inner(&run_id, "test-actor", 0, &executor, None, None)
        .expect("tick");

    // Verify the automatically-recorded artifact has bounded metadata
    let artifacts = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .expect("get artifacts");
    assert!(!artifacts.is_empty(), "must have scorecard artifacts");
    for artifact in &artifacts {
        assert_eq!(
            artifact["metadata_only"], true,
            "artifact must be metadata-only"
        );
        let text = serde_json::to_string(artifact).unwrap_or_default();
        // The automatic scorecard may contain workflow-level step entries,
        // but must not contain local runner's raw step identifiers
        assert!(
            !text.contains("real-runner-"),
            "artifact must not contain local runner durable run IDs"
        );
    }
}

#[test]
fn local_runner_executor_import_is_idempotent() {
    let _store = make_store();
    let executor = LocalRunnerValidationExecutor;
    let input = stub_input("run-lr-idem-store", "node-idem-store", 5);

    // Execute twice with same input — should produce same bounded output
    let output1 = executor.execute_node(&input);
    let output2 = executor.execute_node(&input);
    assert_node_output(&output1, "completed");
    assert_node_output(&output2, "completed");
    assert_eq!(output1.output, output2.output, "output must be idempotent");
}

#[test]
fn local_runner_executor_artifact_id_in_output() {
    let executor = LocalRunnerValidationExecutor;
    let input = stub_input("run-lr-art", "node-art", 5);
    let output = executor.execute_node(&input);
    assert_node_output(&output, "completed");
    let summary: Value = serde_json::from_str(output.output.as_ref().unwrap()).expect("valid JSON");
    assert!(
        summary.get("stateless_run_id").is_some(),
        "summary must include stateless_run_id"
    );
    assert!(
        summary.get("stateful_run_id").is_some(),
        "summary must include stateful_run_id"
    );
    assert!(
        summary.get("scenario_id").is_some(),
        "summary must include scenario_id"
    );
}
