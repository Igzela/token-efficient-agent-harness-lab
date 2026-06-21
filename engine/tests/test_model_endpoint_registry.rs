use engine::feedback::{
    CredentialReference, EndpointHealth, EndpointPricing, ModelEndpointRegistry, ModelEndpointSpec,
    RegistryMutation, ENDPOINT_REGISTRY_SCHEMA_VERSION,
};

fn endpoint(endpoint_id: &str) -> ModelEndpointSpec {
    ModelEndpointSpec {
        schema_version: ENDPOINT_REGISTRY_SCHEMA_VERSION.to_string(),
        endpoint_id: endpoint_id.to_string(),
        provider_id: format!("provider-{endpoint_id}"),
        model_id: format!("model-{endpoint_id}"),
        enabled: true,
        capabilities: vec!["chat".to_string(), "tools".to_string()],
        context_window_tokens: 128_000,
        supports_tools: true,
        supports_parallel_tools: true,
        pricing: EndpointPricing {
            input_cost_per_1k_usd: 0.001,
            output_cost_per_1k_usd: 0.002,
            cache_read_cost_per_1k_usd: Some(0.0001),
            cache_write_cost_per_1k_usd: None,
        },
        health: EndpointHealth {
            status: "healthy".to_string(),
            score: 0.98,
            observed_at: Some("2026-06-21T00:00:00Z".to_string()),
        },
        credential_reference: Some(CredentialReference {
            backend: "env".to_string(),
            reference_id: format!(
                "ACP_{}_API_KEY",
                endpoint_id.to_uppercase().replace('-', "_")
            ),
        }),
    }
}

#[test]
fn registry_upserts_and_emits_deterministic_sorted_snapshot() {
    let mut first = ModelEndpointRegistry::new();
    assert_eq!(
        first.upsert(endpoint("beta")).unwrap(),
        RegistryMutation::Inserted
    );
    assert_eq!(
        first.upsert(endpoint("alpha")).unwrap(),
        RegistryMutation::Inserted
    );

    let mut second = ModelEndpointRegistry::new();
    let mut alpha = endpoint("alpha");
    alpha.capabilities = vec!["tools".to_string(), "chat".to_string(), "chat".to_string()];
    second.upsert(alpha).unwrap();
    second.upsert(endpoint("beta")).unwrap();

    let first_snapshot = first.snapshot();
    let second_snapshot = second.snapshot();
    assert_eq!(
        first_snapshot
            .endpoints
            .iter()
            .map(|endpoint| endpoint.endpoint_id.as_str())
            .collect::<Vec<_>>(),
        vec!["alpha", "beta"]
    );
    assert_eq!(first_snapshot, second_snapshot);
    assert_eq!(
        first_snapshot.schema_version,
        ENDPOINT_REGISTRY_SCHEMA_VERSION
    );
    assert!(first_snapshot.shadow_only);
    assert!(!first_snapshot.live_execution_allowed);
}

#[test]
fn upsert_and_disable_are_idempotent() {
    let mut registry = ModelEndpointRegistry::new();
    let original = endpoint("alpha");
    registry.upsert(original.clone()).unwrap();
    assert_eq!(
        registry.upsert(original.clone()).unwrap(),
        RegistryMutation::Unchanged
    );

    let mut updated = original;
    updated.health.status = "degraded".to_string();
    updated.health.score = 0.6;
    assert_eq!(registry.upsert(updated).unwrap(), RegistryMutation::Updated);
    assert_eq!(
        registry.disable("alpha").unwrap(),
        RegistryMutation::Updated
    );
    assert_eq!(
        registry.disable("alpha").unwrap(),
        RegistryMutation::Unchanged
    );
    assert!(!registry.get("alpha").unwrap().enabled);
}

#[test]
fn registry_rejects_secret_values_without_storing_them() {
    let mut registry = ModelEndpointRegistry::new();
    let mut unsafe_endpoint = endpoint("unsafe");
    unsafe_endpoint.credential_reference = Some(CredentialReference {
        backend: "env".to_string(),
        reference_id: "sk-abcdefghijklmnopqrstuvwxyz".to_string(),
    });

    let error = registry.upsert(unsafe_endpoint).unwrap_err();

    assert!(error
        .violations
        .iter()
        .any(|violation| violation == "sensitive_pattern_detected"));
    assert!(registry.snapshot().endpoints.is_empty());
}

#[test]
fn secret_shaped_endpoint_ids_are_not_echoed_in_errors() {
    let mut registry = ModelEndpointRegistry::new();
    let secret_id = "sk-abcdefghijklmnopqrstuvwxyz";

    let upsert_error = registry.upsert(endpoint(secret_id)).unwrap_err();
    let disable_error = registry.disable(secret_id).unwrap_err();

    assert!(upsert_error.endpoint_id.is_none());
    assert!(disable_error.endpoint_id.is_none());
    assert!(!upsert_error.to_string().contains(secret_id));
    assert!(!disable_error.to_string().contains(secret_id));
}

#[test]
fn non_env_credential_references_must_be_symbolic_and_namespaced() {
    let mut registry = ModelEndpointRegistry::new();
    let mut unsafe_endpoint = endpoint("unsafe-reference");
    unsafe_endpoint.credential_reference = Some(CredentialReference {
        backend: "vault".to_string(),
        reference_id: "abcdefghijklmnopqrstuvwxyz".to_string(),
    });

    let error = registry.upsert(unsafe_endpoint).unwrap_err();

    assert!(error
        .violations
        .iter()
        .any(|violation| violation == "invalid_credential_reference"));

    let mut safe_endpoint = endpoint("safe-reference");
    safe_endpoint.credential_reference = Some(CredentialReference {
        backend: "vault".to_string(),
        reference_id: "vault:production-openai".to_string(),
    });
    assert_eq!(
        registry.upsert(safe_endpoint).unwrap(),
        RegistryMutation::Inserted
    );
}

#[test]
fn registry_validates_endpoint_metadata_before_upsert() {
    let mut registry = ModelEndpointRegistry::new();
    let mut invalid = endpoint("invalid");
    invalid.context_window_tokens = 0;
    invalid.pricing.input_cost_per_1k_usd = f64::NAN;
    invalid.health.status = "unknown-state".to_string();
    invalid.health.score = 1.2;
    invalid.capabilities.push(String::new());

    let error = registry.upsert(invalid).unwrap_err();

    for expected in [
        "invalid_context_window_tokens",
        "invalid_pricing",
        "invalid_health_status",
        "invalid_health_score",
        "invalid_capability",
    ] {
        assert!(error
            .violations
            .iter()
            .any(|violation| violation == expected));
    }
}

#[test]
fn credential_reference_is_metadata_not_execution_configuration() {
    let mut registry = ModelEndpointRegistry::new();
    registry.upsert(endpoint("alpha")).unwrap();

    let snapshot = registry.snapshot();
    let json = serde_json::to_value(&snapshot).unwrap();
    let serialized = json.to_string();

    assert_eq!(
        json["endpoints"][0]["credential_reference"]["backend"],
        "env"
    );
    assert_eq!(
        json["endpoints"][0]["credential_reference"]["reference_id"],
        "ACP_ALPHA_API_KEY"
    );
    assert!(!serialized.contains("base_url"));
    assert!(!serialized.contains("credential_value"));
    assert!(!serialized.contains("api_key="));
    assert!(!snapshot.live_execution_allowed);
}

#[test]
fn snapshot_hash_changes_only_when_registry_content_changes() {
    let mut registry = ModelEndpointRegistry::new();
    registry.upsert(endpoint("alpha")).unwrap();
    let before = registry.snapshot();

    registry.upsert(endpoint("alpha")).unwrap();
    assert_eq!(registry.snapshot().snapshot_hash, before.snapshot_hash);

    registry.disable("alpha").unwrap();
    assert_ne!(registry.snapshot().snapshot_hash, before.snapshot_hash);
}

#[test]
fn disable_unknown_endpoint_returns_not_found_without_mutation() {
    let mut registry = ModelEndpointRegistry::new();

    let error = registry.disable("missing").unwrap_err();

    assert_eq!(error.code, "endpoint_not_found");
    assert!(registry.snapshot().endpoints.is_empty());
}

#[test]
fn registry_rejects_new_endpoints_after_bounded_capacity() {
    let mut registry = ModelEndpointRegistry::new();
    for index in 0..256 {
        registry
            .upsert(endpoint(&format!("endpoint-{index:03}")))
            .unwrap();
    }

    let error = registry.upsert(endpoint("endpoint-overflow")).unwrap_err();

    assert_eq!(error.code, "registry_capacity_exceeded");
    assert_eq!(registry.snapshot().endpoints.len(), 256);
}
