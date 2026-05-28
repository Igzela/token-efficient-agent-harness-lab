use std::collections::HashSet;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::http_server::{build_axum_router, AxumApiState};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
use serde_json::{json, Value};
use tower::ServiceExt;

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
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
