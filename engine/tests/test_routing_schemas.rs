use engine::routing::schemas::*;

#[test]
fn test_make_task_group() {
    assert_eq!(make_task_group("code", "review"), "code/review");
    assert_eq!(make_task_group("docs", "generate"), "docs/generate");
}

#[test]
fn test_parse_task_group() {
    assert_eq!(
        parse_task_group("code/review"),
        ("code".to_string(), "review".to_string())
    );
    assert_eq!(
        parse_task_group("docs/generate"),
        ("docs".to_string(), "generate".to_string())
    );
}

#[test]
fn test_parse_task_group_no_slash() {
    let (domain, intent) = parse_task_group("code");
    assert_eq!(domain, "code");
    assert_eq!(intent, "");
}

#[test]
fn test_routing_observation_to_dict() {
    let obs = RoutingObservation {
        schema_version: ROUTING_OBSERVATION_SCHEMA_VERSION.to_string(),
        observation_id: "obs-001".to_string(),
        arm_id: "arm-cheap_executor".to_string(),
        dispatch_id: "disp-001".to_string(),
        task_domain: "code".to_string(),
        task_intent: "review".to_string(),
        selected_tier: "cheap_executor".to_string(),
        baseline_tier: "balanced_worker".to_string(),
        quality_score: 0.8,
        cost: 0.01,
        latency_ms: 100,
        success: true,
        failure_domain: None,
        budget_violation: false,
        observed_at: "2025-01-01T00:00:00Z".to_string(),
    };
    let d = obs.to_dict();
    assert_eq!(d["observation_id"], "obs-001");
    assert_eq!(d["selected_tier"], "cheap_executor");
    assert!(d["success"].as_bool().unwrap());
}

#[test]
fn test_promotion_verdict_to_dict() {
    let v = PromotionVerdict {
        schema_version: PROMOTION_VERDICT_SCHEMA_VERSION.to_string(),
        verdict: "promote".to_string(),
        task_group: "code/review".to_string(),
        candidate_tier: "cheap_executor".to_string(),
        baseline_tier: "balanced_worker".to_string(),
        sample_count: 50,
        quality_delta: 0.05,
        cost_reduction_pct: 10.0,
        failure_rate_delta: 0.01,
        reasons: vec!["all_gate_conditions_met".to_string()],
        requires_human_review: false,
    };
    let d = v.to_dict();
    assert_eq!(d["verdict"], "promote");
    assert_eq!(d["sample_count"], 50);
}

#[test]
fn test_routing_selection_serde() {
    let sel = RoutingSelection {
        selected_tier: "cheap_executor".to_string(),
        selected_profile_id: None,
        fallback_tier: "balanced_worker".to_string(),
        fallback_profile_id: None,
        shadow_routes: vec![],
        rejected_candidates: vec![],
        routing_reason: "test".to_string(),
        routing_mode: "static".to_string(),
        routing_experiment_id: None,
    };
    let json = serde_json::to_string(&sel).unwrap();
    let deserialized: RoutingSelection = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.selected_tier, "cheap_executor");
    assert_eq!(deserialized.routing_mode, "static");
}

#[test]
fn test_usage_ledger_row_to_dict() {
    let row = UsageLedgerRow {
        row_id: "row-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        model_profile_id: "profile-cheap".to_string(),
        cost_of_pass_group: "scope/code/review/accuracy".to_string(),
        input_tokens: 100,
        output_tokens: 50,
        estimated_cost: 0.001,
        quality_score: 0.9,
        success: true,
        failure_domain: None,
        latency_ms: 50,
    };
    let d = row.to_dict();
    assert_eq!(d["row_id"], "row-001");
    assert_eq!(d["input_tokens"], 100);
}

#[test]
fn test_parse_cost_of_pass_group() {
    let (scope, family, variant, criterion) =
        parse_cost_of_pass_group("scope/code/review/accuracy");
    assert_eq!(scope, "scope");
    assert_eq!(family, "code");
    assert_eq!(variant, "review");
    assert_eq!(criterion, "accuracy");
}

#[test]
fn test_aggregate_cost_of_pass_empty() {
    assert!(aggregate_cost_of_pass(&[]).is_none());
}

#[test]
fn test_aggregate_cost_of_pass_single() {
    let row = UsageLedgerRow {
        row_id: "row-001".to_string(),
        dispatch_id: "disp-001".to_string(),
        model_profile_id: "profile-cheap".to_string(),
        cost_of_pass_group: "scope/code/review/accuracy".to_string(),
        input_tokens: 100,
        output_tokens: 50,
        estimated_cost: 0.01,
        quality_score: 0.8,
        success: true,
        failure_domain: None,
        latency_ms: 50,
    };
    let agg = aggregate_cost_of_pass(&[row]).unwrap();
    assert_eq!(agg.total_count, 1);
    assert_eq!(agg.failure_count, 0);
    assert!((agg.cost_of_pass.unwrap() - 0.0125).abs() < 0.0001);
}

#[test]
fn test_aggregate_cost_of_pass_multiple() {
    let rows = vec![
        UsageLedgerRow {
            row_id: "row-001".to_string(),
            dispatch_id: "disp-001".to_string(),
            model_profile_id: "profile-cheap".to_string(),
            cost_of_pass_group: "scope/code/review/accuracy".to_string(),
            input_tokens: 100,
            output_tokens: 50,
            estimated_cost: 0.01,
            quality_score: 0.8,
            success: true,
            failure_domain: None,
            latency_ms: 50,
        },
        UsageLedgerRow {
            row_id: "row-002".to_string(),
            dispatch_id: "disp-002".to_string(),
            model_profile_id: "profile-cheap".to_string(),
            cost_of_pass_group: "scope/code/review/accuracy".to_string(),
            input_tokens: 200,
            output_tokens: 100,
            estimated_cost: 0.02,
            quality_score: 0.6,
            success: false,
            failure_domain: Some("provider_error".to_string()),
            latency_ms: 100,
        },
    ];
    let agg = aggregate_cost_of_pass(&rows).unwrap();
    assert_eq!(agg.total_count, 2);
    assert_eq!(agg.failure_count, 1);
    assert!((agg.total_cost - 0.03).abs() < 0.0001);
    assert!((agg.total_quality - 1.4).abs() < 0.0001);
}

#[test]
fn test_constants() {
    assert!(EXPERIMENT_STATUSES.contains(&"created"));
    assert!(EXPERIMENT_STATUSES.contains(&"running"));
    assert!(ROUTING_MODES.contains(&"static"));
    assert!(ROUTING_MODES.contains(&"adaptive"));
    assert!(PROMOTION_VERDICTS.contains(&"promote"));
    assert!(PROMOTION_VERDICTS.contains(&"hold"));
    assert!(DOWNGRADE_REASONS.contains(&"cost_optimization"));
    assert!(UPGRADE_REASONS.contains(&"high_uncertainty"));
}
