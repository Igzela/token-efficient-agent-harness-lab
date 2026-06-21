use std::collections::BTreeMap;
use std::fmt;

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::policy_snapshot::stable_hash;
use crate::provider::config::CREDENTIAL_STORAGE_BACKENDS;
use crate::provider::redaction::contains_sensitive_patterns;

pub const ENDPOINT_REGISTRY_SCHEMA_VERSION: &str = "model_endpoint_registry.v1";
const MAX_ENDPOINTS: usize = 256;
const MAX_CAPABILITIES: usize = 64;
const MAX_ID_BYTES: usize = 128;
const MAX_CONTEXT_WINDOW_TOKENS: u64 = 10_000_000;
const HEALTH_STATUSES: &[&str] = &["unknown", "healthy", "degraded", "unavailable"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialReference {
    pub backend: String,
    pub reference_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointPricing {
    pub input_cost_per_1k_usd: f64,
    pub output_cost_per_1k_usd: f64,
    pub cache_read_cost_per_1k_usd: Option<f64>,
    pub cache_write_cost_per_1k_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointHealth {
    pub status: String,
    pub score: f64,
    pub observed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEndpointSpec {
    pub schema_version: String,
    pub endpoint_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub enabled: bool,
    pub capabilities: Vec<String>,
    pub context_window_tokens: u64,
    pub supports_tools: bool,
    pub supports_parallel_tools: bool,
    pub pricing: EndpointPricing,
    pub health: EndpointHealth,
    pub credential_reference: Option<CredentialReference>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryMutation {
    Inserted,
    Updated,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEndpointRegistryError {
    pub code: String,
    pub endpoint_id: Option<String>,
    pub violations: Vec<String>,
}

impl ModelEndpointRegistryError {
    fn validation(endpoint_id: &str, mut violations: Vec<String>) -> Self {
        violations.sort();
        violations.dedup();
        Self {
            code: "endpoint_validation_failed".to_string(),
            endpoint_id: safe_error_endpoint_id(endpoint_id),
            violations,
        }
    }

    fn not_found(endpoint_id: &str) -> Self {
        Self {
            code: "endpoint_not_found".to_string(),
            endpoint_id: safe_error_endpoint_id(endpoint_id),
            violations: Vec::new(),
        }
    }

    fn capacity(endpoint_id: &str) -> Self {
        Self {
            code: "registry_capacity_exceeded".to_string(),
            endpoint_id: safe_error_endpoint_id(endpoint_id),
            violations: vec!["registry_capacity_exceeded".to_string()],
        }
    }
}

impl fmt::Display for ModelEndpointRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} for endpoint {:?}: {:?}",
            self.code, self.endpoint_id, self.violations
        )
    }
}

impl std::error::Error for ModelEndpointRegistryError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEndpointRegistrySnapshot {
    pub schema_version: String,
    pub endpoints: Vec<ModelEndpointSpec>,
    pub snapshot_hash: String,
    pub shadow_only: bool,
    pub live_execution_allowed: bool,
}

#[derive(Debug, Default)]
pub struct ModelEndpointRegistry {
    endpoints: BTreeMap<String, ModelEndpointSpec>,
}

impl ModelEndpointRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(
        &mut self,
        mut endpoint: ModelEndpointSpec,
    ) -> Result<RegistryMutation, ModelEndpointRegistryError> {
        normalize_endpoint(&mut endpoint);
        let violations = validate_endpoint(&endpoint);
        if !violations.is_empty() {
            return Err(ModelEndpointRegistryError::validation(
                &endpoint.endpoint_id,
                violations,
            ));
        }
        if !self.endpoints.contains_key(&endpoint.endpoint_id)
            && self.endpoints.len() >= MAX_ENDPOINTS
        {
            return Err(ModelEndpointRegistryError::capacity(&endpoint.endpoint_id));
        }

        let mutation = match self.endpoints.get(&endpoint.endpoint_id) {
            None => RegistryMutation::Inserted,
            Some(existing) if existing == &endpoint => RegistryMutation::Unchanged,
            Some(_) => RegistryMutation::Updated,
        };
        if mutation != RegistryMutation::Unchanged {
            self.endpoints
                .insert(endpoint.endpoint_id.clone(), endpoint);
        }
        Ok(mutation)
    }

    pub fn disable(
        &mut self,
        endpoint_id: &str,
    ) -> Result<RegistryMutation, ModelEndpointRegistryError> {
        let Some(endpoint) = self.endpoints.get_mut(endpoint_id) else {
            return Err(ModelEndpointRegistryError::not_found(endpoint_id));
        };
        if !endpoint.enabled {
            return Ok(RegistryMutation::Unchanged);
        }
        endpoint.enabled = false;
        Ok(RegistryMutation::Updated)
    }

    pub fn get(&self, endpoint_id: &str) -> Option<&ModelEndpointSpec> {
        self.endpoints.get(endpoint_id)
    }

    pub fn snapshot(&self) -> ModelEndpointRegistrySnapshot {
        let endpoints = self.endpoints.values().cloned().collect::<Vec<_>>();
        let hash_input = json!({
            "schema_version": ENDPOINT_REGISTRY_SCHEMA_VERSION,
            "endpoints": endpoints,
            "shadow_only": true,
            "live_execution_allowed": false,
        });
        ModelEndpointRegistrySnapshot {
            schema_version: ENDPOINT_REGISTRY_SCHEMA_VERSION.to_string(),
            endpoints,
            snapshot_hash: stable_hash(&hash_input),
            shadow_only: true,
            live_execution_allowed: false,
        }
    }
}

fn normalize_endpoint(endpoint: &mut ModelEndpointSpec) {
    endpoint.capabilities.sort();
    endpoint.capabilities.dedup();
}

fn validate_endpoint(endpoint: &ModelEndpointSpec) -> Vec<String> {
    let mut violations = Vec::new();
    if endpoint.schema_version != ENDPOINT_REGISTRY_SCHEMA_VERSION {
        violations.push("invalid_schema_version".to_string());
    }
    if !valid_endpoint_id(&endpoint.endpoint_id)
        || !valid_endpoint_id(&endpoint.provider_id)
        || !valid_endpoint_id(&endpoint.model_id)
    {
        violations.push("invalid_endpoint_identity".to_string());
    }
    if endpoint.capabilities.is_empty() || endpoint.capabilities.len() > MAX_CAPABILITIES {
        violations.push("invalid_capability_count".to_string());
    }
    if endpoint
        .capabilities
        .iter()
        .any(|capability| !valid_capability(capability))
    {
        violations.push("invalid_capability".to_string());
    }
    if endpoint.context_window_tokens == 0
        || endpoint.context_window_tokens > MAX_CONTEXT_WINDOW_TOKENS
    {
        violations.push("invalid_context_window_tokens".to_string());
    }
    if !valid_pricing(&endpoint.pricing) {
        violations.push("invalid_pricing".to_string());
    }
    if !HEALTH_STATUSES.contains(&endpoint.health.status.as_str()) {
        violations.push("invalid_health_status".to_string());
    }
    if !endpoint.health.score.is_finite() || !(0.0..=1.0).contains(&endpoint.health.score) {
        violations.push("invalid_health_score".to_string());
    }
    if endpoint
        .health
        .observed_at
        .as_deref()
        .is_some_and(|value| DateTime::parse_from_rfc3339(value).is_err())
    {
        violations.push("invalid_health_observed_at".to_string());
    }
    if let Some(reference) = &endpoint.credential_reference {
        if !CREDENTIAL_STORAGE_BACKENDS.contains(&reference.backend.as_str())
            || !valid_reference_id(&reference.backend, &reference.reference_id)
        {
            violations.push("invalid_credential_reference".to_string());
        }
    }
    if serde_json::to_string(endpoint)
        .ok()
        .is_some_and(|value| contains_sensitive_patterns(&value))
    {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
}

fn valid_endpoint_id(value: &str) -> bool {
    valid_bounded_ascii(value, |character| {
        character.is_ascii_alphanumeric() || "-_.:/@".contains(character)
    })
}

fn safe_error_endpoint_id(value: &str) -> Option<String> {
    (valid_endpoint_id(value) && !contains_sensitive_patterns(value)).then(|| value.to_string())
}

fn valid_capability(value: &str) -> bool {
    valid_bounded_ascii(value, |character| {
        character.is_ascii_alphanumeric() || "-_.:".contains(character)
    })
}

fn valid_reference_id(backend: &str, value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_ID_BYTES {
        return false;
    }
    if backend == "env" {
        let mut characters = value.chars();
        return characters
            .next()
            .is_some_and(|character| character.is_ascii_uppercase() || character == '_')
            && characters.all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            });
    }

    value
        .strip_prefix(&format!("{backend}:"))
        .is_some_and(|reference| {
            valid_bounded_ascii(reference, |character| {
                character.is_ascii_alphanumeric() || "-_.@".contains(character)
            })
        })
}

fn valid_bounded_ascii(value: &str, allowed: impl Fn(char) -> bool) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && value.chars().all(allowed)
}

fn valid_pricing(pricing: &EndpointPricing) -> bool {
    [
        Some(pricing.input_cost_per_1k_usd),
        Some(pricing.output_cost_per_1k_usd),
        pricing.cache_read_cost_per_1k_usd,
        pricing.cache_write_cost_per_1k_usd,
    ]
    .into_iter()
    .flatten()
    .all(|value| value.is_finite() && value >= 0.0)
}
