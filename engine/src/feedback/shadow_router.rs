use serde::{Deserialize, Serialize};

use super::run_trace_recorder::RunTrace;

pub const SHADOW_ROUTE_SCHEMA_VERSION: &str = "shadow_route.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowRouteOutput {
    pub schema_version: String,
    pub shadow_route_id: String,
    pub dispatch_id: String,
    pub actual_tier: String,
    pub candidate_tier: String,
    pub candidate_profile: Option<String>,
    pub reason: String,
    pub estimated_cost: Option<f64>,
    pub expected_tradeoff: String,
    pub influence_selected_tier: bool,
    pub influence_executor_type: bool,
    pub influence_retry_path: bool,
    pub influence_routing_policy: bool,
    pub source: String,
}

pub struct ShadowRouter;

impl ShadowRouter {
    pub fn generate_shadow_route(
        trace: &RunTrace,
        candidate_tier: &str,
        reason: &str,
        tradeoff: &str,
    ) -> ShadowRouteOutput {
        ShadowRouteOutput {
            schema_version: SHADOW_ROUTE_SCHEMA_VERSION.to_string(),
            shadow_route_id: format!("shadow-{}-{}", trace.dispatch_id, candidate_tier),
            dispatch_id: trace.dispatch_id.clone(),
            actual_tier: trace.selected_tier.clone(),
            candidate_tier: candidate_tier.to_string(),
            candidate_profile: None,
            reason: reason.to_string(),
            estimated_cost: estimate_tier_cost_usd(
                candidate_tier,
                &trace.selected_tier,
                trace.estimated_cost_usd,
                trace.reserved_cost,
            ),
            expected_tradeoff: tradeoff.to_string(),
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            source: "shadow_only".to_string(),
        }
    }

    pub fn generate_for_policy(trace: &RunTrace, candidate_tier: &str) -> ShadowRouteOutput {
        if candidate_tier == trace.selected_tier {
            Self::generate_shadow_route(trace, candidate_tier, "same_as_actual", "no change")
        } else if tier_order(candidate_tier) < tier_order(&trace.selected_tier) {
            Self::generate_shadow_route(
                trace,
                candidate_tier,
                "cost_optimization",
                "lower cost, potentially lower quality",
            )
        } else {
            Self::generate_shadow_route(
                trace,
                candidate_tier,
                "quality_upgrade",
                "higher quality, higher cost",
            )
        }
    }
}

pub fn tier_cost_multiplier(tier: &str) -> f64 {
    match tier {
        "cheap_executor" => 0.5,
        "balanced_worker" => 1.0,
        "strong_planner" => 1.5,
        "claude_code_cli" => 2.0,
        "codex_cli" => 1.2,
        _ => 1.0,
    }
}

fn estimate_tier_cost_usd(
    candidate_tier: &str,
    actual_tier: &str,
    actual_cost: Option<f64>,
    reserved_cost: f64,
) -> Option<f64> {
    let base = actual_cost.unwrap_or(reserved_cost);
    if base <= 0.0 {
        return None;
    }
    let actual_mult = tier_cost_multiplier(actual_tier);
    let cand_mult = tier_cost_multiplier(candidate_tier);
    Some(cand_mult / actual_mult * base)
}

fn tier_order(tier: &str) -> u8 {
    match tier {
        "cheap_executor" => 0,
        "codex_cli" => 1,
        "balanced_worker" => 2,
        "strong_planner" => 3,
        "claude_code_cli" => 4,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_trace() -> RunTrace {
        RunTrace {
            schema_version: "feedback_trace.v1".to_string(),
            trace_id: "trace-001".to_string(),
            dispatch_id: "disp-001".to_string(),
            history_id: None,
            created_at: None,
            task_class: "codegen".to_string(),
            task_domain: None,
            task_intent: None,
            selected_tier: "balanced_worker".to_string(),
            selected_profile: None,
            routing_policy: None,
            complexity_score: None,
            constraints: vec![],
            human_review_flag: false,
            retry_policy: None,
            shadow_routes: vec![],
            executor_type: "local".to_string(),
            execution_status: None,
            latency_ms: Some(1000),
            input_tokens: None,
            output_tokens: None,
            estimated_cost_usd: Some(0.05),
            reserved_cost: 0.05,
            total_cost: 0.0,
            retry_count: 0,
            evaluation_status: "pending".to_string(),
            final_status: "pending".to_string(),
            success: true,
            failure_domain: None,
            analysis: json!(null),
            decision: json!(null),
            execution: json!(null),
            evaluation: json!(null),
        }
    }

    #[test]
    fn all_influence_flags_false() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "cheap_executor");
        assert!(!route.influence_selected_tier);
        assert!(!route.influence_executor_type);
        assert!(!route.influence_retry_path);
        assert!(!route.influence_routing_policy);
    }

    #[test]
    fn source_is_shadow_only() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "strong_planner");
        assert_eq!(route.source, "shadow_only");
    }

    #[test]
    fn schema_version() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "cheap_executor");
        assert_eq!(route.schema_version, SHADOW_ROUTE_SCHEMA_VERSION);
    }

    #[test]
    fn same_tier() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "balanced_worker");
        assert_eq!(route.reason, "same_as_actual");
        assert_eq!(route.expected_tradeoff, "no change");
        assert_eq!(route.candidate_tier, "balanced_worker");
    }

    #[test]
    fn cheaper_tier() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "cheap_executor");
        assert_eq!(route.reason, "cost_optimization");
        assert_eq!(
            route.expected_tradeoff,
            "lower cost, potentially lower quality"
        );
    }

    #[test]
    fn stronger_tier() {
        let trace = minimal_trace();
        let route = ShadowRouter::generate_for_policy(&trace, "strong_planner");
        assert_eq!(route.reason, "quality_upgrade");
        assert_eq!(route.expected_tradeoff, "higher quality, higher cost");
    }

    #[test]
    fn cost_estimation() {
        let trace = minimal_trace();
        let route =
            ShadowRouter::generate_shadow_route(&trace, "cheap_executor", "test", "test tradeoff");
        // cheap_executor=0.5, balanced_worker=1.0, base=0.05
        // 0.5 / 1.0 * 0.05 = 0.025
        let cost = route.estimated_cost.unwrap();
        assert!((cost - 0.025).abs() < 1e-10);
    }

    #[test]
    fn determinism() {
        let trace = minimal_trace();
        let r1 = ShadowRouter::generate_for_policy(&trace, "strong_planner");
        let r2 = ShadowRouter::generate_for_policy(&trace, "strong_planner");
        assert_eq!(r1.shadow_route_id, r2.shadow_route_id);
        assert_eq!(r1.reason, r2.reason);
        assert_eq!(r1.estimated_cost, r2.estimated_cost);
        assert_eq!(r1.schema_version, r2.schema_version);
        assert_eq!(r1.source, r2.source);
    }
}
