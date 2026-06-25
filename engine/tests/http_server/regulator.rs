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
async fn axum_regulator_state_empty_state_returns_zero_counts() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("regulator-empty.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/regulator/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;

    assert_eq!(body["proposals"]["pending_count"], 0);
    assert_eq!(body["proposals"]["active_count"], 0);
    assert_eq!(body["auto_adjustments"]["active_count"], 0);
    assert!(body["auto_adjustments"]["report"]["decisions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(body["active_routing_policy"].is_null());
}

#[tokio::test]
async fn axum_regulator_state_is_idempotent_no_mutation() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("regulator-idem.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/regulator/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    let first_body = response_json(first).await;

    let second = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/regulator/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);
    let second_body = response_json(second).await;

    assert_eq!(
        first_body, second_body,
        "regulator state endpoint must not mutate database between calls"
    );
}

#[tokio::test]
async fn axum_regulator_state_schema_version_is_v1() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("regulator-schema.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/regulator/state")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;

    assert_eq!(body["schema_version"], "regulator_state.v1");
}

