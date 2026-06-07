use std::sync::Arc;

use super::{Provider, ProviderError, ProviderRequest, ProviderResult};
use crate::infrastructure::circuit_breaker::{CircuitBreaker, CircuitState};

/// A provider wrapper that applies circuit breaker protection to invoke calls.
///
/// When the circuit is open, calls return a non-retryable `provider_circuit_open` error
/// without making the underlying provider call. This prevents cascading failures
/// when the provider is unavailable.
pub struct CircuitBreakerProvider {
    inner: Arc<dyn Provider>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl CircuitBreakerProvider {
    pub fn new(inner: Arc<dyn Provider>, circuit_breaker: Arc<CircuitBreaker>) -> Self {
        Self {
            inner,
            circuit_breaker,
        }
    }

    pub fn circuit_breaker(&self) -> &Arc<CircuitBreaker> {
        &self.circuit_breaker
    }
}

#[async_trait::async_trait]
impl Provider for CircuitBreakerProvider {
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }

    fn is_enabled(&self) -> bool {
        self.inner.is_enabled()
    }

    fn default_model(&self) -> Option<&str> {
        self.inner.default_model()
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        let provider_id = self.inner.provider_id().to_string();
        let current = self.circuit_breaker.state();

        // Check if circuit is open.
        match current {
            CircuitState::Open => {
                if !self.circuit_breaker.should_try_reset() {
                    return Err(ProviderError {
                        schema_version: "provider_error.v1".to_string(),
                        provider_id,
                        error_domain: "provider_circuit_open".to_string(),
                        message: "circuit breaker is open; provider calls are temporarily blocked"
                            .to_string(),
                        retryable: false,
                    });
                }
                self.circuit_breaker.transition_to(CircuitState::HalfOpen);
            }
            CircuitState::HalfOpen => {
                // Allow the probe call through.
            }
            CircuitState::Closed => {
                // Normal operation.
            }
        }

        // Make the actual provider call.
        self.circuit_breaker.record_call();
        match self.inner.invoke(request).await {
            Ok(result) => {
                self.circuit_breaker.record_success();
                Ok(result)
            }
            Err(err) => {
                self.circuit_breaker.record_failure();
                Err(err)
            }
        }
    }
}
