use crate::provider::stub::StubProvider;
use crate::provider::{Provider, ProviderRequest, ProviderResponse, ProviderResult};

pub struct FakeProvider {
    inner: StubProvider,
}

impl FakeProvider {
    pub fn new(provider_id: impl Into<String>) -> Self {
        Self {
            inner: StubProvider::new(provider_id),
        }
    }
}

#[async_trait::async_trait]
impl Provider for FakeProvider {
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }

    fn is_enabled(&self) -> bool {
        true
    }

    fn default_model(&self) -> Option<&str> {
        self.inner.default_model()
    }

    async fn invoke(&self, request: &ProviderRequest) -> ProviderResult {
        let stub_result = self.inner.invoke(request).await?;
        Ok(ProviderResponse {
            schema_version: stub_result.schema_version,
            provider_id: stub_result.provider_id,
            model: stub_result.model,
            output: stub_result.output,
            input_tokens: stub_result.input_tokens,
            output_tokens: stub_result.output_tokens,
            estimated_cost: Some(0.0),
            provider_request_id: stub_result.provider_request_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_provider_returns_deterministic_output() {
        let fake = FakeProvider::new("fake-1");
        let request = ProviderRequest::local_stub("fake-1", "m", "test prompt");
        let r1 = fake.invoke(&request).await.unwrap();
        let r2 = fake.invoke(&request).await.unwrap();
        assert_eq!(r1.output, r2.output);
        assert!(r1.output.starts_with("[stub:fake-1] Response for hash "));
    }

    #[tokio::test]
    async fn fake_provider_cost_is_zero() {
        let fake = FakeProvider::new("fake-1");
        let request = ProviderRequest::local_stub("fake-1", "m", "hello");
        let result = fake.invoke(&request).await.unwrap();
        assert_eq!(result.estimated_cost, Some(0.0));
    }

    #[tokio::test]
    async fn fake_provider_is_enabled() {
        let fake = FakeProvider::new("fake-1");
        assert!(fake.is_enabled());
        assert_eq!(fake.provider_id(), "fake-1");
    }
}
