use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use engine::http_server::{build_axum_router, AxumApiState};
use engine::storage::local_product_store::{LocalProductStore, WorkflowPlanIds};
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn sample_artifact(run_id: &str, dispatch_id: Option<&str>) -> Value {
    let step = json!({
        "adapter_step_id": "step-1",
        "adapter_run_id": run_id,
        "step_index": 0,
        "node_name": "root",
        "agent_role": "unknown",
        "operation_kind": "control",
        "input_tokens": 100,
        "output_tokens": 50,
        "context_tokens": 30,
        "repeated_context_tokens": 5,
        "retrieved_refs_count": 0,
        "retrieved_ref_tokens": 0,
        "tool_name": null,
        "tool_call_id": null,
        "status": "pass",
        "error_kind": "none",
        "state_read_bytes": 0,
        "state_write_bytes": 0
    });
    let derived_metrics = json!({
        "total_tokens": 180,
        "tokens_per_passing_run": 180,
        "cost_per_passing_run": 0.01
    });
    let mut scorecard = json!({
        "schema_version": "token_efficiency_scorecard.v1",
        "adapter_run_id": run_id,
        "runtime_kind": "native_harness",
        "runtime_version": "native-harness",
        "scenario_id": "native-run",
        "mode": "native_control_plane",
        "state_strategy": "mixed",
        "status": "pass",
        "pass_fail_reason": "bounded native summary exported",
        "quality_score": 1.0,
        "quality_method": "test",
        "input_token_total": 100,
        "output_token_total": 50,
        "context_token_total": 30,
        "repeated_context_token_total": 5,
        "retrieved_ref_token_total": 0,
        "tool_call_count": 1,
        "redundant_tool_call_count": 0,
        "retry_count": 0,
        "step_count": 1,
        "duration_ms": 25,
        "estimated_cost_usd": 0.01,
        "raw_trace_artifact_id": "bounded-source-artifact",
        "redaction_status": "redacted",
        "derived_metrics": derived_metrics,
        "steps": [step]
    });
    if let Some(dispatch_id) = dispatch_id {
        scorecard["dispatch_id"] = json!(dispatch_id);
    }
    json!({
        "schema_version": "native_scorecard_artifact.v1",
        "artifact_kind": "token_efficiency_scorecard",
        "storage": "app_owned_artifact_json_export",
        "read_only": true,
        "created_at": "2026-07-06T00:00:00Z",
        "artifact_id": format!("scorecard-{run_id}-abc123"),
        "content_sha256": "abc123",
        "scorecard_schema_version": "token_efficiency_scorecard.v1",
        "scorecard": scorecard,
        "next_storage_integration": "removed on persistence"
    })
}

fn make_store() -> (LocalProductStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("scorecards.db")).unwrap();
    (store, dir)
}

fn single_node_plan(ids: &WorkflowPlanIds) -> Value {
    json!({
        "schema_version": "workflow_plan.v1",
        "plan_id": ids.plan_id,
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "status": "planned_read_only",
        "analysis": {"summary": "single node scorecard regression"},
        "boundaries": {"provider_calls": "disabled"},
        "graph": {
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "nodes": [{
                "node_id": "node-1",
                "task_type": "analysis",
                "name": "safe scorecard node",
                "status": "pending"
            }],
            "edges": []
        }
    })
}

fn create_single_node_run(store: &LocalProductStore, raw_request: &str) -> String {
    let plan = store
        .create_workflow_plan(raw_request, "test", "tester", |ids, _| {
            Ok(single_node_plan(ids))
        })
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "tester")
        .unwrap();
    run["run_id"].as_str().unwrap().to_string()
}

#[test]
fn native_scorecard_artifact_persists_and_reads_by_run_and_dispatch() {
    let (store, _dir) = make_store();
    let stored = store
        .record_native_scorecard_artifact(
            &sample_artifact("run-score", Some("dispatch-score")),
            "tester",
        )
        .unwrap();

    assert_eq!(stored["schema_version"], "native_scorecard_artifact.v1");
    assert_eq!(stored["storage"], "local_product_store");
    assert_eq!(stored["read_only"], true);
    assert_eq!(stored["metadata_only"], true);
    assert_eq!(stored["target_repository_writes"], "disabled");
    assert!(stored.get("next_storage_integration").is_none());

    let by_id = store
        .get_native_scorecard_artifact("scorecard-run-score-abc123")
        .unwrap()
        .unwrap();
    assert_eq!(by_id["artifact_id"], "scorecard-run-score-abc123");

    let by_run = store
        .native_scorecard_artifacts_by_run("run-score", 10)
        .unwrap();
    assert_eq!(by_run.len(), 1);
    assert_eq!(by_run[0]["scorecard"]["adapter_run_id"], "run-score");

    let by_dispatch = store
        .native_scorecard_artifacts_by_dispatch("dispatch-score", 10)
        .unwrap();
    assert_eq!(by_dispatch.len(), 1);
    assert_eq!(by_dispatch[0]["scorecard"]["dispatch_id"], "dispatch-score");
}

#[test]
fn native_scorecard_artifact_rejects_schema_drift_and_raw_trace_fields() {
    let (store, _dir) = make_store();

    let mut invalid_schema = sample_artifact("run-bad-schema", None);
    invalid_schema["scorecard_schema_version"] = json!("other.v1");
    let error = store
        .record_native_scorecard_artifact(&invalid_schema, "tester")
        .unwrap_err();
    assert!(error.contains("scorecard_schema_version"));

    let mut raw_trace = sample_artifact("run-raw", None);
    raw_trace["scorecard"]["raw_trace"] = json!({"prompt": "do not store"});
    let error = store
        .record_native_scorecard_artifact(&raw_trace, "tester")
        .unwrap_err();
    assert!(error.contains("raw trace field is not allowed"));

    let mut secret_shaped = sample_artifact("run-secret-shaped", None);
    secret_shaped["scorecard"]["steps"][0]["api_secret_value"] = json!("do not store");
    let error = store
        .record_native_scorecard_artifact(&secret_shaped, "tester")
        .unwrap_err();
    assert!(error.contains("raw trace field is not allowed"));
}

#[test]
fn native_scorecard_artifact_does_not_create_target_output_artifacts() {
    let (store, _dir) = make_store();
    store
        .record_native_scorecard_artifact(&sample_artifact("run-no-target", None), "tester")
        .unwrap();

    assert!(store
        .get_supervised_patch_artifact("scorecard-run-no-target-abc123")
        .unwrap()
        .is_none());
    assert!(store.supervised_patch_artifacts(10).unwrap().is_empty());
}

#[tokio::test]
async fn scorecard_api_reads_by_run_dispatch_and_artifact_id() {
    let (store, _dir) = make_store();
    store
        .record_native_scorecard_artifact(
            &sample_artifact("run-api", Some("dispatch-api")),
            "tester",
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let by_run = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scorecards?run_id=run-api")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(by_run.status(), StatusCode::OK);
    let by_run_body = response_json(by_run).await;
    assert_eq!(by_run_body["read_only"], true);
    assert_eq!(by_run_body["target_repository_writes"], "disabled");
    assert_eq!(by_run_body["artifacts"].as_array().unwrap().len(), 1);

    let by_dispatch = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scorecards?dispatch_id=dispatch-api")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(by_dispatch.status(), StatusCode::OK);
    let by_dispatch_body = response_json(by_dispatch).await;
    assert_eq!(
        by_dispatch_body["artifacts"][0]["scorecard"]["dispatch_id"],
        "dispatch-api"
    );

    let detail = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scorecards/scorecard-run-api-abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body = response_json(detail).await;
    assert_eq!(
        detail_body["artifact"]["artifact_id"],
        "scorecard-run-api-abc123"
    );
}

#[test]
fn successful_native_run_automatically_persists_scorecard_artifact() {
    let (store, _dir) = make_store();
    let run_id = create_single_node_run(&store, "safe scorecard success");

    let tick = store.tick_workflow_run(&run_id, "tester").unwrap();
    assert_eq!(tick["run"]["status"], "completed");

    let artifacts = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .unwrap();
    assert_eq!(artifacts.len(), 1);
    let scorecard = &artifacts[0]["scorecard"];
    assert_eq!(scorecard["schema_version"], "token_efficiency_scorecard.v1");
    assert_eq!(scorecard["adapter_run_id"], run_id);
    assert_eq!(scorecard["status"], "pass");
    assert_eq!(
        scorecard["derived_metrics"]["tokens_per_passing_run"],
        scorecard["derived_metrics"]["total_tokens"]
    );
    assert_eq!(artifacts[0]["read_only"], true);
    assert_eq!(artifacts[0]["target_repository_writes"], "disabled");
}

#[test]
fn failed_native_run_automatically_persists_scorecard_without_passing_metric() {
    let (store, _dir) = make_store();
    let run_id = create_single_node_run(&store, "safe scorecard failure");
    let executor = engine::node_executor::FailNodeExecutor::default();

    let tick = store
        .tick_with_executor(&run_id, "tester", 0, &executor)
        .unwrap();
    assert_eq!(tick["run"]["status"], "failed");

    let artifacts = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .unwrap();
    assert_eq!(artifacts.len(), 1);
    let scorecard = &artifacts[0]["scorecard"];
    assert_eq!(scorecard["status"], "fail");
    assert!(scorecard["derived_metrics"]["tokens_per_passing_run"].is_null());
    assert!(scorecard["derived_metrics"]["cost_per_passing_run"].is_null());
}

#[test]
fn automatic_scorecard_recording_is_idempotent_for_same_run() {
    let (store, _dir) = make_store();
    let run_id = create_single_node_run(&store, "safe scorecard idempotent");

    store.tick_workflow_run(&run_id, "tester").unwrap();
    let first = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .unwrap();
    assert_eq!(first.len(), 1);

    let replayed = store
        .record_automatic_native_scorecard_for_run(&run_id, "tester")
        .unwrap()
        .unwrap();
    assert_eq!(replayed["artifact_id"], first[0]["artifact_id"]);
    let second = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .unwrap();
    assert_eq!(second.len(), 1);
}

#[tokio::test]
async fn automatic_scorecard_is_visible_in_api_and_operator_evidence_metadata() {
    let (store, _dir) = make_store();
    let run_id = create_single_node_run(&store, "safe scorecard api visibility");
    store.tick_workflow_run(&run_id, "tester").unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let scorecards = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/scorecards?run_id={run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(scorecards.status(), StatusCode::OK);
    let scorecards_body = response_json(scorecards).await;
    assert_eq!(scorecards_body["artifacts"].as_array().unwrap().len(), 1);

    let evidence = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/operator/evidence/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence.status(), StatusCode::OK);
    let evidence_body = response_json(evidence).await;
    assert_eq!(evidence_body["scorecard_artifact_count"], 1);
    assert_eq!(evidence_body["scorecards"][0]["read_only"], true);
    assert!(evidence_body["scorecards"][0].get("steps").is_none());
}

struct MetricsNodeExecutor;

impl engine::node_executor::NodeExecutor for MetricsNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "provider"
    }

    fn execute_node(
        &self,
        input: &engine::node_executor::NodeExecutionInput,
    ) -> engine::node_executor::NodeExecutionOutput {
        let is_tool = input.task_type == "command" || input.task_type.contains("tool");
        engine::node_executor::NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: if is_tool { "command" } else { "provider" }.to_string(),
            output: None,
            error_domain: None,
            error_message: None,
            input_tokens: Some(if is_tool { 7 } else { 120 }),
            output_tokens: Some(if is_tool { 3 } else { 40 }),
            estimated_cost: Some(if is_tool { 0.0 } else { 0.012 }),
            latency_ms: Some(if is_tool { 9 } else { 33 }),
        }
    }
}

fn metrics_plan(ids: &WorkflowPlanIds) -> Value {
    json!({
        "schema_version": "workflow_plan.v1",
        "plan_id": ids.plan_id,
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "status": "planned_read_only",
        "analysis": {"summary": "scorecard metrics regression"},
        "boundaries": {"provider_calls": "disabled"},
        "graph": {
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "nodes": [
                {
                    "node_id": "model-1",
                    "task_type": "analysis",
                    "name": "model evidence node",
                    "status": "pending",
                    "context_tokens": 80,
                    "retrieved_refs_count": 2,
                    "retrieved_ref_tokens": 30,
                    "repeated_context_tokens": 10,
                    "state_read_bytes": 512,
                    "state_write_bytes": 128
                },
                {
                    "node_id": "tool-1",
                    "task_type": "command",
                    "name": "safe command node",
                    "status": "pending",
                    "context_tokens": 5,
                    "retrieved_refs_count": 0,
                    "retrieved_ref_tokens": 0,
                    "repeated_context_tokens": 0
                }
            ],
            "edges": []
        }
    })
}

fn create_metrics_run(store: &LocalProductStore) -> String {
    let plan = store
        .create_workflow_plan("safe metrics run", "test", "tester", |ids, _| {
            Ok(metrics_plan(ids))
        })
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "tester")
        .unwrap();
    run["run_id"].as_str().unwrap().to_string()
}

#[test]
fn automatic_scorecard_uses_native_executor_metrics_when_available() {
    let (store, _dir) = make_store();
    let run_id = create_metrics_run(&store);
    let executor = MetricsNodeExecutor;

    assert_eq!(
        store
            .tick_with_executor(&run_id, "tester", 0, &executor)
            .unwrap()["run"]["status"],
        "running"
    );
    assert_eq!(
        store
            .tick_with_executor(&run_id, "tester", 0, &executor)
            .unwrap()["run"]["status"],
        "completed"
    );

    let artifacts = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .unwrap();
    assert_eq!(artifacts.len(), 1);
    let scorecard = &artifacts[0]["scorecard"];
    assert_eq!(scorecard["input_token_total"], 127);
    assert_eq!(scorecard["output_token_total"], 43);
    assert_eq!(scorecard["context_token_total"], 85);
    assert_eq!(scorecard["retrieved_ref_token_total"], 30);
    assert_eq!(scorecard["repeated_context_token_total"], 10);
    assert_eq!(scorecard["tool_call_count"], 1);
    assert_eq!(scorecard["redundant_tool_call_count"], 0);
    assert_eq!(scorecard["retry_count"], 0);
    assert_eq!(scorecard["step_count"], 2);
    assert!(scorecard["duration_ms"].as_i64().unwrap() >= 0);
    assert_eq!(scorecard["estimated_cost_usd"], 0.012);
    assert_eq!(scorecard["steps"][0]["duration_ms"], 33);
    assert_eq!(scorecard["steps"][1]["operation_kind"], "tool_call");
    assert_eq!(scorecard["steps"][1]["input_tokens"], 7);
}

#[test]
fn automatic_scorecard_retry_and_tool_counts_are_from_native_evidence() {
    let (store, _dir) = make_store();
    let run_id = create_single_node_run(&store, "safe scorecard retry failure");
    let executor = engine::node_executor::FailNodeExecutor::default();

    assert_eq!(
        store
            .tick_with_executor(&run_id, "tester", 2, &executor)
            .unwrap()["action"],
        "node_retry"
    );
    assert_eq!(
        store
            .tick_with_executor(&run_id, "tester", 2, &executor)
            .unwrap()["action"],
        "node_retry"
    );
    let terminal = store
        .tick_with_executor(&run_id, "tester", 2, &executor)
        .unwrap();
    assert_eq!(terminal["run"]["status"], "failed");

    let artifacts = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .unwrap();
    let scorecard = &artifacts[0]["scorecard"];
    assert_eq!(scorecard["retry_count"], 2);
    assert_eq!(scorecard["tool_call_count"], 0);
    assert_eq!(scorecard["redundant_tool_call_count"], 0);
    assert!(scorecard["derived_metrics"]["tokens_per_passing_run"].is_null());
}

#[test]
fn automatic_scorecard_safely_defaults_missing_metrics_to_zero() {
    let (store, _dir) = make_store();
    let run_id = create_single_node_run(&store, "safe scorecard no metrics");

    store.tick_workflow_run(&run_id, "tester").unwrap();
    let artifacts = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .unwrap();
    let scorecard = &artifacts[0]["scorecard"];
    assert_eq!(scorecard["input_token_total"], 0);
    assert_eq!(scorecard["output_token_total"], 0);
    assert_eq!(scorecard["context_token_total"], 0);
    assert_eq!(scorecard["retrieved_ref_token_total"], 0);
    assert_eq!(scorecard["repeated_context_token_total"], 0);
    assert_eq!(scorecard["tool_call_count"], 0);
    assert_eq!(scorecard["redundant_tool_call_count"], 0);
    assert_eq!(scorecard["retry_count"], 0);
    assert_eq!(scorecard["derived_metrics"]["total_tokens"], 0);
}
