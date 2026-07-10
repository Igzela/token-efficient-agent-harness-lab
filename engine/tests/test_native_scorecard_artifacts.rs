#![recursion_limit = "256"]

use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use engine::http_server::{build_axum_router, AxumApiState};
use engine::local_scorecard_import::import_scorecard_artifacts;
use engine::storage::local_product_store::{LocalProductStore, WorkflowPlanIds};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn canonical_hash(scorecard: &Value) -> String {
    hex::encode(Sha256::digest(scorecard.to_string().as_bytes()))
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
        "total_tokens": 150,
        "context_share": 0.2,
        "repeated_context_ratio": 0.166667,
        "tool_redundancy_ratio": 0.0,
        "tokens_per_passing_run": 150,
        "cost_per_passing_run": 0.01,
        "step_retry_ratio": 0.0
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
    let content_sha256 = canonical_hash(&scorecard);
    json!({
        "schema_version": "native_scorecard_artifact.v1",
        "artifact_kind": "token_efficiency_scorecard",
        "storage": "app_owned_artifact_json_export",
        "read_only": true,
        "created_at": "2026-07-06T00:00:00Z",
        "artifact_id": format!("scorecard-{run_id}-abc123"),
        "content_sha256": content_sha256,
        "scorecard_schema_version": "token_efficiency_scorecard.v1",
        "scorecard": scorecard,
        "next_storage_integration": "removed on persistence"
    })
}

fn local_runner_artifact(run_id: &str, mode: &str) -> Value {
    let stateful = mode == "stateful_store";
    let total_tokens = if stateful { 130 } else { 220 };
    let repeated_context = if stateful { 12 } else { 80 };
    let retrieved_tokens = if stateful { 10 } else { 0 };
    let scorecard = json!({
        "schema_version": "token_efficiency_scorecard.v1",
        "adapter_run_id": run_id,
        "runtime_kind": "native_harness",
        "runtime_version": "provider-gated-real-runner.v1",
        "scenario_id": "provider_gated_remember_dont_reread_runner",
        "mode": mode,
        "state_strategy": if stateful { "durable_state" } else { "full_history" },
        "status": "pass",
        "pass_fail_reason": "same score threshold met",
        "quality_score": 1.0,
        "quality_method": "rule",
        "input_token_total": total_tokens - 20,
        "output_token_total": 20,
        "context_token_total": total_tokens - 40,
        "repeated_context_token_total": repeated_context,
        "retrieved_ref_token_total": retrieved_tokens,
        "tool_call_count": 2,
        "redundant_tool_call_count": 0,
        "retry_count": 0,
        "step_count": 2,
        "duration_ms": 10,
        "estimated_cost_usd": 0.0,
        "raw_trace_artifact_id": format!("bounded-provider-gated-runner-{mode}"),
        "redaction_status": "redacted",
        "derived_metrics": {
            "total_tokens": total_tokens,
            "context_share": if stateful { 0.692308 } else { 0.818182 },
            "repeated_context_ratio": if stateful { 0.133333 } else { 0.444444 },
            "tool_redundancy_ratio": 0.0,
            "tokens_per_passing_run": total_tokens,
            "cost_per_passing_run": 0.0,
            "step_retry_ratio": 0.0
        },
        "steps": [
            {
                "adapter_step_id": format!("{run_id}-iter-00"),
                "adapter_run_id": run_id,
                "step_index": 0,
                "node_name": "real_experiment_iteration_00",
                "agent_role": "executor",
                "operation_kind": "model_call",
                "input_tokens": 50,
                "output_tokens": 10,
                "context_tokens": 40,
                "repeated_context_tokens": 0,
                "retrieved_refs_count": 0,
                "retrieved_ref_tokens": 0,
                "tool_name": null,
                "tool_call_id": null,
                "status": "pass",
                "error_kind": "none",
                "state_read_bytes": 0,
                "state_write_bytes": 0
            },
            {
                "adapter_step_id": format!("{run_id}-iter-01"),
                "adapter_run_id": run_id,
                "step_index": 1,
                "node_name": "real_experiment_iteration_01",
                "agent_role": "executor",
                "operation_kind": "model_call",
                "input_tokens": total_tokens - 70,
                "output_tokens": 10,
                "context_tokens": total_tokens - 80,
                "repeated_context_tokens": repeated_context,
                "retrieved_refs_count": if stateful { 1 } else { 0 },
                "retrieved_ref_tokens": retrieved_tokens,
                "tool_name": null,
                "tool_call_id": null,
                "status": "pass",
                "error_kind": "none",
                "state_read_bytes": if stateful { 3 } else { 0 },
                "state_write_bytes": if stateful { 96 } else { 0 }
            }
        ]
    });
    let content_sha256 = canonical_hash(&scorecard);
    json!({
        "schema_version": "native_scorecard_artifact.v1",
        "artifact_kind": "token_efficiency_scorecard",
        "storage": "app_owned_artifact_json_export",
        "read_only": true,
        "created_at": "1970-01-01T00:00:00Z",
        "artifact_id": format!("scorecard-{run_id}-{mode}"),
        "content_sha256": content_sha256,
        "scorecard_schema_version": "token_efficiency_scorecard.v1",
        "metadata_only": true,
        "target_repository_writes": "disabled",
        "scorecard": scorecard
    })
}

fn langgraph_artifact(run_id: &str, mode: &str) -> Value {
    let stateful = mode == "stateful_store";
    let input_tokens = if stateful { 7_200 } else { 12_000 };
    let output_tokens = 900;
    let total_tokens = input_tokens + output_tokens;
    let context_tokens = if stateful { 5_000 } else { 10_000 };
    let repeated_tokens = if stateful { 600 } else { 3_900 };
    let cost = if stateful { 0.09 } else { 0.138 };
    let scorecard = json!({
        "schema_version": "token_efficiency_scorecard.v1",
        "adapter_run_id": run_id,
        "runtime_kind": "langgraph",
        "runtime_version": "1.0.2",
        "scenario_id": "langgraph_real_pilot",
        "mode": mode,
        "state_strategy": if stateful { "durable_state" } else { "full_history" },
        "status": "pass",
        "pass_fail_reason": "deterministic evaluator passed",
        "quality_score": 1.0,
        "quality_method": "rule",
        "comparison_contract": {
            "scenario_digest": "1".repeat(64),
            "task_digest": "2".repeat(64),
            "runtime_kind": "langgraph",
            "runtime_version": "1.0.2",
            "provider_id": "no-external-provider",
            "model_id": "deterministic-python-node",
            "tokenizer_id": "cl100k_base",
            "pricing_id": "offline-zero-cost.v1",
            "input_cost_per_1k_usd": 0.01,
            "output_cost_per_1k_usd": 0.02,
            "quality_method": "rule",
            "quality_threshold": 1.0,
            "evaluator_version": "rule-evaluator.v1",
            "redaction_policy": "summary-only.v1",
            "retry_policy": "no-retry.v1",
            "seed": 165
        },
        "input_token_total": input_tokens,
        "output_token_total": output_tokens,
        "context_token_total": context_tokens,
        "repeated_context_token_total": repeated_tokens,
        "retrieved_ref_token_total": if stateful { 800 } else { 0 },
        "tool_call_count": 4,
        "redundant_tool_call_count": 0,
        "retry_count": if stateful { 0 } else { 1 },
        "step_count": 0,
        "duration_ms": if stateful { 18_000 } else { 22_000 },
        "estimated_cost_usd": cost,
        "raw_trace_artifact_id": format!("bounded-langgraph-{mode}"),
        "redaction_status": "redacted",
        "derived_metrics": {
            "total_tokens": total_tokens,
            "context_share": if stateful { 0.617284 } else { 0.775194 },
            "repeated_context_ratio": if stateful { 0.12 } else { 0.39 },
            "tool_redundancy_ratio": 0.0,
            "tokens_per_passing_run": total_tokens,
            "cost_per_passing_run": cost,
            "step_retry_ratio": if stateful { 0.0 } else { 1.0 }
        },
        "steps": []
    });
    let content_sha256 = canonical_hash(&scorecard);
    json!({
        "schema_version": "scorecard_artifact.v2",
        "artifact_kind": "token_efficiency_scorecard",
        "runtime_kind": "langgraph",
        "storage": "app_owned_artifact_json_export",
        "read_only": true,
        "metadata_only": true,
        "target_repository_writes": "disabled",
        "created_at": "2026-07-10T00:00:00Z",
        "artifact_id": format!("scorecard-{run_id}-{}", &content_sha256[..12]),
        "content_sha256": content_sha256,
        "scorecard_schema_version": "token_efficiency_scorecard.v1",
        "scorecard": scorecard
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
fn local_runner_scorecard_artifact_persists_and_reads_by_run_and_id() {
    let (store, _dir) = make_store();
    let artifact = local_runner_artifact("real-runner-stateful_store", "stateful_store");
    let artifact_id = artifact["artifact_id"].as_str().unwrap().to_string();
    let stored = store
        .record_native_scorecard_artifact(&artifact, "local-runner-import")
        .unwrap();

    assert_eq!(stored["storage"], "local_product_store");
    assert_eq!(stored["metadata_only"], true);
    assert_eq!(stored["target_repository_writes"], "disabled");
    assert_eq!(
        stored["scorecard"]["scenario_id"],
        "provider_gated_remember_dont_reread_runner"
    );
    assert_eq!(stored["scorecard"]["mode"], "stateful_store");
    assert_eq!(stored["scorecard"]["state_strategy"], "durable_state");
    assert_eq!(stored["scorecard"]["derived_metrics"]["total_tokens"], 130);

    let by_id = store
        .get_native_scorecard_artifact(&artifact_id)
        .unwrap()
        .unwrap();
    assert_eq!(by_id["artifact_id"], artifact_id);

    let by_run = store
        .native_scorecard_artifacts_by_run("real-runner-stateful_store", 10)
        .unwrap();
    assert_eq!(by_run.len(), 1);
    assert_eq!(by_run[0]["artifact_id"], artifact_id);
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
fn scorecard_persistence_rejects_hash_and_derived_metric_tampering() {
    let (store, _dir) = make_store();

    let mut hash_mismatch = sample_artifact("run-hash-mismatch", None);
    hash_mismatch["content_sha256"] = json!("a".repeat(64));
    let error = store
        .record_scorecard_artifact(&hash_mismatch, "tester")
        .unwrap_err();
    assert!(error.contains("content_sha256"));

    let mut derived_mismatch = sample_artifact("run-derived-mismatch", None);
    derived_mismatch["scorecard"]["derived_metrics"]["total_tokens"] = json!(999);
    let content_sha256 = canonical_hash(&derived_mismatch["scorecard"]);
    derived_mismatch["content_sha256"] = json!(content_sha256);
    let error = store
        .record_scorecard_artifact(&derived_mismatch, "tester")
        .unwrap_err();
    assert!(error.contains("derived_metrics"));
}

#[test]
fn scorecard_persistence_rejects_unbounded_summary_strings() {
    let (store, _dir) = make_store();
    let mut artifact = sample_artifact("run-unbounded-string", None);
    artifact["scorecard"]["pass_fail_reason"] = json!("x".repeat(1025));
    artifact["content_sha256"] = json!(canonical_hash(&artifact["scorecard"]));

    let error = store
        .record_scorecard_artifact(&artifact, "tester")
        .unwrap_err();

    assert!(error.contains("bounded JSON string"));
}

#[test]
fn generic_v2_langgraph_artifacts_reuse_existing_store_idempotently() {
    let (store, _dir) = make_store();
    let artifact = langgraph_artifact("lg-stateful", "stateful_store");

    let stored = store
        .record_scorecard_artifact(&artifact, "langgraph-import")
        .unwrap();
    assert_eq!(stored["schema_version"], "scorecard_artifact.v2");
    assert_eq!(stored["runtime_kind"], "langgraph");
    assert_eq!(stored["scorecard"]["runtime_kind"], "langgraph");

    let repeated = store
        .record_scorecard_artifact(&artifact, "langgraph-import")
        .unwrap();
    assert_eq!(repeated["artifact_id"], stored["artifact_id"]);
    assert_eq!(
        store
            .scorecard_artifacts_by_scenario("langgraph_real_pilot", 10)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn scenario_comparison_rejects_incomparable_contracts() {
    let (store, _dir) = make_store();
    let baseline = langgraph_artifact("lg-stateless", "stateless_reread");
    let mut candidate = langgraph_artifact("lg-stateful", "stateful_store");
    candidate["scorecard"]["comparison_contract"]["provider_id"] = json!("different-provider");
    candidate["content_sha256"] = json!(canonical_hash(&candidate["scorecard"]));
    store
        .record_scorecard_artifact(&baseline, "langgraph-import")
        .unwrap();
    store
        .record_scorecard_artifact(&candidate, "langgraph-import")
        .unwrap();

    let error = store
        .scorecard_comparison_by_scenario("langgraph_real_pilot")
        .unwrap_err();
    assert!(error.contains("provider_id"));
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
        .clone()
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

    let missing = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scorecards/missing-scorecard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(missing).await["code"],
        "native_scorecard_artifact_not_found"
    );
}

#[tokio::test]
async fn scorecard_api_reads_local_runner_artifact_by_run_and_artifact_id() {
    let (store, _dir) = make_store();
    let artifact = local_runner_artifact("real-runner-stateful_store", "stateful_store");
    let artifact_id = artifact["artifact_id"].as_str().unwrap().to_string();
    store
        .record_native_scorecard_artifact(&artifact, "local-runner-import")
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let by_run = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scorecards?run_id=real-runner-stateful_store")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(by_run.status(), StatusCode::OK);
    let by_run_body = response_json(by_run).await;
    assert_eq!(by_run_body["artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(
        by_run_body["artifacts"][0]["scorecard"]["runtime_version"],
        "provider-gated-real-runner.v1"
    );
    assert_eq!(
        by_run_body["artifacts"][0]["scorecard"]["steps"][0]["node_name"],
        "real_experiment_iteration_00"
    );

    let detail = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/scorecards/{artifact_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body = response_json(detail).await;
    assert_eq!(detail_body["artifact"]["artifact_id"], artifact_id);
}

#[tokio::test]
async fn scorecard_api_groups_langgraph_pilot_by_scenario_with_deltas() {
    let (store, _dir) = make_store();
    store
        .record_scorecard_artifact(
            &langgraph_artifact("lg-stateless", "stateless_reread"),
            "langgraph-import",
        )
        .unwrap();
    store
        .record_scorecard_artifact(
            &langgraph_artifact("lg-stateful", "stateful_store"),
            "langgraph-import",
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scorecards?scenario_id=langgraph_real_pilot")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["artifacts"].as_array().unwrap().len(), 2);
    assert_eq!(body["comparison"]["baseline"]["mode"], "stateless_reread");
    assert_eq!(body["comparison"]["candidate"]["mode"], "stateful_store");
    assert_eq!(body["comparison"]["quality_gate"]["both_qualified"], true);
    assert_eq!(body["comparison"]["deltas"]["total_tokens"], -4800);
    assert_eq!(body["comparison"]["deltas"]["estimated_cost_usd"], -0.048);
    assert_eq!(body["comparison"]["deltas"]["duration_ms"], -4000);
    assert_eq!(body["comparison"]["deltas"]["retry_count"], -1);
    assert_eq!(body["comparison"]["advantages"]["token"]["reported"], true);
    assert_eq!(body["comparison"]["advantages"]["cost"]["reported"], true);
}

#[tokio::test]
async fn fixed_langgraph_pilot_import_is_idempotent_and_visible_end_to_end() {
    let (store, _dir) = make_store();
    let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/langgraph_pilot");
    let inputs = vec![
        fixture_dir.join("stateless_reread.artifact.json"),
        fixture_dir.join("stateful_store.artifact.json"),
    ];

    let first = import_scorecard_artifacts(&store, &inputs, "langgraph-pilot-import");
    assert_eq!(first.imported, 2);
    assert_eq!(first.unchanged, 0);
    assert!(first.errors.is_empty());
    let repeated = import_scorecard_artifacts(&store, &inputs, "langgraph-pilot-import");
    assert_eq!(repeated.imported, 0);
    assert_eq!(repeated.unchanged, 2);
    assert!(repeated.errors.is_empty());

    let comparison = store
        .scorecard_comparison_by_scenario("langgraph_offline_state_retention_pilot_2026_07_10")
        .unwrap();
    assert_eq!(comparison["baseline"]["total_tokens"], 38_452);
    assert_eq!(comparison["candidate"]["total_tokens"], 11_294);
    assert_eq!(
        comparison["advantages"]["token"]["reduction_ratio"],
        0.706283
    );
    assert_eq!(comparison["advantages"]["cost"]["reported"], false);
    let audit_events = store
        .search_audit_events_by_run("langgraph-offline-pilot-20260710-stateful_store", 20, 0)
        .unwrap();
    assert!(audit_events
        .iter()
        .any(|event| event["action"] == "scorecard_artifact.record"));
    assert!(!audit_events
        .iter()
        .any(|event| event["action"] == "native_scorecard_artifact.record"));

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/scorecards?scenario_id=langgraph_offline_state_retention_pilot_2026_07_10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["artifacts"].as_array().unwrap().len(), 2);
    assert_eq!(body["comparison"], comparison);
    let serialized = body.to_string().to_ascii_lowercase();
    for forbidden in [
        "raw_prompt",
        "raw_output",
        "transcript",
        "checkpoint",
        "message",
        "span",
        "tool_payload",
    ] {
        assert!(!serialized.contains(forbidden));
    }

    let evidence = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/operator/evidence/langgraph-offline-pilot-20260710-stateful_store")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence.status(), StatusCode::OK);
    let evidence_body = response_json(evidence).await;
    assert_eq!(evidence_body["scorecard_artifact_count"], 1);
    assert_eq!(evidence_body["scorecards"][0]["runtime_kind"], "langgraph");
    assert_eq!(
        evidence_body["scorecards"][0]["schema_version"],
        "scorecard_artifact.v2"
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

#[tokio::test]
async fn local_runner_artifact_is_visible_in_operator_evidence_as_bounded_metadata() {
    let (store, _dir) = make_store();
    store
        .record_native_scorecard_artifact(
            &local_runner_artifact("real-runner-stateful_store", "stateful_store"),
            "local-runner-import",
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let evidence = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/operator/evidence/real-runner-stateful_store")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(evidence.status(), StatusCode::OK);
    let evidence_body = response_json(evidence).await;
    assert_eq!(evidence_body["scorecard_artifact_count"], 1);
    let scorecard = &evidence_body["scorecards"][0];
    assert_eq!(scorecard["runtime_kind"], "native_harness");
    assert_eq!(scorecard["status"], "pass");
    assert_eq!(scorecard["derived_metrics"]["total_tokens"], 130);
    assert!(scorecard.get("steps").is_none());
    assert!(scorecard.get("scorecard").is_none());
    assert!(scorecard.get("raw_trace_artifact_id").is_none());
    let evidence_text = evidence_body.to_string();
    assert!(!evidence_text.contains("real_experiment_iteration_00"));
    assert!(!evidence_text.contains("raw_prompt"));
    assert!(!evidence_text.contains("raw_output"));
    assert!(!evidence_text.contains("transcript"));
}

struct MetricsNodeExecutor;

struct OutputStateMetricsExecutor;

impl engine::node_executor::NodeExecutor for OutputStateMetricsExecutor {
    fn executor_type_name(&self) -> &str {
        "agent_step"
    }

    fn execute_node(
        &self,
        _input: &engine::node_executor::NodeExecutionInput,
    ) -> engine::node_executor::NodeExecutionOutput {
        engine::node_executor::NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "agent_step".to_string(),
            output: Some(
                json!({
                    "action": "update_scratchpad",
                    "state_read_bytes": 321,
                    "state_write_bytes": 123
                })
                .to_string(),
            ),
            error_domain: None,
            error_message: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: Some(0.0),
            latency_ms: Some(1),
        }
    }
}

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
fn automatic_scorecard_extracts_bounded_state_metrics_from_executor_output() {
    let (store, _dir) = make_store();
    let run_id = create_single_node_run(&store, "state metric output regression");

    store
        .tick_with_executor(&run_id, "tester", 0, &OutputStateMetricsExecutor)
        .unwrap();

    let artifacts = store
        .native_scorecard_artifacts_by_run(&run_id, 10)
        .unwrap();
    let step = &artifacts[0]["scorecard"]["steps"][0];
    assert_eq!(step["state_read_bytes"], 321);
    assert_eq!(step["state_write_bytes"], 123);
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
