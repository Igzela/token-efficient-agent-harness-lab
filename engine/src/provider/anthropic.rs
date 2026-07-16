use std::sync::Arc;

use serde_json::json;

use super::audit::ProviderAuditRecorder;
use super::config::CredentialRef;
use super::config::ProviderConfig;
use super::credential::CredentialBoundary;
use super::transport::{HttpError, HttpRequest, HttpTransport};
use super::{Provider, ProviderError, ProviderRequest, ProviderResponse, ProviderResult};

pub struct AnthropicProvider {
    config: ProviderConfig,
    boundary: CredentialBoundary,
    cred_ref: CredentialRef,
    transport: Arc<dyn HttpTransport>,
    audit: Option<Arc<ProviderAuditRecorder>>,
}

impl AnthropicProvider {
    pub fn new(
        config: ProviderConfig,
        boundary: CredentialBoundary,
        cred_ref: CredentialRef,
        transport: Arc<dyn HttpTransport>,
        audit: Option<Arc<ProviderAuditRecorder>>,
    ) -> Self {
        Self {
            config,
            boundary,
            cred_ref,
            transport,
            audit,
        }
    }

    fn compute_cost(&self, input_tokens: i64, output_tokens: i64) -> Option<f64> {
        let input_rate = self.config.input_cost_per_1k?;
        let output_rate = self.config.output_cost_per_1k?;
        let cost = (input_tokens as f64 / 1000.0) * input_rate
            + (output_tokens as f64 / 1000.0) * output_rate;
        Some(cost)
    }

    fn map_http_error(&self, err: HttpError) -> ProviderError {
        let (domain, message, retryable) = match &err {
            HttpError::PreSend(msg) => ("provider_pre_send", msg.clone(), true),
            HttpError::Http { status, reason } => match *status {
                401 | 403 => ("provider_auth", reason.clone(), false),
                429 => ("provider_rate_limit", reason.clone(), true),
                500..=599 => ("provider_capacity", reason.clone(), true),
                _ => ("provider_error", format!("HTTP {status}: {reason}"), false),
            },
            HttpError::Timeout(msg) => ("provider_timeout", msg.clone(), true),
            HttpError::Connection(msg) => ("provider_error", msg.clone(), true),
            HttpError::Parse(msg) => ("provider_error", msg.clone(), false),
        };
        ProviderError {
            schema_version: "provider_error.v1".to_string(),
            provider_id: self.config.provider_id.clone(),
            error_domain: domain.to_string(),
            message,
            retryable,
        }
    }
}

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    fn default_model(&self) -> Option<&str> {
        Some(&self.config.model_id)
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        if !self.config.enabled {
            return Err(ProviderError {
                schema_version: "provider_error.v1".to_string(),
                provider_id: self.config.provider_id.clone(),
                error_domain: "provider_disabled".to_string(),
                message: "provider is disabled".to_string(),
                retryable: false,
            });
        }

        let api_key = self
            .boundary
            .resolve(&self.cred_ref.credential_ref_id)
            .map_err(|e| ProviderError {
                schema_version: "provider_error.v1".to_string(),
                provider_id: self.config.provider_id.clone(),
                error_domain: "provider_auth".to_string(),
                message: e,
                retryable: false,
            })?;

        if let Some(audit) = &self.audit {
            let extra = json!({"model": request.model});
            audit.create_and_record(
                request
                    .metadata
                    .get("dispatch_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                &self.config.provider_id,
                "request_sent",
                Some(&extra),
            );
        }

        let url = format!("{}/v1/messages", self.config.base_url.trim_end_matches('/'));
        let max_tokens = request
            .metadata
            .get("max_tokens")
            .and_then(|value| value.as_u64())
            .filter(|value| (1..=1_000_000).contains(value))
            .unwrap_or(1024);
        let body = json!({
            "model": request.model,
            "max_tokens": max_tokens,
            "messages": [{"role": "user", "content": request.prompt}],
        });

        let http_request = HttpRequest {
            url,
            method: "POST".to_string(),
            headers: vec![
                ("x-api-key".to_string(), api_key),
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ],
            body: Some(body.to_string().into_bytes()),
            timeout_secs: Some(self.config.timeout_ms as f64 / 1000.0),
        };

        let http_response = self
            .transport
            .send(&http_request)
            .await
            .map_err(|e| self.map_http_error(e))?;

        let response_json: serde_json::Value = serde_json::from_slice(&http_response.body)
            .map_err(|e| ProviderError {
                schema_version: "provider_error.v1".to_string(),
                provider_id: self.config.provider_id.clone(),
                error_domain: "provider_error".to_string(),
                message: format!("failed to parse response: {e}"),
                retryable: false,
            })?;

        let output = response_json["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"].as_str() == Some("text"))
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        let input_tokens = response_json["usage"]["input_tokens"].as_i64();
        let output_tokens = response_json["usage"]["output_tokens"].as_i64();
        let _finish_reason = response_json["stop_reason"].as_str().map(String::from);

        let estimated_cost = match (input_tokens, output_tokens) {
            (Some(inp), Some(out)) => self.compute_cost(inp, out),
            _ => None,
        };

        let provider_request_id = response_json["id"].as_str().map(String::from);

        if let Some(audit) = &self.audit {
            let extra = json!({"input_tokens": input_tokens, "output_tokens": output_tokens});
            audit.create_and_record(
                request
                    .metadata
                    .get("dispatch_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                &self.config.provider_id,
                "response_received",
                Some(&extra),
            );
        }

        Ok(ProviderResponse {
            schema_version: "provider_response.v1".to_string(),
            provider_id: self.config.provider_id.clone(),
            model: request.model.clone(),
            output,
            input_tokens,
            output_tokens,
            estimated_cost,
            provider_request_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::transport::{HttpResponse, MockTransport};

    fn make_config() -> ProviderConfig {
        ProviderConfig {
            schema_version: "provider_config.v1".to_string(),
            provider_id: "anthropic-test".to_string(),
            provider_type: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            model_id: "claude-3".to_string(),
            credential_ref: "TEST_ANTHROPIC_KEY".to_string(),
            timeout_ms: 30_000,
            max_retries: 3,
            rate_limit_policy_id: None,
            enabled: true,
            input_cost_per_1k: Some(0.015),
            output_cost_per_1k: Some(0.075),
            currency: "USD".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_cred_ref() -> CredentialRef {
        CredentialRef::new(
            "TEST_ANTHROPIC_KEY",
            "env",
            "TES***KEY",
            "provider:anthropic",
            "2026-01-01T00:00:00Z",
        )
    }

    fn make_response_json() -> serde_json::Value {
        json!({
            "id": "msg-123",
            "type": "message",
            "content": [
                {"type": "text", "text": "Hello from Claude"}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 12, "output_tokens": 8}
        })
    }

    #[tokio::test]
    async fn anthropic_invoke_success() {
        std::env::set_var("TEST_ANTHROPIC_KEY", "sk-ant-test123456789");
        let transport = MockTransport::new(vec![Ok(HttpResponse {
            status: 200,
            body: make_response_json().to_string().into_bytes(),
        })]);
        let config = make_config();
        let provider = AnthropicProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            make_cred_ref(),
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("anthropic-test", "claude-3", "Hello");
        let result = provider.invoke(&request).await.unwrap();

        assert_eq!(result.output, "Hello from Claude");
        assert_eq!(result.input_tokens, Some(12));
        assert_eq!(result.output_tokens, Some(8));
        assert_eq!(result.provider_request_id, Some("msg-123".to_string()));
        assert!(result.estimated_cost.is_some());
        std::env::remove_var("TEST_ANTHROPIC_KEY");
    }

    #[tokio::test]
    async fn anthropic_invoke_disabled() {
        let mut config = make_config();
        config.enabled = false;
        let transport = MockTransport::empty();
        let provider = AnthropicProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            make_cred_ref(),
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("anthropic-test", "claude-3", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_disabled");
    }

    #[tokio::test]
    async fn anthropic_invoke_auth_error() {
        std::env::remove_var("TEST_ANTHROPIC_KEY_MISSING");
        let transport = MockTransport::empty();
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_ANTHROPIC_KEY_MISSING",
            "env",
            "TES***ING",
            "provider:anthropic",
            "2026-01-01T00:00:00Z",
        );
        let provider = AnthropicProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("anthropic-test", "claude-3", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_auth");
    }

    #[tokio::test]
    async fn anthropic_invoke_rate_limit() {
        std::env::set_var("TEST_ANTHROPIC_KEY_RL", "sk-ant-rate");
        let transport = MockTransport::new(vec![Err(HttpError::Http {
            status: 429,
            reason: "rate limited".to_string(),
        })]);
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_ANTHROPIC_KEY_RL",
            "env",
            "TES***RL",
            "provider:anthropic",
            "2026-01-01T00:00:00Z",
        );
        let provider = AnthropicProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("anthropic-test", "claude-3", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_rate_limit");
        assert!(err.retryable);
        std::env::remove_var("TEST_ANTHROPIC_KEY_RL");
    }

    #[tokio::test]
    async fn anthropic_invoke_server_error() {
        std::env::set_var("TEST_ANTHROPIC_KEY_500", "sk-ant-serv");
        let transport = MockTransport::new(vec![Err(HttpError::Http {
            status: 500,
            reason: "internal error".to_string(),
        })]);
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_ANTHROPIC_KEY_500",
            "env",
            "TES***500",
            "provider:anthropic",
            "2026-01-01T00:00:00Z",
        );
        let provider = AnthropicProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("anthropic-test", "claude-3", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_capacity");
        assert!(err.retryable);
        std::env::remove_var("TEST_ANTHROPIC_KEY_500");
    }

    #[tokio::test]
    async fn anthropic_multiple_text_blocks() {
        std::env::set_var("TEST_ANTHROPIC_KEY_MULTI", "sk-ant-multi123456");
        let response = json!({
            "id": "msg-456",
            "content": [
                {"type": "text", "text": "Hello "},
                {"type": "tool_use", "id": "tool1", "name": "search", "input": {}},
                {"type": "text", "text": "world"}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 5, "output_tokens": 3}
        });
        let transport = MockTransport::new(vec![Ok(HttpResponse {
            status: 200,
            body: response.to_string().into_bytes(),
        })]);
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_ANTHROPIC_KEY_MULTI",
            "env",
            "TES***ULT",
            "provider:anthropic",
            "2026-01-01T00:00:00Z",
        );
        let provider = AnthropicProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("anthropic-test", "claude-3", "Hello");
        let result = provider.invoke(&request).await.unwrap();
        assert_eq!(result.output, "Hello world");
        std::env::remove_var("TEST_ANTHROPIC_KEY_MULTI");
    }

    #[tokio::test]
    async fn anthropic_invoke_with_audit() {
        std::env::set_var("TEST_ANTHROPIC_KEY_AUDIT", "sk-ant-audit123456");
        let transport = MockTransport::new(vec![Ok(HttpResponse {
            status: 200,
            body: make_response_json().to_string().into_bytes(),
        })]);
        let config = make_config();
        let audit = Arc::new(ProviderAuditRecorder::new());
        let cred_ref = CredentialRef::new(
            "TEST_ANTHROPIC_KEY_AUDIT",
            "env",
            "TES***DIT",
            "provider:anthropic",
            "2026-01-01T00:00:00Z",
        );
        let provider = AnthropicProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            Some(audit.clone()),
        );

        let mut request = ProviderRequest::local_stub("anthropic-test", "claude-3", "Hello");
        request.metadata = json!({"dispatch_id": "disp-0002"});
        let _ = provider.invoke(&request).await.unwrap();

        let events = audit.list_all();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "request_sent");
        assert_eq!(events[1].event_type, "response_received");
        std::env::remove_var("TEST_ANTHROPIC_KEY_AUDIT");
    }
}
