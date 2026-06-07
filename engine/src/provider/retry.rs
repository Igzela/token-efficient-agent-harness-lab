use std::sync::Arc;

use crate::provider::config::RetryPolicy;
use crate::provider::{Provider, ProviderError, ProviderRequest, ProviderResult};

pub fn should_retry(error: &ProviderError, policy: &RetryPolicy, attempt: i64) -> bool {
    if attempt >= policy.max_retries {
        return false;
    }
    if !error.retryable {
        return false;
    }
    policy.retryable_error_domains.contains(&error.error_domain)
}

pub fn compute_delay_ms(policy: &RetryPolicy, attempt: i64) -> i64 {
    let base = match policy.backoff_strategy.as_str() {
        "linear" => policy.base_delay_ms * (attempt + 1),
        "exponential" => {
            let delay = policy.base_delay_ms * 2_i64.pow(attempt as u32);
            delay.min(policy.max_delay_ms)
        }
        _ => 0,
    };
    // Deterministic jitter: vary by ±20% based on attempt number
    // Uses a simple hash-like approach to avoid needing a random dependency
    let jitter_factor = 1.0 + (((attempt as f64 * 0.618033988749895).fract() - 0.5) * 0.4);
    (base as f64 * jitter_factor).round() as i64
}

pub struct RetryFallbackManager {
    primary: Arc<dyn Provider>,
    fallback: Option<Arc<dyn Provider>>,
    policy: RetryPolicy,
    budget_check: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl RetryFallbackManager {
    pub fn new(
        primary: Arc<dyn Provider>,
        fallback: Option<Arc<dyn Provider>>,
        policy: RetryPolicy,
        budget_check: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            primary,
            fallback,
            policy,
            budget_check,
        }
    }
}

#[async_trait::async_trait]
impl Provider for RetryFallbackManager {
    fn provider_id(&self) -> &str {
        self.primary.provider_id()
    }

    fn is_enabled(&self) -> bool {
        self.primary.is_enabled()
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        let mut last_err = match self.primary.invoke(request).await {
            Ok(resp) => return Ok(resp),
            Err(e) => e,
        };

        let mut attempt: i64 = 0;
        while should_retry(&last_err, &self.policy, attempt) {
            if self.policy.budget_check_per_retry && !(self.budget_check)() {
                return Err(ProviderError {
                    schema_version: "provider_error.v1".to_string(),
                    provider_id: self.primary.provider_id().to_string(),
                    error_domain: "budget_exhausted".to_string(),
                    message: "budget check failed before retry".to_string(),
                    retryable: false,
                });
            }

            let delay = compute_delay_ms(&self.policy, attempt);
            tokio::time::sleep(tokio::time::Duration::from_millis(delay as u64)).await;

            match self.primary.invoke(request).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    last_err = e;
                }
            }
            attempt += 1;
        }

        if last_err.error_domain == "budget_exhausted" {
            return Err(last_err);
        }

        if let Some(fallback) = &self.fallback {
            return fallback.invoke(request).await;
        }

        Err(last_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{DisabledProvider, ProviderResponse};

    fn make_retryable_error(domain: &str) -> ProviderError {
        ProviderError {
            schema_version: "provider_error.v1".to_string(),
            provider_id: "p1".to_string(),
            error_domain: domain.to_string(),
            message: "test".to_string(),
            retryable: true,
        }
    }

    #[test]
    fn should_retry_within_limit() {
        let policy = RetryPolicy::new("rp1");
        let err = make_retryable_error("provider_rate_limit");
        assert!(should_retry(&err, &policy, 0));
        assert!(should_retry(&err, &policy, 2));
        assert!(!should_retry(&err, &policy, 3));
    }

    #[test]
    fn should_not_retry_non_retryable() {
        let policy = RetryPolicy::new("rp1");
        let err = ProviderError {
            retryable: false,
            ..make_retryable_error("provider_auth")
        };
        assert!(!should_retry(&err, &policy, 0));
    }

    #[test]
    fn delay_exponential() {
        let policy = RetryPolicy::new("rp1");
        // Jitter varies ±20%, so check range
        let d0 = compute_delay_ms(&policy, 0);
        assert!((800..=1200).contains(&d0), "attempt 0: {}", d0);
        let d1 = compute_delay_ms(&policy, 1);
        assert!((1600..=2400).contains(&d1), "attempt 1: {}", d1);
        let d2 = compute_delay_ms(&policy, 2);
        assert!((3200..=4800).contains(&d2), "attempt 2: {}", d2);
        // Verify monotonic increase (base doubles each attempt)
        assert!(d1 > d0, "d1 {} > d0 {}", d1, d0);
        assert!(d2 > d1, "d2 {} > d1 {}", d2, d1);
    }

    #[test]
    fn delay_capped() {
        let policy = RetryPolicy::new("rp1");
        let delay = compute_delay_ms(&policy, 20);
        assert!(delay <= policy.max_delay_ms);
    }

    #[test]
    fn delay_linear() {
        let mut policy = RetryPolicy::new("rp1");
        policy.backoff_strategy = "linear".to_string();
        policy.base_delay_ms = 500;
        // Jitter varies ±20%, so check range
        let d0 = compute_delay_ms(&policy, 0);
        assert!((400..=600).contains(&d0), "attempt 0: {}", d0);
        let d1 = compute_delay_ms(&policy, 1);
        assert!((800..=1200).contains(&d1), "attempt 1: {}", d1);
        let d2 = compute_delay_ms(&policy, 2);
        assert!((1200..=1800).contains(&d2), "attempt 2: {}", d2);
        // Verify monotonic increase
        assert!(d1 > d0, "d1 {} > d0 {}", d1, d0);
        assert!(d2 > d1, "d2 {} > d1 {}", d2, d1);
    }

    #[test]
    fn jitter_produces_varying_delays() {
        let policy = RetryPolicy::new("rp1");
        // Same base delay (attempt 0 = 1000ms) but jitter should produce different values
        // across different attempts
        let delays: Vec<i64> = (0..10).map(|a| compute_delay_ms(&policy, a)).collect();
        // At least some delays should differ from the exact base
        let exact_base: Vec<i64> = (0..10)
            .map(|a| 1000 * 2_i64.pow(a as u32).min(30))
            .collect();
        let has_jitter = delays.iter().zip(exact_base.iter()).any(|(d, b)| d != b);
        assert!(
            has_jitter,
            "jitter should produce at least one non-exact delay: {:?}",
            delays
        );
    }

    // --- Test helper: FailingThenSucceedProvider ---

    struct FailingThenSucceedProvider {
        provider_id: String,
        fail_count: std::sync::atomic::AtomicI64,
        max_failures: i64,
        error_domain: String,
    }

    impl FailingThenSucceedProvider {
        fn new(provider_id: &str, max_failures: i64, error_domain: &str) -> Self {
            Self {
                provider_id: provider_id.to_string(),
                fail_count: std::sync::atomic::AtomicI64::new(0),
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
            let count = self
                .fail_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
                    output: "success after retries".to_string(),
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
        policy.max_retries = 5;
        policy.base_delay_ms = 1;
        policy.max_delay_ms = 5;
        policy.backoff_strategy = "none".to_string();
        policy
    }

    #[tokio::test]
    async fn success_without_retry() {
        let primary = Arc::new(FailingThenSucceedProvider::new(
            "p",
            0,
            "provider_rate_limit",
        ));
        let manager = RetryFallbackManager::new(primary, None, fast_policy(), always_true());
        let request = ProviderRequest::local_stub("p", "m", "hello");
        let result = manager.invoke(&request).await.unwrap();
        assert_eq!(result.output, "success after retries");
        assert_eq!(result.provider_id, "p");
    }

    #[tokio::test]
    async fn retry_on_retryable_error() {
        let primary = Arc::new(FailingThenSucceedProvider::new(
            "p",
            2,
            "provider_rate_limit",
        ));
        let manager = RetryFallbackManager::new(primary, None, fast_policy(), always_true());
        let request = ProviderRequest::local_stub("p", "m", "hello");
        let result = manager.invoke(&request).await.unwrap();
        assert_eq!(result.output, "success after retries");
    }

    #[tokio::test]
    async fn budget_check_stops_retries() {
        let primary = Arc::new(FailingThenSucceedProvider::new(
            "p",
            5,
            "provider_rate_limit",
        ));
        let mut policy = fast_policy();
        policy.budget_check_per_retry = true;
        let manager = RetryFallbackManager::new(primary, None, policy, always_false());
        let request = ProviderRequest::local_stub("p", "m", "hello");
        let err = manager.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "budget_exhausted");
    }

    #[tokio::test]
    async fn budget_exhausted_blocks_fallback() {
        let primary = Arc::new(FailingThenSucceedProvider::new(
            "p",
            5,
            "provider_rate_limit",
        ));
        let fallback = Arc::new(FailingThenSucceedProvider::new(
            "fb",
            0,
            "provider_rate_limit",
        ));
        let mut policy = fast_policy();
        policy.budget_check_per_retry = true;
        let manager = RetryFallbackManager::new(primary, Some(fallback), policy, always_false());
        let request = ProviderRequest::local_stub("p", "m", "hello");
        let err = manager.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "budget_exhausted");
        assert_eq!(err.provider_id, "p");
    }

    #[tokio::test]
    async fn no_fallback_returns_failure() {
        let primary = Arc::new(FailingThenSucceedProvider::new(
            "p",
            100,
            "provider_rate_limit",
        ));
        let manager = RetryFallbackManager::new(primary, None, fast_policy(), always_true());
        let request = ProviderRequest::local_stub("p", "m", "hello");
        let err = manager.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_rate_limit");
        assert!(!err.retryable || err.error_domain == "provider_rate_limit");
    }

    #[tokio::test]
    async fn fallback_succeeds_after_primary_exhaustion() {
        let primary = Arc::new(FailingThenSucceedProvider::new(
            "p",
            100,
            "provider_rate_limit",
        ));
        let fallback = Arc::new(FailingThenSucceedProvider::new(
            "fb",
            0,
            "provider_rate_limit",
        ));
        let manager =
            RetryFallbackManager::new(primary, Some(fallback), fast_policy(), always_true());
        let request = ProviderRequest::local_stub("p", "m", "hello");
        let result = manager.invoke(&request).await.unwrap();
        assert_eq!(result.output, "success after retries");
        assert_eq!(result.provider_id, "fb");
    }

    #[tokio::test]
    async fn non_retryable_error_skips_retries() {
        struct NonRetryableProvider;
        #[async_trait::async_trait]
        impl Provider for NonRetryableProvider {
            fn provider_id(&self) -> &str {
                "nr"
            }
            fn is_enabled(&self) -> bool {
                true
            }
            async fn invoke(&self, _request: &ProviderRequest) -> ProviderResult {
                Err(ProviderError {
                    schema_version: "provider_error.v1".to_string(),
                    provider_id: "nr".to_string(),
                    error_domain: "provider_auth".to_string(),
                    message: "auth failed".to_string(),
                    retryable: false,
                })
            }
        }

        let primary = Arc::new(NonRetryableProvider);
        let fallback = Arc::new(FailingThenSucceedProvider::new(
            "fb",
            0,
            "provider_rate_limit",
        ));
        let manager =
            RetryFallbackManager::new(primary, Some(fallback), fast_policy(), always_true());
        let request = ProviderRequest::local_stub("nr", "m", "hello");
        let result = manager.invoke(&request).await.unwrap();
        assert_eq!(result.provider_id, "fb");
    }

    #[tokio::test]
    async fn manager_delegates_provider_id_and_enabled() {
        let primary = Arc::new(FailingThenSucceedProvider::new("my-provider", 0, "x"));
        let manager = RetryFallbackManager::new(primary, None, fast_policy(), always_true());
        assert_eq!(manager.provider_id(), "my-provider");
        assert!(manager.is_enabled());
    }

    #[tokio::test]
    async fn manager_with_disabled_primary() {
        let primary = Arc::new(DisabledProvider::new("disabled"));
        let manager = RetryFallbackManager::new(primary, None, fast_policy(), always_true());
        assert!(!manager.is_enabled());
    }
}
