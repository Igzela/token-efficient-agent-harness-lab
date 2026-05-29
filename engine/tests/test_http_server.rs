use std::collections::HashSet;
use std::fs;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{build_axum_router, build_axum_router_with_dashboard, AxumApiState};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
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
    assert_eq!(
        dashboard_body["boundaries"]["provider_transport"],
        "stub/off"
    );
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
    assert_eq!(body["error"], "admin auth is required for local backup");
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
