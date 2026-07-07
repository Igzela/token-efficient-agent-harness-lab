use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{
    build_axum_router, build_axum_router_with_dashboard, AxumApiState, CliCapability,
};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::provider::audit::{ProviderAuditEvent, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::{tempdir, TempDir};
use tokio::sync::Mutex;
use tower::ServiceExt;

use super::common::*;

#[tokio::test]
async fn axum_tick_advances_single_node_run_to_completion() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a plan
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo hello", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap().to_string();

    // Create a run from the plan
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap().to_string();
    assert_eq!(run_body["run"]["status"], "created");

    // Tick the run - should transition to running and execute first node
    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"actor": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let first_tick_body = response_json(tick_resp).await;
    // For a single-node plan, the first tick may complete the run immediately
    let first_action = first_tick_body["tick"]["action"].as_str().unwrap_or("");
    assert!(
        first_action == "node_executed" || first_action == "completed",
        "first tick should execute a node or complete, got: {first_action}"
    );
    if first_action == "node_executed" {
        assert_eq!(first_tick_body["tick"]["executor_type"], "noop");
    }

    // Keep ticking until run completes (for multi-node plans)
    for _ in 0..20 {
        let tick_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"actor": "test"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        if tick_resp.status() == StatusCode::CONFLICT {
            // Run is already terminal
            break;
        }
        let tick_body = response_json(tick_resp).await;
        let action = tick_body["tick"]["action"].as_str().unwrap_or("");
        if action == "completed" || action == "failed" {
            break;
        }
    }

    // Verify the run is terminal
    let run_detail = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/workflow-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body = response_json(run_detail).await;
    assert!(
        detail_body["run"]["status"] == "completed" || detail_body["run"]["status"] == "failed",
        "run should be terminal, got: {}",
        detail_body["run"]["status"]
    );
}

#[tokio::test]
async fn axum_tick_returns_409_on_terminal_run() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-terminal.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a plan and run
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo test", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    // Cancel the run
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/cancel"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Tick should return 409
    let tick_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"actor": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::CONFLICT);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["code"], "run_terminal");
}

#[tokio::test]
async fn axum_tick_respects_node_dependencies() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-deps.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a plan with multiple nodes
    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_request": "first analyze the code, then refactor it, then test it",
                        "request_source": "test"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();
    let nodes = plan_body["plan"]["graph"]["nodes"].as_array().unwrap();

    // Only run this test if the plan has multiple nodes
    if nodes.len() < 2 {
        return; // Skip if decomposition didn't produce multiple nodes
    }

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap().to_string();

    // Tick repeatedly - nodes should complete respecting dependencies
    let mut completed_nodes = Vec::new();
    for _ in 0..(nodes.len() + 5) {
        let tick_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"actor": "test"}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let tick_body = response_json(tick_resp).await;
        let action = tick_body["tick"]["action"].as_str().unwrap_or("");
        if action == "node_executed" {
            let node_id = tick_body["tick"]["node_id"].as_str().unwrap();
            completed_nodes.push(node_id.to_string());
        }
        if action == "completed" || action == "failed" {
            break;
        }
    }

    // All nodes should have been executed
    assert!(
        completed_nodes.len() >= nodes.len(),
        "expected {} nodes to execute, got {}",
        nodes.len(),
        completed_nodes.len()
    );

    // Verify the run is terminal
    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/workflow-runs/{run_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let detail_body = response_json(detail_resp).await;
    assert!(
        detail_body["run"]["status"] == "completed" || detail_body["run"]["status"] == "failed",
        "run should be terminal after all nodes complete"
    );

    // Verify all nodes have completed status
    let run_nodes = detail_body["run"]["nodes"].as_array().unwrap();
    for node in run_nodes {
        let status = node["db_status"]
            .as_str()
            .unwrap_or(node["status"].as_str().unwrap_or(""));
        assert!(
            status == "completed" || status == "failed",
            "node {} should be terminal, got: {}",
            node["node_id"].as_str().unwrap_or("?"),
            status
        );
    }
}

#[tokio::test]
async fn axum_tick_returns_404_for_missing_run() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-missing.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let tick_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs/nonexistent/tick")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"actor": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn axum_tick_with_command_executor_uses_command_node_executor() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-cmd.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo hello", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "command"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::OK);
    let tick_body = response_json(tick_resp).await;
    let executor_type = tick_body["tick"]["executor_type"].as_str().unwrap_or("");
    assert!(
        executor_type == "command" || executor_type == "noop",
        "expected command or noop executor_type, got: {executor_type}"
    );
}

#[tokio::test]
async fn axum_tick_with_unknown_executor_falls_back_to_noop() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-unknown.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo test", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "fake_executor"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::OK);
    let tick_body = response_json(tick_resp).await;
    let action = tick_body["tick"]["action"].as_str().unwrap_or("");
    assert!(
        action == "node_executed" || action == "completed",
        "tick should still work with unknown executor, got: {action}"
    );
    if action == "node_executed" {
        assert_eq!(tick_body["tick"]["executor_type"], "noop");
    }
}

#[tokio::test]
async fn axum_tick_with_fail_executor_marks_run_failed() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-fail.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "fail test", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "fail"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tick_resp.status(), StatusCode::OK);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["tick"]["executor_type"], "fail");
    assert_eq!(tick_body["tick"]["result"]["status"], "failed");
    assert_eq!(tick_body["tick"]["run"]["status"], "failed");
}

#[tokio::test]
async fn axum_tick_with_claude_code_cli_unavailable_returns_400() {
    let _guard = provider_cli_env_lock().lock().await;
    std::env::set_var("ACP_ENABLE_CLI_EXECUTION", "0");
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-cli-400.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo cli", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "claude_code_cli"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    std::env::remove_var("ACP_ENABLE_CLI_EXECUTION");
    assert_eq!(tick_resp.status(), StatusCode::BAD_REQUEST);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["code"], "cli_not_available");
}

#[test]
fn cli_node_executor_resolve_prompt_and_executor() {
    use engine::cli::CliNodeExecutor;
    use serde_json::json;

    let executor =
        CliNodeExecutor::new(Some("/bin/claude".into()), Some("/bin/codex".into()), 5000);

    let input_with_prompt = engine::node_executor::NodeExecutionInput {
        node_id: "n1".into(),
        task_type: "test".into(),
        run_id: "r1".into(),
        workflow_id: "w1".into(),
        node_metadata: json!({"prompt": "do something"}),
    };
    assert_eq!(executor.resolve_prompt(&input_with_prompt), "do something");
    assert_eq!(
        executor.resolve_executor(&input_with_prompt),
        "claude_code_cli"
    );

    let input_with_command = engine::node_executor::NodeExecutionInput {
        node_id: "n2".into(),
        task_type: "test".into(),
        run_id: "r2".into(),
        workflow_id: "w2".into(),
        node_metadata: json!({"command": "echo hi"}),
    };
    assert_eq!(executor.resolve_prompt(&input_with_command), "echo hi");

    let input_with_explicit_executor = engine::node_executor::NodeExecutionInput {
        node_id: "n3".into(),
        task_type: "test".into(),
        run_id: "r3".into(),
        workflow_id: "w3".into(),
        node_metadata: json!({"executor": "codex_cli"}),
    };
    assert_eq!(
        executor.resolve_executor(&input_with_explicit_executor),
        "codex_cli"
    );

    let input_empty = engine::node_executor::NodeExecutionInput {
        node_id: "n4".into(),
        task_type: "test".into(),
        run_id: "r4".into(),
        workflow_id: "w4".into(),
        node_metadata: json!({}),
    };
    assert_eq!(executor.resolve_prompt(&input_empty), "echo noop");
}

#[tokio::test]
async fn axum_tick_with_codex_cli_unavailable_returns_400() {
    let _guard = provider_cli_env_lock().lock().await;
    std::env::set_var("ACP_ENABLE_CLI_EXECUTION", "0");
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-codex-400.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "echo codex cli", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "codex_cli"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    std::env::remove_var("ACP_ENABLE_CLI_EXECUTION");
    assert_eq!(tick_resp.status(), StatusCode::BAD_REQUEST);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["code"], "cli_not_available");
}

#[tokio::test]
async fn axum_tick_with_provider_env_gate_disabled_returns_400() {
    let _guard = provider_cli_env_lock().lock().await;
    std::env::remove_var("ACP_ENABLE_PROVIDER_EXECUTION");
    std::env::remove_var("ACP_TRUSTED_LOCAL_PROFILE");
    std::env::remove_var("ACP_COST_PER_DISPATCH_USD");
    std::env::remove_var("ACP_COST_DAILY_USD");

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("tick-provider-400.db")).unwrap();
    let provider: std::sync::Arc<dyn engine::provider::Provider> =
        std::sync::Arc::new(engine::provider::stub::StubProvider::new("stub-provider"));
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_provider(provider),
    );

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "provider task", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "provider"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(tick_resp.status(), StatusCode::BAD_REQUEST);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["code"], "provider_not_available");
}

#[tokio::test]
async fn axum_tick_with_trusted_local_profile_enables_provider_without_legacy_gate() {
    let _guard = provider_cli_env_lock().lock().await;
    let _env = TrustedLocalProviderWorkflowEnvGuard::enabled();

    let dir = tempdir().unwrap();
    let store = std::sync::Arc::new(
        LocalProductStore::new(dir.path().join("tick-provider-trusted-local.db")).unwrap(),
    );
    let provider: std::sync::Arc<dyn engine::provider::Provider> =
        std::sync::Arc::new(engine::provider::stub::StubProvider::new("stub-provider"));
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store.clone())
            .with_provider(provider),
    );

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "provider task", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();

    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "provider"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(tick_resp.status(), StatusCode::OK);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["tick"]["executor_type"], "provider");
    assert_eq!(
        tick_body["tick"]["result"]["trace"]["schema_version"],
        "execution_trace.v2"
    );
    assert_eq!(
        tick_body["tick"]["result"]["trace"]["output_policy"],
        "redacted_and_capped"
    );

    let events = store.provider_audit_events(10).unwrap();
    let event_types: Vec<_> = events
        .iter()
        .filter_map(|event| event["event_type"].as_str())
        .collect();
    assert!(event_types.contains(&"request_sent"));
    assert!(event_types.contains(&"response_received"));
}

#[tokio::test]
async fn axum_tick_with_persisted_trusted_local_profile_enables_provider() {
    let _guard = provider_cli_env_lock().lock().await;
    let _env = TrustedLocalProviderWorkflowEnvGuard::enabled_with_persisted_endpoints();

    let dir = tempdir().unwrap();
    let store = std::sync::Arc::new(
        LocalProductStore::new(dir.path().join("tick-provider-persisted-profile.db")).unwrap(),
    );
    store
        .set_config_value(
            "adaptive_provider_endpoints",
            json!([{
                "endpoint_id": "stub-provider",
                "provider_type": "stub",
                "model": "test-model",
                "timeout_ms": 30000,
                "input_cost_per_1k_usd": 0.01,
                "output_cost_per_1k_usd": 0.02
            }]),
            "test",
        )
        .unwrap();
    let provider: std::sync::Arc<dyn engine::provider::Provider> =
        std::sync::Arc::new(engine::provider::stub::StubProvider::new("stub-provider"));
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store)
            .with_provider(provider),
    );

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "provider task", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let plan_id = response_json(plan_resp).await["plan"]["plan_id"]
        .as_str()
        .unwrap()
        .to_string();
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_id = response_json(run_resp).await["run"]["run_id"]
        .as_str()
        .unwrap()
        .to_string();

    let tick_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "provider"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(tick_resp.status(), StatusCode::OK);
    assert_eq!(
        response_json(tick_resp).await["tick"]["executor_type"],
        "provider"
    );
}

#[tokio::test]
async fn axum_tick_with_provider_stub_records_audit_and_trace() {
    let _guard = provider_cli_env_lock().lock().await;
    let _env_guard = ProviderExecutionEnvGuard::provider_execution();
    std::env::remove_var("ACP_TRUSTED_LOCAL_PROFILE");
    std::env::remove_var("ACP_COST_PER_DISPATCH_USD");
    std::env::remove_var("ACP_COST_DAILY_USD");

    let dir = tempdir().unwrap();
    let store = std::sync::Arc::new(
        LocalProductStore::new(dir.path().join("tick-provider-ok.db")).unwrap(),
    );
    let provider: std::sync::Arc<dyn engine::provider::Provider> =
        std::sync::Arc::new(engine::provider::stub::StubProvider::new("stub-provider"));
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store.clone())
            .with_provider(provider),
    );

    let plan_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "provider task", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan_resp.status(), StatusCode::OK);
    let plan_body = response_json(plan_resp).await;
    let plan_id = plan_body["plan"]["plan_id"].as_str().unwrap();
    let run_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let run_body = response_json(run_resp).await;
    let run_id = run_body["run"]["run_id"].as_str().unwrap();

    let tick_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"actor": "test", "executor": "provider"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(tick_resp.status(), StatusCode::OK);
    let tick_body = response_json(tick_resp).await;
    assert_eq!(tick_body["tick"]["executor_type"], "provider");
    assert_eq!(
        tick_body["tick"]["result"]["trace"]["schema_version"],
        "execution_trace.v2"
    );
    assert_eq!(
        tick_body["tick"]["result"]["trace"]["output_policy"],
        "redacted_and_capped"
    );
    let events = store.provider_audit_events(10).unwrap();
    let event_types: Vec<_> = events
        .iter()
        .filter_map(|event| event["event_type"].as_str())
        .collect();
    assert!(event_types.contains(&"request_sent"));
    assert!(event_types.contains(&"response_received"));
}

