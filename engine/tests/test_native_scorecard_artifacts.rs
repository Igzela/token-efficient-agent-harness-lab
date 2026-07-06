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
