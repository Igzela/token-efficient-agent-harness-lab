use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::endpoint_registry::ModelEndpointSpec;
use super::policy_snapshot::stable_hash;
use crate::provider::redaction::contains_sensitive_patterns;

pub const ADAPTIVE_CANDIDATE_SCHEMA_VERSION: &str = "adaptive_candidate.v1";
pub const ADAPTIVE_CANDIDATE_SET_SCHEMA_VERSION: &str = "adaptive_candidate_set.v1";

const DEFAULT_MAX_CANDIDATES: usize = 10;
const DEFAULT_MAX_PANEL_SIZE: usize = 3;
const DEFAULT_MAX_FALLBACK_ENDPOINTS: usize = 3;
const DEFAULT_ESTIMATED_INPUT_TOKENS: u64 = 2_000;
const DEFAULT_ESTIMATED_OUTPUT_TOKENS: u64 = 1_000;
const MIN_FUSION_PANEL: usize = 2;
const MIN_FUSION_ENDPOINTS: usize = 3;
const DEFAULT_LATENCY_MS: u64 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Single,
    OrderedFallback,
    Fusion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FusionRole {
    Panel,
    Judge,
    Synthesizer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateEndpointBinding {
    pub endpoint_id: String,
    pub model_id: String,
    pub fusion_role: Option<FusionRole>,
    pub estimated_cost_usd: f64,
    pub estimated_input_tokens: u64,
    pub estimated_output_tokens: u64,
    pub estimated_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveCandidate {
    pub schema_version: String,
    pub candidate_id: String,
    pub candidate_hash: String,
    pub task_class: String,
    pub objective: String,
    pub candidate_kind: CandidateKind,
    pub member_endpoint_ids: Vec<String>,
    pub endpoint_bindings: Vec<CandidateEndpointBinding>,
    pub estimated_cost_usd: f64,
    pub estimated_input_tokens: u64,
    pub estimated_output_tokens: u64,
    pub estimated_latency_ms: u64,
    pub required_capabilities: Vec<String>,
    pub registry_snapshot_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointRejection {
    pub endpoint_id: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveCandidateConfig {
    pub max_candidates: usize,
    pub max_panel_size: usize,
    pub max_fallback_endpoints: usize,
    pub estimated_input_tokens: u64,
    pub estimated_output_tokens: u64,
}

impl Default for AdaptiveCandidateConfig {
    fn default() -> Self {
        Self {
            max_candidates: DEFAULT_MAX_CANDIDATES,
            max_panel_size: DEFAULT_MAX_PANEL_SIZE,
            max_fallback_endpoints: DEFAULT_MAX_FALLBACK_ENDPOINTS,
            estimated_input_tokens: DEFAULT_ESTIMATED_INPUT_TOKENS,
            estimated_output_tokens: DEFAULT_ESTIMATED_OUTPUT_TOKENS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateGenerationRequest {
    pub task_class: String,
    pub objective: String,
    pub risk_level: String,
    pub required_capabilities: Vec<String>,
    pub max_estimated_cost_usd: f64,
    pub max_estimated_tokens: u64,
    pub max_estimated_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveCandidateSet {
    pub schema_version: String,
    pub request: CandidateGenerationRequest,
    pub candidates: Vec<AdaptiveCandidate>,
    pub rejected_endpoints: Vec<EndpointRejection>,
    pub registry_snapshot_hash: String,
}

pub struct AdaptiveCandidateGenerator;

impl AdaptiveCandidateGenerator {
    pub fn generate(
        request: &CandidateGenerationRequest,
        endpoints: &[ModelEndpointSpec],
        config: &AdaptiveCandidateConfig,
        registry_snapshot_hash: &str,
    ) -> AdaptiveCandidateSet {
        let (mut eligible, rejected) = filter_endpoints(request, endpoints, config);

        eligible.sort_by(|left, right| {
            let ordering = endpoint_utility(right, request, config)
                .partial_cmp(&endpoint_utility(left, request, config))
                .unwrap_or(Ordering::Equal);
            ordering.then_with(|| left.endpoint_id.cmp(&right.endpoint_id))
        });

        let mut candidates = Vec::new();

        for ep in eligible.iter().take(config.max_candidates) {
            let cost = endpoint_estimated_cost(ep, config);
            let latency = endpoint_estimated_latency(ep);
            let binding = CandidateEndpointBinding {
                endpoint_id: ep.endpoint_id.clone(),
                model_id: ep.model_id.clone(),
                fusion_role: None,
                estimated_cost_usd: cost,
                estimated_input_tokens: config.estimated_input_tokens,
                estimated_output_tokens: config.estimated_output_tokens,
                estimated_latency_ms: latency,
            };
            let member_ids = vec![ep.endpoint_id.clone()];
            let hash_input = json!({
                "schema_version": ADAPTIVE_CANDIDATE_SCHEMA_VERSION,
                "candidate_kind": "single",
                "member_endpoint_ids": member_ids,
                "task_class": request.task_class,
                "objective": request.objective,
                "registry_snapshot_hash": registry_snapshot_hash,
            });
            let hash = stable_hash(&hash_input);
            candidates.push(AdaptiveCandidate {
                schema_version: ADAPTIVE_CANDIDATE_SCHEMA_VERSION.to_string(),
                candidate_id: format!("single-{}", &hash[..16]),
                candidate_hash: hash,
                task_class: request.task_class.clone(),
                objective: request.objective.clone(),
                candidate_kind: CandidateKind::Single,
                member_endpoint_ids: member_ids,
                endpoint_bindings: vec![binding],
                estimated_cost_usd: cost,
                estimated_input_tokens: config.estimated_input_tokens,
                estimated_output_tokens: config.estimated_output_tokens,
                estimated_latency_ms: latency,
                required_capabilities: request.required_capabilities.clone(),
                registry_snapshot_hash: registry_snapshot_hash.to_string(),
            });
        }

        let fallback_pool: Vec<_> = eligible
            .iter()
            .take(config.max_fallback_endpoints)
            .collect();
        if fallback_pool.len() >= 2 {
            let bindings: Vec<CandidateEndpointBinding> = fallback_pool
                .iter()
                .map(|ep| {
                    let cost = endpoint_estimated_cost(ep, config);
                    let latency = endpoint_estimated_latency(ep);
                    CandidateEndpointBinding {
                        endpoint_id: ep.endpoint_id.clone(),
                        model_id: ep.model_id.clone(),
                        fusion_role: None,
                        estimated_cost_usd: cost,
                        estimated_input_tokens: config.estimated_input_tokens,
                        estimated_output_tokens: config.estimated_output_tokens,
                        estimated_latency_ms: latency,
                    }
                })
                .collect();
            let member_ids: Vec<String> = bindings.iter().map(|b| b.endpoint_id.clone()).collect();
            let total_cost: f64 = bindings.iter().map(|b| b.estimated_cost_usd).sum();
            let total_tokens: u64 = bindings
                .iter()
                .map(|b| b.estimated_input_tokens + b.estimated_output_tokens)
                .sum();
            let total_latency: u64 = bindings.iter().map(|b| b.estimated_latency_ms).sum();

            if !exceeds_candidate_caps(total_cost, total_tokens, total_latency, request) {
                let hash_input = json!({
                    "schema_version": ADAPTIVE_CANDIDATE_SCHEMA_VERSION,
                    "candidate_kind": "ordered_fallback",
                    "member_endpoint_ids": member_ids,
                    "task_class": request.task_class,
                    "objective": request.objective,
                    "registry_snapshot_hash": registry_snapshot_hash,
                });
                let hash = stable_hash(&hash_input);
                candidates.push(AdaptiveCandidate {
                    schema_version: ADAPTIVE_CANDIDATE_SCHEMA_VERSION.to_string(),
                    candidate_id: format!("fallback-{}", &hash[..16]),
                    candidate_hash: hash,
                    task_class: request.task_class.clone(),
                    objective: request.objective.clone(),
                    candidate_kind: CandidateKind::OrderedFallback,
                    member_endpoint_ids: member_ids,
                    endpoint_bindings: bindings,
                    estimated_cost_usd: total_cost,
                    estimated_input_tokens: total_tokens,
                    estimated_output_tokens: 0,
                    estimated_latency_ms: total_latency,
                    required_capabilities: request.required_capabilities.clone(),
                    registry_snapshot_hash: registry_snapshot_hash.to_string(),
                });
            }
        }

        if eligible.len() >= MIN_FUSION_ENDPOINTS {
            let panel_size = config
                .max_panel_size
                .clamp(MIN_FUSION_PANEL, eligible.len());
            let panel_eps: Vec<&ModelEndpointSpec> = eligible.iter().take(panel_size).collect();
            let judge_ep = eligible
                .iter()
                .min_by(|left, right| {
                    let left_score = left.health.score * 0.7 + left.health.score * 0.3;
                    let right_score = right.health.score * 0.7 + right.health.score * 0.3;
                    right_score
                        .partial_cmp(&left_score)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| left.endpoint_id.cmp(&right.endpoint_id))
                })
                .expect("non-empty eligible");
            let synth_ep = eligible.first().expect("non-empty eligible");

            let mut panel_bindings: Vec<CandidateEndpointBinding> = panel_eps
                .iter()
                .map(|ep| {
                    let cost = endpoint_estimated_cost(ep, config);
                    let latency = endpoint_estimated_latency(ep);
                    CandidateEndpointBinding {
                        endpoint_id: ep.endpoint_id.clone(),
                        model_id: ep.model_id.clone(),
                        fusion_role: Some(FusionRole::Panel),
                        estimated_cost_usd: cost,
                        estimated_input_tokens: config.estimated_input_tokens,
                        estimated_output_tokens: config.estimated_output_tokens,
                        estimated_latency_ms: latency,
                    }
                })
                .collect();
            let judge_binding = CandidateEndpointBinding {
                endpoint_id: judge_ep.endpoint_id.clone(),
                model_id: judge_ep.model_id.clone(),
                fusion_role: Some(FusionRole::Judge),
                estimated_cost_usd: endpoint_estimated_cost(judge_ep, config),
                estimated_input_tokens: config.estimated_input_tokens,
                estimated_output_tokens: config.estimated_output_tokens,
                estimated_latency_ms: endpoint_estimated_latency(judge_ep),
            };
            let synth_binding = CandidateEndpointBinding {
                endpoint_id: synth_ep.endpoint_id.clone(),
                model_id: synth_ep.model_id.clone(),
                fusion_role: Some(FusionRole::Synthesizer),
                estimated_cost_usd: endpoint_estimated_cost(synth_ep, config),
                estimated_input_tokens: config.estimated_input_tokens,
                estimated_output_tokens: config.estimated_output_tokens,
                estimated_latency_ms: endpoint_estimated_latency(synth_ep),
            };

            let mut all_bindings = Vec::new();
            all_bindings.append(&mut panel_bindings);
            all_bindings.push(judge_binding);
            all_bindings.push(synth_binding);

            let member_ids: Vec<String> =
                all_bindings.iter().map(|b| b.endpoint_id.clone()).collect();
            let total_cost: f64 = all_bindings.iter().map(|b| b.estimated_cost_usd).sum();
            let total_tokens: u64 = all_bindings
                .iter()
                .map(|b| b.estimated_input_tokens + b.estimated_output_tokens)
                .sum();
            let panel_max_latency = panel_eps
                .iter()
                .map(|ep| endpoint_estimated_latency(ep))
                .max()
                .unwrap_or(0);
            let total_latency = panel_max_latency
                + endpoint_estimated_latency(judge_ep)
                + endpoint_estimated_latency(synth_ep);

            if !exceeds_candidate_caps(total_cost, total_tokens, total_latency, request) {
                let hash_input = json!({
                    "schema_version": ADAPTIVE_CANDIDATE_SCHEMA_VERSION,
                    "candidate_kind": "fusion",
                    "member_endpoint_ids": member_ids,
                    "panel_endpoint_ids": panel_eps.iter().map(|ep| ep.endpoint_id.clone()).collect::<Vec<_>>(),
                    "judge_endpoint_id": judge_ep.endpoint_id,
                    "synthesizer_endpoint_id": synth_ep.endpoint_id,
                    "task_class": request.task_class,
                    "objective": request.objective,
                    "registry_snapshot_hash": registry_snapshot_hash,
                });
                let hash = stable_hash(&hash_input);
                candidates.push(AdaptiveCandidate {
                    schema_version: ADAPTIVE_CANDIDATE_SCHEMA_VERSION.to_string(),
                    candidate_id: format!("fusion-{}", &hash[..16]),
                    candidate_hash: hash,
                    task_class: request.task_class.clone(),
                    objective: request.objective.clone(),
                    candidate_kind: CandidateKind::Fusion,
                    member_endpoint_ids: member_ids,
                    endpoint_bindings: all_bindings,
                    estimated_cost_usd: total_cost,
                    estimated_input_tokens: total_tokens,
                    estimated_output_tokens: 0,
                    estimated_latency_ms: total_latency,
                    required_capabilities: request.required_capabilities.clone(),
                    registry_snapshot_hash: registry_snapshot_hash.to_string(),
                });
            }
        }

        candidates.truncate(config.max_candidates);

        AdaptiveCandidateSet {
            schema_version: ADAPTIVE_CANDIDATE_SET_SCHEMA_VERSION.to_string(),
            request: request.clone(),
            candidates,
            rejected_endpoints: rejected,
            registry_snapshot_hash: registry_snapshot_hash.to_string(),
        }
    }
}

fn exceeds_candidate_caps(
    total_cost: f64,
    total_tokens: u64,
    total_latency: u64,
    request: &CandidateGenerationRequest,
) -> bool {
    if total_cost > request.max_estimated_cost_usd {
        return true;
    }
    if request.max_estimated_tokens > 0 && total_tokens > request.max_estimated_tokens {
        return true;
    }
    if request.max_estimated_latency_ms > 0 && total_latency > request.max_estimated_latency_ms {
        return true;
    }
    false
}

fn filter_endpoints(
    request: &CandidateGenerationRequest,
    endpoints: &[ModelEndpointSpec],
    config: &AdaptiveCandidateConfig,
) -> (Vec<ModelEndpointSpec>, Vec<EndpointRejection>) {
    let mut eligible = Vec::new();
    let mut rejected = Vec::new();

    let mut id_counts = BTreeMap::new();
    for ep in endpoints {
        *id_counts.entry(ep.endpoint_id.clone()).or_insert(0usize) += 1;
    }
    let duplicate_ids: BTreeSet<String> = id_counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(id, _)| id)
        .collect();

    for ep in endpoints {
        let mut reasons = Vec::new();

        if duplicate_ids.contains(&ep.endpoint_id) {
            let reason = "duplicate_endpoint_id".to_string();
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }

        if !ep.enabled {
            reasons.push("endpoint_disabled".to_string());
        }

        if ep.endpoint_id.trim().is_empty()
            || ep.provider_id.trim().is_empty()
            || ep.model_id.trim().is_empty()
        {
            reasons.push("invalid_endpoint_identity".to_string());
        }

        if ep.health.status == "unavailable" {
            reasons.push("endpoint_unavailable".to_string());
        }

        if !ep.health.score.is_finite() || !(0.0..=1.0).contains(&ep.health.score) {
            reasons.push("invalid_health_score".to_string());
        }

        for capability in &request.required_capabilities {
            if !ep.capabilities.iter().any(|c| c == capability) {
                reasons.push(format!("missing_capability:{capability}"));
            }
        }

        if !request.max_estimated_cost_usd.is_finite() || request.max_estimated_cost_usd < 0.0 {
            reasons.push("invalid_request_budget".to_string());
        } else {
            let cost = endpoint_estimated_cost(ep, config);
            if cost > request.max_estimated_cost_usd {
                reasons.push("estimated_cost_exceeds_budget".to_string());
            }
        }

        let total_tokens = config.estimated_input_tokens + config.estimated_output_tokens;
        if request.max_estimated_tokens > 0 && total_tokens > request.max_estimated_tokens {
            reasons.push("estimated_tokens_exceed_max".to_string());
        }

        let latency = endpoint_estimated_latency(ep);
        if request.max_estimated_latency_ms > 0 && latency > request.max_estimated_latency_ms {
            reasons.push("estimated_latency_exceeds_max".to_string());
        }

        if contains_sensitive_patterns(&serde_json::to_string(ep).unwrap_or_default()) {
            reasons.push("sensitive_pattern_detected".to_string());
        }

        if reasons.is_empty() {
            eligible.push(ep.clone());
        } else {
            rejected.push(EndpointRejection {
                endpoint_id: ep.endpoint_id.clone(),
                reasons,
            });
        }
    }

    (eligible, rejected)
}

fn endpoint_estimated_cost(ep: &ModelEndpointSpec, config: &AdaptiveCandidateConfig) -> f64 {
    let input_cost =
        (config.estimated_input_tokens as f64 / 1000.0) * ep.pricing.input_cost_per_1k_usd;
    let output_cost =
        (config.estimated_output_tokens as f64 / 1000.0) * ep.pricing.output_cost_per_1k_usd;
    input_cost + output_cost
}

fn endpoint_estimated_latency(ep: &ModelEndpointSpec) -> u64 {
    let base = DEFAULT_LATENCY_MS;
    let health_factor = 1.0 - ep.health.score;
    let adjusted = (base as f64 * (1.0 + health_factor)).round() as u64;
    adjusted.max(100)
}

fn endpoint_utility(
    ep: &ModelEndpointSpec,
    request: &CandidateGenerationRequest,
    config: &AdaptiveCandidateConfig,
) -> f64 {
    let cost = endpoint_estimated_cost(ep, config);
    let quality_weight = if request.objective.eq_ignore_ascii_case("quality") {
        0.65
    } else {
        0.25
    };
    let cost_weight = if request.objective.eq_ignore_ascii_case("quality") {
        0.05
    } else {
        0.35
    };

    let quality_score = ep.health.score;
    let cost_score = if cost <= 0.0 {
        1.0
    } else {
        (1.0 / (1.0 + cost)).clamp(0.0, 1.0)
    };

    quality_score * quality_weight + cost_score * cost_weight
}
