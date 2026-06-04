use std::collections::HashSet;
use std::fs;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{build_axum_router, build_axum_router_with_dashboard, AxumApiState};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::provider::audit::{ProviderAuditEvent, PROVIDER_AUDIT_EVENT_SCHEMA_VERSION};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::tempdir;
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn provider_audit_event(event_id: &str, created_at: &str) -> ProviderAuditEvent {
    ProviderAuditEvent {
        schema_version: PROVIDER_AUDIT_EVENT_SCHEMA_VERSION.to_string(),
        event_id: event_id.to_string(),
        dispatch_id: "disp-provider-audit".to_string(),
        provider_id: "stub-provider".to_string(),
        event_type: "response_received".to_string(),
        input_token_count: Some(10),
        output_token_count: Some(5),
        cost: Some(0.001),
        currency: Some("USD".to_string()),
        latency_ms: Some(25),
        error_domain: None,
        redaction_status: "not_applicable".to_string(),
        created_at: created_at.to_string(),
    }
}

#[tokio::test]
async fn axum_health_is_available_without_auth_by_default() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["tenant_id"], "local");
}

#[tokio::test]
async fn axum_ready_is_available_without_auth_by_default() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "ready");
}

#[tokio::test]
async fn axum_dispatch_returns_deterministic_noop_bundle() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "raw_request": "Review this file without provider calls",
                        "request_source": "api"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["execution_result"]["executor_type"], "noop");
    assert_eq!(body["execution_result"]["status"], "not_executed");
    assert_eq!(body["record"]["final_status"], "not_executed");
}

#[tokio::test]
async fn axum_dispatch_rejects_empty_request() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": ""}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response_json(response).await;
    assert_eq!(body["code"], "raw_request_required");
    assert_eq!(body["error"], "raw_request is required");
}

#[tokio::test]
async fn axum_local_store_persists_dispatch_history_and_dashboard_summary() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("local-team.db");

    {
        let store = LocalProductStore::new(&db_path).unwrap();
        let app = build_axum_router(AxumApiState::new().with_local_store(store));
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/dispatch")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "raw_request": "Summarize local team status without provider calls",
                            "request_source": "dashboard"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["execution_result"]["executor_type"], "noop");
    }

    let store = LocalProductStore::new(&db_path).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let history = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dispatches")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(history.status(), StatusCode::OK);
    let history_body = response_json(history).await;
    assert_eq!(history_body["dispatches"].as_array().unwrap().len(), 1);
    assert_eq!(
        history_body["dispatches"][0]["final_status"],
        "not_executed"
    );
    assert_eq!(history_body["dispatches"][0]["request_source"], "dashboard");
    assert!(
        history_body["dispatches"][0]["reserved_cost"]
            .as_f64()
            .unwrap()
            > 0.0
    );

    let dashboard = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard.status(), StatusCode::OK);
    let dashboard_body = response_json(dashboard).await;
    assert_eq!(dashboard_body["counts"]["dispatches"], 1);
    assert_eq!(
        dashboard_body["dispatches"][0]["raw_request"],
        "Summarize local team status without provider calls"
    );
    assert_eq!(dashboard_body["boundaries"]["provider_transport"], "noop");
}

#[tokio::test]
async fn axum_dispatches_filters_by_search_query() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("dispatch.db")).unwrap();
    store
        .record_dispatch(
            "Alpha parser work",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-alpha", "final_status": "not_executed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.1}},
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();
    store
        .record_dispatch(
            "Beta docs review",
            "dashboard",
            &json!({
                "record": {"dispatch_id": "disp-beta", "final_status": "not_executed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.1}},
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
                .uri("/api/v1/dispatches?search=alpha&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let dispatches = body["dispatches"].as_array().unwrap();
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0]["dispatch_id"], "disp-alpha");
}

#[tokio::test]
async fn axum_local_store_exposes_team_config_costs_and_export() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .set_config_value("workspace_name", json!("Local Ops"), "test-admin")
        .unwrap();
    store
        .upsert_team_member("user-admin", "Admin User", "admin")
        .unwrap();
    store
        .record_api_key_metadata(
            "key-admin",
            "user-admin",
            "admin",
            &["health:read".to_string(), "dispatch:read".to_string()],
            "test-admin",
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "Review cost ledger", "request_source": "api"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let config = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(config.status(), StatusCode::OK);
    let config_body = response_json(config).await;
    assert_eq!(config_body["config"]["workspace_name"], "Local Ops");
    assert_eq!(config_body["config"]["provider_transport"], "stub/off");

    let team = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(team.status(), StatusCode::OK);
    let team_body = response_json(team).await;
    assert_eq!(team_body["members"][0]["role"], "admin");
    assert_eq!(team_body["api_keys"][0]["key_id"], "key-admin");
    assert!(team_body["api_keys"][0].get("raw_key").is_none());

    let costs = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/costs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(costs.status(), StatusCode::OK);
    let costs_body = response_json(costs).await;
    assert_eq!(costs_body["dispatch_count"], 1);
    assert!(costs_body["total_reserved_cost"].as_f64().unwrap() > 0.0);

    let export = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export.status(), StatusCode::OK);
    let export_body = response_json(export).await;
    assert_eq!(export_body["schema_version"], "local_team_export.v1");
    assert_eq!(export_body["dispatches"].as_array().unwrap().len(), 1);
    assert_eq!(export_body["team"]["members"][0]["user_id"], "user-admin");
}

#[tokio::test]
async fn axum_cost_details_clamps_negative_limit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .record_dispatch(
            "Cost row",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-cost", "final_status": "not_executed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.1}},
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
                .uri("/api/v1/costs/dispatches?limit=-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(body["dispatches"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn axum_audit_paginates_and_clamps_negative_limit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .append_audit("tester", "first.action", "res-1", &json!({}))
        .unwrap();
    store
        .append_audit("tester", "second.action", "res-2", &json!({}))
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let paged = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=1&offset=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(paged.status(), StatusCode::OK);
    let body = response_json(paged).await;
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["action"], "first.action");

    let negative = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(negative.status(), StatusCode::OK);
    let body = response_json(negative).await;
    assert!(body["events"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn axum_audit_filters_by_search_query() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .append_audit(
            "key-admin",
            "backup.create",
            "backup-0001",
            &json!({"label": "nightly"}),
        )
        .unwrap();
    store
        .append_audit(
            "key-readonly",
            "team.update",
            "user-readonly",
            &json!({"role": "readonly"}),
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?search=backup&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["action"], "backup.create");
}

#[tokio::test]
async fn axum_audit_redacts_sensitive_details_when_requested() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .append_audit(
            "key-admin",
            "provider.configure",
            "provider-local",
            &json!({
                "api_key": "secret123",
                "nested": {"password": "pw"},
                "safe": "kept",
            }),
        )
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let unredacted = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unredacted.status(), StatusCode::OK);
    let body = response_json(unredacted).await;
    assert_eq!(body["redacted"], false);
    assert_eq!(body["events"][0]["details"]["api_key"], "secret123");

    let redacted = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit?limit=1&redact=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(redacted.status(), StatusCode::OK);
    let body = response_json(redacted).await;
    assert_eq!(body["redacted"], true);
    assert_eq!(body["events"][0]["details"]["api_key"], "***");
    assert_eq!(body["events"][0]["details"]["nested"]["password"], "***");
    assert_eq!(body["events"][0]["details"]["safe"], "kept");
}

#[tokio::test]
async fn axum_metrics_reports_operations_summary() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .record_dispatch(
            "Metrics dispatch",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-metrics", "final_status": "provider_completed"},
                "decision": {"selected_tier": "balanced_worker", "budget_reservation": {"reserved_cost": 0.25}},
                "analysis": {"risk_level": "low"},
                "execution_result": {
                    "executor_type": "provider",
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "estimated_cost": 0.05,
                    "latency_ms": 1200
                },
            }),
            "test",
        )
        .unwrap();
    store
        .record_api_key_metadata(
            "key-admin",
            "user-admin",
            "admin",
            &["health:read".to_string()],
            "test",
        )
        .unwrap();

    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 100),
        Some(100),
        1.0,
    ));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/metrics")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["auth_required"], true);
    assert_eq!(body["local_store"], true);
    assert_eq!(body["dispatch_count"], 1);
    assert_eq!(body["api_key_count"], 1);
    assert!(body["audit_event_count"].as_i64().unwrap() >= 2);
    assert_eq!(body["total_reserved_cost"], 0.25);
    assert_eq!(body["total_estimated_cost_usd"], 0.05);
    assert_eq!(body["total_input_tokens"], 100);
    assert_eq!(body["total_output_tokens"], 50);
    assert_eq!(body["estimated_cost_available"], true);
    assert!(body["pricing_configured"].is_boolean());
    assert_eq!(body["boundaries"]["target_repository_writes"], "disabled");
}

#[tokio::test]
async fn axum_auth_rejects_missing_key_when_configured() {
    let state = AxumApiState::new().with_auth(
        TenantResolver::new(),
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    );
    let app = build_axum_router(state);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn axum_admin_backup_requires_scope_confirmation_and_writes_audit_log() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes
            .union(&readonly_scopes)
            .cloned()
            .collect::<HashSet<String>>(),
        rate_limit: Some(100),
    });
    let (_readonly_key, readonly_raw) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let (_admin_key, admin_raw) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let readonly = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {readonly_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "nightly", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readonly.status(), StatusCode::FORBIDDEN);

    let missing_confirmation = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"label": "nightly"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_confirmation.status(), StatusCode::BAD_REQUEST);

    let backup = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "nightly", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(backup.status(), StatusCode::OK);
    let backup_body = response_json(backup).await;
    assert_eq!(backup_body["backup"]["label"], "nightly");

    let audit = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/audit")
                .header(header::AUTHORIZATION, format!("Bearer {admin_raw}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(audit.status(), StatusCode::OK);
    let audit_body = response_json(audit).await;
    assert_eq!(audit_body["events"][0]["action"], "backup.create");
}

#[tokio::test]
async fn axum_backup_requires_auth_boundary_even_with_confirmation() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups")),
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "manual", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = response_json(response).await;
    assert_eq!(body["code"], "backup_admin_required");
    assert_eq!(body["error"], "admin auth is required for local backup");
}

#[tokio::test]
async fn axum_backup_verify_and_restore_dry_run_are_non_destructive() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("team.db");
    let store = LocalProductStore::new(&db_path).unwrap();
    store
        .record_dispatch(
            "Backup verification row",
            "api",
            &json!({
                "record": {"dispatch_id": "disp-backup-verify", "final_status": "not_executed"},
                "decision": {"selected_tier": "cheap_executor", "budget_reservation": {"reserved_cost": 0.1}},
                "analysis": {"risk_level": "low"},
                "execution_result": {"executor_type": "noop"},
            }),
            "test",
        )
        .unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "verify", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let create_body = response_json(create).await;
    let backup_id = create_body["backup"]["backup_id"].as_str().unwrap();

    let verify = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/backups/{backup_id}/verify"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);
    let verify_body = response_json(verify).await;
    assert_eq!(verify_body["verification"]["success"], true);
    assert_eq!(verify_body["verification"]["checksum_ok"], true);
    assert_eq!(verify_body["verification"]["integrity_ok"], true);
    assert_eq!(verify_body["verification"]["dry_run"], false);
    assert!(
        verify_body["verification"]["records_checked"]
            .as_i64()
            .unwrap()
            > 0
    );

    let dry_run = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/backups/{backup_id}/restore/dry-run"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"confirm_restore_dry_run": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dry_run.status(), StatusCode::OK);
    let dry_run_body = response_json(dry_run).await;
    assert_eq!(dry_run_body["restore_dry_run"]["success"], true);
    assert_eq!(dry_run_body["restore_dry_run"]["dry_run"], true);
    assert_eq!(
        dry_run_body["restore_dry_run"]["restore_would_overwrite"],
        true
    );

    let integrity = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/storage/integrity")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(integrity.status(), StatusCode::OK);
    let integrity_body = response_json(integrity).await;
    assert_eq!(integrity_body["integrity"]["status"], "ok");
}

#[tokio::test]
async fn axum_auth_allows_scoped_dispatch_key() {
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["dispatch:read".to_string(), "health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "tenant-a".to_string(),
        name: "Tenant A".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(5),
    });
    let (_key, raw_key) = resolver
        .create_api_key("tenant-a", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_auth(
        resolver,
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "Summarize docs", "request_source": "api"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn axum_rate_limit_blocks_after_tenant_limit() {
    let mut resolver = TenantResolver::new();
    let scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "tenant-a".to_string(),
        name: "Tenant A".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(1),
    });
    let (_key, raw_key) = resolver
        .create_api_key("tenant-a", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_auth(
        resolver,
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    ));

    let request = || {
        Request::builder()
            .method(Method::GET)
            .uri("/api/v1/health")
            .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
            .body(Body::empty())
            .unwrap()
    };

    let first = app.clone().oneshot(request()).await.unwrap();
    let second = app.oneshot(request()).await.unwrap();

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn axum_preflight_returns_cors_headers() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/v1/dispatch")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        Some(&"*".parse().unwrap())
    );
}

#[tokio::test]
async fn axum_openapi_document_lists_dispatch_endpoint() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["openapi"], "3.1.0");
    assert!(body["paths"]["/api/v1/dispatch"]["post"].is_object());
    assert!(body["paths"]["/api/v1/metrics"]["get"].is_object());
    assert!(body["paths"]["/api/v1/backups/{backup_id}/verify"]["get"].is_object());
    assert!(body["paths"]["/api/v1/backups/{backup_id}/restore/dry-run"]["post"].is_object());
    assert_eq!(
        body["paths"]["/api/v1/audit"]["get"]["parameters"][3]["name"],
        "redact"
    );
    assert!(body["paths"]["/api/v1/provider/audit"]["get"].is_object());
    assert_eq!(
        body["paths"]["/api/v1/provider/audit"]["get"]["parameters"][0]["name"],
        "limit"
    );
    assert_eq!(
        body["paths"]["/api/v1/provider/audit"]["get"]["parameters"][1]["name"],
        "offset"
    );
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

#[tokio::test]
async fn axum_provider_health_noop_when_no_provider() {
    let app = build_axum_router(AxumApiState::new());
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "noop");
    assert_eq!(body["message"], "no provider configured");
}

#[tokio::test]
async fn axum_provider_health_noop_with_multi_executor_and_no_provider() {
    let engine = engine::dispatch_engine::DispatchEngine::with_multi_executor(
        engine::cli::MultiExecutor::new(std::collections::HashMap::new()),
    );
    let app = build_axum_router(AxumApiState::new().with_engine(engine));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "noop");
    assert_eq!(body["message"], "no provider configured");
}

#[tokio::test]
async fn axum_provider_health_ok_with_stub_provider() {
    use engine::provider::stub::StubProvider;
    use engine::provider::Provider;
    use std::sync::Arc;

    let provider: Arc<dyn Provider> = Arc::new(StubProvider::new("stub-health"));
    let state = AxumApiState::new().with_provider(provider);
    let app = build_axum_router(state);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["provider_id"], "stub-health");
    assert_eq!(body["enabled"], true);
}

#[tokio::test]
async fn axum_provider_audit_paginates_and_clamps_negative_limit() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    store
        .record_provider_audit_event(&provider_audit_event("evt-old", "2026-05-29T12:00:00Z"))
        .unwrap();
    store
        .record_provider_audit_event(&provider_audit_event("evt-new", "2026-05-29T12:01:00Z"))
        .unwrap();

    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let paged = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/audit?limit=1&offset=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(paged.status(), StatusCode::OK);
    let body = response_json(paged).await;
    let events = body["events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_id"], "evt-old");

    let negative = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/audit?limit=-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(negative.status(), StatusCode::OK);
    let body = response_json(negative).await;
    assert!(body["events"].as_array().unwrap().is_empty());
}

fn make_admin_app() -> (axum::Router, String) {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let mut admin_scopes = HashSet::new();
    admin_scopes.insert("team:read".to_string());
    admin_scopes.insert("team:admin".to_string());
    admin_scopes.insert("dispatch:read".to_string());
    admin_scopes.insert("health:read".to_string());
    admin_scopes.insert("audit:read".to_string());
    admin_scopes.insert("backup:admin".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(admin_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));
    (app, raw_key)
}

#[tokio::test]
async fn axum_create_api_key_returns_raw_key() {
    let (app, admin_key) = make_admin_app();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(body["key_id"].as_str().unwrap().starts_with("key_"));
    assert!(body["raw_key"].as_str().unwrap().starts_with("harness_"));
    assert_eq!(body["user_id"], "u1");
    assert_eq!(body["role"], "admin");
}

#[tokio::test]
async fn axum_list_api_keys_returns_metadata() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    // Create two keys
    for uid in &["u1", "u2"] {
        app_clone
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/keys")
                    .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"user_id": uid, "role": "readonly", "scopes": ["dispatch:read"]})
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    let keys = body["keys"].as_array().unwrap();
    assert!(keys.len() >= 2);
    // Keys should have metadata fields but no raw_key
    for key in keys {
        assert!(key["key_id"].as_str().is_some());
        assert!(key["user_id"].as_str().is_some());
        assert!(key["role"].as_str().is_some());
        assert!(key["scopes"].as_array().is_some());
        assert!(key["created_at"].as_str().is_some());
        assert!(
            key.get("raw_key").is_none(),
            "list must not return raw keys"
        );
    }
}

#[tokio::test]
async fn axum_list_api_keys_requires_team_read_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let mut readonly_scopes = HashSet::new();
    readonly_scopes.insert("dispatch:read".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_revoke_api_key_blocks_future_auth() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read", "team:read"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let new_key = create_body["raw_key"].as_str().unwrap().to_string();
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let use_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(use_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let revoke_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{key_id}/revoke"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoke_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let blocked_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(blocked_resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn axum_rotate_api_key_creates_new_revokes_old() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read", "team:read", "team:admin"]}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let old_key = create_body["raw_key"].as_str().unwrap().to_string();
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let rotate_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{key_id}/rotate"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotate_resp.status(), StatusCode::OK);
    let rotate_body = response_json(rotate_resp).await;
    let new_key = rotate_body["raw_key"].as_str().unwrap().to_string();
    assert_ne!(old_key, new_key);
    assert_eq!(rotate_body["rotated_from"], key_id);

    let app_clone = app.clone();
    let old_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {old_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_resp.status(), StatusCode::UNAUTHORIZED);

    let app_clone = app.clone();
    let new_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {new_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(new_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn axum_delete_api_key_removes_metadata() {
    let (app, admin_key) = make_admin_app();
    let app_clone = app.clone();

    let create_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let create_body = response_json(create_resp).await;
    let key_id = create_body["key_id"].as_str().unwrap().to_string();

    let app_clone = app.clone();
    let del_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/keys/{key_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK);

    let app_clone = app.clone();
    let team_resp = app_clone
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let keys = team_body["api_keys"].as_array().unwrap();
    assert!(!keys.iter().any(|k| k["key_id"] == key_id));
}

#[tokio::test]
async fn axum_create_team_member_adds_to_snapshot() {
    let (app, admin_key) = make_admin_app();

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "alice", "display_name": "Alice", "role": "admin"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    assert!(members.iter().any(|m| m["user_id"] == "alice"));
}

#[tokio::test]
async fn axum_update_member_role_changes_role() {
    let (app, admin_key) = make_admin_app();

    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "bob", "display_name": "Bob", "role": "readonly"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let update_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/team/bob")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"role": "admin"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    let bob = members.iter().find(|m| m["user_id"] == "bob").unwrap();
    assert_eq!(bob["role"], "admin");
}

#[tokio::test]
async fn axum_delete_member_removes_from_snapshot() {
    let (app, admin_key) = make_admin_app();

    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "carol", "display_name": "Carol", "role": "admin"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let del_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/team/carol")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del_resp.status(), StatusCode::OK);

    let team_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let team_body = response_json(team_resp).await;
    let members = team_body["members"].as_array().unwrap();
    assert!(!members.iter().any(|m| m["user_id"] == "carol"));
}

#[tokio::test]
async fn axum_team_mutation_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut resolver = TenantResolver::new();
    let mut readonly_scopes = HashSet::new();
    readonly_scopes.insert("team:read".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "role": "admin", "scopes": ["dispatch:read"]})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::FORBIDDEN);

    let member_resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"user_id": "u1", "display_name": "U1", "role": "admin"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(member_resp.status(), StatusCode::FORBIDDEN);
}

// --- dispatch detail tests ---

#[tokio::test]
async fn axum_dispatch_detail_returns_bundle_for_existing_dispatch() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("dispatch.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    // Create a dispatch first
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "test dispatch", "request_source": "api"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body = response_json(create_resp).await;
    let dispatch_id = create_body["record"]["dispatch_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Get detail
    let detail_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/v1/dispatches/{dispatch_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail_body = response_json(detail_resp).await;
    assert_eq!(detail_body["dispatch"]["dispatch_id"], dispatch_id);
    assert!(detail_body["dispatch"]["bundle"].is_object());
}

#[tokio::test]
async fn axum_dispatch_detail_returns_404_for_missing() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("dispatch.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dispatches/nonexistent-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// --- list backups tests ---

#[tokio::test]
async fn axum_list_backups_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_list_backups_returns_empty_when_no_backups() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    assert_eq!(body["backups"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn axum_list_backups_after_create() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    // Create a backup
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "test", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    // List backups
    let list_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let body = response_json(list_resp).await;
    assert_eq!(body["backups"].as_array().unwrap().len(), 1);
    assert_eq!(body["backups"][0]["label"], "test");
}

#[tokio::test]
async fn axum_create_backup_uses_real_timestamp() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string(), "health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    // Create a backup
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "ts-check", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);

    // List and verify timestamp is not the old hardcoded value
    let list_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response_json(list_resp).await;
    let created_at = body["backups"][0]["created_at"].as_str().unwrap();
    assert_ne!(
        created_at, "2026-05-29T00:00:00Z",
        "backup timestamp should not be hardcoded"
    );
    assert!(
        created_at.starts_with("2026-") || created_at.starts_with("2027-"),
        "backup timestamp should be a recent ISO date, got: {created_at}"
    );
}

// --- delete backup tests ---

#[tokio::test]
async fn axum_delete_backup_requires_admin_scope() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let readonly_scopes = HashSet::from(["health:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: readonly_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(readonly_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/backups/backup-0001")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn axum_delete_backup_404_for_missing() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from(["backup:admin".to_string(), "audit:read".to_string()]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri("/api/v1/backups/nonexistent-backup")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn axum_delete_backup_removes_backup() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();

    let mut resolver = TenantResolver::new();
    let admin_scopes = HashSet::from([
        "backup:admin".to_string(),
        "audit:read".to_string(),
        "health:read".to_string(),
    ]);
    resolver.add_tenant(Tenant {
        tenant_id: "local-team".to_string(),
        name: "Local Team".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local-team", Some(admin_scopes), None, 1.0)
        .unwrap();

    let app = build_axum_router(
        AxumApiState::new()
            .with_local_store(store)
            .with_backup_dir(dir.path().join("backups"))
            .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0),
    );

    // Create a backup
    let create_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"label": "to-delete", "confirm_local_backup": true}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body = response_json(create_resp).await;
    let backup_id = create_body["backup"]["backup_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Delete it
    let delete_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/v1/backups/{backup_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::OK);
    let delete_body = response_json(delete_resp).await;
    assert_eq!(delete_body["ok"], true);

    // Verify list is empty
    let list_resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/backups")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_body = response_json(list_resp).await;
    assert_eq!(list_body["backups"].as_array().unwrap().len(), 0);
}
