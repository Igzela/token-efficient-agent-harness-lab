use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PROVIDER_CONFIG_SCHEMA_VERSION: &str = "provider_config.v1";
pub const CREDENTIAL_REF_SCHEMA_VERSION: &str = "credential_ref.v1";
pub const RETRY_POLICY_SCHEMA_VERSION: &str = "retry_policy.v1";
pub const ACP_PROVIDER_INPUT_COST_PER_1K_USD: &str = "ACP_PROVIDER_INPUT_COST_PER_1K_USD";
pub const ACP_PROVIDER_OUTPUT_COST_PER_1K_USD: &str = "ACP_PROVIDER_OUTPUT_COST_PER_1K_USD";

pub const PROVIDER_TYPES: &[&str] = &["openai_compatible", "anthropic", "local"];
pub const CREDENTIAL_STORAGE_BACKENDS: &[&str] = &["env", "file", "keyring", "vault"];
pub const BACKOFF_STRATEGIES: &[&str] = &["linear", "exponential", "none"];
pub const PROVIDER_AUDIT_EVENT_TYPES: &[&str] = &[
    "request_sent",
    "response_received",
    "error",
    "timeout",
    "retry",
    "fallback",
];
pub const REDACTION_STATUSES: &[&str] = &["redacted", "not_applicable"];

fn default_timeout_ms() -> i64 {
    30_000
}
fn default_max_retries() -> i64 {
    3
}
fn default_true() -> bool {
    true
}
fn default_currency() -> String {
    "USD".to_string()
}
fn default_backoff() -> String {
    "exponential".to_string()
}
fn default_base_delay_ms() -> i64 {
    1000
}
fn default_max_delay_ms() -> i64 {
    30_000
}
fn default_retryable_domains() -> Vec<String> {
    vec![
        "provider_rate_limit".to_string(),
        "provider_timeout".to_string(),
        "provider_capacity".to_string(),
    ]
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProviderConfig {
    pub schema_version: String,
    pub provider_id: String,
    pub provider_type: String,
    pub base_url: String,
    pub model_id: String,
    pub credential_ref: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: i64,
    #[serde(default = "default_max_retries")]
    pub max_retries: i64,
    pub rate_limit_policy_id: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
    #[serde(default = "default_currency")]
    pub currency: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderPricingConfig {
    pub input_cost_per_1k: Option<f64>,
    pub output_cost_per_1k: Option<f64>,
}

impl ProviderPricingConfig {
    pub fn configured(&self) -> bool {
        self.input_cost_per_1k.is_some() && self.output_cost_per_1k.is_some()
    }
}

pub fn parse_provider_pricing(
    input_cost_per_1k: Option<&str>,
    output_cost_per_1k: Option<&str>,
) -> ProviderPricingConfig {
    ProviderPricingConfig {
        input_cost_per_1k: parse_non_negative_finite(input_cost_per_1k),
        output_cost_per_1k: parse_non_negative_finite(output_cost_per_1k),
    }
}

pub fn provider_pricing_from_env() -> ProviderPricingConfig {
    let input = std::env::var(ACP_PROVIDER_INPUT_COST_PER_1K_USD).ok();
    let output = std::env::var(ACP_PROVIDER_OUTPUT_COST_PER_1K_USD).ok();
    parse_provider_pricing(input.as_deref(), output.as_deref())
}

fn parse_non_negative_finite(value: Option<&str>) -> Option<f64> {
    let parsed = value?.trim().parse::<f64>().ok()?;
    if parsed.is_finite() && parsed >= 0.0 {
        Some(parsed)
    } else {
        None
    }
}

impl ProviderConfig {
    pub fn new(
        provider_id: &str,
        provider_type: &str,
        base_url: &str,
        model_id: &str,
        credential_ref: &str,
        created_at: &str,
    ) -> Self {
        Self {
            schema_version: PROVIDER_CONFIG_SCHEMA_VERSION.to_string(),
            provider_id: provider_id.to_string(),
            provider_type: provider_type.to_string(),
            base_url: base_url.to_string(),
            model_id: model_id.to_string(),
            credential_ref: credential_ref.to_string(),
            timeout_ms: 30_000,
            max_retries: 3,
            rate_limit_policy_id: None,
            enabled: true,
            input_cost_per_1k: None,
            output_cost_per_1k: None,
            currency: "USD".to_string(),
            created_at: created_at.to_string(),
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({}))
    }

    pub fn apply_pricing(&mut self, pricing: &ProviderPricingConfig) {
        self.input_cost_per_1k = pricing.input_cost_per_1k;
        self.output_cost_per_1k = pricing.output_cost_per_1k;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CredentialRef {
    pub schema_version: String,
    pub credential_ref_id: String,
    pub storage_backend: String,
    pub redacted_display: String,
    pub scope: String,
    pub created_at: String,
}

impl CredentialRef {
    pub fn new(
        credential_ref_id: &str,
        storage_backend: &str,
        redacted_display: &str,
        scope: &str,
        created_at: &str,
    ) -> Self {
        Self {
            schema_version: CREDENTIAL_REF_SCHEMA_VERSION.to_string(),
            credential_ref_id: credential_ref_id.to_string(),
            storage_backend: storage_backend.to_string(),
            redacted_display: redacted_display.to_string(),
            scope: scope.to_string(),
            created_at: created_at.to_string(),
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({}))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetryPolicy {
    pub schema_version: String,
    pub policy_id: String,
    #[serde(default = "default_max_retries")]
    pub max_retries: i64,
    #[serde(default = "default_backoff")]
    pub backoff_strategy: String,
    #[serde(default = "default_base_delay_ms")]
    pub base_delay_ms: i64,
    #[serde(default = "default_max_delay_ms")]
    pub max_delay_ms: i64,
    #[serde(default = "default_retryable_domains")]
    pub retryable_error_domains: Vec<String>,
    #[serde(default = "default_true")]
    pub budget_check_per_retry: bool,
}

impl RetryPolicy {
    pub fn new(policy_id: &str) -> Self {
        Self {
            schema_version: RETRY_POLICY_SCHEMA_VERSION.to_string(),
            policy_id: policy_id.to_string(),
            max_retries: 3,
            backoff_strategy: "exponential".to_string(),
            base_delay_ms: 1000,
            max_delay_ms: 30_000,
            retryable_error_domains: default_retryable_domains(),
            budget_check_per_retry: true,
        }
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_config_defaults() {
        let c = ProviderConfig::new(
            "p1",
            "openai_compatible",
            "https://api.openai.com/v1",
            "gpt-4",
            "OPENAI_API_KEY",
            "2026-01-01T00:00:00Z",
        );
        assert_eq!(c.schema_version, "provider_config.v1");
        assert_eq!(c.timeout_ms, 30_000);
        assert_eq!(c.max_retries, 3);
        assert!(c.enabled);
        assert_eq!(c.currency, "USD");
        assert!(c.input_cost_per_1k.is_none());
    }

    #[test]
    fn parse_provider_pricing_accepts_non_negative_finite_rates() {
        let pricing = parse_provider_pricing(Some("0.015"), Some("0.075"));
        assert!(pricing.configured());
        assert_eq!(pricing.input_cost_per_1k, Some(0.015));
        assert_eq!(pricing.output_cost_per_1k, Some(0.075));
    }

    #[test]
    fn parse_provider_pricing_rejects_missing_invalid_or_negative_rates() {
        assert!(!parse_provider_pricing(None, Some("0.075")).configured());
        assert!(!parse_provider_pricing(Some("nan"), Some("0.075")).configured());
        assert!(!parse_provider_pricing(Some("-0.1"), Some("0.075")).configured());
    }

    #[test]
    fn provider_config_applies_pricing_rates() {
        let mut config = ProviderConfig::new(
            "p1",
            "anthropic",
            "https://api.anthropic.com",
            "claude-3",
            "ANTHROPIC_KEY",
            "2026-01-01T00:00:00Z",
        );
        let pricing = parse_provider_pricing(Some("0.001"), Some("0.002"));
        config.apply_pricing(&pricing);
        assert_eq!(config.input_cost_per_1k, Some(0.001));
        assert_eq!(config.output_cost_per_1k, Some(0.002));
    }

    #[test]
    fn provider_config_roundtrip() {
        let c = ProviderConfig::new(
            "p1",
            "anthropic",
            "https://api.anthropic.com",
            "claude-3",
            "ANTHROPIC_KEY",
            "2026-01-01T00:00:00Z",
        );
        let v = c.to_value();
        let c2: ProviderConfig = serde_json::from_value(v).unwrap();
        assert_eq!(c, c2);
    }

    #[test]
    fn credential_ref_roundtrip() {
        let r = CredentialRef::new(
            "OPENAI_API_KEY",
            "env",
            "OPE***KEY",
            "provider:openai",
            "2026-01-01T00:00:00Z",
        );
        let v = r.to_value();
        let r2: CredentialRef = serde_json::from_value(v).unwrap();
        assert_eq!(r, r2);
    }

    #[test]
    fn retry_policy_defaults() {
        let p = RetryPolicy::new("rp1");
        assert_eq!(p.max_retries, 3);
        assert_eq!(p.backoff_strategy, "exponential");
        assert_eq!(p.base_delay_ms, 1000);
        assert!(p.budget_check_per_retry);
        assert_eq!(p.retryable_error_domains.len(), 3);
    }

    #[test]
    fn constants_match_python() {
        assert!(PROVIDER_TYPES.contains(&"openai_compatible"));
        assert!(PROVIDER_TYPES.contains(&"anthropic"));
        assert!(PROVIDER_TYPES.contains(&"local"));
        assert!(BACKOFF_STRATEGIES.contains(&"exponential"));
        assert!(PROVIDER_AUDIT_EVENT_TYPES.contains(&"request_sent"));
    }
}
