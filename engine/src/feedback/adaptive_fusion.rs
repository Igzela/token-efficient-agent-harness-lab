use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const ADAPTIVE_FUSION_PLAN_SCHEMA_VERSION: &str = "adaptive_fusion_plan.v1";
const MAX_PANEL_SIZE: usize = 3;
const MIN_FUSION_ENDPOINTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveProfile {
    Efficient,
    Quality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliberationMode {
    Auto,
    Single,
    Fusion,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveWeights {
    pub quality: f64,
    pub success: f64,
    pub cost_efficiency: f64,
    pub latency_efficiency: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEndpointObservation {
    pub endpoint_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub enabled: bool,
    pub capabilities: Vec<String>,
    pub quality_score: f64,
    pub success_score: f64,
    pub cost_efficiency_score: f64,
    pub latency_efficiency_score: f64,
    pub estimated_cost_usd: f64,
    pub sample_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortfolioRequest {
    pub objective: ObjectiveProfile,
    pub deliberation: DeliberationMode,
    pub required_capabilities: Vec<String>,
    pub complexity_score: f64,
    pub risk_level: String,
    pub max_estimated_cost_usd: f64,
    pub min_quality_score: f64,
    pub max_panel_size: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EndpointScorecard {
    pub endpoint_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub eligible: bool,
    pub utility_score: Option<f64>,
    pub sample_count: u64,
    pub estimated_cost_usd: f64,
    pub quality_score: f64,
    pub success_score: f64,
    pub cost_efficiency_score: f64,
    pub latency_efficiency_score: f64,
    pub rejected_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveFusionPlan {
    pub schema_version: String,
    pub objective: ObjectiveProfile,
    pub weights: ObjectiveWeights,
    pub mode: String,
    pub primary_endpoint_id: Option<String>,
    pub panel_endpoint_ids: Vec<String>,
    pub judge_endpoint_id: Option<String>,
    pub synthesizer_endpoint_id: Option<String>,
    pub estimated_plan_cost_usd: f64,
    pub scorecards: Vec<EndpointScorecard>,
    pub reasons: Vec<String>,
    pub shadow_only: bool,
    pub influence_selected_tier: bool,
    pub influence_executor_type: bool,
    pub influence_retry_path: bool,
    pub influence_routing_policy: bool,
}

pub struct AdaptiveFusionPlanner;

impl AdaptiveFusionPlanner {
    pub fn plan(
        request: &PortfolioRequest,
        endpoints: &[ModelEndpointObservation],
    ) -> AdaptiveFusionPlan {
        let endpoint_counts =
            endpoints
                .iter()
                .fold(BTreeMap::<&str, usize>::new(), |mut counts, endpoint| {
                    *counts.entry(endpoint.endpoint_id.as_str()).or_default() += 1;
                    counts
                });
        let mut scorecards: Vec<EndpointScorecard> = endpoints
            .iter()
            .map(|endpoint| {
                score_endpoint(
                    request,
                    endpoint,
                    endpoint_counts
                        .get(endpoint.endpoint_id.as_str())
                        .copied()
                        .unwrap_or_default()
                        > 1,
                )
            })
            .collect();
        scorecards.sort_by(|left, right| left.endpoint_id.cmp(&right.endpoint_id));

        let mut eligible: Vec<&ModelEndpointObservation> = endpoints
            .iter()
            .filter(|endpoint| {
                scorecards
                    .iter()
                    .find(|scorecard| scorecard.endpoint_id == endpoint.endpoint_id)
                    .is_some_and(|scorecard| scorecard.eligible)
            })
            .collect();
        eligible.sort_by(|left, right| rank_endpoints(request.objective, left, right));

        let Some(primary) = eligible.first().copied() else {
            return base_plan(
                request,
                "unavailable",
                scorecards,
                vec!["no_eligible_model_endpoint".to_string()],
            );
        };

        let wants_fusion = match request.deliberation {
            DeliberationMode::Single => false,
            DeliberationMode::Fusion => true,
            DeliberationMode::Auto => {
                request.objective == ObjectiveProfile::Quality
                    && (normalized(request.complexity_score) >= 0.7
                        || high_impact_risk(&request.risk_level))
            }
        };

        if !wants_fusion || eligible.len() < MIN_FUSION_ENDPOINTS {
            let mut reasons = vec!["single_endpoint_plan".to_string()];
            if wants_fusion {
                reasons.push("fusion_requires_three_eligible_endpoints".to_string());
            }
            return single_plan(request, primary, scorecards, reasons);
        }

        let panel_limit = request.max_panel_size.clamp(2, MAX_PANEL_SIZE);
        let mut panel: Vec<&ModelEndpointObservation> =
            eligible.iter().take(panel_limit).copied().collect();
        let judge = eligible
            .iter()
            .copied()
            .min_by(|left, right| rank_judges(left, right))
            .expect("eligible endpoints are non-empty");

        while panel.len() > 2
            && fusion_cost(&panel, judge, primary) > request.max_estimated_cost_usd
        {
            panel.pop();
        }

        if fusion_cost(&panel, judge, primary) > request.max_estimated_cost_usd {
            return single_plan(
                request,
                primary,
                scorecards,
                vec![
                    "fusion_plan_exceeds_budget".to_string(),
                    "single_endpoint_fallback".to_string(),
                ],
            );
        }

        let trigger_reason = match request.deliberation {
            DeliberationMode::Fusion => "explicit_fusion_requested",
            DeliberationMode::Auto => "quality_auto_deliberation_threshold",
            DeliberationMode::Single => unreachable!("single mode cannot request fusion"),
        };
        AdaptiveFusionPlan {
            schema_version: ADAPTIVE_FUSION_PLAN_SCHEMA_VERSION.to_string(),
            objective: request.objective,
            weights: objective_weights(request.objective),
            mode: "fusion".to_string(),
            primary_endpoint_id: Some(primary.endpoint_id.clone()),
            panel_endpoint_ids: panel
                .iter()
                .map(|endpoint| endpoint.endpoint_id.clone())
                .collect(),
            judge_endpoint_id: Some(judge.endpoint_id.clone()),
            synthesizer_endpoint_id: Some(primary.endpoint_id.clone()),
            estimated_plan_cost_usd: fusion_cost(&panel, judge, primary),
            scorecards,
            reasons: vec![
                trigger_reason.to_string(),
                "panel_judge_synthesizer_plan".to_string(),
            ],
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
        }
    }
}

fn score_endpoint(
    request: &PortfolioRequest,
    endpoint: &ModelEndpointObservation,
    duplicate_endpoint_id: bool,
) -> EndpointScorecard {
    let mut rejected_reasons = Vec::new();
    if !endpoint.enabled {
        rejected_reasons.push("endpoint_disabled".to_string());
    }
    if endpoint.endpoint_id.trim().is_empty()
        || endpoint.provider_id.trim().is_empty()
        || endpoint.model_id.trim().is_empty()
    {
        rejected_reasons.push("invalid_endpoint_identity".to_string());
    }
    if duplicate_endpoint_id {
        rejected_reasons.push("duplicate_endpoint_id".to_string());
    }
    if !request.max_estimated_cost_usd.is_finite() || request.max_estimated_cost_usd < 0.0 {
        rejected_reasons.push("invalid_request_budget".to_string());
    }
    if !request.min_quality_score.is_finite() || !(0.0..=1.0).contains(&request.min_quality_score) {
        rejected_reasons.push("invalid_min_quality_score".to_string());
    } else if endpoint.quality_score < request.min_quality_score {
        rejected_reasons.push("quality_below_minimum".to_string());
    }
    for capability in &request.required_capabilities {
        if !endpoint
            .capabilities
            .iter()
            .any(|candidate| candidate == capability)
        {
            rejected_reasons.push(format!("missing_capability:{capability}"));
        }
    }
    if ![
        endpoint.quality_score,
        endpoint.success_score,
        endpoint.cost_efficiency_score,
        endpoint.latency_efficiency_score,
    ]
    .iter()
    .all(|score| score.is_finite() && (0.0..=1.0).contains(score))
    {
        rejected_reasons.push("invalid_normalized_score".to_string());
    }
    if !endpoint.estimated_cost_usd.is_finite() || endpoint.estimated_cost_usd < 0.0 {
        rejected_reasons.push("invalid_estimated_cost".to_string());
    } else if endpoint.estimated_cost_usd > request.max_estimated_cost_usd {
        rejected_reasons.push("estimated_cost_exceeds_budget".to_string());
    }

    let eligible = rejected_reasons.is_empty();
    EndpointScorecard {
        endpoint_id: endpoint.endpoint_id.clone(),
        provider_id: endpoint.provider_id.clone(),
        model_id: endpoint.model_id.clone(),
        eligible,
        utility_score: eligible.then(|| utility(request.objective, endpoint)),
        sample_count: endpoint.sample_count,
        estimated_cost_usd: endpoint.estimated_cost_usd,
        quality_score: endpoint.quality_score,
        success_score: endpoint.success_score,
        cost_efficiency_score: endpoint.cost_efficiency_score,
        latency_efficiency_score: endpoint.latency_efficiency_score,
        rejected_reasons,
    }
}

fn utility(objective: ObjectiveProfile, endpoint: &ModelEndpointObservation) -> f64 {
    let weights = objective_weights(objective);
    endpoint.quality_score * weights.quality
        + endpoint.success_score * weights.success
        + endpoint.cost_efficiency_score * weights.cost_efficiency
        + endpoint.latency_efficiency_score * weights.latency_efficiency
}

pub fn objective_weights(objective: ObjectiveProfile) -> ObjectiveWeights {
    match objective {
        ObjectiveProfile::Efficient => ObjectiveWeights {
            quality: 0.25,
            success: 0.25,
            cost_efficiency: 0.35,
            latency_efficiency: 0.15,
        },
        ObjectiveProfile::Quality => ObjectiveWeights {
            quality: 0.65,
            success: 0.25,
            cost_efficiency: 0.05,
            latency_efficiency: 0.05,
        },
    }
}

fn rank_endpoints(
    objective: ObjectiveProfile,
    left: &ModelEndpointObservation,
    right: &ModelEndpointObservation,
) -> Ordering {
    utility(objective, right)
        .partial_cmp(&utility(objective, left))
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.endpoint_id.cmp(&right.endpoint_id))
}

fn rank_judges(left: &ModelEndpointObservation, right: &ModelEndpointObservation) -> Ordering {
    let left_score = left.quality_score * 0.7 + left.success_score * 0.3;
    let right_score = right.quality_score * 0.7 + right.success_score * 0.3;
    right_score
        .partial_cmp(&left_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.endpoint_id.cmp(&right.endpoint_id))
}

fn fusion_cost(
    panel: &[&ModelEndpointObservation],
    judge: &ModelEndpointObservation,
    synthesizer: &ModelEndpointObservation,
) -> f64 {
    panel
        .iter()
        .map(|endpoint| endpoint.estimated_cost_usd)
        .sum::<f64>()
        + judge.estimated_cost_usd
        + synthesizer.estimated_cost_usd
}

fn single_plan(
    request: &PortfolioRequest,
    primary: &ModelEndpointObservation,
    scorecards: Vec<EndpointScorecard>,
    reasons: Vec<String>,
) -> AdaptiveFusionPlan {
    let mut plan = base_plan(request, "single", scorecards, reasons);
    plan.primary_endpoint_id = Some(primary.endpoint_id.clone());
    plan.estimated_plan_cost_usd = primary.estimated_cost_usd;
    plan
}

fn base_plan(
    request: &PortfolioRequest,
    mode: &str,
    scorecards: Vec<EndpointScorecard>,
    reasons: Vec<String>,
) -> AdaptiveFusionPlan {
    AdaptiveFusionPlan {
        schema_version: ADAPTIVE_FUSION_PLAN_SCHEMA_VERSION.to_string(),
        objective: request.objective,
        weights: objective_weights(request.objective),
        mode: mode.to_string(),
        primary_endpoint_id: None,
        panel_endpoint_ids: Vec::new(),
        judge_endpoint_id: None,
        synthesizer_endpoint_id: None,
        estimated_plan_cost_usd: 0.0,
        scorecards,
        reasons,
        shadow_only: true,
        influence_selected_tier: false,
        influence_executor_type: false,
        influence_retry_path: false,
        influence_routing_policy: false,
    }
}

fn normalized(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.5
    }
}

fn high_impact_risk(value: &str) -> bool {
    value.eq_ignore_ascii_case("high") || value.eq_ignore_ascii_case("critical")
}
