use engine::provider::{DisabledProvider, Provider, ProviderRequest};

#[test]
fn disabled_provider_is_off_by_default() {
    let provider = DisabledProvider::new("stub-provider");

    assert_eq!(provider.provider_id(), "stub-provider");
    assert!(!provider.is_enabled());
}

#[test]
fn disabled_provider_never_returns_transport_response() {
    let provider = DisabledProvider::new("stub-provider");
    let request = ProviderRequest::local_stub("stub-provider", "noop-model", "hello");

    let error = provider.invoke(&request).unwrap_err();

    assert_eq!(error.schema_version, "provider_error.v1");
    assert_eq!(error.provider_id, "stub-provider");
    assert_eq!(error.error_domain, "provider_disabled");
    assert!(!error.retryable);
}

#[test]
fn provider_request_is_deterministic_data_only() {
    let request_a = ProviderRequest::local_stub("stub-provider", "noop-model", "hello");
    let request_b = ProviderRequest::local_stub("stub-provider", "noop-model", "hello");

    assert_eq!(request_a, request_b);
    assert_eq!(request_a.schema_version, "provider_request.v1");
}
