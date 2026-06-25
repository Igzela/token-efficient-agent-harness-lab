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

async fn create_test_proposal(app: &axum::Router, raw_key: &str) -> String {
    let resp = app
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
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    body["proposal"]["proposal_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_proposal_rejects_cli_tier_override() {
    let (app, admin_key) = make_admin_app();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/proposals")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "task_domain": "docs",
                        "task_intent": "review",
                        "target_tier": "codex_cli",
                        "payload": {"type": "tier_map_override"},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "invalid_policy_proposal");
}

#[tokio::test]
async fn test_proposal_rejects_provider_tier_override() {
    let (app, admin_key) = make_admin_app();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/proposals")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "task_domain": "docs",
                        "task_intent": "review",
                        "target_tier": "provider_model",
                        "payload": {"type": "tier_map_override"},
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "invalid_policy_proposal");
}

#[tokio::test]
async fn test_proposal_approve_requires_team_admin() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("safety-admin.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "dispatch:read".to_string(),
        "team:admin".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_admin_key, admin_raw) = resolver
        .create_api_key("local", Some(admin_scopes), None, 1.0)
        .unwrap();

    let non_admin_scopes = HashSet::from(["dispatch:read".to_string(), "health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "readonly".to_string(),
        name: "Readonly".to_string(),
        scopes: non_admin_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_readonly_key, readonly_raw) = resolver
        .create_api_key("readonly", Some(non_admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let proposal_id = create_test_proposal(&app, &admin_raw).await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {readonly_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "test", "confirm_policy_override": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_proposal_rollback_requires_confirm_policy_override() {
    let (app, admin_key) = make_admin_app();
    let proposal_id = create_test_proposal(&app, &admin_key).await;

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/rollback"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "invalid_policy_proposal");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("confirm_policy_override"),
        "error should mention confirm_policy_override requirement"
    );
}

#[tokio::test]
async fn test_proposal_deactivate_requires_confirm_policy_override() {
    let (app, admin_key) = make_admin_app();

    // First approve a proposal so it becomes active (required for deactivate)
    let proposal_id = create_test_proposal(&app, &admin_key).await;
    let approve_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "activate for deactivate test", "confirm_policy_override": true})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve_resp.status(), StatusCode::OK);

    // Now try deactivate without confirm_policy_override
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/deactivate"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reason": "test"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "invalid_policy_proposal");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("confirm_policy_override"),
        "error should mention confirm_policy_override requirement"
    );
}

#[tokio::test]
async fn test_proposal_approve_requires_auth_configured() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("safety-noauth.db")).unwrap();

    // Create proposal via store directly (no auth needed for store operations)
    let proposal = store
        .create_policy_proposal(
            &json!({
                "task_domain": "docs",
                "task_intent": "review",
                "target_tier": "verifier",
                "payload": {"type": "tier_map_override"},
            }),
            "test-actor",
        )
        .unwrap();
    let proposal_id = proposal["proposal_id"].as_str().unwrap();

    // Build router WITHOUT auth configured
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/proposals/{proposal_id}/approve"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"reason": "test", "confirm_policy_override": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response_json(response).await;
    assert_eq!(body["code"], "auth_required_for_policy_override");
}

#[tokio::test]
async fn test_feedback_traces_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("traces-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_traces.v1");
    assert_eq!(body["total"], 0);
    assert!(body["traces"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_feedback_cost_of_pass_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("cop-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/cost-of-pass")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_cost_of_pass.v1");
    assert!(body["rows"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_simulation_report_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("sim-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/simulation/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "dispatch_simulation_report.v1");
    assert_eq!(body["totals"]["dispatch_count"], 0);
    assert_eq!(body["totals"]["shadow_route_count"], 0);
    assert!(body["report"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_simulation_report_with_shadow_routes() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("sim-shadow.db")).unwrap();
    store
        .record_dispatch(
            "Simulation shadow dispatch",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-shadow", "final_status": "not_executed"},
                "decision": {
                    "selected_tier": "balanced_worker",
                    "budget_reservation": {"reserved_cost": 0.1},
                    "shadow_routes": [
                        {"tier": "cheap_executor", "score": 0.8},
                        {"tier": "premium_worker", "score": 0.6}
                    ]
                },
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/simulation/report")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "dispatch_simulation_report.v1");
    assert!(body["totals"]["dispatch_count"].as_i64().unwrap() >= 1);
    assert!(body["totals"]["shadow_route_count"].as_i64().unwrap() >= 2);

    let report = body["report"].as_array().unwrap();
    assert!(!report.is_empty());
    assert_eq!(report[0]["dispatch_id"], "disp-shadow");
    assert_eq!(report[0]["status"], "shadow_only");
    let shadow_routes = report[0]["shadow_routes"].as_array().unwrap();
    assert_eq!(shadow_routes.len(), 2);
    assert_eq!(shadow_routes[0]["tier"], "cheap_executor");
    assert_eq!(shadow_routes[1]["tier"], "premium_worker");
}

#[tokio::test]
async fn test_feedback_patterns_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patterns-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/patterns")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_patterns.v1");
    assert_eq!(body["total"], 0);
    assert!(body["patterns"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_feedback_patterns_with_dispatch_data() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patterns-data.db")).unwrap();

    for i in 0..5 {
        let status = if i < 4 { "failed" } else { "completed" };
        store
            .record_dispatch(
                &format!("Pattern dispatch {i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("disp-pat-{i}"), "final_status": status},
                    "decision": {"selected_tier": "fragile_worker", "budget_reservation": {"reserved_cost": 0.05}},
                    "analysis": {"risk_level": "high"},
                    "execution_result": {"executor_type": "noop"},
                }),
                "test",
            )
            .unwrap();
    }

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/patterns")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_patterns.v1");
    let patterns = body["patterns"].as_array().unwrap();
    assert!(!patterns.is_empty());

    let first = &patterns[0];
    assert!(first["pattern_id"].as_str().is_some());
    assert!(first["pattern_type"].as_str().is_some());
    assert!(first["severity"].as_str().is_some());
    let evidence = first["evidence_trace_ids"].as_array().unwrap();
    assert!(!evidence.is_empty());
}

#[tokio::test]
async fn test_feedback_patterns_with_task_class_filter() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("patterns-filter.db")).unwrap();

    for i in 0..4 {
        store
            .record_dispatch(
                &format!("Code review dispatch {i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("disp-cr-{i}"), "final_status": "failed"},
                    "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.05}},
                    "analysis": {"risk_level": "high", "task_class": "code_review"},
                    "execution_result": {"executor_type": "noop"},
                }),
                "test",
            )
            .unwrap();
    }

    for i in 0..4 {
        store
            .record_dispatch(
                &format!("Docs dispatch {i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("disp-docs-{i}"), "final_status": "completed"},
                    "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.02}},
                    "analysis": {"risk_level": "low", "task_class": "docs_update"},
                    "execution_result": {"executor_type": "noop"},
                }),
                "test",
            )
            .unwrap();
    }

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/patterns?task_class=code_review")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let patterns = body["patterns"].as_array().unwrap();
    for pattern in patterns {
        let tc = pattern["affected_task_class"].as_str().unwrap_or("");
        assert!(
            tc.eq_ignore_ascii_case("code_review") || tc.is_empty(),
            "unexpected task_class in filtered result: {tc}"
        );
    }
}

#[tokio::test]
async fn test_feedback_traces_returns_stable_schema() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("traces-schema.db")).unwrap();

    store
        .record_dispatch(
            "Trace schema check",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-trace-schema", "final_status": "completed"},
                "decision": {
                    "selected_tier": "balanced_worker",
                    "budget_reservation": {"reserved_cost": 0.08}
                },
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop", "estimated_cost": 0.004},
                "evaluation_result": {"status": "pass"},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_traces.v1");
    assert!(body["total"].as_u64().unwrap() >= 1);

    let traces = body["traces"].as_array().unwrap();
    assert!(!traces.is_empty());
    let trace = &traces[0];
    assert!(trace["trace_id"].as_str().is_some());
    assert!(trace["dispatch_id"].as_str().is_some());
    assert!(trace["decision"].is_object() || trace["decision"].is_null());
    assert!(trace["execution"].is_object() || trace["execution"].is_null());
    assert!(trace["evaluation"].is_object() || trace["evaluation"].is_null());
}

#[tokio::test]
async fn test_feedback_traces_supports_filters() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("traces-filters.db")).unwrap();

    store
        .record_dispatch(
            "Filter test alpha",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-filter-a", "final_status": "completed"},
                "decision": {"selected_tier": "premium_worker", "budget_reservation": {"reserved_cost": 0.2}},
                "analysis": {"risk_level": "low", "task_class": "code_review"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    store
        .record_dispatch(
            "Filter test beta",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-filter-b", "final_status": "failed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.01}},
                "analysis": {"risk_level": "high", "task_class": "docs_update"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces?task_class=code_review")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    for trace in body["traces"].as_array().unwrap() {
        assert_eq!(trace["task_class"], "code_review");
    }

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces?tier=premium_worker")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    for trace in body["traces"].as_array().unwrap() {
        assert_eq!(trace["tier"], "premium_worker");
    }

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/traces?status=fail")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    for trace in body["traces"].as_array().unwrap() {
        assert_eq!(trace["status"], "fail");
    }
}

#[tokio::test]
async fn test_cost_of_pass_still_works() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("cop-data.db")).unwrap();

    store
        .record_dispatch(
            "Cost aggregation A",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-cop-a", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.10}},
                "analysis": {"risk_level": "low", "task_class": "code_review"},
                "execution_result": {"executor_type": "noop", "estimated_cost": 0.05},
            }),
            "test",
        )
        .unwrap();

    store
        .record_dispatch(
            "Cost aggregation B",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-cop-b", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.08}},
                "analysis": {"risk_level": "low", "task_class": "code_review"},
                "execution_result": {"executor_type": "noop", "estimated_cost": 0.03},
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/feedback/cost-of-pass")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "feedback_cost_of_pass.v1");
    let rows = body["rows"].as_array().unwrap();
    assert!(!rows.is_empty());

    let code_review_row = rows
        .iter()
        .find(|r| r["task_class"] == "code_review")
        .expect("expected code_review cost-of-pass row");
    assert_eq!(code_review_row["dispatch_count"].as_i64().unwrap(), 2);
    assert!(code_review_row["average_cost_usd"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn test_policy_simulation_report_empty() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/simulation/policy-delta")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "policy_simulation_report.v1");
    assert_eq!(body["input_trace_count"], 0);
    assert_eq!(body["safety"], "shadow_only / no_live_influence");
    assert_eq!(body["success_rate_delta"], 0.0);
}

#[tokio::test]
async fn test_policy_simulation_report_with_traces() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-traces.db")).unwrap();
    store
        .record_dispatch(
            "Pass dispatch",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-pass", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.05}, "shadow_routes": []},
                "analysis": {"task_class": "implementation"},
                "execution_result": {"executor_type": "noop", "status": "completed", "success": true, "latency_ms": 1000, "estimated_cost": 0.03},
                "evaluation_result": {"status": "pass"}
            }),
            "test",
        )
        .unwrap();
    store
        .record_dispatch(
            "Fail dispatch",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-fail", "final_status": "failed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.02}, "shadow_routes": []},
                "analysis": {"task_class": "code_review"},
                "execution_result": {"executor_type": "noop", "status": "failed", "success": false, "latency_ms": 500, "estimated_cost": 0.01},
                "evaluation_result": {"status": "fail"}
            }),
            "test",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/simulation/policy-delta?policy=cheapest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["schema_version"], "policy_simulation_report.v1");
    assert_eq!(body["input_trace_count"], 2);
    assert_eq!(body["safety"], "shadow_only / no_live_influence");
    assert!(body["success_rate_delta"].is_f64());
    assert!(body["cost_delta"].is_f64());
    assert!(body["latency_delta"].is_f64());
    assert!(body["human_review_rate_delta"].is_f64());
    assert!(body["actual_success_rate"].is_f64());
    assert!(body["simulated_success_rate"].is_f64());
    assert_eq!(body["evidence_trace_ids"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_shadow_simulation_does_not_alter_dispatch_tier() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-notier.db")).unwrap();
    store
        .record_dispatch(
            "Tier check",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-tier", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.05}, "shadow_routes": []},
                "analysis": {"task_class": "implementation"},
                "execution_result": {"executor_type": "noop", "status": "completed", "success": true, "latency_ms": 1000},
                "evaluation_result": {"status": "pass"}
            }),
            "test",
        )
        .unwrap();

    let _report = store.policy_simulation_report(10).unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(dispatches.len(), 1);
    let bundle = &dispatches[0]["bundle"];
    assert_eq!(bundle["decision"]["selected_tier"], "balanced_worker");
}

#[tokio::test]
async fn test_shadow_simulation_does_not_alter_executor_type() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-noexec.db")).unwrap();
    store
        .record_dispatch(
            "Exec check",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-exec", "final_status": "completed"},
                "decision": {"selected_tier": "strong_planner", "budget_reservation": {"reserved_cost": 0.1}, "shadow_routes": []},
                "analysis": {"task_class": "architecture"},
                "execution_result": {"executor_type": "cli", "status": "completed", "success": true, "latency_ms": 5000},
                "evaluation_result": {"status": "pass"}
            }),
            "test",
        )
        .unwrap();

    let _report = store
        .policy_simulation_report_with_policy(10, "cheapest")
        .unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(
        dispatches[0]["bundle"]["execution_result"]["executor_type"],
        "cli"
    );
}

#[tokio::test]
async fn test_shadow_simulation_does_not_mutate_routing_policy() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("psim-nopol.db")).unwrap();
    store
        .record_dispatch(
            "Policy check",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-pol", "final_status": "completed"},
                "decision": {"selected_tier": "balanced_worker", "routing_policy": "default", "budget_reservation": {"reserved_cost": 0.05}, "shadow_routes": []},
                "analysis": {"task_class": "implementation"},
                "execution_result": {"executor_type": "noop", "status": "completed", "success": true},
                "evaluation_result": {"status": "pass"}
            }),
            "test",
        )
        .unwrap();

    let _report = store.policy_simulation_report(10).unwrap();

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(
        dispatches[0]["bundle"]["decision"]["routing_policy"],
        "default"
    );
}

