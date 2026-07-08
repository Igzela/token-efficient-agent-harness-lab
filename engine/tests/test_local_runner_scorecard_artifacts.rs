use axum::body::{to_bytes, Body};
use axum::http::{Method, Request, StatusCode};
use engine::http_server::{build_axum_router, AxumApiState};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn make_store() -> (LocalProductStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("local-runner-scorecards.db")).unwrap();
    (store, dir)
}

fn local_runner_artifact(run_id: &str, mode: &str, state_strategy: &str, total_tokens: i64) -> Value {
    let input_tokens = total_tokens - 20;
    let output_tokens = 20;
    let repeated_context_tokens = if mode == "stateful_store" { 8 } else { 120 };
    let step = json!({
        "adapter_step_id": format!("{run_id}-step-0"),
        "adapter_run_id": run_id,
        "step_index": 0,
        "node_name": "local runner validation step",
        "agent_role": "executor",
        "operation_kind": "model_call",
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "context_tokens": 160,
        "repeated_context_tokens": repeated_context_tokens,
        "retrieved_refs_count": 1,
        "retrieved_ref_tokens": 24,
        "tool_name": null,
        "tool_call_id": null,
        "status": "pass",
        "error_kind": "none",
        "state_read_bytes": 32,
        "state_write_bytes": 64
    });
    let scorecard = json!({
        "schema_version": "token_efficiency_scorecard.v1",
        "adapter_run_id": run_id,
        "runtime_kind": "native_harness",
        "runtime_version": "provider-gated-real-runner.v1",
        "scenario_id": "provider_gated_remember_dont_reread_runner",
        "mode": mode,
        "state_strategy": state_strategy,
        "status": "pass",
        "pass_fail_reason": "same score threshold met",
        "quality_score": 1.0,
        "quality_method": "rule",
        "input_token_total": input_tokens,
        "output_token_total": output_tokens,
        "context_token_total": 160,
        "repeated_context_token_total": repeated_context_tokens,
        "retrieved_ref_token_total": 24,
        "tool_call_count": 1,
        "redundant_tool_call_count": 0,
        "retry_count": 0,
        "step_count": 1,
        "duration_ms": 5,
        "estimated_cost_usd": 0.0,
        "raw_trace_artifact_id": format!("bounded-provider-gated-runner-{mode}"),
        "redaction_status": "redacted",
        "derived_metrics": {
            "total_tokens": total_tokens,
            "context_share": 0.5,
            "repeated_context_ratio": if mode == "stateful_store" { 0.05 } else { 0.75 },
            "tool_redundancy_ratio": 0.0,
            "tokens_per_passing_run": total_tokens,
            "cost_per_passing_run": 0.0,
            "step_retry_ratio": 0.0
        },
        "steps": [step]
    });
    json!({
        "schema_version": "native_scorecard_artifact.v1",
        "artifact_kind": "token_efficiency_scorecard",
        "storage": "app_owned_artifact_json_export",
        "read_only": true,
        "created_at": "1970-01-01T00:00:00Z",
        "artifact_id": format!("scorecard-{run_id}-{mode}"),
        "content_sha256": format!("local-runner-{mode}"),
        "scorecard_schema_version": "token_efficiency_scorecard.v1",
        "scorecard": scorecard,
        "metadata_only": true,
        "target_repository_writes": "disabled"
    })
}

#[test]
fn local_runner_artifact_persists_as_native_scorecard_artifact() {
    let (store, _dir) = make_store();
    let stored = store
        .record_native_scorecard_artifact(
            &local_runner_artifact(
                "real-runner-stateful_store",
                "stateful_store",
                "durable_state",
                320,
            ),
            "local-runner-validator",
        )
        .unwrap();

    assert_eq!(stored["schema_version"], "native_scorecard_artifact.v1");
    assert_eq!(stored["storage"], "local_product_store");
    assert_eq!(stored["read_only"], true);
    assert_eq!(stored["metadata_only"], true);
    assert_eq!(stored["target_repository_writes"], "disabled");
    assert_eq!(stored["scorecard"]["mode"], "stateful_store");
    assert_eq!(stored["scorecard"]["state_strategy"], "durable_state");

    let by_run = store
        .native_scorecard_artifacts_by_run("real-runner-stateful_store", 10)
        .unwrap();
    assert_eq!(by_run.len(), 1);
    assert_eq!(by_run[0]["scorecard"]["runtime_version"], "provider-gated-real-runner.v1");
}

#[tokio::test]
async fn local_runner_artifact_is_visible_in_operator_evidence_metadata() {
    let (store, _dir) = make_store();
    store
        .record_native_scorecard_artifact(
            &local_runner_artifact(
                "real-runner-stateful_store",
                "stateful_store",
                "durable_state",
                320,
            ),
            "local-runner-validator",
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
    assert_eq!(evidence_body["scorecards"][0]["read_only"], true);
    assert_eq!(evidence_body["scorecards"][0]["runtime_kind"], "native_harness");
    assert_eq!(evidence_body["scorecards"][0]["derived_metrics"]["total_tokens"], 320);
    assert!(evidence_body["scorecards"][0].get("steps").is_none());
}
