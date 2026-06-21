use engine::feedback::{
    AdaptiveFusionPlanner, DeliberationMode, ModelEndpointObservation, ObjectiveProfile,
    PortfolioRequest,
};

fn endpoint(
    endpoint_id: &str,
    quality: f64,
    success: f64,
    cost_efficiency: f64,
    latency_efficiency: f64,
    estimated_cost_usd: f64,
    capabilities: &[&str],
) -> ModelEndpointObservation {
    ModelEndpointObservation {
        endpoint_id: endpoint_id.to_string(),
        provider_id: format!("provider-{endpoint_id}"),
        model_id: format!("model-{endpoint_id}"),
        enabled: true,
        capabilities: capabilities.iter().map(|value| value.to_string()).collect(),
        quality_score: quality,
        success_score: success,
        cost_efficiency_score: cost_efficiency,
        latency_efficiency_score: latency_efficiency,
        estimated_cost_usd,
        sample_count: 20,
    }
}

fn request(objective: ObjectiveProfile) -> PortfolioRequest {
    PortfolioRequest {
        objective,
        deliberation: DeliberationMode::Auto,
        required_capabilities: vec!["tools".to_string()],
        complexity_score: 0.4,
        risk_level: "low".to_string(),
        max_estimated_cost_usd: 1.0,
        min_quality_score: 0.7,
        max_panel_size: 3,
    }
}

#[test]
fn efficient_auto_prefers_cost_effective_single_endpoint() {
    let endpoints = vec![
        endpoint("efficient", 0.75, 0.90, 1.0, 1.0, 0.02, &["tools"]),
        endpoint("premium", 0.98, 0.98, 0.1, 0.2, 0.40, &["tools"]),
    ];

    let plan = AdaptiveFusionPlanner::plan(&request(ObjectiveProfile::Efficient), &endpoints);

    assert_eq!(plan.mode, "single");
    assert_eq!(plan.primary_endpoint_id.as_deref(), Some("efficient"));
    assert!(plan.panel_endpoint_ids.is_empty());
    assert!(plan.judge_endpoint_id.is_none());
    assert!(plan.synthesizer_endpoint_id.is_none());
    assert!(plan.shadow_only);
    assert!(!plan.influence_selected_tier);
    assert!(!plan.influence_executor_type);
    assert!(!plan.influence_retry_path);
    assert!(!plan.influence_routing_policy);
    assert_eq!(plan.weights.quality, 0.25);
    assert_eq!(plan.weights.success, 0.25);
    assert_eq!(plan.weights.cost_efficiency, 0.35);
    assert_eq!(plan.weights.latency_efficiency, 0.15);
    assert_eq!(
        plan.weights.quality
            + plan.weights.success
            + plan.weights.cost_efficiency
            + plan.weights.latency_efficiency,
        1.0
    );
    let selected = plan
        .scorecards
        .iter()
        .find(|scorecard| scorecard.endpoint_id == "efficient")
        .unwrap();
    assert_eq!(selected.quality_score, 0.75);
    assert_eq!(selected.success_score, 0.90);
    assert_eq!(selected.cost_efficiency_score, 1.0);
    assert_eq!(selected.latency_efficiency_score, 1.0);
}

#[test]
fn quality_auto_builds_bounded_fusion_plan_for_complex_task() {
    let endpoints = vec![
        endpoint("fast", 0.80, 0.88, 0.95, 0.95, 0.03, &["tools"]),
        endpoint("expert", 0.99, 0.98, 0.20, 0.30, 0.25, &["tools"]),
        endpoint("balanced", 0.92, 0.94, 0.65, 0.70, 0.10, &["tools"]),
        endpoint("backup", 0.86, 0.91, 0.80, 0.75, 0.06, &["tools"]),
    ];
    let mut portfolio_request = request(ObjectiveProfile::Quality);
    portfolio_request.complexity_score = 0.85;

    let plan = AdaptiveFusionPlanner::plan(&portfolio_request, &endpoints);

    assert_eq!(plan.mode, "fusion");
    assert_eq!(plan.primary_endpoint_id.as_deref(), Some("expert"));
    assert_eq!(plan.panel_endpoint_ids.len(), 3);
    assert_eq!(plan.panel_endpoint_ids[0], "expert");
    assert_eq!(plan.judge_endpoint_id.as_deref(), Some("expert"));
    assert_eq!(plan.synthesizer_endpoint_id.as_deref(), Some("expert"));
    assert!(plan.estimated_plan_cost_usd <= portfolio_request.max_estimated_cost_usd);
    assert_eq!(plan.weights.quality, 0.65);
    assert_eq!(plan.weights.success, 0.25);
    assert_eq!(plan.weights.cost_efficiency, 0.05);
    assert_eq!(plan.weights.latency_efficiency, 0.05);
    assert!(plan.shadow_only);
}

#[test]
fn explicit_fusion_is_bounded_for_efficient_profile() {
    let endpoints = vec![
        endpoint("alpha", 0.90, 0.90, 0.90, 0.90, 0.05, &["tools"]),
        endpoint("beta", 0.88, 0.92, 0.95, 0.95, 0.05, &["tools"]),
        endpoint("gamma", 0.95, 0.95, 0.40, 0.40, 0.10, &["tools"]),
    ];
    let mut portfolio_request = request(ObjectiveProfile::Efficient);
    portfolio_request.deliberation = DeliberationMode::Fusion;
    portfolio_request.max_panel_size = 2;

    let plan = AdaptiveFusionPlanner::plan(&portfolio_request, &endpoints);

    assert_eq!(plan.mode, "fusion");
    assert_eq!(plan.panel_endpoint_ids.len(), 2);
    assert!(plan
        .reasons
        .iter()
        .any(|reason| reason == "explicit_fusion_requested"));
}

#[test]
fn fusion_falls_back_to_single_when_total_plan_exceeds_budget() {
    let endpoints = vec![
        endpoint("alpha", 0.95, 0.95, 0.80, 0.80, 0.20, &["tools"]),
        endpoint("beta", 0.94, 0.94, 0.80, 0.80, 0.20, &["tools"]),
        endpoint("gamma", 0.93, 0.93, 0.80, 0.80, 0.20, &["tools"]),
    ];
    let mut portfolio_request = request(ObjectiveProfile::Quality);
    portfolio_request.complexity_score = 0.9;
    portfolio_request.max_estimated_cost_usd = 0.5;

    let plan = AdaptiveFusionPlanner::plan(&portfolio_request, &endpoints);

    assert_eq!(plan.mode, "single");
    assert!(plan
        .reasons
        .iter()
        .any(|reason| reason == "fusion_plan_exceeds_budget"));
    assert!(plan.estimated_plan_cost_usd <= portfolio_request.max_estimated_cost_usd);
}

#[test]
fn hard_constraints_filter_before_scoring() {
    let endpoints = vec![
        endpoint("missing-tools", 1.0, 1.0, 1.0, 1.0, 0.01, &["text"]),
        endpoint("over-budget", 1.0, 1.0, 1.0, 1.0, 2.0, &["tools"]),
        endpoint("invalid-score", 1.2, 1.0, 1.0, 1.0, 0.01, &["tools"]),
        endpoint("below-quality-floor", 0.6, 1.0, 1.0, 1.0, 0.01, &["tools"]),
    ];

    let plan = AdaptiveFusionPlanner::plan(&request(ObjectiveProfile::Quality), &endpoints);

    assert_eq!(plan.mode, "unavailable");
    assert!(plan.primary_endpoint_id.is_none());
    assert_eq!(
        plan.scorecards
            .iter()
            .filter(|scorecard| !scorecard.eligible)
            .count(),
        4
    );
    assert!(plan.scorecards.iter().any(|scorecard| scorecard
        .rejected_reasons
        .iter()
        .any(|reason| reason == "missing_capability:tools")));
    assert!(plan.scorecards.iter().any(|scorecard| scorecard
        .rejected_reasons
        .iter()
        .any(|reason| reason == "estimated_cost_exceeds_budget")));
    assert!(plan.scorecards.iter().any(|scorecard| scorecard
        .rejected_reasons
        .iter()
        .any(|reason| reason == "invalid_normalized_score")));
    assert!(plan.scorecards.iter().any(|scorecard| scorecard
        .rejected_reasons
        .iter()
        .any(|reason| reason == "quality_below_minimum")));
}

#[test]
fn planning_is_deterministic_across_input_order() {
    let endpoints = vec![
        endpoint("alpha", 0.90, 0.90, 0.60, 0.60, 0.10, &["tools"]),
        endpoint("beta", 0.90, 0.90, 0.60, 0.60, 0.10, &["tools"]),
        endpoint("gamma", 0.90, 0.90, 0.60, 0.60, 0.10, &["tools"]),
    ];
    let mut reversed = endpoints.clone();
    reversed.reverse();
    let mut portfolio_request = request(ObjectiveProfile::Quality);
    portfolio_request.complexity_score = 0.9;

    let first = AdaptiveFusionPlanner::plan(&portfolio_request, &endpoints);
    let second = AdaptiveFusionPlanner::plan(&portfolio_request, &reversed);

    assert_eq!(first.primary_endpoint_id, second.primary_endpoint_id);
    assert_eq!(first.panel_endpoint_ids, second.panel_endpoint_ids);
    assert_eq!(first.judge_endpoint_id, second.judge_endpoint_id);
    assert_eq!(
        first.synthesizer_endpoint_id,
        second.synthesizer_endpoint_id
    );
    assert_eq!(first.scorecards, second.scorecards);
}

#[test]
fn duplicate_endpoint_ids_and_invalid_request_budget_are_rejected() {
    let endpoints = vec![
        endpoint("duplicate", 0.90, 0.90, 0.90, 0.90, 0.05, &["tools"]),
        endpoint("duplicate", 0.80, 0.80, 0.80, 0.80, 0.04, &["tools"]),
    ];
    let mut portfolio_request = request(ObjectiveProfile::Efficient);
    portfolio_request.max_estimated_cost_usd = f64::NAN;
    portfolio_request.min_quality_score = f64::NAN;

    let plan = AdaptiveFusionPlanner::plan(&portfolio_request, &endpoints);

    assert_eq!(plan.mode, "unavailable");
    assert!(plan.scorecards.iter().all(|scorecard| {
        scorecard
            .rejected_reasons
            .iter()
            .any(|reason| reason == "duplicate_endpoint_id")
            && scorecard
                .rejected_reasons
                .iter()
                .any(|reason| reason == "invalid_request_budget")
            && scorecard
                .rejected_reasons
                .iter()
                .any(|reason| reason == "invalid_min_quality_score")
    }));
}
