use std::collections::HashSet;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{build_axum_router, AxumApiState};
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

fn admin_app() -> (axum::Router, String) {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("auth-hardening.db")).unwrap();
    let scopes = HashSet::from([
        "dispatch:read".to_string(),
        "dispatch:execute".to_string(),
        "team:read".to_string(),
        "team:admin".to_string(),
    ]);
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
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
async fn scope_update_immediately_restricts_existing_token() {
    let (app, admin_key) = admin_app();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "scoped-user",
                        "role": "readonly",
                        "scopes": ["dispatch:read", "team:read"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created = response_json(created).await;
    let key_id = created["key_id"].as_str().unwrap();
    let raw_key = created["raw_key"].as_str().unwrap();

    let updated = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{key_id}/scopes"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"scopes": ["dispatch:read"]}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    let team = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(team.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn rotation_preserves_key_expiry_metadata() {
    let (app, admin_key) = admin_app();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "expiring-user",
                        "role": "readonly",
                        "scopes": ["dispatch:read"],
                        "expires_at": 10.0
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let created = response_json(created).await;
    let old_key_id = created["key_id"].as_str().unwrap();

    let rotated = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/keys/{old_key_id}/rotate"))
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rotated.status(), StatusCode::OK);
    let rotated = response_json(rotated).await;
    let new_key_id = rotated["key_id"].as_str().unwrap();

    let listed = app
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
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = response_json(listed).await;
    let rotated_metadata = listed["keys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|key| key["key_id"] == new_key_id)
        .unwrap();
    assert_eq!(rotated_metadata["expires_at"], 10.0);
}

#[tokio::test]
async fn command_tick_requires_execute_scope_before_dispatch() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("command-scope.db")).unwrap();
    let scopes = HashSet::from(["dispatch:read".to_string()]);
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));

    let plan = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/plans")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "scope check", "request_source": "test"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(plan.status(), StatusCode::OK);
    let plan_id = response_json(plan).await["plan"]["plan_id"]
        .as_str()
        .unwrap()
        .to_string();

    let run = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/workflow-runs")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"plan_id": plan_id}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(run.status(), StatusCode::OK);
    let run_id = response_json(run).await["run"]["run_id"]
        .as_str()
        .unwrap()
        .to_string();

    let tick = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/workflow-runs/{run_id}/tick"))
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "executor": "command",
                        "command": "definitely-not-an-allowlisted-command"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(tick.status(), StatusCode::FORBIDDEN);
    assert_eq!(response_json(tick).await["code"], "missing_scope");
}

#[tokio::test]
async fn live_auth_uses_wall_clock_for_expiry_checks() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("live-clock.db")).unwrap();
    let scopes = HashSet::from(["team:read".to_string()]);
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_, expired_key) = resolver
        .create_api_key("local", Some(scopes), Some(1.0), 0.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth_live(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/team")
                .header(header::AUTHORIZATION, format!("Bearer {expired_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response_json(response).await["code"], "auth_required");
}

#[tokio::test]
async fn revoked_key_cannot_be_rotated_back_into_service() {
    let (app, admin_key) = admin_app();
    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/keys")
                .header(header::AUTHORIZATION, format!("Bearer {admin_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "user_id": "revoked-user",
                        "role": "readonly",
                        "scopes": ["dispatch:read"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let key_id = response_json(created).await["key_id"]
        .as_str()
        .unwrap()
        .to_string();

    let revoked = app
        .clone()
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
    assert_eq!(revoked.status(), StatusCode::OK);

    let rotated = app
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

    assert_eq!(rotated.status(), StatusCode::NOT_FOUND);
}
