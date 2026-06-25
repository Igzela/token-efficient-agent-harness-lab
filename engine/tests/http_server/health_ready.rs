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
async fn axum_health_bypass_allows_unauthenticated_health_probe_when_auth_configured() {
    let state = AxumApiState::new().with_auth(
        TenantResolver::new(),
        RateLimiter::new(60.0, 10),
        Some(60),
        1.0,
    );
    let app = build_axum_router(state);

    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    let body = response_json(health).await;
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["tenant_id"], "local");

    let ready = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);
    let body = response_json(ready).await;
    assert_eq!(body["status"], "ready");
}

#[tokio::test]
async fn axum_health_bypass_is_skipped_when_auth_header_present() {
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "t1".to_string(),
        name: "T1".to_string(),
        scopes: HashSet::new(),
        rate_limit: Some(100),
    });
    let (_key, raw_key) = resolver
        .create_api_key(
            "t1",
            Some(HashSet::from(["dispatch:read".to_string()])),
            None,
            1.0,
        )
        .unwrap();
    let state = AxumApiState::new().with_auth(resolver, RateLimiter::new(60.0, 10), Some(60), 1.0);
    let app = build_axum_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/health")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

