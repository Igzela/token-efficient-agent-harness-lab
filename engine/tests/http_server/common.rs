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

pub(crate) async fn response_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

pub(crate) async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), 1_048_576).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

pub(crate) fn auto_adjustment_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn provider_cli_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) struct TrustedLocalProviderWorkflowEnvGuard;

impl TrustedLocalProviderWorkflowEnvGuard {
    pub(crate) fn enabled() -> Self {
        for key in [
            "ACP_ENABLE_PROVIDER_EXECUTION",
            "ACP_TRUSTED_LOCAL_PROFILE",
            "ACP_REQUIRE_AUTH",
            "ACP_ADMIN_API_KEY",
            "ACP_COST_PER_DISPATCH_USD",
            "ACP_COST_DAILY_USD",
            "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON",
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
            r#"[{"endpoint_id":"stub-provider","provider_type":"stub","model":"test-model","timeout_ms":30000,"input_cost_per_1k_usd":0.01,"output_cost_per_1k_usd":0.02}]"#,
        );
        Self
    }

    pub(crate) fn enabled_with_persisted_endpoints() -> Self {
        let guard = Self::enabled();
        std::env::remove_var("ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON");
        guard
    }
}

impl Drop for TrustedLocalProviderWorkflowEnvGuard {
    fn drop(&mut self) {
        for key in [
            "ACP_ENABLE_PROVIDER_EXECUTION",
            "ACP_TRUSTED_LOCAL_PROFILE",
            "ACP_REQUIRE_AUTH",
            "ACP_ADMIN_API_KEY",
            "ACP_COST_PER_DISPATCH_USD",
            "ACP_COST_DAILY_USD",
            "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON",
        ] {
            std::env::remove_var(key);
        }
    }
}

pub(crate) fn adaptive_operator_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// RAII guard for provider/adaptive-fusion execution env vars.
/// Ensures cleanup on panic so leaked env vars don't cascade failures
/// to other tests (e.g. dashboard operator status assertions).
pub(crate) struct ProviderExecutionEnvGuard;

impl ProviderExecutionEnvGuard {
    pub(crate) fn provider_execution() -> Self {
        std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
        Self
    }

    pub(crate) fn with_fusion() -> Self {
        std::env::set_var("ACP_ENABLE_PROVIDER_EXECUTION", "1");
        std::env::set_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION", "1");
        Self
    }
}

impl Drop for ProviderExecutionEnvGuard {
    fn drop(&mut self) {
        std::env::remove_var("ACP_ENABLE_PROVIDER_EXECUTION");
        std::env::remove_var("ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION");
    }
}

pub(crate) struct AdaptiveOperatorEnvGuard;

impl AdaptiveOperatorEnvGuard {
    pub(crate) fn invalid_policies() -> Self {
        std::env::set_var("ACP_ADAPTIVE_EXPERIMENT_TRAFFIC_RATE", "0.5");
        std::env::set_var("ACP_ADAPTIVE_AUTO_PROMOTION_MIN_CONFIDENCE", "2.0");
        std::env::set_var("ACP_ADAPTIVE_AUTO_PROMOTION_ROLLOUT_PERCENTAGE", "0");
        Self
    }
}

impl Drop for AdaptiveOperatorEnvGuard {
    fn drop(&mut self) {
        std::env::remove_var("ACP_ADAPTIVE_EXPERIMENT_TRAFFIC_RATE");
        std::env::remove_var("ACP_ADAPTIVE_AUTO_PROMOTION_MIN_CONFIDENCE");
        std::env::remove_var("ACP_ADAPTIVE_AUTO_PROMOTION_ROLLOUT_PERCENTAGE");
    }
}

pub(crate) fn target_repo_output_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) struct TargetRepoOutputEnvGuard;

impl TargetRepoOutputEnvGuard {
    pub(crate) fn enable_local_remote() -> Self {
        std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
        std::env::set_var("ACP_TARGET_REPO_ALLOW_LOCAL_REMOTE", "1");
        std::env::remove_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH");
        Self
    }
}

impl Drop for TargetRepoOutputEnvGuard {
    fn drop(&mut self) {
        std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
        std::env::remove_var("ACP_TARGET_REPO_ALLOW_LOCAL_REMOTE");
        std::env::remove_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH");
    }
}

pub(crate) fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub(crate) fn provider_audit_event(event_id: &str, created_at: &str) -> ProviderAuditEvent {
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

pub(crate) fn make_admin_app() -> (axum::Router, String) {
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

pub(crate) struct GeneratedAppFixture {
    pub(crate) app: axum::Router,
    pub(crate) key: String,
    pub(crate) readonly_key: String,
    pub(crate) db_path: PathBuf,
    pub(crate) _dir: TempDir,
}

pub(crate) fn make_generated_fixture() -> GeneratedAppFixture {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("generated-proof.db");
    let store = LocalProductStore::new(&db_path).unwrap();

    // Seed 5 failing dispatches on cheap_executor for code_generate to trigger
    // TierFailureConcentration pattern
    for i in 0..5 {
        store
            .record_dispatch(
                &format!("task {i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("disp-gen-{i}"), "final_status": "failed"},
                    "decision": {"selected_tier": "cheap_executor", "routing_policy": "default",
                        "budget_reservation": {"reserved_cost": 0.01},
                        "shadow_routes": []},
                    "analysis": {"task_class": "code_generate"},
                    "execution_result": {"executor_type": "noop", "status": "failed", "success": false},
                    "evaluation_result": {"status": "fail"}
                }),
                "test",
            )
            .unwrap();
    }

    // Seed high-cost strong_planner traces for code_debug. These produce a
    // high-confidence cost-optimization candidate with non-regressing simulation
    // deltas for active-apply tests.
    for i in 0..3 {
        let success = i == 0;
        store
            .record_dispatch(
                &format!("debug task {i}"),
                "api",
                &json!({
                    "record": {
                        "dispatch_id": format!("disp-cost-{i}"),
                        "final_status": if success { "completed" } else { "failed" }
                    },
                    "decision": {
                        "selected_tier": "strong_planner",
                        "routing_policy": "default",
                        "budget_reservation": {"reserved_cost": 1.00},
                        "shadow_routes": []
                    },
                    "analysis": {"task_class": "code_debug", "human_review_flag": true},
                    "execution_result": {
                        "executor_type": "noop",
                        "status": if success { "completed" } else { "failed" },
                        "success": success,
                        "estimated_cost": 1.00,
                        "latency_ms": 1000
                    },
                    "evaluation_result": {"status": if success { "pass" } else { "fail" }}
                }),
                "test",
            )
            .unwrap();
    }

    let mut resolver = TenantResolver::new();
    let mut admin_scopes = HashSet::new();
    admin_scopes.insert("dispatch:read".to_string());
    admin_scopes.insert("team:admin".to_string());
    admin_scopes.insert("health:read".to_string());
    admin_scopes.insert("audit:read".to_string());
    resolver.add_tenant(Tenant {
        tenant_id: "local".to_string(),
        name: "Local".to_string(),
        scopes: admin_scopes.clone(),
        rate_limit: Some(10_000),
    });
    let (_key, raw_key) = resolver
        .create_api_key("local", Some(admin_scopes), None, 1.0)
        .unwrap();
    let readonly_scopes = HashSet::from(["dispatch:read".to_string(), "audit:read".to_string()]);
    let (_readonly_key, readonly_raw_key) = resolver
        .create_api_key("local", Some(readonly_scopes), None, 1.0)
        .unwrap();
    let app = build_axum_router(AxumApiState::new().with_local_store(store).with_auth(
        resolver,
        RateLimiter::new(60.0, 10_000),
        Some(10_000),
        1.0,
    ));
    GeneratedAppFixture {
        app,
        key: raw_key,
        readonly_key: readonly_raw_key,
        db_path,
        _dir: dir,
    }
}

pub(crate) fn make_generated_app() -> (axum::Router, String) {
    let fixture = make_generated_fixture();
    (fixture.app, fixture.key)
}

pub(crate) async fn generated_high_cost_candidate_id(app: &axum::Router, key: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals/generated?limit=50")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    body["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| {
            candidate["policy_key"]
                .as_str()
                .unwrap_or("")
                .contains("strong_planner->balanced_worker")
                && candidate["confidence"].as_f64().unwrap_or(0.0) >= 0.85
        })
        .and_then(|candidate| candidate["proposal_id"].as_str())
        .unwrap()
        .to_string()
}

pub(crate) async fn generated_candidate_id_for_task(
    app: &axum::Router,
    key: &str,
    task_domain: &str,
    task_intent: &str,
) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/v1/proposals/generated?limit=50")
                .header(header::AUTHORIZATION, format!("Bearer {key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = response_json(resp).await;
    body["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| {
            candidate["task_domain"].as_str() == Some(task_domain)
                && candidate["task_intent"].as_str() == Some(task_intent)
                && candidate["confidence"].as_f64().unwrap_or(0.0) >= 0.85
        })
        .and_then(|candidate| candidate["proposal_id"].as_str())
        .unwrap()
        .to_string()
}

pub(crate) struct AutoAdjustmentEnvCleanup;

impl Drop for AutoAdjustmentEnvCleanup {
    fn drop(&mut self) {
        std::env::remove_var("ACP_ENABLE_AUTO_ADJUSTMENT");
        std::env::remove_var("ACP_AUTO_ADJUSTMENT_DRY_RUN");
        std::env::remove_var("ACP_AUTO_ADJUSTMENT_ACTIVE");
    }
}

pub(crate) struct AdaptivePolicyEnvCleanup;

impl Drop for AdaptivePolicyEnvCleanup {
    fn drop(&mut self) {
        std::env::remove_var("ACP_ENABLE_ADAPTIVE_POLICY_PROMOTION");
        std::env::remove_var("ACP_ADAPTIVE_POLICY_PROMOTION_ACTIVE");
        std::env::remove_var("ACP_ENABLE_ADAPTIVE_EXPLORATION");
        std::env::remove_var("ACP_ADAPTIVE_EXPLORATION_ACTIVE");
        std::env::remove_var("ACP_ADAPTIVE_EXPLORATION_KILL_SWITCH");
    }
}

