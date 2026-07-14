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
async fn axum_provider_endpoints_roundtrip_safe_multi_provider_config() {
    struct CredentialGuard;
    impl CredentialGuard {
        fn new() -> Self {
            std::env::set_var("OPENAI_QUALITY_KEY", "test-openai-key");
            std::env::set_var("ANTHROPIC_JUDGE_KEY", "test-anthropic-key");
            Self
        }
    }
    impl Drop for CredentialGuard {
        fn drop(&mut self) {
            std::env::remove_var("OPENAI_QUALITY_KEY");
            std::env::remove_var("ANTHROPIC_JUDGE_KEY");
        }
    }
    let _credentials = CredentialGuard::new();
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("provider-endpoints.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let initial = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/endpoints")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(initial.status(), StatusCode::OK);
    let initial_body = response_json(initial).await;
    assert_eq!(initial_body["source"], "none");
    assert!(initial_body["endpoints"].as_array().unwrap().is_empty());
    assert_eq!(
        initial_body["safety"]["credential_storage"],
        "env_reference_only"
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/provider/endpoints")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "confirm_provider_endpoint_config": true,
                        "endpoints": [
                            {
                                "endpoint_id": "openai-quality",
                                "provider_type": "openai_compatible",
                                "base_url": "https://api.openai.example/v1",
                                "model": "quality-model",
                                "credential_env": "OPENAI_QUALITY_KEY",
                                "timeout_ms": 30000,
                                "input_cost_per_1k_usd": 0.01,
                                "output_cost_per_1k_usd": 0.03
                            },
                            {
                                "endpoint_id": "anthropic-judge",
                                "provider_type": "anthropic",
                                "base_url": "https://api.anthropic.example",
                                "model": "judge-model",
                                "credential_env": "ANTHROPIC_JUDGE_KEY",
                                "timeout_ms": 30000,
                                "input_cost_per_1k_usd": 0.02,
                                "output_cost_per_1k_usd": 0.04
                            },
                            {
                                "endpoint_id": "local-stub",
                                "provider_type": "stub",
                                "model": "stub-model",
                                "timeout_ms": 30000
                            }
                        ]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["source"], "local_config");
    let endpoints = body["endpoints"].as_array().unwrap();
    assert_eq!(endpoints.len(), 3);
    assert_eq!(endpoints[0]["endpoint_id"], "anthropic-judge");
    assert_eq!(endpoints[1]["endpoint_id"], "local-stub");
    assert_eq!(endpoints[2]["endpoint_id"], "openai-quality");
    assert_eq!(body["safety"]["raw_secrets_allowed"], false);
    assert_eq!(body["runtime"]["completion_executor_configured"], true);
    assert_eq!(body["runtime"]["completion_registry_configured"], true);
    assert_eq!(
        body["runtime"]["local_config_apply_requires_restart"],
        false
    );
    assert_eq!(
        body["runtime"]["local_config_applies_to_completion_api"],
        true
    );

    let stored = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/endpoints")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stored.status(), StatusCode::OK);
    let stored_body = response_json(stored).await;
    assert_eq!(stored_body["source"], "local_config");
    assert_eq!(stored_body["endpoints"], body["endpoints"]);
}

#[tokio::test]
async fn axum_provider_endpoints_lazily_applies_stored_local_config_to_completion_runtime() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("provider-endpoints-lazy.db")).unwrap();
    store
        .set_config_value(
            "adaptive_provider_endpoints",
            json!([{
                "endpoint_id": "local-stub",
                "provider_type": "stub",
                "model": "stub-model",
                "timeout_ms": 30000,
                "input_cost_per_1k_usd": 0.01,
                "output_cost_per_1k_usd": 0.02
            }]),
            "test",
        )
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/provider/endpoints")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["source"], "local_config");
    assert_eq!(body["runtime"]["completion_executor_configured"], true);
    assert_eq!(
        body["runtime"]["local_config_applies_to_completion_api"],
        true
    );
    assert_eq!(
        body["runtime"]["local_config_apply_requires_restart"],
        false
    );
}

#[tokio::test]
async fn axum_provider_endpoints_rejects_unconfirmed_or_secret_shaped_config() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("provider-endpoints-reject.db")).unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store));

    let unconfirmed = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/provider/endpoints")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "endpoints": [{
                            "endpoint_id": "local-stub",
                            "provider_type": "stub",
                            "model": "stub-model"
                        }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unconfirmed.status(), StatusCode::BAD_REQUEST);
    let unconfirmed_body = response_json(unconfirmed).await;
    assert_eq!(
        unconfirmed_body["code"],
        "provider_endpoint_config_not_confirmed"
    );

    let missing_credential = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/provider/endpoints")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "confirm_provider_endpoint_config": true,
                        "endpoints": [{
                            "endpoint_id": "missing-credential",
                            "provider_type": "openai_compatible",
                            "base_url": "https://api.openai.example/v1",
                            "model": "safe-model",
                            "credential_env": "ACP_TEST_MISSING_PROVIDER_KEY",
                            "timeout_ms": 30000,
                            "input_cost_per_1k_usd": 0.01,
                            "output_cost_per_1k_usd": 0.02
                        }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_credential.status(), StatusCode::BAD_REQUEST);
    let missing_credential_body = response_json(missing_credential).await;
    assert_eq!(
        missing_credential_body["code"],
        "credential_env_unavailable"
    );

    let secret = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/provider/endpoints")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "confirm_provider_endpoint_config": true,
                        "endpoints": [{
                            "endpoint_id": "secret-endpoint",
                            "provider_type": "openai_compatible",
                            "base_url": "https://api.openai.example/v1",
                            "model": "secret-model",
                            "credential_env": "sk-abcdefghijklmnopqrstuvwxyz",
                            "timeout_ms": 30000,
                            "input_cost_per_1k_usd": 0.01,
                            "output_cost_per_1k_usd": 0.02
                        }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(secret.status(), StatusCode::BAD_REQUEST);
    let secret_body = response_json(secret).await;
    assert_eq!(secret_body["code"], "sensitive_pattern_detected");
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
