use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use super::dispatch_decision::{RejectedCandidate, ShadowRoute, MODEL_TIERS};
use super::task_analyzer::TaskAnalysis;

static DEFAULT_TIER_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("code_generate", "balanced_worker");
    m.insert("code_review", "balanced_worker");
    m.insert("code_debug", "strong_planner");
    m.insert("code_refactor", "balanced_worker");
    m.insert("docs_summarize", "cheap_executor");
    m.insert("docs_generate", "cheap_executor");
    m.insert("docs_review", "cheap_executor");
    m.insert("docs_explain", "cheap_executor");
    m.insert("config_review", "cheap_executor");
    m.insert("config_generate", "balanced_worker");
    m.insert("infra_review", "balanced_worker");
    m.insert("infra_plan", "strong_planner");
    m.insert("math_generate", "strong_planner");
    m.insert("math_explain", "balanced_worker");
    m.insert("architecture_plan", "strong_planner");
    m.insert("architecture_design", "strong_planner");
    m.insert("repo_ops_review", "cheap_executor");
    m.insert("repo_ops_generate", "balanced_worker");
    m.insert("governance_audit", "verifier");
    m.insert("governance_review", "verifier");
    m.insert("other_classify", "cheap_executor");
    m
});

static HIGH_RISK_OVERRIDES: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("cheap_executor", "balanced_worker");
    m.insert("balanced_worker", "strong_planner");
    m
});

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DispatchRoutingPolicy {
    pub policy_id: String,
    pub tier_map: HashMap<String, String>,
    pub description: String,
}

impl DispatchRoutingPolicy {
    pub fn select_tier(&self, analysis: &TaskAnalysis) -> String {
        let key = format!("{}_{}", analysis.task_domain, analysis.task_intent);
        let mut tier = self
            .tier_map
            .get(&key)
            .cloned()
            .unwrap_or_else(|| "balanced_worker".to_string());
        if analysis.risk_level == "critical" || analysis.risk_level == "high" {
            if let Some(override_tier) = HIGH_RISK_OVERRIDES.get(tier.as_str()) {
                tier = override_tier.to_string();
            }
        }
        tier
    }
}

impl Default for DispatchRoutingPolicy {
    fn default() -> Self {
        Self {
            policy_id: "default_v1".to_string(),
            tier_map: DEFAULT_TIER_MAP
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            description: "Default routing policy for Phase 2".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelSelection {
    pub selected_tier: String,
    pub selected_profile_id: Option<String>,
    pub fallback_tier: String,
    pub fallback_profile_id: Option<String>,
    pub shadow_routes: Vec<ShadowRoute>,
    pub rejected_candidates: Vec<RejectedCandidate>,
    pub routing_reason: String,
}

pub struct ModelSelector {
    policy: DispatchRoutingPolicy,
}

impl ModelSelector {
    pub fn new(policy: Option<DispatchRoutingPolicy>) -> Self {
        Self {
            policy: policy.unwrap_or_default(),
        }
    }

    pub fn select(&self, analysis: &TaskAnalysis) -> ModelSelection {
        let mut selected_tier = self.policy.select_tier(analysis);
        let mut rejected: Vec<RejectedCandidate> = vec![];
        let mut reasons = vec![format!(
            "policy_map:{}_{}",
            analysis.task_domain, analysis.task_intent
        )];

        if analysis.confidence_label == "low" {
            reasons.push("low_confidence_escalation".to_string());
            rejected.push(RejectedCandidate {
                tier: selected_tier.clone(),
                profile_id: None,
                reason: "low confidence".to_string(),
                constraint_failed: Some("confidence_threshold".to_string()),
                estimated_cost: None,
            });
            selected_tier = "strong_planner".to_string();
        }

        if analysis.risk_level == "critical"
            && selected_tier != "strong_planner"
            && selected_tier != "advisor"
        {
            rejected.push(RejectedCandidate {
                tier: selected_tier.clone(),
                profile_id: None,
                reason: "critical risk requires stronger tier".to_string(),
                constraint_failed: Some("risk_level".to_string()),
                estimated_cost: None,
            });
            selected_tier = "strong_planner".to_string();
            reasons.push("critical_risk_override".to_string());
        }

        if analysis.context_budget_estimate < 500 {
            rejected.push(RejectedCandidate {
                tier: "strong_planner".to_string(),
                profile_id: None,
                reason: "budget too low for strong_planner".to_string(),
                constraint_failed: Some("budget_threshold".to_string()),
                estimated_cost: None,
            });
            reasons.push("budget_constrained".to_string());
        }

        let fallback_tier = self.fallback_tier(&selected_tier);
        let shadow_routes = self.build_shadow_routes(analysis, &selected_tier, &fallback_tier);
        let routing_reason = reasons.join("; ");

        ModelSelection {
            selected_tier,
            selected_profile_id: None,
            fallback_tier,
            fallback_profile_id: None,
            shadow_routes,
            rejected_candidates: rejected,
            routing_reason,
        }
    }

    fn fallback_tier(&self, selected: &str) -> String {
        let idx = MODEL_TIERS.iter().position(|&t| t == selected).unwrap_or(1);
        if idx < MODEL_TIERS.len() - 1 {
            MODEL_TIERS[idx + 1].to_string()
        } else {
            MODEL_TIERS[MODEL_TIERS.len() - 1].to_string()
        }
    }

    fn build_shadow_routes(
        &self,
        _analysis: &TaskAnalysis,
        selected: &str,
        fallback: &str,
    ) -> Vec<ShadowRoute> {
        let mut routes = vec![];

        if fallback != selected {
            routes.push(ShadowRoute {
                tier: fallback.to_string(),
                profile_id: None,
                reason: "fallback option".to_string(),
                admission_scope: "diagnostic".to_string(),
                estimated_cost: None,
                expected_tradeoff: "lower cost, potentially lower quality".to_string(),
            });
        }

        if selected != "cheap_executor" {
            routes.push(ShadowRoute {
                tier: "cheap_executor".to_string(),
                profile_id: None,
                reason: "cost-optimized alternative".to_string(),
                admission_scope: "diagnostic".to_string(),
                estimated_cost: None,
                expected_tradeoff: "lowest cost, adequate for simple tasks".to_string(),
            });
        }

        if routes.is_empty() {
            routes.push(ShadowRoute {
                tier: selected.to_string(),
                profile_id: None,
                reason: "self-diagnostic (no cheaper alternative)".to_string(),
                admission_scope: "diagnostic".to_string(),
                estimated_cost: None,
                expected_tradeoff: "same tier, diagnostic comparison".to_string(),
            });
        }

        routes
    }
}
