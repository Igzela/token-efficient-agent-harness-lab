use std::sync::Arc;

use serde_json::json;

use super::audit::ProviderAuditRecorder;
use super::config::CredentialRef;
use super::config::ProviderConfig;
use super::credential::CredentialBoundary;
use super::transport::{HttpError, HttpRequest, HttpTransport};
use super::{Provider, ProviderError, ProviderRequest, ProviderResponse, ProviderResult};

pub struct OpenAiProvider {
    config: ProviderConfig,
    boundary: CredentialBoundary,
    cred_ref: CredentialRef,
    transport: Arc<dyn HttpTransport>,
    audit: Option<Arc<ProviderAuditRecorder>>,
}

impl OpenAiProvider {
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
impl Provider for OpenAiProvider {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn is_enabled(&self) -> bool {
        self.config.enabled
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

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = json!({
            "model": request.model,
            "messages": [{"role": "user", "content": request.prompt}],
            "max_tokens": 1024,
        });

        let http_request = HttpRequest {
            url,
            method: "POST".to_string(),
            headers: vec![
                ("Authorization".to_string(), format!("Bearer {api_key}")),
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

        let output = response_json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let input_tokens = response_json["usage"]["prompt_tokens"].as_i64();
        let output_tokens = response_json["usage"]["completion_tokens"].as_i64();

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
    use crate::provider::transport::HttpResponse;
    use crate::provider::transport::MockTransport;

    fn make_config() -> ProviderConfig {
        ProviderConfig {
            schema_version: "provider_config.v1".to_string(),
            provider_id: "openai-test".to_string(),
            provider_type: "openai_compatible".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            model_id: "gpt-4".to_string(),
            credential_ref: "TEST_OPENAI_KEY".to_string(),
            timeout_ms: 30_000,
            max_retries: 3,
            rate_limit_policy_id: None,
            enabled: true,
            input_cost_per_1k: Some(0.03),
            output_cost_per_1k: Some(0.06),
            currency: "USD".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_cred_ref() -> CredentialRef {
        CredentialRef::new(
            "TEST_OPENAI_KEY",
            "env",
            "TES***KEY",
            "provider:openai",
            "2026-01-01T00:00:00Z",
        )
    }

    fn make_response_json() -> serde_json::Value {
        json!({
            "id": "chatcmpl-123",
            "choices": [{"message": {"content": "Hello from OpenAI"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        })
    }

    #[tokio::test]
    async fn openai_invoke_success() {
        std::env::set_var("TEST_OPENAI_KEY", "sk-test123456789");
        let transport = MockTransport::new(vec![Ok(HttpResponse {
            status: 200,
            body: make_response_json().to_string().into_bytes(),
        })]);
        let config = make_config();
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            make_cred_ref(),
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        let result = provider.invoke(&request).await.unwrap();

        assert_eq!(result.output, "Hello from OpenAI");
        assert_eq!(result.input_tokens, Some(10));
        assert_eq!(result.output_tokens, Some(5));
        assert_eq!(result.provider_request_id, Some("chatcmpl-123".to_string()));
        assert!(result.estimated_cost.is_some());
        let cost = result.estimated_cost.unwrap();
        assert!((cost - 0.0006).abs() < 0.000001);
        std::env::remove_var("TEST_OPENAI_KEY");
    }

    #[tokio::test]
    async fn openai_invoke_disabled() {
        let mut config = make_config();
        config.enabled = false;
        let transport = MockTransport::empty();
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            make_cred_ref(),
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_disabled");
    }

    #[tokio::test]
    async fn openai_invoke_auth_error_missing_key() {
        std::env::remove_var("TEST_OPENAI_KEY_MISSING_VAR");
        let transport = MockTransport::empty();
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_OPENAI_KEY_MISSING_VAR",
            "env",
            "TES***ING",
            "provider:openai",
            "2026-01-01T00:00:00Z",
        );
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_auth");
    }

    #[tokio::test]
    async fn openai_invoke_http_401() {
        std::env::set_var("TEST_OPENAI_KEY_401", "sk-bad");
        let transport = MockTransport::new(vec![Err(HttpError::Http {
            status: 401,
            reason: "Unauthorized".to_string(),
        })]);
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_OPENAI_KEY_401",
            "env",
            "TES***401",
            "provider:openai",
            "2026-01-01T00:00:00Z",
        );
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_auth");
        assert!(!err.retryable);
        std::env::remove_var("TEST_OPENAI_KEY_401");
    }

    #[tokio::test]
    async fn openai_invoke_http_429() {
        std::env::set_var("TEST_OPENAI_KEY_429", "sk-rate");
        let transport = MockTransport::new(vec![Err(HttpError::Http {
            status: 429,
            reason: "Too Many Requests".to_string(),
        })]);
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_OPENAI_KEY_429",
            "env",
            "TES***429",
            "provider:openai",
            "2026-01-01T00:00:00Z",
        );
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_rate_limit");
        assert!(err.retryable);
        std::env::remove_var("TEST_OPENAI_KEY_429");
    }

    #[tokio::test]
    async fn openai_invoke_http_500() {
        std::env::set_var("TEST_OPENAI_KEY_500", "sk-serv");
        let transport = MockTransport::new(vec![Err(HttpError::Http {
            status: 500,
            reason: "Internal Server Error".to_string(),
        })]);
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_OPENAI_KEY_500",
            "env",
            "TES***500",
            "provider:openai",
            "2026-01-01T00:00:00Z",
        );
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_capacity");
        assert!(err.retryable);
        std::env::remove_var("TEST_OPENAI_KEY_500");
    }

    #[tokio::test]
    async fn openai_invoke_timeout() {
        std::env::set_var("TEST_OPENAI_KEY_TIMEOUT", "sk-time");
        let transport = MockTransport::new(vec![Err(HttpError::Timeout(
            "request timed out".to_string(),
        ))]);
        let config = make_config();
        let cred_ref = CredentialRef::new(
            "TEST_OPENAI_KEY_TIMEOUT",
            "env",
            "TES***OUT",
            "provider:openai",
            "2026-01-01T00:00:00Z",
        );
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        let err = provider.invoke(&request).await.unwrap_err();
        assert_eq!(err.error_domain, "provider_timeout");
        assert!(err.retryable);
        std::env::remove_var("TEST_OPENAI_KEY_TIMEOUT");
    }

    #[tokio::test]
    async fn openai_invoke_with_audit() {
        std::env::set_var("TEST_OPENAI_KEY_AUD2", "sk-audit123456");
        let transport = MockTransport::new(vec![Ok(HttpResponse {
            status: 200,
            body: make_response_json().to_string().into_bytes(),
        })]);
        let mut config = make_config();
        config.credential_ref = "TEST_OPENAI_KEY_AUD2".to_string();
        let audit = Arc::new(ProviderAuditRecorder::new());
        let cred_ref = CredentialRef::new(
            "TEST_OPENAI_KEY_AUD2",
            "env",
            "TES***UD2",
            "provider:openai",
            "2026-01-01T00:00:00Z",
        );
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            cred_ref,
            Arc::new(transport),
            Some(audit.clone()),
        );

        let mut request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        request.metadata = json!({"dispatch_id": "disp-0001"});
        let _ = provider.invoke(&request).await.unwrap();

        let events = audit.list_all();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "request_sent");
        assert_eq!(events[1].event_type, "response_received");
        assert_eq!(events[0].dispatch_id, "disp-0001");
        std::env::remove_var("TEST_OPENAI_KEY_AUD2");
    }

    #[tokio::test]
    async fn openai_invoke_no_cost_fields() {
        std::env::set_var("TEST_OPENAI_KEY", "sk-nocost1234");
        let transport = MockTransport::new(vec![Ok(HttpResponse {
            status: 200,
            body: make_response_json().to_string().into_bytes(),
        })]);
        let mut config = make_config();
        config.input_cost_per_1k = None;
        config.output_cost_per_1k = None;
        let provider = OpenAiProvider::new(
            config,
            CredentialBoundary::new("env").unwrap(),
            make_cred_ref(),
            Arc::new(transport),
            None,
        );

        let request = ProviderRequest::local_stub("openai-test", "gpt-4", "Hello");
        let result = provider.invoke(&request).await.unwrap();
        assert!(result.estimated_cost.is_none());
        std::env::remove_var("TEST_OPENAI_KEY");
    }
}
