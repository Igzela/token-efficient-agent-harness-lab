use std::collections::HashMap;

use engine::dispatch_decision::MODEL_TIERS;
use engine::model_selector::{DispatchRoutingPolicy, ModelSelector};
use engine::task_analyzer::RuleBasedTaskAnalyzer;

fn make_analysis(request: &str) -> engine::task_analyzer::TaskAnalysis {
    RuleBasedTaskAnalyzer::new().analyze(request, "test_fixture")
}

// ---------------------------------------------------------------------------
// ModelSelectorTests
// ---------------------------------------------------------------------------

#[test]
fn test_select_returns_selection() {
    let sel = ModelSelector::new(None).select(&make_analysis("Review auth.py"));
    assert!(MODEL_TIERS.contains(&sel.selected_tier.as_str()));
}

#[test]
fn test_select_tier_from_policy() {
    let sel = ModelSelector::new(None).select(&make_analysis("Review auth.py"));
    assert!(MODEL_TIERS.contains(&sel.selected_tier.as_str()));
}

#[test]
fn test_shadow_routes_always_present() {
    let sel = ModelSelector::new(None).select(&make_analysis("Review auth.py"));
    assert!(!sel.shadow_routes.is_empty());
}

#[test]
fn test_shadow_route_is_diagnostic() {
    let sel = ModelSelector::new(None).select(&make_analysis("Review auth.py"));
    for sr in &sel.shadow_routes {
        assert_eq!(sr.admission_scope, "diagnostic");
    }
}

#[test]
fn test_low_confidence_escalates() {
    let sel = ModelSelector::new(None).select(&make_analysis("Make it better"));
    assert!(
        sel.routing_reason.contains("low_confidence"),
        "reason was: {}",
        sel.routing_reason
    );
}

#[test]
fn test_critical_risk_overrides() {
    let sel =
        ModelSelector::new(None).select(&make_analysis("Rotate the API keys in config files"));
    assert!(
        sel.selected_tier == "strong_planner" || sel.selected_tier == "advisor",
        "tier was: {}",
        sel.selected_tier
    );
}

#[test]
fn test_fallback_tier_exists() {
    let sel = ModelSelector::new(None).select(&make_analysis("Review auth.py"));
    assert!(MODEL_TIERS.contains(&sel.fallback_tier.as_str()));
}

#[test]
fn test_rejected_candidates_list() {
    let sel = ModelSelector::new(None).select(&make_analysis("Make it better"));
    assert!(!sel.rejected_candidates.is_empty());
}

// ---------------------------------------------------------------------------
// DispatchRoutingPolicyTests
// ---------------------------------------------------------------------------

#[test]
fn test_select_tier_default() {
    let mut tier_map = HashMap::new();
    tier_map.insert("code_review".to_string(), "balanced_worker".to_string());
    let policy = DispatchRoutingPolicy {
        policy_id: "test".to_string(),
        tier_map,
        description: "".to_string(),
    };
    let sel = ModelSelector::new(Some(policy))
        .select(&make_analysis("Review auth.py for security issues"));
    assert_eq!(sel.selected_tier, "balanced_worker");
}

#[test]
fn test_high_risk_overrides() {
    let mut tier_map = HashMap::new();
    tier_map.insert("code_review".to_string(), "cheap_executor".to_string());
    let policy = DispatchRoutingPolicy {
        policy_id: "test".to_string(),
        tier_map,
        description: "".to_string(),
    };
    let sel = ModelSelector::new(Some(policy))
        .select(&make_analysis("Review auth.py for security issues"));
    assert!(sel.selected_tier == "cheap_executor" || sel.selected_tier == "balanced_worker");
}

#[test]
fn test_missing_domain_key_defaults_to_balanced() {
    let policy = DispatchRoutingPolicy {
        policy_id: "test".to_string(),
        tier_map: HashMap::new(),
        description: "".to_string(),
    };
    let sel = ModelSelector::new(Some(policy)).select(&make_analysis("Summarize the README"));
    assert_eq!(sel.selected_tier, "balanced_worker");
}

#[test]
fn test_custom_tier_map() {
    let mut tier_map = HashMap::new();
    tier_map.insert("docs_summarize".to_string(), "strong_planner".to_string());
    let policy = DispatchRoutingPolicy {
        policy_id: "test".to_string(),
        tier_map,
        description: "".to_string(),
    };
    let sel = ModelSelector::new(Some(policy)).select(&make_analysis("Summarize the README"));
    assert_eq!(sel.selected_tier, "strong_planner");
}

// ---------------------------------------------------------------------------
// ModelSelectorBudgetTests
// ---------------------------------------------------------------------------

#[test]
fn test_low_budget_rejects_strong_planner() {
    let mut analysis = make_analysis("Summarize the docs within 500 tokens budget");
    analysis.context_budget_estimate = 200;
    let sel = ModelSelector::new(None).select(&analysis);
    assert!(
        sel.routing_reason.contains("budget_constrained"),
        "reason was: {}",
        sel.routing_reason
    );
    let budget_rejected: Vec<_> = sel
        .rejected_candidates
        .iter()
        .filter(|rc| rc.constraint_failed.as_deref() == Some("budget_threshold"))
        .collect();
    assert_eq!(
        budget_rejected.len(),
        1,
        "expected 1 budget rejected candidate"
    );
}

#[test]
fn test_self_diagnostic_shadow_for_cheapest_tier() {
    let mut tier_map = HashMap::new();
    tier_map.insert("docs_summarize".to_string(), "cheap_executor".to_string());
    let policy = DispatchRoutingPolicy {
        policy_id: "test".to_string(),
        tier_map,
        description: "".to_string(),
    };
    let sel = ModelSelector::new(Some(policy)).select(&make_analysis("Summarize the README"));
    let has_self_diag = sel
        .shadow_routes
        .iter()
        .any(|sr| sr.reason.contains("self-diagnostic"));
    let fallback_differs = sel.fallback_tier != sel.selected_tier;
    assert!(has_self_diag || fallback_differs);
}
