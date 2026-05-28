use crate::dispatch_decision::{RejectedCandidate, ShadowRoute, MODEL_TIERS};
use crate::model_selector::ModelSelector;
use crate::task_analyzer::TaskAnalysis;

use super::cost_of_pass_router::CostOfPassRouter;
use super::history_store::RoutingHistoryStore;
use super::promotion_gate::{PromotionGate, RoutingObservationStore};
use super::schemas::{make_task_group, RoutingSelection};

pub struct DynamicTierSelector {
    cost_router: CostOfPassRouter,
    promotion_gate: PromotionGate,
}

impl DynamicTierSelector {
    pub fn new(cost_router: CostOfPassRouter, promotion_gate: PromotionGate) -> Self {
        Self {
            cost_router,
            promotion_gate,
        }
    }

    pub fn select(
        &self,
        analysis: &TaskAnalysis,
        history_store: &mut RoutingHistoryStore,
        observation_store: &RoutingObservationStore,
        static_selector: &ModelSelector,
    ) -> RoutingSelection {
        let task_group = make_task_group(&analysis.task_domain, &analysis.task_intent);

        if self
            .cost_router
            .can_route_adaptively(history_store, &task_group)
        {
            if let Some((best_tier, _cop)) = self
                .cost_router
                .best_tier_for_task_group(history_store, &task_group)
            {
                let verdict = self.promotion_gate.evaluate(
                    observation_store,
                    &task_group,
                    &best_tier,
                    "balanced_worker",
                );
                if verdict.verdict == "promote" {
                    let mut rejected: Vec<serde_json::Value> = Vec::new();
                    let reasons: Vec<String> = vec!["adaptive_routing:cost_of_pass".to_string()];

                    let (selected_tier, rejected_typed, reasons_out) =
                        apply_hard_constraints(analysis, &best_tier, rejected, reasons);
                    rejected = rejected_typed;
                    let reasons = reasons_out;

                    let fallback_tier = fallback_tier(&selected_tier);
                    let shadow_routes = build_shadow_routes(&selected_tier, &fallback_tier);

                    return RoutingSelection {
                        selected_tier,
                        selected_profile_id: None,
                        fallback_tier,
                        fallback_profile_id: None,
                        shadow_routes,
                        rejected_candidates: rejected,
                        routing_reason: reasons.join("; "),
                        routing_mode: "adaptive".to_string(),
                        routing_experiment_id: None,
                    };
                }
            }
        }

        // Cold-start fallback: delegate to static selector
        let static_result = static_selector.select(analysis);
        RoutingSelection {
            selected_tier: static_result.selected_tier.clone(),
            selected_profile_id: static_result.selected_profile_id.clone(),
            fallback_tier: static_result.fallback_tier.clone(),
            fallback_profile_id: static_result.fallback_profile_id.clone(),
            shadow_routes: static_result
                .shadow_routes
                .iter()
                .map(|r| serde_json::to_value(r).unwrap_or_default())
                .collect(),
            rejected_candidates: static_result
                .rejected_candidates
                .iter()
                .map(|r| serde_json::to_value(r).unwrap_or_default())
                .collect(),
            routing_reason: format!(
                "adaptive_cold_start_fallback; {}",
                static_result.routing_reason
            ),
            routing_mode: "static".to_string(),
            routing_experiment_id: None,
        }
    }
}

fn apply_hard_constraints(
    analysis: &TaskAnalysis,
    selected_tier: &str,
    mut rejected: Vec<serde_json::Value>,
    mut reasons: Vec<String>,
) -> (String, Vec<serde_json::Value>, Vec<String>) {
    let mut tier = selected_tier.to_string();

    if analysis.confidence_label == "low" {
        reasons.push("low_confidence_escalation".to_string());
        rejected.push(
            serde_json::to_value(&RejectedCandidate {
                tier: tier.clone(),
                profile_id: None,
                reason: "low confidence".to_string(),
                constraint_failed: Some("confidence_threshold".to_string()),
                estimated_cost: None,
            })
            .unwrap_or_default(),
        );
        tier = "strong_planner".to_string();
    }

    if analysis.risk_level == "critical" && tier != "strong_planner" && tier != "advisor" {
        rejected.push(
            serde_json::to_value(&RejectedCandidate {
                tier: tier.clone(),
                profile_id: None,
                reason: "critical risk requires stronger tier".to_string(),
                constraint_failed: Some("risk_level".to_string()),
                estimated_cost: None,
            })
            .unwrap_or_default(),
        );
        tier = "strong_planner".to_string();
        reasons.push("critical_risk_override".to_string());
    }

    if analysis.context_budget_estimate < 500 {
        rejected.push(
            serde_json::to_value(&RejectedCandidate {
                tier: "strong_planner".to_string(),
                profile_id: None,
                reason: "budget too low for strong_planner".to_string(),
                constraint_failed: Some("budget_threshold".to_string()),
                estimated_cost: None,
            })
            .unwrap_or_default(),
        );
        reasons.push("budget_constrained".to_string());
    }

    (tier, rejected, reasons)
}

fn fallback_tier(selected: &str) -> String {
    let tier_order: Vec<&str> = MODEL_TIERS.to_vec();
    let idx = tier_order.iter().position(|t| *t == selected).unwrap_or(1);
    if idx < tier_order.len() - 1 {
        tier_order[idx + 1].to_string()
    } else {
        tier_order.last().unwrap().to_string()
    }
}

fn build_shadow_routes(selected: &str, fallback: &str) -> Vec<serde_json::Value> {
    let mut routes: Vec<serde_json::Value> = Vec::new();
    if fallback != selected {
        routes.push(
            serde_json::to_value(&ShadowRoute {
                tier: fallback.to_string(),
                profile_id: None,
                reason: "fallback option".to_string(),
                admission_scope: "diagnostic".to_string(),
                estimated_cost: None,
                expected_tradeoff: "lower cost, potentially lower quality".to_string(),
            })
            .unwrap_or_default(),
        );
    }
    if selected != "cheap_executor" {
        routes.push(
            serde_json::to_value(&ShadowRoute {
                tier: "cheap_executor".to_string(),
                profile_id: None,
                reason: "cost-optimized alternative".to_string(),
                admission_scope: "diagnostic".to_string(),
                estimated_cost: None,
                expected_tradeoff: "lowest cost, adequate for simple tasks".to_string(),
            })
            .unwrap_or_default(),
        );
    }
    if routes.is_empty() {
        routes.push(
            serde_json::to_value(&ShadowRoute {
                tier: selected.to_string(),
                profile_id: None,
                reason: "self-diagnostic (no cheaper alternative)".to_string(),
                admission_scope: "diagnostic".to_string(),
                estimated_cost: None,
                expected_tradeoff: "same tier, diagnostic comparison".to_string(),
            })
            .unwrap_or_default(),
        );
    }
    routes
}
