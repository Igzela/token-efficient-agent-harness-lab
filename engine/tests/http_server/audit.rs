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
    // GA-4: new observability fields
    assert!(
        body["secret_block_count"].is_number(),
        "secret_block_count should be present"
    );
    assert!(
        body["queue_length"].is_number(),
        "queue_length should be present"
    );
    assert_eq!(body["secret_block_count"], 0, "no artifacts yet");
    assert_eq!(body["queue_length"], 0, "no pending nodes");
}

