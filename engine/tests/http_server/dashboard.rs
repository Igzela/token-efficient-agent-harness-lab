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
async fn axum_policy_proposal_activation_requires_confirmation_and_affects_dispatch() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("policy.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from([
        "dispatch:read".to_string(),
        "team:admin".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 100),
        Some(100),
        1.0,
    ));

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/proposals")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "task_domain": "docs",
                        "task_intent": "review",
                        "target_tier": "verifier",
                        "payload": {"type": "tier_map_override"},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let body = response_json(created).await;
    let proposal_id = body["proposal"]["proposal_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(body["proposal"]["status"], "pending");

    let missing_confirm = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "pilot"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirm.status(), StatusCode::BAD_REQUEST);

    let approved = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "pilot", "confirm_policy_override": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);
    let body = response_json(approved).await;
    assert_eq!(body["proposal"]["status"], "active");

    let dispatch = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_request": "Review docs for consistency",
                        "request_source": "api"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dispatch.status(), StatusCode::OK);
    let body = response_json(dispatch).await;
    assert_eq!(body["decision"]["selected_tier"], "verifier");
}

#[tokio::test]
async fn axum_dashboard_exposes_read_only_cli_capability() {
    let default_app = build_axum_router(AxumApiState::new());
    let default_response = default_app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(default_response.status(), StatusCode::OK);
    let default_body = response_json(default_response).await;
    assert_eq!(
        default_body["cli"],
        json!({
            "enabled": false,
            "claude_code": false,
            "codex": false,
        })
    );

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("local-team.db")).unwrap();
    let configured_app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_cli_capability(CliCapability {
                enabled: true,
                claude_code: true,
                codex: false,
            }),
    );
    let configured_response = configured_app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(configured_response.status(), StatusCode::OK);
    let configured_body = response_json(configured_response).await;
    assert_eq!(
        configured_body["cli"],
        json!({
            "enabled": true,
            "claude_code": true,
            "codex": false,
        })
    );
}

#[tokio::test]
async fn axum_dashboard_exposes_adaptive_fusion_operator_status() {
    let _guard = adaptive_operator_env_lock().lock().await;
    let default_app = build_axum_router(AxumApiState::new());
    let default_response = default_app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(default_response.status(), StatusCode::OK);
    let default_body = response_json(default_response).await;
    assert_eq!(
        default_body["adaptive_fusion"],
        json!({
            "schema_version": "adaptive_fusion_operator_status.v1",
            "trusted_local_profile": {
                "schema_version": "trusted_local_profile.v1",
                "requested": false,
                "ready": false,
                "blockers": [],
                "capabilities": {
                    "provider_execution": false,
                    "adaptive_execution": false,
                    "default_routing": false,
                    "experiments": false,
                    "auto_promotion": false,
                },
            },
            "trusted_local_task_advancement": {
                "schema_version": "trusted_local_task_advancement.v1",
                "requested": false,
                "ready": false,
                "blockers": [],
                "executor_type": "adaptive_provider",
                "worker_count": 1,
                "max_concurrent": 4,
            },
            "completion_api": {
                "available": true,
                "ready_for_live_completion": false,
                "executor_configured": false,
                "registry_configured": false,
                "storage_configured": false,
                "default_routing_enabled": false,
            },
            "gates": {
                "provider_execution": false,
                "adaptive_execution": false,
                "auth": false,
                "fusion_kill_switch": false,
                "experiments_enabled": false,
                "experiments_active": false,
                "experiments_paused": false,
                "experiments_kill_switch": false,
                "auto_promotion_enabled": false,
                "auto_promotion_active": false,
                "auto_promotion_kill_switch": false,
            },
            "policy": {
                "active_policy_count": 0,
                "snapshot_count": 0,
                "active_snapshot_count": 0,
                "live_execution_authority": false,
                "requires_explicit_adaptive_plan": true,
            },
            "authority": {
                "provider_execution_active": false,
                "adaptive_execution_active": false,
                "default_routing_active": false,
                "experiments_active": false,
                "auto_promotion_active": false,
                "task_advancement_active": false,
            },
            "bounds": {
                "per_dispatch_cost_cap_usd": null,
                "daily_cost_cap_usd": null,
                "today_cost_usd": 0.0,
                "daily_cost_remaining_usd": null,
                "experiment_traffic_rate": 0.01,
                "experiment_max_cost_usd": 1.0,
                "experiment_max_total_tokens": 32768,
                "experiment_max_calls": 8,
                "experiment_max_elapsed_ms": 300000,
                "experiment_max_concurrency": 3,
                "experiment_policy_valid": true,
                "experiment_policy_blockers": [],
                "auto_promotion_rollout_percentage": 10,
                "auto_promotion_policy_valid": true,
                "auto_promotion_policy_blockers": [],
                "worker_count": 1,
                "worker_max_concurrent": 4,
            },
            "observations": {
                "count": 0,
                "success_count": 0,
                "failure_count": 0,
                "total_cost_usd": 0.0,
                "latest_at": null,
            },
            "scheduler": {
                "enabled": false,
                "running": false,
                "supervised_workers_enabled": false,
                "paused": false,
                "kill_requested": false,
                "worker_count": 0,
                "max_concurrent": 0,
                "executor_type": null,
                "active_runs": 0,
                "tick_count": 0,
                "error_count": 0,
                "last_tick_at": null,
            },
        })
    );
}

#[tokio::test]
async fn axum_dashboard_reports_invalid_adaptive_operator_policies() {
    let _guard = adaptive_operator_env_lock().lock().await;
    let _env = AdaptiveOperatorEnvGuard::invalid_policies();
    let app = build_axum_router(AxumApiState::new());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let status = &body["adaptive_fusion"];
    assert_eq!(status["authority"]["experiments_active"], false);
    assert_eq!(status["authority"]["auto_promotion_active"], false);
    assert_eq!(status["bounds"]["experiment_policy_valid"], false);
    assert_eq!(
        status["bounds"]["experiment_policy_blockers"],
        json!(["invalid_traffic_rate"])
    );
    assert_eq!(status["bounds"]["auto_promotion_policy_valid"], false);
    assert_eq!(
        status["bounds"]["auto_promotion_policy_blockers"],
        json!(["invalid_minimum_confidence", "invalid_rollout_percentage"])
    );
}

#[tokio::test]
async fn axum_dashboard_adaptive_operator_evidence_is_aggregated_and_secret_safe() {
    use engine::feedback::ObjectiveProfile;
    use engine::scheduler::{SchedulerConfig, WorkflowScheduler};
    use engine::storage::local_product_store::{
        AdaptiveObservationInput, ADAPTIVE_OBSERVATION_SCHEMA_VERSION,
    };
    use std::sync::{Arc, Mutex};

    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("iae3.db")).unwrap());
    store
        .record_adaptive_observation(
            &AdaptiveObservationInput {
                schema_version: ADAPTIVE_OBSERVATION_SCHEMA_VERSION.to_string(),
                run_id: "private-run-id".to_string(),
                request_id: "private-request-id".to_string(),
                task_class: "coding".to_string(),
                objective: ObjectiveProfile::Quality,
                risk_level: "low".to_string(),
                candidate_id: "private-candidate-id".to_string(),
                candidate_hash: "a".repeat(64),
                policy_hash: Some("b".repeat(64)),
                candidate_kind: "single".to_string(),
                success: true,
                quality_score: 0.9,
                quality_score_source: "execution_success_proxy".to_string(),
                tool_success_score: 1.0,
                cost_usd: 0.08,
                latency_ms: 240,
                input_tokens: 120,
                output_tokens: 40,
            },
            "operator",
        )
        .unwrap();
    let mut scheduler = WorkflowScheduler::new(
        store.clone(),
        SchedulerConfig {
            interval_ms: 2_000,
            max_concurrent: 1,
            worker_count: 1,
            supervised_workers_enabled: true,
            ..Default::default()
        },
    );
    scheduler.start().unwrap();
    let scheduler = Arc::new(Mutex::new(scheduler));
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store_arc(store)
            .with_scheduler(scheduler.clone()),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let status = &body["adaptive_fusion"];
    assert_eq!(status["observations"]["count"], 1);
    assert_eq!(status["observations"]["success_count"], 1);
    assert_eq!(status["observations"]["failure_count"], 0);
    assert_eq!(status["observations"]["total_cost_usd"], 0.08);
    assert!(status["observations"]["latest_at"].is_string());
    assert_eq!(status["completion_api"]["storage_configured"], true);
    assert_eq!(status["scheduler"]["enabled"], true);
    assert_eq!(status["scheduler"]["running"], true);
    assert_eq!(status["scheduler"]["worker_count"], 1);
    assert_eq!(status["scheduler"]["max_concurrent"], 1);
    assert_eq!(status["scheduler"]["executor_type"], "noop");
    assert_eq!(status["authority"]["task_advancement_active"], false);
    let serialized = serde_json::to_string(status).unwrap();
    assert!(!serialized.contains("private-run-id"));
    assert!(!serialized.contains("private-request-id"));
    assert!(!serialized.contains("private-candidate-id"));

    scheduler.lock().unwrap().stop().unwrap();
}

#[tokio::test]
async fn axum_dashboard_serves_static_index_when_configured() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("index.html"),
        "<!doctype html><title>Agent Control Plane</title>",
    )
    .unwrap();
    let app = build_axum_router_with_dashboard(AxumApiState::new(), dir.path());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    let body = response_text(response).await;
    assert!(body.contains("Agent Control Plane"));
}

#[tokio::test]
async fn axum_dashboard_serves_static_assets_when_configured() {
    let dir = tempdir().unwrap();
    let asset_dir = dir.path().join("_next/static/chunks");
    fs::create_dir_all(&asset_dir).unwrap();
    fs::write(asset_dir.join("app.js"), "console.log('ok');").unwrap();
    fs::write(dir.path().join("index.html"), "<!doctype html>").unwrap();
    let app = build_axum_router_with_dashboard(AxumApiState::new(), dir.path());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/_next/static/chunks/app.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(response_text(response).await, "console.log('ok');");
}

#[tokio::test]
async fn axum_dashboard_does_not_mask_unknown_api_routes() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("index.html"), "<!doctype html>").unwrap();
    let app = build_axum_router_with_dashboard(AxumApiState::new(), dir.path());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response_text(response).await, "not found");
}

