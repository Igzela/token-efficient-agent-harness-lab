use serde::{Deserialize, Serialize};
use serde_json::json;

use super::policy_snapshot::stable_hash;
use crate::provider::redaction::contains_sensitive_patterns;
use crate::trusted_local::EffectiveExecutionGates;

pub const ADAPTIVE_EXPERIMENT_SCHEMA_VERSION: &str = "adaptive_experiment.v1";

const MAX_TRAFFIC_RATE: f64 = 0.05;
const MAX_ID_BYTES: usize = 160;
const DEFAULT_TRAFFIC_RATE: f64 = 0.01;
const DEFAULT_MAX_COST_USD: f64 = 1.0;
const DEFAULT_MAX_TOTAL_TOKENS: u64 = 32_768;
const DEFAULT_MAX_CALLS: usize = 8;
const DEFAULT_MAX_ELAPSED_MS: u64 = 300_000;
const DEFAULT_MAX_CONCURRENCY: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveExperimentPolicy {
    pub traffic_rate: f64,
    pub max_cost_usd: f64,
    pub max_total_tokens: u64,
    pub max_calls: usize,
    pub max_elapsed_ms: u64,
    pub max_concurrency: usize,
}

impl Default for AdaptiveExperimentPolicy {
    fn default() -> Self {
        Self {
            traffic_rate: DEFAULT_TRAFFIC_RATE,
            max_cost_usd: DEFAULT_MAX_COST_USD,
            max_total_tokens: DEFAULT_MAX_TOTAL_TOKENS,
            max_calls: DEFAULT_MAX_CALLS,
            max_elapsed_ms: DEFAULT_MAX_ELAPSED_MS,
            max_concurrency: DEFAULT_MAX_CONCURRENCY,
        }
    }
}

impl AdaptiveExperimentPolicy {
    pub fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            traffic_rate: env_f64("ACP_ADAPTIVE_EXPERIMENT_TRAFFIC_RATE")
                .unwrap_or(defaults.traffic_rate),
            max_cost_usd: env_f64("ACP_ADAPTIVE_EXPERIMENT_MAX_COST_USD")
                .unwrap_or(defaults.max_cost_usd),
            max_total_tokens: env_u64("ACP_ADAPTIVE_EXPERIMENT_MAX_TOTAL_TOKENS")
                .unwrap_or(defaults.max_total_tokens),
            max_calls: env_usize("ACP_ADAPTIVE_EXPERIMENT_MAX_CALLS").unwrap_or(defaults.max_calls),
            max_elapsed_ms: env_u64("ACP_ADAPTIVE_EXPERIMENT_MAX_ELAPSED_MS")
                .unwrap_or(defaults.max_elapsed_ms),
            max_concurrency: env_usize("ACP_ADAPTIVE_EXPERIMENT_MAX_CONCURRENCY")
                .unwrap_or(defaults.max_concurrency),
        }
    }

    pub fn validation_errors(&self) -> Vec<String> {
        validate_policy(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveExperimentGate {
    enabled: bool,
    active: bool,
    paused: bool,
    killed: bool,
}

impl AdaptiveExperimentGate {
    pub fn from_env() -> Self {
        let gates = EffectiveExecutionGates::from_env();
        Self::from_effective_gates(&gates)
    }

    pub fn from_effective_gates(gates: &EffectiveExecutionGates) -> Self {
        Self::from_flags(
            gates.experiments_enabled,
            gates.experiments_active,
            env_enabled("ACP_ADAPTIVE_EXPERIMENTS_PAUSED"),
            env_enabled("ACP_ADAPTIVE_EXPERIMENTS_KILL_SWITCH"),
        )
    }

    pub fn from_flags(enabled: bool, active: bool, paused: bool, killed: bool) -> Self {
        Self {
            enabled,
            active,
            paused,
            killed,
        }
    }

    pub fn is_configured(self) -> bool {
        self.enabled || self.active || self.paused || self.killed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveExperimentRequest {
    pub request_id: String,
    pub exploration_seed: u64,
    pub risk_level: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveExperimentLimits {
    pub reserved_cost_usd: f64,
    pub max_cost_usd: f64,
    pub max_total_tokens: u64,
    pub max_calls: usize,
    pub max_elapsed_ms: u64,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveExperimentDecision {
    pub schema_version: String,
    pub assigned: bool,
    pub bucket: f64,
    pub traffic_rate: f64,
    pub blocked_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveExperimentError {
    pub code: String,
    pub violations: Vec<String>,
}

pub struct AdaptiveExperimentController;

impl AdaptiveExperimentController {
    pub fn decide(
        request: &AdaptiveExperimentRequest,
        policy: &AdaptiveExperimentPolicy,
        gate: &AdaptiveExperimentGate,
    ) -> Result<AdaptiveExperimentDecision, AdaptiveExperimentError> {
        let mut violations = validate_request(request);
        violations.extend(validate_policy(policy));
        violations.sort();
        violations.dedup();
        if !violations.is_empty() {
            return Err(AdaptiveExperimentError {
                code: "adaptive_experiment_validation_failed".to_string(),
                violations,
            });
        }

        let bucket = deterministic_bucket(&request.request_id, request.exploration_seed);
        let mut blocked_reasons = Vec::new();
        if !gate.enabled || !gate.active {
            blocked_reasons.push("adaptive_experiment_gates_disabled".to_string());
        } else if gate.killed {
            blocked_reasons.push("adaptive_experiment_kill_switch_active".to_string());
        } else if gate.paused {
            blocked_reasons.push("adaptive_experiment_paused".to_string());
        } else if matches!(request.risk_level.as_str(), "high" | "critical") {
            blocked_reasons.push("adaptive_experiment_risk_blocked".to_string());
        }
        let assigned = blocked_reasons.is_empty() && bucket < policy.traffic_rate;
        Ok(AdaptiveExperimentDecision {
            schema_version: ADAPTIVE_EXPERIMENT_SCHEMA_VERSION.to_string(),
            assigned,
            bucket,
            traffic_rate: policy.traffic_rate,
            blocked_reasons,
        })
    }

    pub fn validate_limits(
        limits: &AdaptiveExperimentLimits,
        policy: &AdaptiveExperimentPolicy,
    ) -> Result<(), &'static str> {
        if !limits.reserved_cost_usd.is_finite()
            || limits.reserved_cost_usd < 0.0
            || limits.reserved_cost_usd > policy.max_cost_usd
            || !limits.max_cost_usd.is_finite()
            || limits.max_cost_usd < 0.0
            || limits.max_cost_usd > policy.max_cost_usd
        {
            return Err("adaptive_experiment_cost_cap_exceeded");
        }
        if limits.max_total_tokens == 0 || limits.max_total_tokens > policy.max_total_tokens {
            return Err("adaptive_experiment_token_cap_exceeded");
        }
        if limits.max_calls == 0 || limits.max_calls > policy.max_calls {
            return Err("adaptive_experiment_call_cap_exceeded");
        }
        if limits.max_elapsed_ms == 0 || limits.max_elapsed_ms > policy.max_elapsed_ms {
            return Err("adaptive_experiment_time_cap_exceeded");
        }
        if limits.max_concurrency == 0 || limits.max_concurrency > policy.max_concurrency {
            return Err("adaptive_experiment_concurrency_cap_exceeded");
        }
        Ok(())
    }
}

fn validate_request(request: &AdaptiveExperimentRequest) -> Vec<String> {
    let mut violations = Vec::new();
    if !valid_id(&request.request_id) {
        violations.push("invalid_request_id".to_string());
    }
    if !matches!(
        request.risk_level.as_str(),
        "low" | "medium" | "high" | "critical"
    ) {
        violations.push("invalid_risk_level".to_string());
    }
    if contains_sensitive_patterns(&serde_json::to_string(request).unwrap_or_default()) {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
}

fn validate_policy(policy: &AdaptiveExperimentPolicy) -> Vec<String> {
    let mut violations = Vec::new();
    if !policy.traffic_rate.is_finite() || !(0.0..=MAX_TRAFFIC_RATE).contains(&policy.traffic_rate)
    {
        violations.push("invalid_traffic_rate".to_string());
    }
    if !policy.max_cost_usd.is_finite() || policy.max_cost_usd < 0.0 {
        violations.push("invalid_experiment_cost_cap".to_string());
    }
    if policy.max_total_tokens == 0 {
        violations.push("invalid_experiment_token_cap".to_string());
    }
    if policy.max_calls == 0 {
        violations.push("invalid_experiment_call_cap".to_string());
    }
    if policy.max_elapsed_ms == 0 {
        violations.push("invalid_experiment_time_cap".to_string());
    }
    if policy.max_concurrency == 0 || policy.max_concurrency > DEFAULT_MAX_CONCURRENCY {
        violations.push("invalid_experiment_concurrency_cap".to_string());
    }
    violations
}

fn deterministic_bucket(request_id: &str, seed: u64) -> f64 {
    let hash = stable_hash(&json!({
        "request_id": request_id,
        "seed": seed,
    }));
    let sample = u64::from_str_radix(&hash[..16], 16).unwrap_or_default();
    sample as f64 / u64::MAX as f64
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn env_enabled(key: &str) -> bool {
    std::env::var(key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn env_f64(key: &str) -> Option<f64> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_u64(key: &str) -> Option<u64> {
    std::env::var(key).ok()?.parse().ok()
}

fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key).ok()?.parse().ok()
}

#[cfg(test)]
mod policy_validation_tests {
    use super::*;

    #[test]
    fn validation_errors_reuse_runtime_policy_rules() {
        let policy = AdaptiveExperimentPolicy {
            traffic_rate: 0.5,
            max_cost_usd: -1.0,
            max_total_tokens: 0,
            max_calls: 0,
            max_elapsed_ms: 0,
            max_concurrency: 4,
        };

        assert_eq!(
            policy.validation_errors(),
            vec![
                "invalid_traffic_rate",
                "invalid_experiment_cost_cap",
                "invalid_experiment_token_cap",
                "invalid_experiment_call_cap",
                "invalid_experiment_time_cap",
                "invalid_experiment_concurrency_cap",
            ]
        );
        assert!(AdaptiveExperimentPolicy::default()
            .validation_errors()
            .is_empty());
    }
}
