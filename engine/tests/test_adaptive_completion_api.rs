use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, OnceLock};

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use engine::feedback::{
    EndpointHealth, EndpointPricing, ModelEndpointRegistry, ModelEndpointSpec,
    ENDPOINT_REGISTRY_SCHEMA_VERSION,
};
use engine::http_server::{build_axum_router, AxumApiState};
use engine::infrastructure::auth::{Tenant, TenantResolver};
use engine::infrastructure::rate_limiter::RateLimiter;
use engine::provider::adaptive_execution::{
    AdaptiveExecutionExecutor, AdaptiveExecutionKillSwitch,
};
use engine::provider::{Provider, ProviderAuditRecorder};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::tempdir;
use tokio::sync::Mutex;
use tower::ServiceExt;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct AdaptiveEnv;

impl AdaptiveEnv {
    fn enabled() -> Self {
        std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
        std::env::set_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION", "1");
        std::env::remove_var("ACP_ADAPTIVE_FUSION_KILL_SWITCH");
        std::env::remove_var("ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING");
        std::env::remove_var("ACP_COST_PER_DISPATCH_USD");
        Self
    }
}

impl Drop for AdaptiveEnv {
    fn drop(&mut self) {
        for key in [
            "ACP_ENABLE_PROVIDER_EXECUTION",
            "ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION",
            "ACP_ADAPTIVE_FUSION_KILL_SWITCH",
            "ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING",
            "ACP_COST_PER_DISPATCH_USD",
        ] {
            std::env::remove_var(key);
        }
    }
}

struct TrustedLocalEnv;

impl TrustedLocalEnv {
    fn enabled() -> Self {
        for key in [
            "ACP_ENABLE_PROVIDER_EXECUTION",
            "ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION",
            "ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING",
            "ACP_ENABLE_ADAPTIVE_EXPERIMENTS",
            "ACP_ADAPTIVE_EXPERIMENTS_ACTIVE",
            "ACP_ENABLE_ADAPTIVE_AUTO_PROMOTION",
            "ACP_ADAPTIVE_AUTO_PROMOTION_ACTIVE",
        ] {
            std::env::remove_var(key);
        }
        std::env::set_var("ACP_TRUSTED_LOCAL_PROFILE", "1");
        std::env::set_var("ACP_REQUIRE_AUTH", "1");
        std::env::set_var("ACP_ADMIN_API_KEY", format!("harness_{}", "a".repeat(64)));
        std::env::set_var("ACP_COST_PER_DISPATCH_USD", "1.0");
        std::env::set_var("ACP_COST_DAILY_USD", "10.0");
        std::env::set_var(
            "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON",
            r#"[
                {"endpoint_id":"fast","provider_type":"stub","model":"test-model","timeout_ms":30000,"input_cost_per_1k_usd":0.01,"output_cost_per_1k_usd":0.02},
                {"endpoint_id":"judge","provider_type":"stub","model":"test-model","timeout_ms":30000,"input_cost_per_1k_usd":0.03,"output_cost_per_1k_usd":0.04},
                {"endpoint_id":"quality","provider_type":"stub","model":"test-model","timeout_ms":30000,"input_cost_per_1k_usd":0.02,"output_cost_per_1k_usd":0.03}
            ]"#,
        );
        Self
    }

    fn enabled_without_endpoint_config() -> Self {
        let env = Self::enabled();
        std::env::remove_var("ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON");
        env
    }
}

impl Drop for TrustedLocalEnv {
    fn drop(&mut self) {
        for key in [
            "ACP_TRUSTED_LOCAL_PROFILE",
            "ACP_REQUIRE_AUTH",
            "ACP_ADMIN_API_KEY",
            "ACP_COST_PER_DISPATCH_USD",
            "ACP_COST_DAILY_USD",
            "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON",
            "ACP_ADAPTIVE_FUSION_KILL_SWITCH",
            "ACP_ADAPTIVE_EXPERIMENTS_PAUSED",
            "ACP_ADAPTIVE_EXPERIMENTS_KILL_SWITCH",
            "ACP_ADAPTIVE_AUTO_PROMOTION_KILL_SWITCH",
        ] {
            std::env::remove_var(key);
        }
    }
}

async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn registry_snapshot() -> engine::feedback::ModelEndpointRegistrySnapshot {
    let mut registry = ModelEndpointRegistry::new();
    for (index, endpoint_id) in ["fast", "quality", "judge"].into_iter().enumerate() {
        registry
            .upsert(ModelEndpointSpec {
                schema_version: ENDPOINT_REGISTRY_SCHEMA_VERSION.to_string(),
                endpoint_id: endpoint_id.to_string(),
                provider_id: endpoint_id.to_string(),
                model_id: "test-model".to_string(),
                enabled: true,
                capabilities: vec!["completion".to_string()],
                context_window_tokens: 32_768,
                supports_tools: false,
                supports_parallel_tools: false,
                pricing: EndpointPricing {
                    input_cost_per_1k_usd: 0.01 + index as f64 * 0.01,
                    output_cost_per_1k_usd: 0.02 + index as f64 * 0.01,
                    cache_read_cost_per_1k_usd: None,
                    cache_write_cost_per_1k_usd: None,
                },
                health: EndpointHealth {
                    status: "healthy".to_string(),
                    score: 0.8 + index as f64 * 0.05,
                    observed_at: None,
                },
                credential_reference: None,
            })
            .unwrap();
    }
    registry.snapshot()
}

fn app() -> (
    axum::Router,
    Arc<LocalProductStore>,
    String,
    tempfile::TempDir,
) {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("team.db")).unwrap());
    let providers = ["fast", "quality", "judge"]
        .into_iter()
        .map(|endpoint_id| {
            (
                endpoint_id.to_string(),
                Arc::new(
                    engine::provider::stub::StubProvider::new(endpoint_id)
                        .with_default_model("test-model"),
                ) as Arc<dyn Provider>,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let executor = Arc::new(AdaptiveExecutionExecutor::new(
        providers,
        Arc::new(ProviderAuditRecorder::with_store(store.clone())),
        AdaptiveExecutionKillSwitch::new(),
    ));

    let scopes = HashSet::from(["dispatch:read".to_string(), "dispatch:execute".to_string()]);
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let state = AxumApiState::new()
        .with_local_store_arc(store.clone())
        .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0)
        .with_adaptive_provider_executor(executor)
        .with_adaptive_registry_snapshot(registry_snapshot());
    (build_axum_router(state), store, raw_key, dir)
}

fn app_without_adaptive_runtime() -> (
    axum::Router,
    Arc<LocalProductStore>,
    String,
    tempfile::TempDir,
) {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("team.db")).unwrap());
    let scopes = HashSet::from([
        "config:admin".to_string(),
        "config:read".to_string(),
        "dispatch:execute".to_string(),
        "dispatch:read".to_string(),
        "health:read".to_string(),
    ]);
    let mut resolver = TenantResolver::new();
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: scopes.clone(),
        rate_limit: Some(100),
    });
    let (_, raw_key) = resolver
        .create_api_key("local", Some(scopes), None, 1.0)
        .unwrap();
    let state = AxumApiState::new()
        .with_local_store_arc(store.clone())
        .with_auth(resolver, RateLimiter::new(60.0, 100), Some(100), 1.0);
    (build_axum_router(state), store, raw_key, dir)
}

fn completion_request(api_key: Option<&str>, body: Value) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/adaptive-fusion/completions")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(api_key) = api_key {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {api_key}"));
    }
    builder.body(Body::from(body.to_string())).unwrap()
}

fn save_provider_endpoints_request(api_key: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(Method::PUT)
        .uri("/api/v1/provider/endpoints")
        .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn provider_endpoints_request(api_key: &str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri("/api/v1/provider/endpoints")
        .header(header::AUTHORIZATION, format!("Bearer {api_key}"))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn completion_is_default_off_and_requires_auth() {
    let _guard = env_lock().lock().await;
    let (app, _, raw_key, _dir) = app();
    let disabled = app
        .clone()
        .oneshot(completion_request(
            Some(&raw_key),
            json!({"prompt": "solve"}),
        ))
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(disabled).await["code"],
        "adaptive_provider_not_available"
    );

    let _env = AdaptiveEnv::enabled();
    let unauthorized = app
        .oneshot(completion_request(None, json!({"prompt": "solve"})))
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn completion_returns_compact_output_and_optional_routing_metadata() {
    let _guard = env_lock().lock().await;
    let _env = AdaptiveEnv::enabled();
    let (app, store, raw_key, _dir) = app();
    let compact = app
        .clone()
        .oneshot(completion_request(
            Some(&raw_key),
            json!({
                "prompt": "solve",
                "task_class": "coding",
                "objective": "quality",
                "risk_level": "low",
                "metadata": {"client_tag": "ignored-private-context"}
            }),
        ))
        .await
        .unwrap();
    assert_eq!(compact.status(), StatusCode::OK);
    let compact = response_json(compact).await;
    assert!(compact["output"].is_string());
    assert!(compact["usage"]["input_tokens"].is_number());
    assert!(compact.get("routing_metadata").is_none());
    assert!(compact.get("candidate_id").is_none());
    assert!(compact.get("policy_hash").is_none());

    let detailed = app
        .oneshot(completion_request(
            Some(&raw_key),
            json!({
                "prompt": "solve again",
                "task_class": "coding",
                "objective": "efficient",
                "risk_level": "low",
                "include_routing_metadata": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(detailed.status(), StatusCode::OK);
    let detailed = response_json(detailed).await;
    assert!(detailed["routing_metadata"]["candidate_id"].is_string());
    assert!(detailed["routing_metadata"]["candidate_hash"].is_string());
    assert!(detailed["routing_metadata"]["observation_id"].is_string());
    assert_eq!(store.adaptive_observations().unwrap().len(), 2);
    let persisted = serde_json::to_string(&store.adaptive_observations().unwrap()).unwrap();
    assert!(!persisted.contains("ignored-private-context"));
    assert!(!persisted.contains("solve again"));
}

#[tokio::test]
async fn completion_uses_safe_provider_endpoints_saved_in_local_config() {
    let _guard = env_lock().lock().await;
    let _env = AdaptiveEnv::enabled();
    let (app, store, raw_key, _dir) = app_without_adaptive_runtime();

    let saved = app
        .clone()
        .oneshot(save_provider_endpoints_request(
            &raw_key,
            json!({
                "confirm_provider_endpoint_config": true,
                "endpoints": [{
                    "endpoint_id": "local-stub",
                    "provider_type": "stub",
                    "model": "test-model",
                    "timeout_ms": 30000,
                    "input_cost_per_1k_usd": 0.01,
                    "output_cost_per_1k_usd": 0.02
                }]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let saved_body = response_json(saved).await;
    assert_eq!(saved_body["source"], "local_config");
    assert_eq!(
        saved_body["runtime"]["completion_executor_configured"],
        true
    );

    let completed = app
        .oneshot(completion_request(
            Some(&raw_key),
            json!({
                "prompt": "solve from saved local config",
                "task_class": "coding",
                "objective": "efficient",
                "risk_level": "low",
                "include_routing_metadata": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(completed.status(), StatusCode::OK);
    let completed = response_json(completed).await;
    assert!(completed["output"].is_string());
    assert_eq!(completed["routing_metadata"]["candidate_kind"], "single");
    assert_eq!(store.adaptive_observations().unwrap().len(), 1);
}

#[tokio::test]
async fn environment_provider_endpoints_remain_authoritative_over_saved_local_config() {
    let _guard = env_lock().lock().await;
    let _env = TrustedLocalEnv::enabled();
    let (app, _, raw_key, _dir) = app_without_adaptive_runtime();

    let saved = app
        .clone()
        .oneshot(save_provider_endpoints_request(
            &raw_key,
            json!({
                "confirm_provider_endpoint_config": true,
                "endpoints": [{
                    "endpoint_id": "local-stub",
                    "provider_type": "stub",
                    "model": "local-model",
                    "timeout_ms": 30000,
                    "input_cost_per_1k_usd": 0.01,
                    "output_cost_per_1k_usd": 0.02
                }]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);
    let saved = response_json(saved).await;
    assert_eq!(
        saved["runtime"]["local_config_applies_to_completion_api"],
        false
    );
    assert_eq!(
        saved["runtime"]["local_config_error_code"],
        "environment_override_active"
    );

    let active = app
        .oneshot(provider_endpoints_request(&raw_key))
        .await
        .unwrap();
    assert_eq!(active.status(), StatusCode::OK);
    let active = response_json(active).await;
    assert_eq!(active["source"], "environment");
    assert_eq!(active["endpoints"][0]["endpoint_id"], "fast");
}

#[tokio::test]
async fn invalid_environment_provider_config_does_not_fall_back_to_saved_local_config() {
    let _guard = env_lock().lock().await;
    let _env = TrustedLocalEnv::enabled();
    let (app, store, raw_key, _dir) = app_without_adaptive_runtime();
    store
        .set_config_value(
            "adaptive_provider_endpoints",
            json!([{
                "endpoint_id": "local-stub",
                "provider_type": "stub",
                "model": "local-model",
                "input_cost_per_1k_usd": 0.01,
                "output_cost_per_1k_usd": 0.02
            }]),
            "test",
        )
        .unwrap();
    std::env::set_var("ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON", "invalid");

    let response = app
        .oneshot(provider_endpoints_request(&raw_key))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["source"], "environment");
    assert!(body["endpoints"].as_array().unwrap().is_empty());
    assert_eq!(
        body["runtime"]["local_config_error_code"],
        "invalid_endpoint_config_json"
    );
}

#[tokio::test]
async fn trusted_local_profile_activates_after_provider_endpoints_are_saved() {
    let _guard = env_lock().lock().await;
    let _env = TrustedLocalEnv::enabled_without_endpoint_config();
    let (app, store, raw_key, _dir) = app_without_adaptive_runtime();

    let saved = app
        .clone()
        .oneshot(save_provider_endpoints_request(
            &raw_key,
            json!({
                "confirm_provider_endpoint_config": true,
                "endpoints": [{
                    "endpoint_id": "local-stub",
                    "provider_type": "stub",
                    "model": "test-model",
                    "timeout_ms": 30000,
                    "input_cost_per_1k_usd": 0.01,
                    "output_cost_per_1k_usd": 0.02
                }]
            }),
        ))
        .await
        .unwrap();
    assert_eq!(saved.status(), StatusCode::OK);

    let completed = app
        .clone()
        .oneshot(completion_request(
            Some(&raw_key),
            json!({"prompt": "solve with persisted trusted-local config"}),
        ))
        .await
        .unwrap();
    assert_eq!(completed.status(), StatusCode::OK);

    let dashboard = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/dashboard")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dashboard.status(), StatusCode::OK);
    assert_eq!(
        response_json(dashboard).await["adaptive_fusion"]["trusted_local_profile"]["ready"],
        true
    );

    let dispatched = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"raw_request": "dispatch with persisted trusted-local config"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(dispatched.status(), StatusCode::OK);
    assert!(response_json(dispatched).await.get("record").is_none());
    assert_eq!(store.adaptive_observations().unwrap().len(), 2);
}

#[tokio::test]
async fn invalid_persisted_provider_config_does_not_block_deterministic_dispatch() {
    let _guard = env_lock().lock().await;
    let (app, store, raw_key, _dir) = app_without_adaptive_runtime();
    store
        .set_config_value(
            "adaptive_provider_endpoints",
            json!({"invalid": "shape"}),
            "test",
        )
        .unwrap();

    let config = app
        .clone()
        .oneshot(provider_endpoints_request(&raw_key))
        .await
        .unwrap();
    assert_eq!(config.status(), StatusCode::OK);
    let config = response_json(config).await;
    assert_eq!(config["source"], "local_config");
    assert!(config["endpoints"].as_array().unwrap().is_empty());
    assert_eq!(
        config["runtime"]["local_config_error_code"],
        "invalid_endpoint_config_json"
    );

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": "solve"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response_json(response).await.get("record").is_some());
}

#[tokio::test]
async fn completion_respects_cost_and_kill_gates() {
    let _guard = env_lock().lock().await;
    let _env = AdaptiveEnv::enabled();
    std::env::set_var("ACP_COST_PER_DISPATCH_USD", "0.0001");
    let (over_budget_app, _, raw_key, _dir) = app();
    let over_budget = over_budget_app
        .oneshot(completion_request(
            Some(&raw_key),
            json!({"prompt": "solve"}),
        ))
        .await
        .unwrap();
    assert_eq!(over_budget.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(over_budget).await["code"],
        "adaptive_global_cost_gate_blocked"
    );

    std::env::remove_var("ACP_COST_PER_DISPATCH_USD");
    std::env::set_var("ACP_ADAPTIVE_FUSION_KILL_SWITCH", "1");
    let (app, _, raw_key, _dir) = app();
    let killed = app
        .oneshot(completion_request(
            Some(&raw_key),
            json!({"prompt": "solve"}),
        ))
        .await
        .unwrap();
    assert_eq!(killed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response_json(killed).await["code"],
        "adaptive_execution_killed"
    );
}

#[tokio::test]
async fn dispatch_delegation_requires_explicit_default_live_gate() {
    let _guard = env_lock().lock().await;
    let _env = AdaptiveEnv::enabled();
    let (app, _, raw_key, _dir) = app();
    let ordinary = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": "solve"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ordinary.status(), StatusCode::OK);
    assert!(response_json(ordinary).await.get("record").is_some());

    std::env::set_var("ACP_ADAPTIVE_DEFAULT_LIVE_ROUTING", "1");
    let delegated = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": "solve"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delegated.status(), StatusCode::OK);
    let delegated = response_json(delegated).await;
    assert!(delegated["output"].is_string());
    assert!(delegated.get("record").is_none());
}

#[tokio::test]
async fn trusted_local_profile_enables_adaptive_dispatch_without_legacy_flags() {
    let _guard = env_lock().lock().await;
    let _env = TrustedLocalEnv::enabled();
    let (app, _, raw_key, _dir) = app();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/dispatch")
                .header(header::AUTHORIZATION, format!("Bearer {raw_key}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"raw_request": "solve"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert!(body["output"].is_string());
    assert!(body.get("record").is_none());
}
