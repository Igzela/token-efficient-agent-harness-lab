use sha2::{Digest, Sha256};

use super::{Provider, ProviderRequest, ProviderResponse, ProviderResult};

pub struct StubProvider {
    provider_id: String,
    default_model: String,
}

impl StubProvider {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            default_model: "stub-model".to_string(),
        }
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }
}

#[async_trait::async_trait]
impl Provider for StubProvider {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn default_model(&self) -> Option<&str> {
        Some(&self.default_model)
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        let mut hasher = Sha256::new();
        hasher.update(request.prompt.as_bytes());
        let hash = hex::encode(hasher.finalize());
        let short_hash = &hash[..16];

        let output = format!(
            "[stub:{}] Response for hash {}",
            self.provider_id, short_hash
        );

        let input_tokens = std::cmp::max(1, request.prompt.len() as i64 / 4);
        let output_tokens = std::cmp::max(1, output.len() as i64 / 4);
        let cost = (input_tokens + output_tokens) as f64 / 1000.0 * 0.002;

        Ok(ProviderResponse {
            schema_version: "provider_response.v1".to_string(),
            provider_id: self.provider_id.clone(),
            model: request.model.clone(),
            output,
            input_tokens: Some(input_tokens),
            output_tokens: Some(output_tokens),
            estimated_cost: Some(cost),
            provider_request_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_deterministic_output() {
        let stub = StubProvider::new("stub-1");
        let request = ProviderRequest::local_stub("stub-1", "stub-model", "hello world");
        let r1 = stub.invoke(&request).await.unwrap();
        let r2 = stub.invoke(&request).await.unwrap();
        assert_eq!(r1.output, r2.output);
        assert!(r1.output.starts_with("[stub:stub-1] Response for hash "));
    }

    #[tokio::test]
    async fn stub_different_prompts_different_output() {
        let stub = StubProvider::new("stub-1");
        let r1 = stub
            .invoke(&ProviderRequest::local_stub("stub-1", "m", "prompt A"))
            .await
            .unwrap();
        let r2 = stub
            .invoke(&ProviderRequest::local_stub("stub-1", "m", "prompt B"))
            .await
            .unwrap();
        assert_ne!(r1.output, r2.output);
    }

    #[tokio::test]
    async fn stub_token_counts() {
        let stub = StubProvider::new("stub-1");
        let request = ProviderRequest::local_stub("stub-1", "m", "hello");
        let result = stub.invoke(&request).await.unwrap();
        let input = result.input_tokens.unwrap();
        let output = result.output_tokens.unwrap();
        assert!(input >= 1);
        assert!(output >= 1);
    }

    #[tokio::test]
    async fn stub_cost_positive() {
        let stub = StubProvider::new("stub-1");
        let request = ProviderRequest::local_stub("stub-1", "m", "hello");
        let result = stub.invoke(&request).await.unwrap();
        let cost = result.estimated_cost.unwrap();
        assert!(cost > 0.0);
    }

    #[tokio::test]
    async fn stub_is_enabled() {
        let stub = StubProvider::new("stub-1");
        assert!(stub.is_enabled());
        assert_eq!(stub.provider_id(), "stub-1");
    }

    #[tokio::test]
    async fn stub_cost_formula() {
        let stub = StubProvider::new("stub-1");
        let request = ProviderRequest::local_stub("stub-1", "m", "hello");
        let result = stub.invoke(&request).await.unwrap();
        let input_tokens = result.input_tokens.unwrap();
        let output_tokens = result.output_tokens.unwrap();
        let expected_cost = (input_tokens + output_tokens) as f64 / 1000.0 * 0.002;
        let actual = result.estimated_cost.unwrap();
        assert!((actual - expected_cost).abs() < 0.000001);
    }

    #[tokio::test]
    async fn stub_hash_16_hex_chars() {
        let stub = StubProvider::new("stub-1");
        let request = ProviderRequest::local_stub("stub-1", "m", "test");
        let result = stub.invoke(&request).await.unwrap();
        let output = &result.output;
        let hash_start = output.find("hash ").unwrap() + 5;
        let hash_part = &output[hash_start..];
        assert_eq!(hash_part.len(), 16);
        assert!(hash_part.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
