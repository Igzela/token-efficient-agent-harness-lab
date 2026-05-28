use engine::routing::promotion_gate::*;
use engine::routing::schemas::*;

fn make_obs(
    tier: &str,
    domain: &str,
    intent: &str,
    quality: f64,
    cost: f64,
    success: bool,
) -> RoutingObservation {
    RoutingObservation {
        schema_version: ROUTING_OBSERVATION_SCHEMA_VERSION.to_string(),
        observation_id: format!("obs-{tier}-{domain}"),
        arm_id: format!("arm-{tier}"),
        dispatch_id: "disp-001".to_string(),
        task_domain: domain.to_string(),
        task_intent: intent.to_string(),
        selected_tier: tier.to_string(),
        baseline_tier: "balanced_worker".to_string(),
        quality_score: quality,
        cost,
        latency_ms: 50,
        success,
        failure_domain: None,
        budget_violation: false,
        observed_at: "2025-01-01T00:00:00Z".to_string(),
    }
}

fn make_store_with_observations() -> RoutingObservationStore {
    let mut store = RoutingObservationStore::new();
    // 40 candidate observations: quality 0.85, cost 0.01
    for _ in 0..40 {
        store.add_observation(make_obs(
            "cheap_executor",
            "code",
            "review",
            0.85,
            0.01,
            true,
        ));
    }
    // 40 baseline observations: quality 0.80, cost 0.03
    for _ in 0..40 {
        store.add_observation(make_obs(
            "balanced_worker",
            "code",
            "review",
            0.80,
            0.03,
            true,
        ));
    }
    store
}

#[test]
fn test_observation_store_add_and_count() {
    let mut store = RoutingObservationStore::new();
    store.add_observation(make_obs(
        "cheap_executor",
        "code",
        "review",
        0.8,
        0.01,
        true,
    ));
    assert_eq!(store.total_count(), 1);
    assert_eq!(
        store.count_for_tier_and_group("cheap_executor", "code", "review"),
        1
    );
    assert_eq!(
        store.count_for_tier_and_group("balanced_worker", "code", "review"),
        0
    );
}

#[test]
fn test_observations_for_tier_and_group() {
    let store = make_store_with_observations();
    let obs = store.observations_for_tier_and_group("cheap_executor", "code", "review");
    assert_eq!(obs.len(), 40);
}

#[test]
fn test_promote_when_all_conditions_met() {
    let store = make_store_with_observations();
    let gate = PromotionGate::new(Some(30), Some(0.05), Some(5.0), false);
    let verdict = gate.evaluate(&store, "code/review", "cheap_executor", "balanced_worker");
    assert_eq!(verdict.verdict, "promote");
    assert!(verdict
        .reasons
        .contains(&"all_gate_conditions_met".to_string()));
}

#[test]
fn test_insufficient_data_low_sample_count() {
    let mut store = RoutingObservationStore::new();
    for _ in 0..5 {
        store.add_observation(make_obs(
            "cheap_executor",
            "code",
            "review",
            0.8,
            0.01,
            true,
        ));
    }
    let gate = PromotionGate::new(Some(30), None, None, false);
    let verdict = gate.evaluate(&store, "code/review", "cheap_executor", "balanced_worker");
    assert_eq!(verdict.verdict, "insufficient_data");
}

#[test]
fn test_hold_on_quality_regression() {
    let mut store = RoutingObservationStore::new();
    // Candidate quality lower than baseline
    for _ in 0..40 {
        store.add_observation(make_obs(
            "cheap_executor",
            "code",
            "review",
            0.6,
            0.01,
            true,
        ));
    }
    for _ in 0..40 {
        store.add_observation(make_obs(
            "balanced_worker",
            "code",
            "review",
            0.9,
            0.03,
            true,
        ));
    }
    let gate = PromotionGate::new(Some(30), None, None, false);
    let verdict = gate.evaluate(&store, "code/review", "cheap_executor", "balanced_worker");
    assert_eq!(verdict.verdict, "hold");
    assert!(verdict.quality_delta < 0.0);
}

#[test]
fn test_hold_on_insufficient_cost_reduction() {
    let mut store = RoutingObservationStore::new();
    // Same cost: no reduction
    for _ in 0..40 {
        store.add_observation(make_obs(
            "cheap_executor",
            "code",
            "review",
            0.85,
            0.03,
            true,
        ));
    }
    for _ in 0..40 {
        store.add_observation(make_obs(
            "balanced_worker",
            "code",
            "review",
            0.80,
            0.03,
            true,
        ));
    }
    let gate = PromotionGate::new(Some(30), None, Some(5.0), false);
    let verdict = gate.evaluate(&store, "code/review", "cheap_executor", "balanced_worker");
    assert_eq!(verdict.verdict, "hold");
}

#[test]
fn test_hold_on_high_failure_rate_delta() {
    let mut store = RoutingObservationStore::new();
    // Candidate: high failure rate
    for _ in 0..20 {
        store.add_observation(make_obs(
            "cheap_executor",
            "code",
            "review",
            0.85,
            0.01,
            true,
        ));
    }
    for _ in 0..20 {
        store.add_observation(make_obs(
            "cheap_executor",
            "code",
            "review",
            0.85,
            0.01,
            false,
        ));
    }
    // Baseline: no failures
    for _ in 0..40 {
        store.add_observation(make_obs(
            "balanced_worker",
            "code",
            "review",
            0.80,
            0.03,
            true,
        ));
    }
    let gate = PromotionGate::new(Some(30), Some(0.05), None, false);
    let verdict = gate.evaluate(&store, "code/review", "cheap_executor", "balanced_worker");
    assert_eq!(verdict.verdict, "hold");
}

#[test]
fn test_hold_when_human_review_required() {
    let store = make_store_with_observations();
    let gate = PromotionGate::new(Some(30), Some(0.05), Some(5.0), true);
    let verdict = gate.evaluate(&store, "code/review", "cheap_executor", "balanced_worker");
    assert_eq!(verdict.verdict, "hold");
    assert!(verdict.requires_human_review);
    assert!(verdict
        .reasons
        .contains(&"human_review_required".to_string()));
}

#[test]
fn test_check_sample_count() {
    let store = make_store_with_observations();
    let gate = PromotionGate::new(Some(30), None, None, false);
    let (enough, count) = gate.check_sample_count(&store, "code/review", "cheap_executor");
    assert!(enough);
    assert_eq!(count, 40);
    let (enough2, count2) = gate.check_sample_count(&store, "code/review", "advisor");
    assert!(!enough2);
    assert_eq!(count2, 0);
}
