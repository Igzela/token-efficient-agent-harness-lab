use serde::{Deserialize, Serialize};
use serde_json::json;

use super::policy_snapshot::stable_hash;
use super::shadow_router::{ShadowReplayComparison, ShadowRouter};
use super::OfflineReplayStatus;
use crate::provider::redaction::contains_sensitive_patterns;
use crate::trusted_local::EffectiveExecutionGates;

pub const ADAPTIVE_EXPERIMENT_SCHEMA_VERSION: &str = "adaptive_experiment.v1";
pub const ADAPTIVE_CANARY_SCHEMA_VERSION: &str = "adaptive_canary.v1";

const MAX_TRAFFIC_RATE: f64 = 0.05;
const MAX_ID_BYTES: usize = 160;
const DEFAULT_TRAFFIC_RATE: f64 = 0.01;
const DEFAULT_MAX_COST_USD: f64 = 1.0;
const DEFAULT_MAX_TOTAL_TOKENS: u64 = 32_768;
const DEFAULT_MAX_CALLS: usize = 8;
const DEFAULT_MAX_ELAPSED_MS: u64 = 300_000;
const DEFAULT_MAX_CONCURRENCY: usize = 3;
const MAX_CANARY_EVIDENCE: usize = 10_000;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveCanaryRequest {
    pub canary_id: String,
    pub task_class: String,
    pub scope: String,
    pub policy_version: String,
    pub policy_hash: String,
    pub candidate_id: String,
    pub candidate_version: String,
    pub candidate_definition_sha256: String,
    pub rollout_percentage: u8,
    pub duration_seconds: u64,
    pub minimum_evidence: usize,
    pub confirm_canary: bool,
    pub permission_granted: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdaptiveCanaryDecision {
    pub schema_version: String,
    pub status: String,
    pub canary_id: String,
    pub idempotency_key: String,
    pub policy_version: String,
    pub policy_hash: String,
    pub candidate_id: String,
    pub candidate_version: String,
    pub candidate_definition_sha256: String,
    pub rollout_percentage: u8,
    pub duration_seconds: u64,
    pub minimum_evidence: usize,
    pub source_shadow_content_sha256: String,
    pub rollback_target: String,
    pub paused: bool,
    pub compensation_required: bool,
    pub blocked_reasons: Vec<String>,
    pub content_sha256: String,
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

    pub fn start_canary(
        request: &AdaptiveCanaryRequest,
        shadow: &ShadowReplayComparison,
        gate: &AdaptiveExperimentGate,
    ) -> Result<AdaptiveCanaryDecision, AdaptiveExperimentError> {
        let mut blocked_reasons = validate_canary_request(request);
        if let Err(error) = ShadowRouter::validate_replay_comparison(shadow) {
            blocked_reasons.push(error);
        }
        if shadow.status != OfflineReplayStatus::Sufficient {
            blocked_reasons.push("shadow_evidence_not_sufficient".to_string());
        }
        if shadow.policy_version.as_deref() != Some(request.policy_version.as_str())
            || shadow.policy_hash.as_deref() != Some(request.policy_hash.as_str())
        {
            blocked_reasons.push("shadow_policy_binding_mismatch".to_string());
        }
        let candidate_binding_matches = shadow.comparisons.iter().any(|comparison| {
            comparison.policy_version == request.policy_version
                && comparison.policy_hash == request.policy_hash
                && comparison.predicted.selection.candidate_id == request.candidate_id
                && comparison.predicted.selection.candidate_version == request.candidate_version
                && comparison.predicted.selection.candidate_definition_sha256
                    == request.candidate_definition_sha256
                && comparison.observed.task_class == request.task_class
                && comparison.coverage_trace_ids.len() >= request.minimum_evidence
        });
        if !candidate_binding_matches {
            blocked_reasons.push("shadow_candidate_binding_or_coverage_mismatch".to_string());
        }
        if !gate.enabled || !gate.active {
            blocked_reasons.push("adaptive_canary_gates_disabled".to_string());
        }
        if gate.killed {
            blocked_reasons.push("adaptive_canary_kill_switch_active".to_string());
        }
        if gate.paused {
            blocked_reasons.push("adaptive_canary_paused".to_string());
        }
        if !request.confirm_canary {
            blocked_reasons.push("confirm_canary is required".to_string());
        }
        if !request.permission_granted {
            blocked_reasons.push("canary_permission_required".to_string());
        }
        blocked_reasons.sort();
        blocked_reasons.dedup();
        let status = if blocked_reasons.contains(&"adaptive_canary_paused".to_string()) {
            "paused"
        } else if blocked_reasons.is_empty() {
            "started"
        } else {
            "blocked"
        };
        let mut decision = AdaptiveCanaryDecision {
            schema_version: ADAPTIVE_CANARY_SCHEMA_VERSION.to_string(),
            status: status.to_string(),
            canary_id: request.canary_id.clone(),
            idempotency_key: request.idempotency_key.clone(),
            policy_version: request.policy_version.clone(),
            policy_hash: request.policy_hash.clone(),
            candidate_id: request.candidate_id.clone(),
            candidate_version: request.candidate_version.clone(),
            candidate_definition_sha256: request.candidate_definition_sha256.clone(),
            rollout_percentage: request.rollout_percentage,
            duration_seconds: request.duration_seconds,
            minimum_evidence: request.minimum_evidence,
            source_shadow_content_sha256: shadow.content_sha256.clone(),
            rollback_target: format!("policy:{}", request.policy_hash),
            paused: status == "paused",
            compensation_required: status == "started",
            blocked_reasons,
            content_sha256: String::new(),
        };
        decision.content_sha256 = stable_hash(&json!({
            "schema_version": decision.schema_version,
            "status": decision.status,
            "canary_id": decision.canary_id,
            "idempotency_key": decision.idempotency_key,
            "policy_version": decision.policy_version,
            "policy_hash": decision.policy_hash,
            "candidate_id": decision.candidate_id,
            "candidate_version": decision.candidate_version,
            "candidate_definition_sha256": decision.candidate_definition_sha256,
            "rollout_percentage": decision.rollout_percentage,
            "duration_seconds": decision.duration_seconds,
            "minimum_evidence": decision.minimum_evidence,
            "source_shadow_content_sha256": decision.source_shadow_content_sha256,
            "rollback_target": decision.rollback_target,
            "paused": decision.paused,
            "compensation_required": decision.compensation_required,
            "blocked_reasons": decision.blocked_reasons,
        }));
        Ok(decision)
    }
}

fn validate_canary_request(request: &AdaptiveCanaryRequest) -> Vec<String> {
    let mut violations = Vec::new();
    for value in [
        &request.canary_id,
        &request.task_class,
        &request.scope,
        &request.policy_version,
        &request.candidate_id,
        &request.candidate_version,
        &request.idempotency_key,
    ] {
        if !valid_id(value) {
            violations.push("invalid_canary_identity".to_string());
            break;
        }
    }
    for hash in [&request.policy_hash, &request.candidate_definition_sha256] {
        if !valid_hash(hash) {
            violations.push("invalid_canary_binding_hash".to_string());
        }
    }
    if request.rollout_percentage == 0 || request.rollout_percentage > 5 {
        violations.push("canary_rollout_must_be_between_1_and_5_percent".to_string());
    }
    if request.duration_seconds == 0 || request.duration_seconds > 86_400 {
        violations.push("canary_duration_must_be_between_1_second_and_24_hours".to_string());
    }
    if request.minimum_evidence == 0 || request.minimum_evidence > MAX_CANARY_EVIDENCE {
        violations.push("invalid_canary_minimum_evidence".to_string());
    }
    if contains_sensitive_patterns(&serde_json::to_string(request).unwrap_or_default()) {
        violations.push("sensitive_pattern_detected".to_string());
    }
    violations
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

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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

#[cfg(test)]
mod canary_tests {
    use super::*;
    use crate::feedback::{
        shadow_replay_comparison_sha256, OfflineCounterfactualEstimate, OfflineObservedFacts,
        ShadowDriftEvidence, ShadowPolicyComparison, ShadowReplayComparison,
        SHADOW_REPLAY_COMPARISON_SCHEMA_VERSION,
    };

    fn request(confirm: bool, permission: bool) -> AdaptiveCanaryRequest {
        AdaptiveCanaryRequest {
            canary_id: "canary-1".to_string(),
            task_class: "coding".to_string(),
            scope: "tenant-local".to_string(),
            policy_version: "policy-v1".to_string(),
            policy_hash: format!("{:064x}", 1),
            candidate_id: "candidate".to_string(),
            candidate_version: "candidate-v1".to_string(),
            candidate_definition_sha256: format!("{:064x}", 2),
            rollout_percentage: 5,
            duration_seconds: 3_600,
            minimum_evidence: 3,
            confirm_canary: confirm,
            permission_granted: permission,
            idempotency_key: "canary-key-1".to_string(),
        }
    }

    fn shadow() -> ShadowReplayComparison {
        let policy_hash = format!("{:064x}", 1);
        let candidate_definition_sha256 = format!("{:064x}", 2);
        let observed = OfflineObservedFacts {
            task_class: "coding".to_string(),
            candidate_id: "baseline".to_string(),
            candidate_version: "baseline-v1".to_string(),
            candidate_definition_sha256: format!("{:064x}", 3),
            member_endpoint_ids: vec!["endpoint".to_string()],
            trace_ids: vec!["trace-1".to_string()],
            evidence_content_sha256: vec![format!("{:064x}", 4)],
            sample_count: 3,
            success_rate: 1.0,
            average_quality_score: 0.8,
            average_tool_success_score: 0.8,
            average_cost_usd: 0.1,
            average_latency_ms: 100.0,
            average_total_tokens: 10.0,
            average_retry_count: 0.0,
        };
        let predicted = OfflineCounterfactualEstimate {
            policy_id: "policy".to_string(),
            policy_version: "policy-v1".to_string(),
            policy_hash: policy_hash.clone(),
            task_class: "coding".to_string(),
            selection: crate::feedback::OfflinePolicySelection {
                candidate_id: "candidate".to_string(),
                candidate_version: "candidate-v1".to_string(),
                candidate_definition_sha256,
            },
            source_candidate_id: "candidate".to_string(),
            source_candidate_version: "candidate-v1".to_string(),
            source_candidate_definition_sha256: format!("{:064x}", 2),
            source_trace_ids: vec![
                "trace-1".to_string(),
                "trace-2".to_string(),
                "trace-3".to_string(),
            ],
            source_evidence_content_sha256: vec![format!("{:064x}", 5)],
            sample_count: 3,
            estimated_success_rate: 1.0,
            estimated_quality_score: 0.9,
            estimated_tool_success_score: 0.9,
            estimated_cost_usd: 0.08,
            estimated_latency_ms: 90.0,
            estimated_total_tokens: 9.0,
            estimated_retry_count: 0.0,
            estimation_method: "observed_comparable_candidate_cohort".to_string(),
        };
        let mut comparison = ShadowReplayComparison {
            schema_version: SHADOW_REPLAY_COMPARISON_SCHEMA_VERSION.to_string(),
            status: OfflineReplayStatus::Sufficient,
            reason_codes: Vec::new(),
            policy_id: Some("policy".to_string()),
            policy_version: Some("policy-v1".to_string()),
            policy_hash: Some(policy_hash),
            source_trace_ids: vec!["trace-1".to_string()],
            source_evidence_content_sha256: vec![format!("{:064x}", 4)],
            comparisons: vec![ShadowPolicyComparison {
                policy_id: "policy".to_string(),
                policy_version: "policy-v1".to_string(),
                policy_hash: format!("{:064x}", 1),
                task_class: "coding".to_string(),
                observed,
                predicted,
                success_rate_delta: 0.0,
                quality_score_delta: 0.1,
                tool_success_score_delta: 0.1,
                cost_usd_delta: -0.02,
                latency_ms_delta: -10.0,
                total_tokens_delta: -1.0,
                retry_count_delta: 0.0,
                coverage_trace_ids: vec![
                    "trace-1".to_string(),
                    "trace-2".to_string(),
                    "trace-3".to_string(),
                ],
                coverage_evidence_content_sha256: vec![format!("{:064x}", 5)],
                drift: ShadowDriftEvidence {
                    status: "observed_comparable_cohort_not_live_candidate".to_string(),
                    reason_codes: vec![
                        "counterfactual_estimate_is_not_live_candidate_observation".to_string()
                    ],
                    observed_sample_count: 3,
                    predicted_sample_count: 3,
                },
            }],
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            content_sha256: String::new(),
        };
        comparison.content_sha256 = shadow_replay_comparison_sha256(&comparison);
        comparison
    }

    #[test]
    fn canary_requires_bound_shadow_evidence_and_is_bounded() {
        let decision = AdaptiveExperimentController::start_canary(
            &request(true, true),
            &shadow(),
            &AdaptiveExperimentGate::from_flags(true, true, false, false),
        )
        .unwrap();
        assert_eq!(decision.status, "started");
        assert!(decision.compensation_required);
        assert_eq!(decision.rollout_percentage, 5);
        assert_eq!(decision.rollback_target, format!("policy:{:064x}", 1));
        assert_eq!(
            decision,
            AdaptiveExperimentController::start_canary(
                &request(true, true),
                &shadow(),
                &AdaptiveExperimentGate::from_flags(true, true, false, false),
            )
            .unwrap()
        );
    }

    #[test]
    fn canary_pause_kill_confirmation_and_permission_fail_closed() {
        let paused = AdaptiveExperimentController::start_canary(
            &request(true, true),
            &shadow(),
            &AdaptiveExperimentGate::from_flags(true, true, true, false),
        )
        .unwrap();
        assert_eq!(paused.status, "paused");
        assert!(paused.paused);
        assert!(paused
            .blocked_reasons
            .contains(&"adaptive_canary_paused".to_string()));

        let blocked = AdaptiveExperimentController::start_canary(
            &request(false, false),
            &shadow(),
            &AdaptiveExperimentGate::from_flags(true, true, false, true),
        )
        .unwrap();
        assert_eq!(blocked.status, "blocked");
        assert!(blocked
            .blocked_reasons
            .contains(&"confirm_canary is required".to_string()));
        assert!(blocked
            .blocked_reasons
            .contains(&"canary_permission_required".to_string()));
        assert!(blocked
            .blocked_reasons
            .contains(&"adaptive_canary_kill_switch_active".to_string()));
    }
}
