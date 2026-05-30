use engine::dispatch_decision::DispatchDecision;
use engine::executor_adapter::Executor;
use engine::provider::audit::ProviderAuditRecorder;
use engine::provider::cost_gate::{check_cost_gates, CostGateBlock, CostGateConfig};
use engine::provider::executor::{make_not_executed_result, ProviderExecutor};
use engine::provider::retry::RetryFallbackManager;
use engine::provider::{
    DisabledProvider, Provider, ProviderError, ProviderRequest, ProviderResponse, ProviderResult,
    RetryPolicy,
};
use engine::runtime::FixtureRuntime;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

// --- Test helper providers ---

struct AlwaysFailProvider {
    provider_id: String,
    error_domain: String,
    call_count: AtomicI64,
}

impl AlwaysFailProvider {
    fn new(provider_id: &str, error_domain: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            error_domain: error_domain.to_string(),
            call_count: AtomicI64::new(0),
        }
    }

    fn calls(&self) -> i64 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Provider for AlwaysFailProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }
    fn is_enabled(&self) -> bool {
        true
    }
    async fn invoke(&self, _request: &ProviderRequest) -> ProviderResult {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Err(ProviderError {
            schema_version: "provider_error.v1".to_string(),
            provider_id: self.provider_id.clone(),
            error_domain: self.error_domain.clone(),
            message: format!("{} failed", self.error_domain),
            retryable: self.error_domain == "provider_rate_limit"
                || self.error_domain == "provider_timeout"
                || self.error_domain == "provider_capacity",
        })
    }
}

struct FailingThenSucceedProvider {
    provider_id: String,
    fail_count: AtomicI64,
    max_failures: i64,
    error_domain: String,
}

impl FailingThenSucceedProvider {
    fn new(provider_id: &str, max_failures: i64, error_domain: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            fail_count: AtomicI64::new(0),
            max_failures,
            error_domain: error_domain.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl Provider for FailingThenSucceedProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }
    fn is_enabled(&self) -> bool {
        true
    }
    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if count < self.max_failures {
            Err(ProviderError {
                schema_version: "provider_error.v1".to_string(),
                provider_id: self.provider_id.clone(),
                error_domain: self.error_domain.clone(),
                message: format!("failure #{}", count + 1),
                retryable: true,
            })
        } else {
            Ok(ProviderResponse {
                schema_version: "provider_response.v1".to_string(),
                provider_id: self.provider_id.clone(),
                model: request.model.clone(),
                output: "success".to_string(),
                input_tokens: Some(10),
                output_tokens: Some(5),
                estimated_cost: Some(0.001),
                provider_request_id: None,
            })
        }
    }
}

fn always_true() -> Arc<dyn Fn() -> bool + Send + Sync> {
    Arc::new(|| true)
}

fn always_false() -> Arc<dyn Fn() -> bool + Send + Sync> {
    Arc::new(|| false)
}

fn fast_policy() -> RetryPolicy {
    let mut policy = RetryPolicy::new("rp-test");
    policy.max_retries = 3;
    policy.base_delay_ms = 1;
    policy.max_delay_ms = 5;
    policy.backoff_strategy = "none".to_string();
    policy
}

fn make_decision() -> DispatchDecision {
    DispatchDecision {
        decision_id: "dec-0001".to_string(),
        analysis_id: "ana-0001".to_string(),
        analysis_snapshot: serde_json::json!({"selected_model": "test-model"}),
        selected_tier: "balanced_worker".to_string(),
        decision_status: "decided".to_string(),
        created_at: "2000-01-01T00:00:00+00:00".to_string(),
        ..DispatchDecision::default()
    }
}

// --- Failure matrix tests ---

#[tokio::test]
async fn retry_exhaustion_returns_final_error() {
    let primary = Arc::new(AlwaysFailProvider::new("p1", "provider_rate_limit"));
    let manager = RetryFallbackManager::new(primary.clone(), None, fast_policy(), always_true());
    let request = ProviderRequest::local_stub("p1", "m", "hello");

    let err = manager.invoke(&request).await.unwrap_err();

    assert_eq!(err.error_domain, "provider_rate_limit");
    assert_eq!(primary.calls(), 4);
}

#[tokio::test]
async fn fallback_succeeds_after_primary_exhaustion() {
    let primary = Arc::new(AlwaysFailProvider::new("p1", "provider_rate_limit"));
    let fallback = Arc::new(FailingThenSucceedProvider::new(
        "fb",
        0,
        "provider_rate_limit",
    ));
    let manager = RetryFallbackManager::new(
        primary.clone(),
        Some(fallback),
        fast_policy(),
        always_true(),
    );
    let request = ProviderRequest::local_stub("p1", "m", "hello");

    let result = manager.invoke(&request).await.unwrap();

    assert_eq!(result.provider_id, "fb");
    assert_eq!(result.output, "success");
    assert_eq!(primary.calls(), 4);
}

#[tokio::test]
async fn fallback_also_fails_returns_last_error() {
    let primary = Arc::new(AlwaysFailProvider::new("p1", "provider_rate_limit"));
    let fallback = Arc::new(AlwaysFailProvider::new("fb", "provider_timeout"));
    let manager = RetryFallbackManager::new(primary, Some(fallback), fast_policy(), always_true());
    let request = ProviderRequest::local_stub("p1", "m", "hello");

    let err = manager.invoke(&request).await.unwrap_err();

    assert_eq!(err.provider_id, "fb");
    assert_eq!(err.error_domain, "provider_timeout");
}

#[tokio::test]
async fn budget_exhausted_mid_retry_blocks_fallback() {
    let primary = Arc::new(AlwaysFailProvider::new("p1", "provider_rate_limit"));
    let fallback = Arc::new(FailingThenSucceedProvider::new(
        "fb",
        0,
        "provider_rate_limit",
    ));
    let mut policy = fast_policy();
    policy.budget_check_per_retry = true;
    let manager = RetryFallbackManager::new(primary, Some(fallback), policy, always_false());
    let request = ProviderRequest::local_stub("p1", "m", "hello");

    let err = manager.invoke(&request).await.unwrap_err();

    assert_eq!(err.error_domain, "budget_exhausted");
    assert_eq!(err.provider_id, "p1");
}

#[tokio::test]
async fn non_retryable_error_skips_all_retries() {
    let primary = Arc::new(AlwaysFailProvider::new("p1", "provider_auth"));
    let fallback = Arc::new(FailingThenSucceedProvider::new(
        "fb",
        0,
        "provider_rate_limit",
    ));
    let manager = RetryFallbackManager::new(
        primary.clone(),
        Some(fallback),
        fast_policy(),
        always_true(),
    );
    let request = ProviderRequest::local_stub("p1", "m", "hello");

    let result = manager.invoke(&request).await.unwrap();

    assert_eq!(result.provider_id, "fb");
    assert_eq!(primary.calls(), 1);
}

#[tokio::test]
async fn disabled_provider_returns_immediate_error() {
    let provider = DisabledProvider::new("disabled-p");
    let request = ProviderRequest::local_stub("disabled-p", "m", "hello");

    let err = provider.invoke(&request).await.unwrap_err();

    assert_eq!(err.error_domain, "provider_disabled");
    assert!(!err.retryable);
}

#[tokio::test]
async fn disabled_provider_not_retried_by_manager() {
    let primary = Arc::new(DisabledProvider::new("disabled-p"));
    let manager = RetryFallbackManager::new(primary, None, fast_policy(), always_true());
    let request = ProviderRequest::local_stub("disabled-p", "m", "hello");

    let err = manager.invoke(&request).await.unwrap_err();

    assert_eq!(err.error_domain, "provider_disabled");
    assert_eq!(err.provider_id, "disabled-p");
}

#[test]
fn cost_gate_per_dispatch_block() {
    let config = CostGateConfig::new(Some(0.01), None);
    let result = check_cost_gates(&config, 0.05, 0.0);
    assert_eq!(
        result,
        Err(CostGateBlock::PerDispatchExceeded {
            cap: 0.01,
            reserved: 0.05,
        })
    );
}

#[test]
fn cost_gate_daily_block() {
    let config = CostGateConfig::new(None, Some(1.0));
    let result = check_cost_gates(&config, 0.5, 0.8);
    assert_eq!(
        result,
        Err(CostGateBlock::DailyExceeded {
            cap: 1.0,
            today_total: 0.8,
        })
    );
}

#[test]
fn cost_gate_passes_within_limits() {
    let config = CostGateConfig::new(Some(0.1), Some(10.0));
    assert_eq!(check_cost_gates(&config, 0.05, 5.0), Ok(()));
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_executor_records_audit_on_success() {
    let provider = Arc::new(FailingThenSucceedProvider::new("audit-p", 0, "x"));
    let recorder = Arc::new(ProviderAuditRecorder::new());
    let executor = ProviderExecutor::new(provider).with_audit_recorder(recorder.clone());
    let decision = make_decision();
    let mut runtime = FixtureRuntime::new();

    let result = executor.execute(&decision, "hello", "disp-001", &mut runtime);

    assert_eq!(result.status, "provider_completed");
    assert_eq!(recorder.count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_executor_records_audit_on_failure() {
    let provider = Arc::new(AlwaysFailProvider::new("fail-p", "provider_timeout"));
    let recorder = Arc::new(ProviderAuditRecorder::new());
    let executor = ProviderExecutor::new(provider).with_audit_recorder(recorder.clone());
    let decision = make_decision();
    let mut runtime = FixtureRuntime::new();

    let result = executor.execute(&decision, "hello", "disp-002", &mut runtime);

    assert_eq!(result.status, "failed");
    assert_eq!(result.error_domain.as_deref(), Some("provider_timeout"));
    assert_eq!(recorder.count(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn provider_executor_disabled_provider_failure() {
    let provider = Arc::new(DisabledProvider::new("disabled-exec"));
    let executor = ProviderExecutor::new(provider);
    let decision = make_decision();
    let mut runtime = FixtureRuntime::new();

    let result = executor.execute(&decision, "hello", "disp-003", &mut runtime);

    assert_eq!(result.status, "failed");
    assert_eq!(result.error_domain.as_deref(), Some("provider_disabled"));
}

#[test]
fn not_executed_result_for_governance_block() {
    let decision = make_decision();
    let mut runtime = FixtureRuntime::new();
    let result = make_not_executed_result(
        &decision,
        "disp-004",
        &mut runtime,
        "cost_gate_block",
        "per-dispatch cost cap exceeded",
    );

    assert_eq!(result.status, "not_executed");
    assert_eq!(result.executor_type, "provider");
    assert_eq!(result.error_domain.as_deref(), Some("cost_gate_block"));
    assert!(result.output.is_none());
    assert!(result.input_tokens.is_none());
}

#[tokio::test]
async fn retry_exhaustion_then_fallback_failure_chain() {
    let primary = Arc::new(AlwaysFailProvider::new("p1", "provider_capacity"));
    let fallback = Arc::new(AlwaysFailProvider::new("fb1", "provider_auth"));
    let manager = RetryFallbackManager::new(
        primary.clone(),
        Some(fallback),
        fast_policy(),
        always_true(),
    );
    let request = ProviderRequest::local_stub("p1", "m", "hello");

    let err = manager.invoke(&request).await.unwrap_err();

    assert_eq!(err.provider_id, "fb1");
    assert_eq!(err.error_domain, "provider_auth");
    assert_eq!(primary.calls(), 4);
}

#[tokio::test]
async fn concurrent_provider_invocations_through_manager() {
    let primary = Arc::new(FailingThenSucceedProvider::new(
        "conc-p",
        1,
        "provider_rate_limit",
    ));
    let manager = Arc::new(RetryFallbackManager::new(
        primary,
        None,
        fast_policy(),
        always_true(),
    ));

    let thread_count = 6;
    let barrier = Arc::new(Barrier::new(thread_count));

    let handles: Vec<_> = (0..thread_count)
        .map(|t| {
            let manager = manager.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let request = ProviderRequest::local_stub("conc-p", "m", &format!("req-{t}"));
                    let result = manager.invoke(&request).await.unwrap();
                    assert_eq!(result.output, "success");
                });
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn cost_gate_both_caps_per_dispatch_fails_first() {
    let config = CostGateConfig::new(Some(0.01), Some(1.0));
    let result = check_cost_gates(&config, 0.05, 0.5);
    assert!(matches!(
        result,
        Err(CostGateBlock::PerDispatchExceeded { .. })
    ));
}

#[test]
fn cost_gate_boundary_exact_cap_passes() {
    let config = CostGateConfig::new(Some(0.01), Some(1.0));
    assert_eq!(check_cost_gates(&config, 0.01, 0.0), Ok(()));
    assert_eq!(check_cost_gates(&config, 0.0, 1.0), Ok(()));
}

#[test]
fn cost_gate_zero_caps_block_everything() {
    let config = CostGateConfig::new(Some(0.0), Some(0.0));
    let result = check_cost_gates(&config, 0.001, 0.0);
    assert!(matches!(
        result,
        Err(CostGateBlock::PerDispatchExceeded { .. })
    ));
}

#[tokio::test]
async fn retry_with_exponential_backoff_delays_increase() {
    let mut policy = fast_policy();
    policy.backoff_strategy = "exponential".to_string();
    policy.base_delay_ms = 10;
    policy.max_delay_ms = 1000;
    let primary = Arc::new(AlwaysFailProvider::new("p1", "provider_rate_limit"));
    let manager = RetryFallbackManager::new(primary, None, policy, always_true());
    let request = ProviderRequest::local_stub("p1", "m", "hello");

    let start = std::time::Instant::now();
    let _ = manager.invoke(&request).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() >= 30,
        "Expected at least 30ms of backoff delays, got {:?}",
        elapsed
    );
}

#[tokio::test]
async fn retry_with_linear_backoff_delays_increase() {
    let mut policy = fast_policy();
    policy.backoff_strategy = "linear".to_string();
    policy.base_delay_ms = 10;
    policy.max_delay_ms = 1000;
    let primary = Arc::new(AlwaysFailProvider::new("p1", "provider_rate_limit"));
    let manager = RetryFallbackManager::new(primary, None, policy, always_true());
    let request = ProviderRequest::local_stub("p1", "m", "hello");

    let start = std::time::Instant::now();
    let _ = manager.invoke(&request).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() >= 30,
        "Expected at least 30ms of linear backoff delays, got {:?}",
        elapsed
    );
}
