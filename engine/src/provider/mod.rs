pub mod anthropic;
pub mod audit;
pub mod config;
pub mod cost_gate;
pub mod credential;
pub mod executor;
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
}
