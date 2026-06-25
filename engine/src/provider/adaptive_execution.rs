use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;

use super::anthropic::AnthropicProvider;
use super::audit::ProviderAuditRecorder;
use super::circuit_breaker_provider::CircuitBreakerProvider;
use super::config::{CredentialRef, ProviderConfig};
use super::cost_gate::{check_cost_gates, CostGateConfig};
use super::credential::CredentialBoundary;
use super::openai::OpenAiProvider;
use super::redaction::{contains_sensitive_patterns, redact_sensitive_patterns};
use super::stub::StubProvider;
use super::transport::ReqwestTransport;
use super::{Provider, ProviderRequest, ProviderResponse};
use crate::feedback::policy_snapshot::stable_hash;
use crate::feedback::{
    contextual_policy_key, AdaptiveExperimentController, AdaptiveExperimentGate,
    AdaptiveExperimentLimits, AdaptiveExperimentPolicy, AdaptiveExperimentRequest,
    AdaptiveExplorationGate, ContextualBanditEngine, ContextualBanditObservation,
    CredentialReference, EndpointHealth, EndpointPricing, ModelEndpointRegistry,
    ModelEndpointRegistrySnapshot, ModelEndpointSpec, PromotedAdaptivePolicy,
    ENDPOINT_REGISTRY_SCHEMA_VERSION,
};
use crate::infrastructure::circuit_breaker::{CircuitBreaker, CircuitBreakerRegistry};
use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
use crate::trusted_local::EffectiveExecutionGates;

use super::adaptive_observation::{
    AdaptiveNodeExecutionConfig, AdaptiveObservationContext, AdaptiveObservationDraft,
    AdaptivePolicyNodeExecutionConfig,
};

pub const ADAPTIVE_EXECUTION_SCHEMA_VERSION: &str = "adaptive_execution.v1";
pub const ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON: &str = "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON";
pub const ADAPTIVE_PROVIDER_ENDPOINTS_CONFIG_KEY: &str = "adaptive_provider_endpoints";
const MAX_EXECUTION_CALLS: usize = 8;
const MAX_ADAPTIVE_ENDPOINTS: usize = 8;
const MAX_EXECUTION_COST_USD: f64 = 1_000.0;
const MAX_EXECUTION_ELAPSED_MS: u64 = 300_000;
const MAX_EXECUTION_TOKENS: u64 = 1_000_000;
const DEFAULT_MAX_EXECUTION_TOKENS: u64 = 32_768;
const DEFAULT_OUTPUT_TOKEN_RESERVE: u64 = 1_024;
const MAX_PROMPT_BYTES: usize = 131_072;
const MAX_COMPOSED_PROMPT_BYTES: usize = 524_288;
const MAX_OUTPUT_BYTES: usize = 65_536;
const MAX_ENDPOINT_ID_BYTES: usize = 160;
const MAX_CONTEXTUAL_CANDIDATE_PLANS: usize = 8;
const MIN_FUSION_PANEL_SIZE: usize = 2;
const MAX_FUSION_PANEL_SIZE: usize = 3;
const MAX_FUSION_PANEL_CONCURRENCY: usize = 3;
const COST_EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveProviderEndpointConfig {
    pub endpoint_id: String,
    pub provider_type: String,
    #[serde(default)]
    pub base_url: Option<String>,
    pub model: String,
    #[serde(default)]
    pub credential_env: Option<String>,
    #[serde(default = "default_endpoint_timeout_ms")]
    pub timeout_ms: i64,
    #[serde(default)]
    pub input_cost_per_1k_usd: Option<f64>,
    #[serde(default)]
    pub output_cost_per_1k_usd: Option<f64>,
}

fn default_endpoint_timeout_ms() -> i64 {
    30_000
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveProviderEndpointConfigError {
    pub code: String,
    pub message: String,
}

impl AdaptiveProviderEndpointConfigError {
    fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

impl std::fmt::Display for AdaptiveProviderEndpointConfigError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AdaptiveProviderEndpointConfigError {}

pub fn parse_adaptive_provider_endpoints_json(
    raw: &str,
) -> Result<Vec<AdaptiveProviderEndpointConfig>, AdaptiveProviderEndpointConfigError> {
    if contains_sensitive_patterns(raw) {
        return Err(AdaptiveProviderEndpointConfigError::new(
            "sensitive_pattern_detected",
            "adaptive endpoint config must contain credential references, not secret values",
        ));
    }
    let mut configs =
        serde_json::from_str::<Vec<AdaptiveProviderEndpointConfig>>(raw).map_err(|_| {
            AdaptiveProviderEndpointConfigError::new(
                "invalid_endpoint_config_json",
                "adaptive endpoint config must be a JSON array",
            )
        })?;
    if configs.is_empty() {
        return Err(AdaptiveProviderEndpointConfigError::new(
            "endpoint_config_empty",
            "at least one adaptive endpoint is required",
        ));
    }
    if configs.len() > MAX_ADAPTIVE_ENDPOINTS {
        return Err(AdaptiveProviderEndpointConfigError::new(
            "endpoint_limit_exceeded",
            "adaptive endpoint count exceeds the AF-3 limit",
        ));
    }
    configs.sort_by(|left, right| left.endpoint_id.cmp(&right.endpoint_id));
    let mut seen = BTreeSet::new();
    for config in &configs {
        if !seen.insert(config.endpoint_id.as_str()) {
            return Err(AdaptiveProviderEndpointConfigError::new(
                "duplicate_endpoint_id",
                "adaptive endpoint IDs must be unique",
            ));
        }
        validate_adaptive_provider_endpoint_config(config)?;
    }
    Ok(configs)
}

pub fn adaptive_provider_endpoint_configs_from_sources(
    env_raw: Option<&str>,
    persisted_configs: Option<&[AdaptiveProviderEndpointConfig]>,
) -> Result<Option<Vec<AdaptiveProviderEndpointConfig>>, AdaptiveProviderEndpointConfigError> {
    if let Some(raw) = env_raw.filter(|raw| !raw.trim().is_empty()) {
        return parse_adaptive_provider_endpoints_json(raw).map(Some);
    }
    Ok(persisted_configs
        .filter(|configs| !configs.is_empty())
        .map(|configs| configs.to_vec()))
}

pub fn persisted_adaptive_provider_endpoint_configs(
    store: &crate::storage::local_product_store::LocalProductStore,
) -> Result<Option<Vec<AdaptiveProviderEndpointConfig>>, String> {
    let config = store.config_snapshot()?;
    let Some(value) = config.get(ADAPTIVE_PROVIDER_ENDPOINTS_CONFIG_KEY) else {
        return Ok(None);
    };
    let raw = serde_json::to_string(value)
        .map_err(|_| "adaptive endpoint config must be serializable".to_string())?;
    parse_adaptive_provider_endpoints_json(&raw)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub fn adaptive_registry_snapshot_from_configs(
    configs: &[AdaptiveProviderEndpointConfig],
) -> Result<ModelEndpointRegistrySnapshot, AdaptiveProviderEndpointConfigError> {
    let mut registry = ModelEndpointRegistry::new();
    for config in configs {
        validate_adaptive_provider_endpoint_config(config)?;
        registry
            .upsert(ModelEndpointSpec {
                schema_version: ENDPOINT_REGISTRY_SCHEMA_VERSION.to_string(),
                endpoint_id: config.endpoint_id.clone(),
                provider_id: config.provider_type.clone(),
                model_id: config.model.clone(),
                enabled: true,
                capabilities: vec!["completion".to_string()],
                context_window_tokens: MAX_EXECUTION_TOKENS,
                supports_tools: false,
                supports_parallel_tools: false,
                pricing: EndpointPricing {
                    input_cost_per_1k_usd: config.input_cost_per_1k_usd.unwrap_or(0.0),
                    output_cost_per_1k_usd: config.output_cost_per_1k_usd.unwrap_or(0.0),
                    cache_read_cost_per_1k_usd: None,
                    cache_write_cost_per_1k_usd: None,
                },
                health: EndpointHealth {
                    status: "healthy".to_string(),
                    score: 1.0,
                    observed_at: None,
                },
                credential_reference: config.credential_env.as_ref().map(|reference_id| {
                    CredentialReference {
                        backend: "env".to_string(),
                        reference_id: reference_id.clone(),
                    }
                }),
            })
            .map_err(|_| {
                AdaptiveProviderEndpointConfigError::new(
                    "adaptive_registry_invalid",
                    "adaptive endpoint config could not populate the model registry",
                )
            })?;
    }
    Ok(registry.snapshot())
}

pub fn adaptive_runtime_hash_from_configs(
    configs: &[AdaptiveProviderEndpointConfig],
) -> Result<String, AdaptiveProviderEndpointConfigError> {
    serde_json::to_value(configs)
        .map(|value| stable_hash(&value))
        .map_err(|_| {
            AdaptiveProviderEndpointConfigError::new(
                "invalid_endpoint_config_json",
                "adaptive endpoint config must be serializable",
            )
        })
}

pub fn build_adaptive_provider_runtime_from_configs(
    configs: &[AdaptiveProviderEndpointConfig],
    store: &std::sync::Arc<crate::storage::local_product_store::LocalProductStore>,
    cb_registry: &std::sync::Arc<CircuitBreakerRegistry>,
) -> Result<
    (
        std::sync::Arc<AdaptiveExecutionExecutor>,
        ModelEndpointRegistrySnapshot,
    ),
    AdaptiveProviderEndpointConfigError,
> {
    let registry_snapshot = adaptive_registry_snapshot_from_configs(configs)?;
    let providers = build_adaptive_providers(configs, cb_registry)?;
    let recorder = std::sync::Arc::new(ProviderAuditRecorder::with_store(store.clone()));
    Ok((
        std::sync::Arc::new(AdaptiveExecutionExecutor::new(
            providers,
            recorder,
            AdaptiveExecutionKillSwitch::new(),
        )),
        registry_snapshot,
    ))
}

fn build_adaptive_providers(
    configs: &[AdaptiveProviderEndpointConfig],
    cb_registry: &std::sync::Arc<CircuitBreakerRegistry>,
) -> Result<BTreeMap<String, Arc<dyn Provider>>, AdaptiveProviderEndpointConfigError> {
    let mut providers: BTreeMap<String, Arc<dyn Provider>> = BTreeMap::new();
    for endpoint in configs {
        let base_provider: Arc<dyn Provider> = match endpoint.provider_type.as_str() {
            "stub" => Arc::new(
                StubProvider::new(&endpoint.endpoint_id).with_default_model(&endpoint.model),
            ),
            "openai_compatible" | "anthropic" => {
                let credential_env = endpoint.credential_env.as_deref().ok_or_else(|| {
                    AdaptiveProviderEndpointConfigError::new(
                        "invalid_credential_env",
                        "validated adaptive credential reference is missing",
                    )
                })?;
                let boundary = CredentialBoundary::new("env").map_err(|_| {
                    AdaptiveProviderEndpointConfigError::new(
                        "invalid_credential_backend",
                        "adaptive endpoint credential backend is invalid",
                    )
                })?;
                if !boundary.validate(credential_env) {
                    return Err(AdaptiveProviderEndpointConfigError::new(
                        "credential_env_unavailable",
                        "adaptive endpoint credential environment variable is not set",
                    ));
                }
                let credential_ref = CredentialRef::new(
                    credential_env,
                    "env",
                    "***",
                    &format!("provider:{}", endpoint.endpoint_id),
                    "2026-01-01T00:00:00Z",
                );
                let mut config = ProviderConfig::new(
                    &endpoint.endpoint_id,
                    &endpoint.provider_type,
                    endpoint.base_url.as_deref().unwrap_or_default(),
                    &endpoint.model,
                    credential_env,
                    "2026-01-01T00:00:00Z",
                );
                config.timeout_ms = endpoint.timeout_ms;
                config.input_cost_per_1k = endpoint.input_cost_per_1k_usd;
                config.output_cost_per_1k = endpoint.output_cost_per_1k_usd;
                let transport = Arc::new(ReqwestTransport::new());
                if endpoint.provider_type == "openai_compatible" {
                    Arc::new(OpenAiProvider::new(
                        config,
                        boundary,
                        credential_ref,
                        transport,
                        None,
                    ))
                } else {
                    Arc::new(AnthropicProvider::new(
                        config,
                        boundary,
                        credential_ref,
                        transport,
                        None,
                    ))
                }
            }
            _ => {
                return Err(AdaptiveProviderEndpointConfigError::new(
                    "invalid_provider_type",
                    "adaptive endpoint provider type is unsupported",
                ))
            }
        };
        let circuit_breaker = Arc::new(CircuitBreaker::new(
            format!("provider:{}", endpoint.endpoint_id),
            std::env::var("ACP_CIRCUIT_BREAKER_THRESHOLD")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(5),
            std::env::var("ACP_CIRCUIT_BREAKER_RECOVERY_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(30_000),
        ));
        cb_registry.register(circuit_breaker.clone());
        providers.insert(
            endpoint.endpoint_id.clone(),
            Arc::new(CircuitBreakerProvider::new(base_provider, circuit_breaker)),
        );
    }
    Ok(providers)
}

pub fn validate_adaptive_provider_endpoint_config(
    config: &AdaptiveProviderEndpointConfig,
) -> Result<(), AdaptiveProviderEndpointConfigError> {
    if !valid_id(&config.endpoint_id) || !valid_id(&config.model) {
        return Err(AdaptiveProviderEndpointConfigError::new(
            "invalid_endpoint_identity",
            "adaptive endpoint identity is invalid",
        ));
    }
    if !(1_000..=300_000).contains(&config.timeout_ms) {
        return Err(AdaptiveProviderEndpointConfigError::new(
            "invalid_timeout_ms",
            "adaptive endpoint timeout is outside the allowed range",
        ));
    }
    let pricing_valid = match (config.input_cost_per_1k_usd, config.output_cost_per_1k_usd) {
        (None, None) => true,
        (Some(input), Some(output)) => {
            input.is_finite() && input >= 0.0 && output.is_finite() && output >= 0.0
        }
        _ => false,
    };
    if !pricing_valid {
        return Err(AdaptiveProviderEndpointConfigError::new(
            "invalid_pricing",
            "adaptive endpoint pricing must be absent or a complete non-negative pair",
        ));
    }

    match config.provider_type.as_str() {
        "stub" => Ok(()),
        "openai_compatible" | "anthropic" => {
            let credential_env = config.credential_env.as_deref().unwrap_or_default();
            if !valid_credential_env(credential_env) {
                return Err(AdaptiveProviderEndpointConfigError::new(
                    "invalid_credential_env",
                    "real adaptive endpoints require a symbolic credential environment name",
                ));
            }
            let base_url = config.base_url.as_deref().unwrap_or_default();
            if !valid_provider_base_url(base_url) {
                return Err(AdaptiveProviderEndpointConfigError::new(
                    "invalid_base_url",
                    "real adaptive endpoints require HTTPS or loopback HTTP without URL credentials",
                ));
            }
            Ok(())
        }
        _ => Err(AdaptiveProviderEndpointConfigError::new(
            "invalid_provider_type",
            "adaptive endpoint provider type is unsupported",
        )),
    }
}

fn valid_credential_env(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_uppercase() || character == '_')
        && characters.all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

fn valid_provider_base_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    match url.scheme() {
        "https" => true,
        "http" => url.host_str().is_some_and(is_loopback_host),
        _ => false,
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveEndpointInvocation {
    pub endpoint_id: String,
    pub model: String,
    pub reserved_cost_usd: f64,
}

impl AdaptiveEndpointInvocation {
    pub fn new(endpoint_id: &str, model: &str, reserved_cost_usd: f64) -> Self {
        Self {
            endpoint_id: endpoint_id.to_string(),
            model: model.to_string(),
            reserved_cost_usd,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum AdaptiveExecutionPlan {
    Single {
        endpoint: AdaptiveEndpointInvocation,
    },
    OrderedFallback {
        endpoints: Vec<AdaptiveEndpointInvocation>,
    },
    Fusion {
        panel: Vec<AdaptiveEndpointInvocation>,
        judge: AdaptiveEndpointInvocation,
        synthesizer: AdaptiveEndpointInvocation,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveExecutionLimits {
    pub max_calls: usize,
    pub max_cost_usd: f64,
    pub max_elapsed_ms: u64,
    pub max_concurrency: usize,
    #[serde(default = "default_max_execution_tokens")]
    pub max_total_tokens: u64,
    #[serde(default)]
    pub min_successful_panel_calls: usize,
}

fn default_max_execution_tokens() -> u64 {
    DEFAULT_MAX_EXECUTION_TOKENS
}

impl AdaptiveExecutionLimits {
    pub fn new(
        max_calls: usize,
        max_cost_usd: f64,
        max_elapsed_ms: u64,
        max_concurrency: usize,
    ) -> Self {
        Self {
            max_calls,
            max_cost_usd,
            max_elapsed_ms,
            max_concurrency,
            max_total_tokens: DEFAULT_MAX_EXECUTION_TOKENS,
            min_successful_panel_calls: 0,
        }
    }

    pub fn with_max_total_tokens(mut self, max_total_tokens: u64) -> Self {
        self.max_total_tokens = max_total_tokens;
        self
    }

    pub fn with_min_successful_panel_calls(mut self, min_successful_panel_calls: usize) -> Self {
        self.min_successful_panel_calls = min_successful_panel_calls;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveExecutionRequest {
    pub schema_version: String,
    pub dispatch_id: String,
    pub prompt: String,
    pub plan: AdaptiveExecutionPlan,
    pub limits: AdaptiveExecutionLimits,
}

impl AdaptiveExecutionRequest {
    pub fn new(
        dispatch_id: &str,
        prompt: &str,
        plan: AdaptiveExecutionPlan,
        limits: AdaptiveExecutionLimits,
    ) -> Self {
        Self {
            schema_version: ADAPTIVE_EXECUTION_SCHEMA_VERSION.to_string(),
            dispatch_id: dispatch_id.to_string(),
            prompt: prompt.to_string(),
            plan,
            limits,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveExecutionGate {
    provider_execution_enabled: bool,
    adaptive_execution_enabled: bool,
    auth_enabled: bool,
}

impl AdaptiveExecutionGate {
    pub fn from_env(auth_enabled: bool) -> Self {
        let gates = EffectiveExecutionGates::from_env();
        Self::from_flags(
            gates.provider_execution,
            gates.adaptive_execution,
            auth_enabled,
        )
    }

    pub fn from_flags(
        provider_execution_enabled: bool,
        adaptive_execution_enabled: bool,
        auth_enabled: bool,
    ) -> Self {
        Self {
            provider_execution_enabled,
            adaptive_execution_enabled,
            auth_enabled,
        }
    }

    pub fn is_enabled(self) -> bool {
        self.provider_execution_enabled && self.adaptive_execution_enabled && self.auth_enabled
    }
}

fn env_enabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[derive(Clone)]
pub struct AdaptiveExecutionKillSwitch {
    killed: Arc<AtomicBool>,
}

impl Default for AdaptiveExecutionKillSwitch {
    fn default() -> Self {
        Self::from_flags(false)
    }
}

impl AdaptiveExecutionKillSwitch {
    pub fn new() -> Self {
        Self::from_flags(env_enabled("ACP_ADAPTIVE_FUSION_KILL_SWITCH"))
    }

    pub fn from_flags(killed: bool) -> Self {
        Self {
            killed: Arc::new(AtomicBool::new(killed)),
        }
    }

    pub fn kill(&self) {
        self.killed.store(true, Ordering::SeqCst);
    }

    pub fn reset(&self) {
        self.killed.store(false, Ordering::SeqCst);
    }

    pub fn is_killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdaptiveCallRole {
    Single,
    Fallback,
    Panel,
    Judge,
    Synthesizer,
}

impl AdaptiveCallRole {
    fn event_name(self, suffix: &str) -> String {
        let role = match self {
            Self::Single => "single",
            Self::Fallback => "fallback",
            Self::Panel => "panel",
            Self::Judge => "judge",
            Self::Synthesizer => "synthesizer",
        };
        format!("adaptive_{role}_{suffix}")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveCallEvidence {
    pub endpoint_id: String,
    pub role: AdaptiveCallRole,
    pub status: String,
    pub reserved_cost_usd: f64,
    pub provider_cost_usd: Option<f64>,
    pub reserved_token_count: u64,
    pub input_token_count: Option<u64>,
    pub output_token_count: Option<u64>,
    pub latency_ms: u64,
    pub error_domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveExecutionResult {
    pub schema_version: String,
    pub dispatch_id: String,
    pub output: Option<String>,
    pub output_truncated: bool,
    pub selected_endpoint_id: Option<String>,
    pub calls: Vec<AdaptiveCallEvidence>,
    pub total_reserved_cost_usd: f64,
    pub total_provider_cost_usd: f64,
    pub total_reserved_token_count: u64,
    pub total_input_token_count: u64,
    pub total_output_token_count: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveExecutionError {
    pub schema_version: Box<str>,
    pub code: Box<str>,
    pub message: Box<str>,
    pub calls: Vec<AdaptiveCallEvidence>,
    pub total_reserved_cost_usd: f64,
    pub total_provider_cost_usd: f64,
    pub total_reserved_token_count: u64,
    pub total_input_token_count: u64,
    pub total_output_token_count: u64,
    pub elapsed_ms: u64,
}

impl std::fmt::Display for AdaptiveExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AdaptiveExecutionError {}

#[derive(Clone)]
pub struct AdaptiveExecutionExecutor {
    providers: BTreeMap<String, Arc<dyn Provider>>,
    audit: Arc<ProviderAuditRecorder>,
    kill_switch: AdaptiveExecutionKillSwitch,
}

pub struct AdaptiveProviderNodeExecutor {
    executor: Arc<AdaptiveExecutionExecutor>,
    gate: AdaptiveExecutionGate,
    cost_gate_config: CostGateConfig,
    daily_cost_usd: f64,
    contextual_policies: BTreeMap<String, PromotedAdaptivePolicy>,
    persisted_observations: Vec<ContextualBanditObservation>,
    exploration_gate: AdaptiveExplorationGate,
    experiment_policy: Option<AdaptiveExperimentPolicy>,
    experiment_gate: Option<AdaptiveExperimentGate>,
    last_observation: Arc<Mutex<Option<AdaptiveObservationDraft>>>,
}

impl AdaptiveProviderNodeExecutor {
    pub fn new(executor: Arc<AdaptiveExecutionExecutor>, gate: AdaptiveExecutionGate) -> Self {
        Self {
            executor,
            gate,
            cost_gate_config: CostGateConfig::new(None, None),
            daily_cost_usd: 0.0,
            contextual_policies: BTreeMap::new(),
            persisted_observations: Vec::new(),
            exploration_gate: AdaptiveExplorationGate::from_env(),
            experiment_policy: None,
            experiment_gate: None,
            last_observation: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_cost_gate(mut self, config: CostGateConfig, daily_cost_usd: f64) -> Self {
        self.cost_gate_config = config;
        self.daily_cost_usd = daily_cost_usd;
        self
    }

    pub fn with_persisted_observations(
        mut self,
        observations: Vec<ContextualBanditObservation>,
    ) -> Self {
        self.persisted_observations = observations;
        self
    }

    pub fn with_online_experiments(
        mut self,
        policy: AdaptiveExperimentPolicy,
        gate: AdaptiveExperimentGate,
    ) -> Self {
        self.experiment_policy = Some(policy);
        self.experiment_gate = Some(gate);
        self
    }

    pub fn with_contextual_policies(
        mut self,
        policies: Vec<PromotedAdaptivePolicy>,
        exploration_gate: AdaptiveExplorationGate,
    ) -> Self {
        self.contextual_policies = policies
            .into_iter()
            .filter(PromotedAdaptivePolicy::is_valid)
            .map(|policy| (policy.policy_key.clone(), policy))
            .collect();
        self.exploration_gate = exploration_gate;
        self
    }

    pub fn take_observation(&self) -> Option<AdaptiveObservationDraft> {
        self.last_observation
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    fn prompt(input: &NodeExecutionInput) -> String {
        input
            .node_metadata
            .get("prompt")
            .or_else(|| input.node_metadata.get("command"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string()
    }

    fn dispatch_ref(input: &NodeExecutionInput) -> String {
        format!("workflow:{}:{}", input.run_id, input.node_id)
    }
}

impl NodeExecutor for AdaptiveProviderNodeExecutor {
    fn executor_type_name(&self) -> &str {
        "adaptive_provider"
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let dispatch_ref = Self::dispatch_ref(input);
        let config = match self.resolve_node_config(input, &dispatch_ref) {
            Ok(config) => config,
            Err((code, message)) => {
                self.executor.audit_node_block(&dispatch_ref, code, None);
                return adaptive_node_error(code, message, None, None);
            }
        };
        let reserved_cost = plan_reserved_cost(&config.plan);
        if let Err(error) =
            check_cost_gates(&self.cost_gate_config, reserved_cost, self.daily_cost_usd)
        {
            self.executor.audit_node_block(
                &dispatch_ref,
                "adaptive_global_cost_gate_blocked",
                Some(reserved_cost),
            );
            return adaptive_node_error(
                "adaptive_global_cost_gate_blocked",
                &error.to_string(),
                Some(reserved_cost),
                None,
            );
        }
        let request = AdaptiveExecutionRequest::new(
            &dispatch_ref,
            &Self::prompt(input),
            config.plan.clone(),
            config.limits.clone(),
        );
        let executor = self.executor.clone();
        let gate = self.gate;
        let execution = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("adaptive provider runtime");
            runtime.block_on(executor.execute(&request, &gate))
        })
        .join();

        match execution {
            Ok(Ok(result)) => {
                self.capture_observation(
                    input,
                    &config,
                    true,
                    result.total_provider_cost_usd,
                    result.elapsed_ms,
                    result.total_input_token_count,
                    result.total_output_token_count,
                );
                NodeExecutionOutput {
                    status: "completed".to_string(),
                    executor_type: "adaptive_provider".to_string(),
                    output: result.output,
                    error_domain: None,
                    error_message: None,
                    input_tokens: i64::try_from(result.total_input_token_count).ok(),
                    output_tokens: i64::try_from(result.total_output_token_count).ok(),
                    estimated_cost: Some(result.total_provider_cost_usd),
                    latency_ms: Some(result.elapsed_ms as i64),
                }
            }
            Ok(Err(error)) => {
                let cost = error
                    .total_provider_cost_usd
                    .max(error.total_reserved_cost_usd);
                self.capture_observation(
                    input,
                    &config,
                    false,
                    cost,
                    error.elapsed_ms,
                    error.total_input_token_count,
                    error.total_output_token_count,
                );
                adaptive_node_error(
                    &error.code,
                    &error.message,
                    Some(cost),
                    Some(error.elapsed_ms),
                )
            }
            Err(_) => {
                self.executor.audit_node_failure(
                    &dispatch_ref,
                    "adaptive_runtime_failure",
                    None,
                    None,
                );
                adaptive_node_error(
                    "adaptive_runtime_failure",
                    "adaptive execution runtime thread failed",
                    None,
                    None,
                )
            }
        }
    }
}

impl AdaptiveProviderNodeExecutor {
    #[allow(clippy::too_many_arguments)]
    fn capture_observation(
        &self,
        input: &NodeExecutionInput,
        config: &AdaptiveNodeExecutionConfig,
        success: bool,
        cost_usd: f64,
        latency_ms: u64,
        input_tokens: u64,
        output_tokens: u64,
    ) {
        let Some(context) = &config.observation_context else {
            return;
        };
        let proxy_score = f64::from(success);
        let draft = AdaptiveObservationDraft {
            run_id: input.run_id.clone(),
            request_id: context.request_id.clone(),
            task_class: context.task_class.clone(),
            objective: context.objective,
            risk_level: context.risk_level.clone(),
            candidate_id: context.candidate_id.clone(),
            candidate_hash: stable_hash(&serde_json::json!({
                "plan": config.plan,
                "limits": config.limits,
            })),
            policy_hash: context.policy_hash.clone(),
            candidate_kind: plan_kind(&config.plan).to_string(),
            success,
            quality_score: proxy_score,
            quality_score_source: "execution_success_proxy".to_string(),
            tool_success_score: proxy_score,
            cost_usd,
            latency_ms,
            input_tokens,
            output_tokens,
        };
        *self
            .last_observation
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(draft);
    }

    fn resolve_node_config(
        &self,
        input: &NodeExecutionInput,
        dispatch_ref: &str,
    ) -> Result<AdaptiveNodeExecutionConfig, (&'static str, &'static str)> {
        if let Some(value) = input.node_metadata.get("adaptive_policy_execution") {
            return self.resolve_contextual_config(value, dispatch_ref);
        }
        let Some(config) = input.node_metadata.get("adaptive_execution") else {
            return Err((
                "adaptive_plan_missing",
                "adaptive execution plan is required",
            ));
        };
        serde_json::from_value::<AdaptiveNodeExecutionConfig>(config.clone()).map_err(|_| {
            (
                "adaptive_plan_invalid",
                "adaptive execution plan is invalid",
            )
        })
    }

    fn resolve_contextual_config(
        &self,
        value: &serde_json::Value,
        dispatch_ref: &str,
    ) -> Result<AdaptiveNodeExecutionConfig, (&'static str, &'static str)> {
        let config = serde_json::from_value::<AdaptivePolicyNodeExecutionConfig>(value.clone())
            .map_err(|_| {
                (
                    "adaptive_policy_plan_invalid",
                    "adaptive contextual execution config is invalid",
                )
            })?;
        if config.candidate_plans.is_empty()
            || config.candidate_plans.len() > MAX_CONTEXTUAL_CANDIDATE_PLANS
        {
            return Err((
                "adaptive_policy_plan_invalid",
                "adaptive contextual candidate plan count is invalid",
            ));
        }
        let policy_key =
            contextual_policy_key(&config.request.task_class, config.request.objective);
        let policy = self.contextual_policies.get(&policy_key).ok_or((
            "adaptive_policy_not_promoted",
            "no promoted adaptive policy matches the task context",
        ))?;
        if !config.candidate_plans.contains_key(&policy.candidate_id) {
            return Err((
                "adaptive_policy_plan_missing",
                "promoted candidate has no explicit bounded execution plan",
            ));
        }
        let mut observations = config.observations.clone();
        let explicit_ids = observations
            .iter()
            .map(|observation| observation.observation_id.clone())
            .collect::<BTreeSet<_>>();
        let explicit_run_candidates = observations
            .iter()
            .map(|observation| (observation.run_id.clone(), observation.candidate_id.clone()))
            .collect::<BTreeSet<_>>();
        observations.extend(
            self.persisted_observations
                .iter()
                .filter(|observation| {
                    observation.task_class == config.request.task_class
                        && observation.objective == config.request.objective
                        && config
                            .candidate_plans
                            .contains_key(&observation.candidate_id)
                        && !explicit_ids.contains(&observation.observation_id)
                        && !explicit_run_candidates.contains(&(
                            observation.run_id.clone(),
                            observation.candidate_id.clone(),
                        ))
                })
                .cloned(),
        );
        let experiment_decision =
            self.experiment_policy
                .zip(self.experiment_gate)
                .map(|(policy, gate)| {
                    AdaptiveExperimentController::decide(
                        &AdaptiveExperimentRequest {
                            request_id: config.request.request_id.clone(),
                            exploration_seed: config.request.exploration_seed,
                            risk_level: config.request.risk_level.clone(),
                        },
                        &policy,
                        &gate,
                    )
                    .map(|decision| (policy, decision))
                });
        let experiment_assigned = experiment_decision
            .as_ref()
            .and_then(|decision| decision.as_ref().ok())
            .is_some_and(|(_, decision)| decision.assigned);
        if experiment_decision.as_ref().is_some_and(Result::is_err) {
            self.executor.audit.create_and_record(
                dispatch_ref,
                &policy.candidate_id,
                "adaptive_experiment_blocked",
                Some(&serde_json::json!({
                    "error_domain": "adaptive_experiment_validation_failed",
                })),
            );
        }
        let exploration_gate = if self.experiment_policy.is_some() {
            AdaptiveExplorationGate::from_assignment(experiment_assigned)
        } else {
            self.exploration_gate
        };
        let decision = ContextualBanditEngine::decide(
            &config.request,
            &config.evaluation,
            &observations,
            &exploration_gate,
        )
        .map_err(|_| {
            (
                "adaptive_policy_decision_invalid",
                "adaptive contextual policy decision is invalid",
            )
        })?;
        let mut candidate_id = if decision.exploration_assigned {
            decision.selected_candidate_id.as_str()
        } else {
            policy.candidate_id.as_str()
        };
        let mut selected = config.candidate_plans.get(candidate_id).ok_or((
            "adaptive_policy_plan_missing",
            "selected adaptive candidate has no explicit bounded execution plan",
        ))?;
        if decision.exploration_assigned {
            if let Some(Ok((experiment_policy, experiment_decision))) = &experiment_decision {
                let limits = AdaptiveExperimentLimits {
                    reserved_cost_usd: plan_reserved_cost(&selected.plan),
                    max_cost_usd: selected.limits.max_cost_usd,
                    max_total_tokens: selected.limits.max_total_tokens,
                    max_calls: selected.limits.max_calls,
                    max_elapsed_ms: selected.limits.max_elapsed_ms,
                    max_concurrency: selected.limits.max_concurrency,
                };
                if let Err(error_domain) =
                    AdaptiveExperimentController::validate_limits(&limits, experiment_policy)
                {
                    self.executor.audit.create_and_record(
                        dispatch_ref,
                        candidate_id,
                        "adaptive_experiment_blocked",
                        Some(&serde_json::json!({"error_domain": error_domain})),
                    );
                    candidate_id = policy.candidate_id.as_str();
                    selected = config.candidate_plans.get(candidate_id).ok_or((
                        "adaptive_policy_plan_missing",
                        "promoted candidate has no explicit bounded execution plan",
                    ))?;
                } else {
                    self.executor.audit.create_and_record(
                        dispatch_ref,
                        candidate_id,
                        "adaptive_experiment_assigned",
                        Some(&serde_json::json!({
                            "traffic_rate": experiment_decision.traffic_rate,
                        })),
                    );
                }
            }
        }
        self.executor.audit.create_and_record(
            dispatch_ref,
            candidate_id,
            "adaptive_policy_selected",
            None,
        );
        Ok(AdaptiveNodeExecutionConfig {
            plan: selected.plan.clone(),
            limits: selected.limits.clone(),
            observation_context: Some(AdaptiveObservationContext {
                request_id: config.request.request_id,
                task_class: config.request.task_class,
                objective: config.request.objective,
                risk_level: config.request.risk_level,
                candidate_id: candidate_id.to_string(),
                policy_hash: Some(policy.policy_hash.clone()),
            }),
        })
    }
}

fn plan_kind(plan: &AdaptiveExecutionPlan) -> &'static str {
    match plan {
        AdaptiveExecutionPlan::Single { .. } => "single",
        AdaptiveExecutionPlan::OrderedFallback { .. } => "ordered_fallback",
        AdaptiveExecutionPlan::Fusion { .. } => "fusion",
    }
}

fn plan_reserved_cost(plan: &AdaptiveExecutionPlan) -> f64 {
    match plan {
        AdaptiveExecutionPlan::Single { endpoint } => endpoint.reserved_cost_usd,
        AdaptiveExecutionPlan::OrderedFallback { endpoints } => endpoints
            .iter()
            .map(|endpoint| endpoint.reserved_cost_usd)
            .sum(),
        AdaptiveExecutionPlan::Fusion {
            panel,
            judge,
            synthesizer,
        } => {
            panel
                .iter()
                .map(|endpoint| endpoint.reserved_cost_usd)
                .sum::<f64>()
                + judge.reserved_cost_usd
                + synthesizer.reserved_cost_usd
        }
    }
}

impl AdaptiveExecutionExecutor {
    pub fn new(
        providers: BTreeMap<String, Arc<dyn Provider>>,
        audit: Arc<ProviderAuditRecorder>,
        kill_switch: AdaptiveExecutionKillSwitch,
    ) -> Self {
        Self {
            providers,
            audit,
            kill_switch,
        }
    }

    pub fn endpoint_ids(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    fn audit_node_block(&self, dispatch_id: &str, error_domain: &str, cost: Option<f64>) {
        self.audit.create_and_record(
            dispatch_id,
            "adaptive-fusion",
            "adaptive_execution_blocked",
            Some(&serde_json::json!({
                "cost": cost,
                "currency": cost.map(|_| "USD"),
                "error_domain": error_domain,
            })),
        );
    }

    fn audit_node_failure(
        &self,
        dispatch_id: &str,
        error_domain: &str,
        cost: Option<f64>,
        latency_ms: Option<u64>,
    ) {
        self.audit.create_and_record(
            dispatch_id,
            "adaptive-fusion",
            "adaptive_execution_failed",
            Some(&serde_json::json!({
                "cost": cost,
                "currency": cost.map(|_| "USD"),
                "latency_ms": latency_ms.map(|value| value as i64),
                "error_domain": error_domain,
            })),
        );
    }

    pub async fn execute(
        &self,
        request: &AdaptiveExecutionRequest,
        gate: &AdaptiveExecutionGate,
    ) -> Result<AdaptiveExecutionResult, AdaptiveExecutionError> {
        let started = Instant::now();
        if !gate.is_enabled() {
            self.audit_block(request, "adaptive_execution_disabled");
            return Err(execution_error(
                "adaptive_execution_disabled",
                "provider, adaptive execution, and authentication gates are required",
                started,
                Vec::new(),
                0.0,
                0.0,
            ));
        }
        if self.kill_switch.is_killed() {
            self.audit_block(request, "adaptive_execution_killed");
            return Err(execution_error(
                "adaptive_execution_killed",
                "adaptive execution kill switch is active",
                started,
                Vec::new(),
                0.0,
                0.0,
            ));
        }
        if let Err(error) = self.validate(request, started) {
            self.audit_block(request, &error.code);
            return Err(error);
        }

        let mut state = ExecutionState::new(started);
        match &request.plan {
            AdaptiveExecutionPlan::Single { endpoint } => {
                let call = self
                    .invoke(
                        request,
                        endpoint,
                        AdaptiveCallRole::Single,
                        &request.prompt,
                        &mut state,
                    )
                    .await
                    .map_err(|error| self.audit_failure(request, error))?;
                let result = success_result(
                    request,
                    call.output,
                    call.output_truncated,
                    Some(endpoint.endpoint_id.clone()),
                    state,
                );
                self.audit_success(&result);
                Ok(result)
            }
            AdaptiveExecutionPlan::OrderedFallback { endpoints } => {
                for endpoint in endpoints {
                    match self
                        .invoke(
                            request,
                            endpoint,
                            AdaptiveCallRole::Fallback,
                            &request.prompt,
                            &mut state,
                        )
                        .await
                    {
                        Ok(call) => {
                            let result = success_result(
                                request,
                                call.output,
                                call.output_truncated,
                                Some(endpoint.endpoint_id.clone()),
                                state,
                            );
                            self.audit_success(&result);
                            return Ok(result);
                        }
                        Err(error)
                            if matches!(
                                error.code.as_ref(),
                                "adaptive_provider_error" | "adaptive_provider_disabled"
                            ) => {}
                        Err(error) => return Err(self.audit_failure(request, error)),
                    }
                }
                let error = state.error(
                    "adaptive_fallback_exhausted",
                    "ordered fallback exhausted without a successful response",
                );
                Err(self.audit_failure(request, error))
            }
            AdaptiveExecutionPlan::Fusion {
                panel,
                judge,
                synthesizer,
            } => {
                let panel_outputs = self
                    .execute_panel(request, panel, &mut state)
                    .await
                    .map_err(|error| self.audit_failure(request, error))?;
                let judge_prompt = fusion_judge_prompt(&request.prompt, &panel_outputs);
                let judge_call = self
                    .invoke(
                        request,
                        judge,
                        AdaptiveCallRole::Judge,
                        &judge_prompt,
                        &mut state,
                    )
                    .await
                    .map_err(|error| self.audit_failure(request, error))?;
                let synth_prompt =
                    fusion_synthesizer_prompt(&request.prompt, &panel_outputs, &judge_call.output);
                let synth_call = self
                    .invoke(
                        request,
                        synthesizer,
                        AdaptiveCallRole::Synthesizer,
                        &synth_prompt,
                        &mut state,
                    )
                    .await
                    .map_err(|error| self.audit_failure(request, error))?;
                let result = success_result(
                    request,
                    synth_call.output,
                    synth_call.output_truncated,
                    Some(synthesizer.endpoint_id.clone()),
                    state,
                );
                self.audit_success(&result);
                Ok(result)
            }
        }
    }

    fn validate(
        &self,
        request: &AdaptiveExecutionRequest,
        started: Instant,
    ) -> Result<(), AdaptiveExecutionError> {
        if request.schema_version != ADAPTIVE_EXECUTION_SCHEMA_VERSION
            || !valid_id(&request.dispatch_id)
            || request.prompt.is_empty()
            || request.prompt.len() > MAX_PROMPT_BYTES
        {
            return Err(execution_error(
                "adaptive_request_invalid",
                "adaptive execution request is invalid",
                started,
                Vec::new(),
                0.0,
                0.0,
            ));
        }
        let limits = &request.limits;
        if limits.max_calls == 0 || limits.max_calls > MAX_EXECUTION_CALLS {
            return Err(execution_error(
                "adaptive_call_limit_invalid",
                "max_calls is outside the allowed range",
                started,
                Vec::new(),
                0.0,
                0.0,
            ));
        }
        if !limits.max_cost_usd.is_finite()
            || limits.max_cost_usd <= 0.0
            || limits.max_cost_usd > MAX_EXECUTION_COST_USD
        {
            return Err(execution_error(
                "adaptive_cost_limit_invalid",
                "max_cost_usd is outside the allowed range",
                started,
                Vec::new(),
                0.0,
                0.0,
            ));
        }
        if limits.max_elapsed_ms == 0 || limits.max_elapsed_ms > MAX_EXECUTION_ELAPSED_MS {
            return Err(execution_error(
                "adaptive_timeout_limit_invalid",
                "max_elapsed_ms is outside the allowed range",
                started,
                Vec::new(),
                0.0,
                0.0,
            ));
        }
        if limits.max_total_tokens == 0 || limits.max_total_tokens > MAX_EXECUTION_TOKENS {
            return Err(execution_error(
                "adaptive_token_limit_invalid",
                "max_total_tokens is outside the allowed range",
                started,
                Vec::new(),
                0.0,
                0.0,
            ));
        }
        match &request.plan {
            AdaptiveExecutionPlan::Fusion { panel, .. } => {
                if limits.max_concurrency == 0
                    || limits.max_concurrency > MAX_FUSION_PANEL_CONCURRENCY
                {
                    return Err(execution_error(
                        "adaptive_concurrency_limit_invalid",
                        "fusion max_concurrency is outside the allowed range",
                        started,
                        Vec::new(),
                        0.0,
                        0.0,
                    ));
                }
                if limits.min_successful_panel_calls > panel.len() {
                    return Err(execution_error(
                        "adaptive_panel_quorum_invalid",
                        "minimum successful panel calls exceeds the panel size",
                        started,
                        Vec::new(),
                        0.0,
                        0.0,
                    ));
                }
            }
            _ if limits.max_concurrency != 1 => {
                return Err(execution_error(
                    "adaptive_concurrency_not_supported",
                    "parallel execution is supported only for fusion panel calls",
                    started,
                    Vec::new(),
                    0.0,
                    0.0,
                ));
            }
            _ if limits.min_successful_panel_calls != 0 => {
                return Err(execution_error(
                    "adaptive_panel_quorum_invalid",
                    "panel success quorum applies only to fusion plans",
                    started,
                    Vec::new(),
                    0.0,
                    0.0,
                ));
            }
            _ => {}
        }

        let invocations = plan_invocations(&request.plan, started)?;
        if invocations.len() > limits.max_calls {
            return Err(execution_error(
                "adaptive_call_limit_exceeded",
                "plan requires more calls than max_calls",
                started,
                Vec::new(),
                0.0,
                0.0,
            ));
        }
        let total_reserved_cost = invocations
            .iter()
            .map(|invocation| invocation.reserved_cost_usd)
            .sum::<f64>();
        if !total_reserved_cost.is_finite()
            || total_reserved_cost > limits.max_cost_usd + COST_EPSILON
        {
            return Err(execution_error(
                "adaptive_cost_limit_exceeded",
                "plan reservations exceed max_cost_usd",
                started,
                Vec::new(),
                total_reserved_cost,
                0.0,
            ));
        }
        for invocation in invocations {
            if !valid_id(&invocation.endpoint_id)
                || !valid_id(&invocation.model)
                || !invocation.reserved_cost_usd.is_finite()
                || invocation.reserved_cost_usd < 0.0
            {
                return Err(execution_error(
                    "adaptive_endpoint_invalid",
                    "endpoint invocation is invalid",
                    started,
                    Vec::new(),
                    0.0,
                    0.0,
                ));
            }
            if !self.providers.contains_key(&invocation.endpoint_id) {
                return Err(execution_error(
                    "adaptive_endpoint_not_found",
                    "plan references an unavailable endpoint",
                    started,
                    Vec::new(),
                    0.0,
                    0.0,
                ));
            }
            let provider = self
                .providers
                .get(&invocation.endpoint_id)
                .expect("adaptive endpoint exists");
            match provider.default_model() {
                Some(model) if model == invocation.model => {}
                Some(_) => {
                    return Err(execution_error(
                        "adaptive_endpoint_model_mismatch",
                        "plan model does not match the configured endpoint model",
                        started,
                        Vec::new(),
                        0.0,
                        0.0,
                    ));
                }
                None => {
                    return Err(execution_error(
                        "adaptive_endpoint_model_unbound",
                        "configured adaptive endpoint has no fixed model binding",
                        started,
                        Vec::new(),
                        0.0,
                        0.0,
                    ));
                }
            }
        }
        Ok(())
    }

    async fn execute_panel(
        &self,
        request: &AdaptiveExecutionRequest,
        panel: &[AdaptiveEndpointInvocation],
        state: &mut ExecutionState,
    ) -> Result<Vec<(String, String)>, AdaptiveExecutionError> {
        let prompt = bounded_text(&request.prompt, MAX_COMPOSED_PROMPT_BYTES).0;
        let mut reserved_tokens = state.total_reserved_token_count;
        let mut admissions = Vec::with_capacity(panel.len());
        for (index, invocation) in panel.iter().enumerate() {
            let reservation =
                reserve_tokens(&prompt, reserved_tokens, request.limits.max_total_tokens)
                    .ok_or_else(|| {
                        state.error(
                            "adaptive_token_limit_exceeded",
                            "remaining token budget cannot admit the fusion panel",
                        )
                    })?;
            reserved_tokens = reserved_tokens.saturating_add(reservation.total());
            admissions.push(PanelAdmission {
                index,
                invocation: invocation.clone(),
                max_total_tokens: reservation.total(),
            });
        }

        let concurrency = request.limits.max_concurrency.min(admissions.len());
        let required_successes = if request.limits.min_successful_panel_calls == 0 {
            panel.len()
        } else {
            request.limits.min_successful_panel_calls
        };
        let mut outcomes = (0..admissions.len()).map(|_| None).collect::<Vec<_>>();
        let mut processed = 0;
        let mut successful = 0;
        for wave in admissions.chunks(concurrency) {
            if self.kill_switch.is_killed() {
                break;
            }
            let mut tasks = JoinSet::new();
            for admission in wave {
                self.spawn_panel_call(
                    &mut tasks,
                    request,
                    &prompt,
                    state.started,
                    admission.clone(),
                );
            }
            let mut wave_has_fatal_failure = false;
            while let Some(joined) = tasks.join_next().await {
                let outcome = joined.map_err(|_| {
                    state.error(
                        "adaptive_runtime_failure",
                        "parallel panel execution task failed",
                    )
                })?;
                wave_has_fatal_failure |= outcome
                    .result
                    .as_ref()
                    .is_err_and(|error| !recoverable_panel_error(error));
                successful += usize::from(outcome.result.is_ok());
                processed += 1;
                let index = outcome.index;
                outcomes[index] = Some(outcome);
            }
            let remaining = admissions.len().saturating_sub(processed);
            if wave_has_fatal_failure
                || self.kill_switch.is_killed()
                || successful + remaining < required_successes
            {
                break;
            }
        }

        let mut outputs = Vec::with_capacity(panel.len());
        let mut first_fatal = None;
        let mut recoverable_failures = 0;
        for outcome in outcomes.into_iter().flatten() {
            state.merge(outcome.state);
            match outcome.result {
                Ok(call) => outputs.push((outcome.endpoint_id, call.output)),
                Err(error) if recoverable_panel_error(&error) => {
                    recoverable_failures += 1;
                }
                Err(error) if first_fatal.is_none() => {
                    first_fatal = Some((error.code, error.message));
                }
                Err(_) => {}
            }
        }

        if self.kill_switch.is_killed() {
            return Err(state.error(
                "adaptive_execution_killed",
                "adaptive execution kill switch became active during the fusion panel",
            ));
        }
        if let Some((code, message)) = first_fatal {
            return Err(state.error(&code, &message));
        }
        if outputs.len() < required_successes {
            self.audit.create_and_record(
                &request.dispatch_id,
                "adaptive-fusion",
                "adaptive_panel_quorum_failed",
                Some(&serde_json::json!({
                    "cost": state.total_provider_cost_usd,
                    "currency": "USD",
                    "latency_ms": elapsed_ms(state.started) as i64,
                    "error_domain": "adaptive_panel_quorum_not_met",
                })),
            );
            return Err(state.error(
                "adaptive_panel_quorum_not_met",
                "fusion panel did not meet the configured success quorum",
            ));
        }
        if recoverable_failures > 0 {
            self.audit.create_and_record(
                &request.dispatch_id,
                "adaptive-fusion",
                "adaptive_panel_partial_failure",
                Some(&serde_json::json!({
                    "cost": state.total_provider_cost_usd,
                    "currency": "USD",
                    "latency_ms": elapsed_ms(state.started) as i64,
                    "error_domain": "adaptive_panel_partial_failure",
                })),
            );
        } else if request.limits.max_concurrency > 1 {
            self.audit.create_and_record(
                &request.dispatch_id,
                "adaptive-fusion",
                "adaptive_panel_parallel_completed",
                Some(&serde_json::json!({
                    "cost": state.total_provider_cost_usd,
                    "currency": "USD",
                    "latency_ms": elapsed_ms(state.started) as i64,
                })),
            );
        }
        Ok(outputs)
    }

    fn spawn_panel_call(
        &self,
        tasks: &mut JoinSet<PanelTaskOutcome>,
        request: &AdaptiveExecutionRequest,
        prompt: &str,
        started: Instant,
        admission: PanelAdmission,
    ) {
        let executor = self.clone();
        let mut call_request = request.clone();
        call_request.limits.max_calls = 1;
        call_request.limits.max_cost_usd = admission.invocation.reserved_cost_usd;
        call_request.limits.max_concurrency = 1;
        call_request.limits.max_total_tokens = admission.max_total_tokens;
        call_request.limits.min_successful_panel_calls = 0;
        let prompt = prompt.to_string();
        tasks.spawn(async move {
            let mut call_state = ExecutionState::new(started);
            let result = executor
                .invoke(
                    &call_request,
                    &admission.invocation,
                    AdaptiveCallRole::Panel,
                    &prompt,
                    &mut call_state,
                )
                .await;
            PanelTaskOutcome {
                index: admission.index,
                endpoint_id: admission.invocation.endpoint_id,
                result,
                state: call_state,
            }
        });
    }

    async fn invoke(
        &self,
        request: &AdaptiveExecutionRequest,
        invocation: &AdaptiveEndpointInvocation,
        role: AdaptiveCallRole,
        prompt: &str,
        state: &mut ExecutionState,
    ) -> Result<SanitizedCall, AdaptiveExecutionError> {
        if self.kill_switch.is_killed() {
            return Err(state.error(
                "adaptive_execution_killed",
                "adaptive execution kill switch is active",
            ));
        }
        if state.calls.len() >= request.limits.max_calls {
            return Err(state.error(
                "adaptive_call_limit_exceeded",
                "max_calls reached before provider invocation",
            ));
        }
        let next_reserved = state.total_reserved_cost_usd + invocation.reserved_cost_usd;
        if next_reserved > request.limits.max_cost_usd + COST_EPSILON {
            return Err(state.error(
                "adaptive_cost_limit_exceeded",
                "next provider reservation exceeds max_cost_usd",
            ));
        }
        let Some(remaining) = remaining_duration(state.started, request.limits.max_elapsed_ms)
        else {
            return Err(state.error(
                "adaptive_execution_timeout",
                "adaptive execution elapsed-time limit reached",
            ));
        };
        let provider = self
            .providers
            .get(&invocation.endpoint_id)
            .expect("validated adaptive endpoint");
        if !provider.is_enabled() {
            state.calls.push(AdaptiveCallEvidence {
                endpoint_id: invocation.endpoint_id.clone(),
                role,
                status: "disabled".to_string(),
                reserved_cost_usd: invocation.reserved_cost_usd,
                provider_cost_usd: None,
                reserved_token_count: 0,
                input_token_count: None,
                output_token_count: None,
                latency_ms: 0,
                error_domain: Some("provider_disabled".to_string()),
            });
            self.audit.create_and_record(
                &request.dispatch_id,
                &invocation.endpoint_id,
                &role.event_name("error"),
                Some(&serde_json::json!({"error_domain": "provider_disabled"})),
            );
            return Err(state.error(
                "adaptive_provider_disabled",
                "adaptive endpoint provider is disabled",
            ));
        }

        let bounded_prompt = bounded_text(prompt, MAX_COMPOSED_PROMPT_BYTES).0;
        let token_reservation = reserve_tokens(
            &bounded_prompt,
            state.total_reserved_token_count,
            request.limits.max_total_tokens,
        )
        .ok_or_else(|| {
            state.error(
                "adaptive_token_limit_exceeded",
                "remaining token budget cannot admit the next provider call",
            )
        })?;
        state.total_reserved_cost_usd = next_reserved;
        state.total_reserved_token_count += token_reservation.total();
        let call_started = Instant::now();
        self.audit.create_and_record(
            &request.dispatch_id,
            &invocation.endpoint_id,
            &role.event_name("request"),
            Some(&serde_json::json!({
                "cost": invocation.reserved_cost_usd,
                "currency": "USD",
            })),
        );
        let provider_request = ProviderRequest {
            schema_version: "provider_request.v1".to_string(),
            provider_id: provider.provider_id().to_string(),
            model: invocation.model.clone(),
            prompt: bounded_prompt,
            metadata: serde_json::json!({
                "dispatch_id": request.dispatch_id,
                "adaptive_endpoint_id": invocation.endpoint_id,
                "adaptive_role": role,
                "reserved_cost_usd": invocation.reserved_cost_usd,
                "max_tokens": token_reservation.output,
            }),
        };

        let response = tokio::time::timeout(remaining, provider.invoke(&provider_request)).await;
        let latency_ms = call_started.elapsed().as_millis() as u64;
        match response {
            Err(_) => {
                let evidence = AdaptiveCallEvidence {
                    endpoint_id: invocation.endpoint_id.clone(),
                    role,
                    status: "timeout".to_string(),
                    reserved_cost_usd: invocation.reserved_cost_usd,
                    provider_cost_usd: None,
                    reserved_token_count: token_reservation.total(),
                    input_token_count: None,
                    output_token_count: None,
                    latency_ms,
                    error_domain: Some("adaptive_execution_timeout".to_string()),
                };
                state.calls.push(evidence);
                self.audit.create_and_record(
                    &request.dispatch_id,
                    &invocation.endpoint_id,
                    &role.event_name("timeout"),
                    Some(&serde_json::json!({
                        "latency_ms": latency_ms as i64,
                        "error_domain": "adaptive_execution_timeout",
                    })),
                );
                Err(state.error(
                    "adaptive_execution_timeout",
                    "provider call exceeded the remaining execution time",
                ))
            }
            Ok(Err(error)) => {
                let error_domain = error.error_domain.clone();
                state.calls.push(AdaptiveCallEvidence {
                    endpoint_id: invocation.endpoint_id.clone(),
                    role,
                    status: "failed".to_string(),
                    reserved_cost_usd: invocation.reserved_cost_usd,
                    provider_cost_usd: None,
                    reserved_token_count: token_reservation.total(),
                    input_token_count: None,
                    output_token_count: None,
                    latency_ms,
                    error_domain: Some(error_domain.clone()),
                });
                self.audit.create_and_record(
                    &request.dispatch_id,
                    &invocation.endpoint_id,
                    &role.event_name("error"),
                    Some(&serde_json::json!({
                        "latency_ms": latency_ms as i64,
                        "error_domain": error_domain,
                    })),
                );
                Err(state.error(
                    "adaptive_provider_error",
                    "adaptive endpoint provider call failed",
                ))
            }
            Ok(Ok(response)) => self.complete_success(
                request,
                invocation,
                role,
                response,
                &provider_request.provider_id,
                token_reservation,
                latency_ms,
                state,
            ),
        }
    }

    fn complete_success(
        &self,
        request: &AdaptiveExecutionRequest,
        invocation: &AdaptiveEndpointInvocation,
        role: AdaptiveCallRole,
        response: ProviderResponse,
        expected_provider_id: &str,
        token_reservation: TokenReservation,
        latency_ms: u64,
        state: &mut ExecutionState,
    ) -> Result<SanitizedCall, AdaptiveExecutionError> {
        let provider_cost = response
            .estimated_cost
            .unwrap_or(invocation.reserved_cost_usd);
        if provider_cost.is_finite() && provider_cost >= 0.0 {
            state.total_provider_cost_usd += provider_cost;
        }
        let (input_tokens, output_tokens) = match (
            resolved_token_count(response.input_tokens, token_reservation.input),
            resolved_token_count(response.output_tokens, token_reservation.output),
        ) {
            (Ok(input), Ok(output)) => (input, output),
            _ => {
                state.calls.push(AdaptiveCallEvidence {
                    endpoint_id: invocation.endpoint_id.clone(),
                    role,
                    status: "invalid_token_usage".to_string(),
                    reserved_cost_usd: invocation.reserved_cost_usd,
                    provider_cost_usd: response.estimated_cost,
                    reserved_token_count: token_reservation.total(),
                    input_token_count: None,
                    output_token_count: None,
                    latency_ms,
                    error_domain: Some("adaptive_provider_token_invalid".to_string()),
                });
                self.audit.create_and_record(
                    &request.dispatch_id,
                    &invocation.endpoint_id,
                    &role.event_name("error"),
                    Some(&serde_json::json!({
                        "cost": response.estimated_cost,
                        "currency": "USD",
                        "latency_ms": latency_ms as i64,
                        "error_domain": "adaptive_provider_token_invalid",
                    })),
                );
                return Err(state.error(
                    "adaptive_provider_token_invalid",
                    "provider-reported token usage is invalid",
                ));
            }
        };
        state.total_input_token_count = state.total_input_token_count.saturating_add(input_tokens);
        state.total_output_token_count =
            state.total_output_token_count.saturating_add(output_tokens);
        if response.schema_version != "provider_response.v1"
            || response.provider_id != expected_provider_id
            || response.model != invocation.model
        {
            state.calls.push(AdaptiveCallEvidence {
                endpoint_id: invocation.endpoint_id.clone(),
                role,
                status: "identity_mismatch".to_string(),
                reserved_cost_usd: invocation.reserved_cost_usd,
                provider_cost_usd: response.estimated_cost,
                reserved_token_count: token_reservation.total(),
                input_token_count: Some(input_tokens),
                output_token_count: Some(output_tokens),
                latency_ms,
                error_domain: Some("adaptive_provider_identity_mismatch".to_string()),
            });
            self.audit.create_and_record(
                &request.dispatch_id,
                &invocation.endpoint_id,
                &role.event_name("error"),
                Some(&serde_json::json!({
                    "input_token_count": i64::try_from(input_tokens).ok(),
                    "output_token_count": i64::try_from(output_tokens).ok(),
                    "cost": response.estimated_cost,
                    "currency": "USD",
                    "latency_ms": latency_ms as i64,
                    "error_domain": "adaptive_provider_identity_mismatch",
                })),
            );
            return Err(state.error(
                "adaptive_provider_identity_mismatch",
                "provider response identity does not match the configured endpoint",
            ));
        }
        let actual_tokens = input_tokens.saturating_add(output_tokens);
        if actual_tokens > token_reservation.total() {
            state.calls.push(AdaptiveCallEvidence {
                endpoint_id: invocation.endpoint_id.clone(),
                role,
                status: "token_overrun".to_string(),
                reserved_cost_usd: invocation.reserved_cost_usd,
                provider_cost_usd: response.estimated_cost,
                reserved_token_count: token_reservation.total(),
                input_token_count: Some(input_tokens),
                output_token_count: Some(output_tokens),
                latency_ms,
                error_domain: Some("adaptive_provider_token_over_reservation".to_string()),
            });
            self.audit.create_and_record(
                &request.dispatch_id,
                &invocation.endpoint_id,
                &role.event_name("error"),
                Some(&serde_json::json!({
                    "input_token_count": i64::try_from(input_tokens).ok(),
                    "output_token_count": i64::try_from(output_tokens).ok(),
                    "cost": response.estimated_cost,
                    "currency": "USD",
                    "latency_ms": latency_ms as i64,
                    "error_domain": "adaptive_provider_token_over_reservation",
                })),
            );
            return Err(state.error(
                "adaptive_provider_token_over_reservation",
                "provider-reported tokens exceeded the admitted reservation",
            ));
        }
        if !provider_cost.is_finite()
            || provider_cost < 0.0
            || provider_cost > invocation.reserved_cost_usd + COST_EPSILON
        {
            state.calls.push(AdaptiveCallEvidence {
                endpoint_id: invocation.endpoint_id.clone(),
                role,
                status: "cost_overrun".to_string(),
                reserved_cost_usd: invocation.reserved_cost_usd,
                provider_cost_usd: response.estimated_cost,
                reserved_token_count: token_reservation.total(),
                input_token_count: Some(input_tokens),
                output_token_count: Some(output_tokens),
                latency_ms,
                error_domain: Some("adaptive_provider_cost_over_reservation".to_string()),
            });
            self.audit.create_and_record(
                &request.dispatch_id,
                &invocation.endpoint_id,
                &role.event_name("error"),
                Some(&serde_json::json!({
                    "cost": response.estimated_cost,
                    "currency": "USD",
                    "latency_ms": latency_ms as i64,
                    "error_domain": "adaptive_provider_cost_over_reservation",
                })),
            );
            return Err(state.error(
                "adaptive_provider_cost_over_reservation",
                "provider-reported cost exceeded the admitted reservation",
            ));
        }
        if state.total_provider_cost_usd > request.limits.max_cost_usd + COST_EPSILON {
            return Err(state.error(
                "adaptive_cost_limit_exceeded",
                "provider-reported total cost exceeded max_cost_usd",
            ));
        }
        let (output, output_truncated) = sanitize_output(&response.output);
        state.calls.push(AdaptiveCallEvidence {
            endpoint_id: invocation.endpoint_id.clone(),
            role,
            status: "completed".to_string(),
            reserved_cost_usd: invocation.reserved_cost_usd,
            provider_cost_usd: Some(provider_cost),
            reserved_token_count: token_reservation.total(),
            input_token_count: Some(input_tokens),
            output_token_count: Some(output_tokens),
            latency_ms,
            error_domain: None,
        });
        self.audit.create_and_record(
            &request.dispatch_id,
            &invocation.endpoint_id,
            &role.event_name("response"),
            Some(&serde_json::json!({
                "input_token_count": response.input_tokens,
                "output_token_count": response.output_tokens,
                "cost": provider_cost,
                "currency": "USD",
                "latency_ms": latency_ms as i64,
            })),
        );
        Ok(SanitizedCall {
            output,
            output_truncated,
        })
    }

    fn audit_block(&self, request: &AdaptiveExecutionRequest, error_domain: &str) {
        self.audit.create_and_record(
            &request.dispatch_id,
            "adaptive-fusion",
            "adaptive_execution_blocked",
            Some(&serde_json::json!({"error_domain": error_domain})),
        );
    }

    fn audit_success(&self, result: &AdaptiveExecutionResult) {
        self.audit.create_and_record(
            &result.dispatch_id,
            "adaptive-fusion",
            "adaptive_execution_completed",
            Some(&serde_json::json!({
                "cost": result.total_provider_cost_usd,
                "currency": "USD",
                "latency_ms": result.elapsed_ms as i64,
            })),
        );
    }

    fn audit_failure(
        &self,
        request: &AdaptiveExecutionRequest,
        error: AdaptiveExecutionError,
    ) -> AdaptiveExecutionError {
        self.audit.create_and_record(
            &request.dispatch_id,
            "adaptive-fusion",
            "adaptive_execution_failed",
            Some(&serde_json::json!({
                "cost": error.total_provider_cost_usd,
                "currency": "USD",
                "latency_ms": error.elapsed_ms as i64,
                "error_domain": error.code,
            })),
        );
        error
    }
}

fn adaptive_node_error(
    error_domain: &str,
    error_message: &str,
    estimated_cost: Option<f64>,
    elapsed_ms: Option<u64>,
) -> NodeExecutionOutput {
    NodeExecutionOutput {
        status: "failed".to_string(),
        executor_type: "adaptive_provider".to_string(),
        output: None,
        error_domain: Some(error_domain.to_string()),
        error_message: Some(error_message.to_string()),
        input_tokens: None,
        output_tokens: None,
        estimated_cost,
        latency_ms: elapsed_ms.map(|value| value as i64),
    }
}

struct ExecutionState {
    started: Instant,
    calls: Vec<AdaptiveCallEvidence>,
    total_reserved_cost_usd: f64,
    total_provider_cost_usd: f64,
    total_reserved_token_count: u64,
    total_input_token_count: u64,
    total_output_token_count: u64,
}

impl ExecutionState {
    fn new(started: Instant) -> Self {
        Self {
            started,
            calls: Vec::new(),
            total_reserved_cost_usd: 0.0,
            total_provider_cost_usd: 0.0,
            total_reserved_token_count: 0,
            total_input_token_count: 0,
            total_output_token_count: 0,
        }
    }

    fn error(&self, code: &str, message: &str) -> AdaptiveExecutionError {
        execution_error(
            code,
            message,
            self.started,
            self.calls.clone(),
            self.total_reserved_cost_usd,
            self.total_provider_cost_usd,
        )
    }

    fn merge(&mut self, other: Self) {
        self.calls.extend(other.calls);
        self.total_reserved_cost_usd += other.total_reserved_cost_usd;
        self.total_provider_cost_usd += other.total_provider_cost_usd;
        self.total_reserved_token_count = self
            .total_reserved_token_count
            .saturating_add(other.total_reserved_token_count);
        self.total_input_token_count = self
            .total_input_token_count
            .saturating_add(other.total_input_token_count);
        self.total_output_token_count = self
            .total_output_token_count
            .saturating_add(other.total_output_token_count);
    }
}

struct SanitizedCall {
    output: String,
    output_truncated: bool,
}

#[derive(Clone)]
struct PanelAdmission {
    index: usize,
    invocation: AdaptiveEndpointInvocation,
    max_total_tokens: u64,
}

struct PanelTaskOutcome {
    index: usize,
    endpoint_id: String,
    result: Result<SanitizedCall, AdaptiveExecutionError>,
    state: ExecutionState,
}

fn recoverable_panel_error(error: &AdaptiveExecutionError) -> bool {
    matches!(
        error.code.as_ref(),
        "adaptive_provider_error" | "adaptive_provider_disabled"
    )
}

#[derive(Debug, Clone, Copy)]
struct TokenReservation {
    input: u64,
    output: u64,
}

impl TokenReservation {
    fn total(self) -> u64 {
        self.input + self.output
    }
}

fn plan_invocations(
    plan: &AdaptiveExecutionPlan,
    started: Instant,
) -> Result<Vec<&AdaptiveEndpointInvocation>, AdaptiveExecutionError> {
    let invocations = match plan {
        AdaptiveExecutionPlan::Single { endpoint } => vec![endpoint],
        AdaptiveExecutionPlan::OrderedFallback { endpoints } => {
            if endpoints.is_empty() {
                return Err(execution_error(
                    "adaptive_plan_invalid",
                    "ordered fallback requires at least one endpoint",
                    started,
                    Vec::new(),
                    0.0,
                    0.0,
                ));
            }
            let unique = endpoints
                .iter()
                .map(|endpoint| endpoint.endpoint_id.as_str())
                .collect::<BTreeSet<_>>();
            if unique.len() != endpoints.len() {
                return Err(execution_error(
                    "adaptive_plan_invalid",
                    "ordered fallback endpoint IDs must be unique",
                    started,
                    Vec::new(),
                    0.0,
                    0.0,
                ));
            }
            endpoints.iter().collect()
        }
        AdaptiveExecutionPlan::Fusion {
            panel,
            judge,
            synthesizer,
        } => {
            if !(MIN_FUSION_PANEL_SIZE..=MAX_FUSION_PANEL_SIZE).contains(&panel.len()) {
                return Err(execution_error(
                    "adaptive_plan_invalid",
                    "fusion panel size is outside the allowed range",
                    started,
                    Vec::new(),
                    0.0,
                    0.0,
                ));
            }
            let unique = panel
                .iter()
                .map(|endpoint| endpoint.endpoint_id.as_str())
                .collect::<BTreeSet<_>>();
            if unique.len() != panel.len() {
                return Err(execution_error(
                    "adaptive_plan_invalid",
                    "fusion panel endpoint IDs must be unique",
                    started,
                    Vec::new(),
                    0.0,
                    0.0,
                ));
            }
            let mut values = panel.iter().collect::<Vec<_>>();
            values.push(judge);
            values.push(synthesizer);
            values
        }
    };
    Ok(invocations)
}

fn success_result(
    request: &AdaptiveExecutionRequest,
    output: String,
    output_truncated: bool,
    selected_endpoint_id: Option<String>,
    state: ExecutionState,
) -> AdaptiveExecutionResult {
    AdaptiveExecutionResult {
        schema_version: ADAPTIVE_EXECUTION_SCHEMA_VERSION.to_string(),
        dispatch_id: request.dispatch_id.clone(),
        output: Some(output),
        output_truncated,
        selected_endpoint_id,
        calls: state.calls,
        total_reserved_cost_usd: state.total_reserved_cost_usd,
        total_provider_cost_usd: state.total_provider_cost_usd,
        total_reserved_token_count: state.total_reserved_token_count,
        total_input_token_count: state.total_input_token_count,
        total_output_token_count: state.total_output_token_count,
        elapsed_ms: elapsed_ms(state.started),
    }
}

fn execution_error(
    code: &str,
    message: &str,
    started: Instant,
    calls: Vec<AdaptiveCallEvidence>,
    total_reserved_cost_usd: f64,
    total_provider_cost_usd: f64,
) -> AdaptiveExecutionError {
    let total_reserved_token_count = calls.iter().map(|call| call.reserved_token_count).sum();
    let total_input_token_count = calls.iter().filter_map(|call| call.input_token_count).sum();
    let total_output_token_count = calls
        .iter()
        .filter_map(|call| call.output_token_count)
        .sum();
    AdaptiveExecutionError {
        schema_version: ADAPTIVE_EXECUTION_SCHEMA_VERSION.into(),
        code: code.into(),
        message: message.into(),
        calls,
        total_reserved_cost_usd,
        total_provider_cost_usd,
        total_reserved_token_count,
        total_input_token_count,
        total_output_token_count,
        elapsed_ms: elapsed_ms(started),
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis() as u64
}

fn remaining_duration(started: Instant, max_elapsed_ms: u64) -> Option<Duration> {
    Duration::from_millis(max_elapsed_ms).checked_sub(started.elapsed())
}

fn reserve_tokens(
    prompt: &str,
    already_reserved: u64,
    max_total_tokens: u64,
) -> Option<TokenReservation> {
    let remaining = max_total_tokens.checked_sub(already_reserved)?;
    let input = ((prompt.len() as u64).saturating_add(3) / 4).max(1);
    let output = remaining
        .checked_sub(input)?
        .min(DEFAULT_OUTPUT_TOKEN_RESERVE);
    (output > 0).then_some(TokenReservation { input, output })
}

fn resolved_token_count(reported: Option<i64>, reserved: u64) -> Result<u64, ()> {
    match reported {
        Some(value) => u64::try_from(value).map_err(|_| ()),
        None => Ok(reserved),
    }
}

fn sanitize_output(output: &str) -> (String, bool) {
    bounded_text(&redact_sensitive_patterns(output), MAX_OUTPUT_BYTES)
}

fn bounded_text(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    let mut boundary = max_bytes;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    (value[..boundary].to_string(), true)
}

fn fusion_judge_prompt(prompt: &str, panel_outputs: &[(String, String)]) -> String {
    let candidates = panel_outputs
        .iter()
        .map(|(endpoint_id, output)| format!("ENDPOINT {endpoint_id}:\n{output}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    bounded_text(
        &format!(
            "Evaluate the candidate answers for the task. Return a concise judgment.\n\nTASK:\n{prompt}\n\nCANDIDATES:\n{candidates}"
        ),
        MAX_COMPOSED_PROMPT_BYTES,
    )
    .0
}

fn fusion_synthesizer_prompt(
    prompt: &str,
    panel_outputs: &[(String, String)],
    judge_output: &str,
) -> String {
    let candidates = panel_outputs
        .iter()
        .map(|(endpoint_id, output)| format!("ENDPOINT {endpoint_id}:\n{output}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    bounded_text(
        &format!(
            "Produce the final answer using the candidate answers and judgment.\n\nTASK:\n{prompt}\n\nCANDIDATES:\n{candidates}\n\nJUDGMENT:\n{judge_output}"
        ),
        MAX_COMPOSED_PROMPT_BYTES,
    )
    .0
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ENDPOINT_ID_BYTES
        && !contains_sensitive_patterns(value)
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:/@".contains(character))
}
