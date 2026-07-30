pub mod adaptive_execution;
pub mod adaptive_observation;
pub mod agent_decision;
pub mod anthropic;
pub mod audit;
pub mod circuit_breaker_provider;
pub mod config;
pub mod cost_gate;
pub mod credential;
pub mod embedding;
pub mod executor;
pub mod fake;
pub mod managed_deepseek;
pub mod openai;
pub mod redaction;
pub mod retry;
pub mod stub;
pub mod transport;

pub use audit::{ProviderAuditEvent, ProviderAuditRecorder};
pub use config::{CredentialRef, ProviderConfig, RetryPolicy};
pub use cost_gate::{check_cost_gates, CostGateBlock, CostGateConfig};
pub use credential::CredentialBoundary;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderRequest {
    pub schema_version: String,
    pub provider_id: String,
    pub model: String,
    pub prompt: String,
    pub metadata: Value,
}

impl ProviderRequest {
    pub fn local_stub(provider_id: &str, model: &str, prompt: &str) -> Self {
        Self {
            schema_version: "provider_request.v1".to_string(),
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            prompt: prompt.to_string(),
            metadata: Value::Object(Default::default()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderResponse {
    pub schema_version: String,
    pub provider_id: String,
    pub model: String,
    pub output: String,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub estimated_cost: Option<f64>,
    pub provider_request_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderError {
    pub schema_version: String,
    pub provider_id: String,
    pub error_domain: String,
    pub message: String,
    pub retryable: bool,
}

pub type ProviderResult = Result<ProviderResponse, ProviderError>;

#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    fn provider_id(&self) -> &str;
    fn is_enabled(&self) -> bool;
    fn default_model(&self) -> Option<&str> {
        None
    }
    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult;
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisabledProvider {
    provider_id: String,
}

impl DisabledProvider {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
        }
    }
}

#[async_trait::async_trait]
impl Provider for DisabledProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn is_enabled(&self) -> bool {
        false
    }

    async fn invoke(&self, _request: &ProviderRequest) -> ProviderResult {
        Err(ProviderError {
            schema_version: "provider_error.v1".to_string(),
            provider_id: self.provider_id.clone(),
            error_domain: "provider_disabled".to_string(),
            message: "real provider calls are disabled by default".to_string(),
            retryable: false,
        })
    }
}

/// Bridges synchronous `harness::model_gateway::ModelProvider` to async `Provider`.
///
/// The harness ModelProvider trait is sync (`fn invoke(...) -> ModelResponse`).
/// The provider::Provider trait is async (`async fn invoke(...) -> ProviderResult`).
/// ProviderAdapter wraps the sync trait via `Arc` so `spawn_blocking` can own it.
/// Tier/token metadata flows through `ProviderRequest.metadata`.
pub struct ProviderAdapter {
    provider_id: String,
    inner: std::sync::Arc<dyn crate::harness::model_gateway::ModelProvider>,
}

impl ProviderAdapter {
    pub fn new(
        provider_id: impl Into<String>,
        inner: impl crate::harness::model_gateway::ModelProvider + 'static,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            inner: std::sync::Arc::new(inner),
        }
    }

    pub fn from_arc(
        provider_id: impl Into<String>,
        inner: std::sync::Arc<dyn crate::harness::model_gateway::ModelProvider>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            inner,
        }
    }
}

#[async_trait::async_trait]
impl Provider for ProviderAdapter {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        let tier_name = request
            .metadata
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("cheap_executor")
            .to_string();
        let max_tokens = request
            .metadata
            .get("max_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(2048);
        let prompt = request.prompt.clone();
        let provider_id = request.provider_id.clone();
        let provider_id_err = provider_id.clone();
        let model = request.model.clone();
        let inner = self.inner.clone();

        let response = tokio::task::spawn_blocking(move || {
            let tier = crate::harness::model_gateway::ModelTier {
                name: tier_name,
                provider: provider_id.clone(),
                model_id: model.clone(),
                max_tokens,
                cost_per_1k_tokens: 0.001,
            };
            inner.invoke(&tier, &prompt, max_tokens)
        })
        .await
        .map_err(|_| ProviderError {
            schema_version: "provider_error.v1".to_string(),
            provider_id: provider_id_err.clone(),
            error_domain: "adapter_panic".to_string(),
            message: "spawn_blocking task panicked".to_string(),
            retryable: false,
        })?;

        Ok(ProviderResponse {
            schema_version: "provider_response.v1".to_string(),
            provider_id: provider_id_err,
            model: response.model_id,
            output: response.content,
            input_tokens: Some(response.token_usage),
            output_tokens: None,
            estimated_cost: None,
            provider_request_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_provider_returns_error() {
        let p = DisabledProvider::new("test");
        assert!(!p.is_enabled());
        let req = ProviderRequest::local_stub("test", "m", "hello");
        let result = p.invoke(&req).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.error_domain, "provider_disabled");
    }

    #[tokio::test]
    async fn provider_adapter_wraps_model_provider() {
        use crate::harness::model_gateway::StubModelProvider;

        let adapter = ProviderAdapter::new("adapted_stub", StubModelProvider::new());
        assert!(adapter.is_enabled());
        assert_eq!(adapter.provider_id(), "adapted_stub");

        let req = ProviderRequest {
            schema_version: "provider_request.v1".to_string(),
            provider_id: "adapted_stub".to_string(),
            model: "stub-planner".to_string(),
            prompt: "hello world".to_string(),
            metadata: serde_json::json!({"tier": "strong_planner", "max_tokens": 1024}),
        };
        let result = adapter.invoke(&req).await.unwrap();
        assert_eq!(result.provider_id, "adapted_stub");
        assert!(!result.output.is_empty());
        assert!(result.input_tokens.is_some());
    }

    #[tokio::test]
    async fn provider_adapter_deterministic() {
        use crate::harness::model_gateway::StubModelProvider;

        let adapter = ProviderAdapter::new("det", StubModelProvider::new());
        let req = ProviderRequest {
            schema_version: "provider_request.v1".to_string(),
            provider_id: "det".to_string(),
            model: "m".to_string(),
            prompt: "deterministic check".to_string(),
            metadata: serde_json::json!({"tier": "cheap_executor", "max_tokens": 512}),
        };
        let r1 = adapter.invoke(&req).await.unwrap();
        let r2 = adapter.invoke(&req).await.unwrap();
        assert_eq!(r1.output, r2.output);
        assert_eq!(r1.input_tokens, r2.input_tokens);
    }
}
