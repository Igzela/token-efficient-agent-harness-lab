use engine::feedback::{
    AdaptiveExplorationGate, CandidateAggregate, CandidateKind, ContextualBanditEngine,
    ContextualBanditObservation, ContextualPolicyPromotion, ContextualPolicyPromotionGate,
    ContextualPolicyRequest, ObjectiveProfile, TaskClassEvaluation,
    CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION, CONTEXTUAL_POLICY_SCHEMA_VERSION,
};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::json;

fn candidate(id: &str, quality: f64, cost: f64, latency: f64) -> CandidateAggregate {
    CandidateAggregate {
        candidate_id: id.to_string(),
        candidate_kind: CandidateKind::Endpoint,
        member_endpoint_ids: vec![id.to_string()],
        sample_count: 40,
        evidence_run_ids: (0..40).map(|index| format!("run-{id}-{index}")).collect(),
        success_rate: quality,
        average_quality_score: quality,
        average_tool_success_score: quality,
        average_cost_usd: cost,
        average_latency_ms: latency,
    }
}

fn evaluation() -> TaskClassEvaluation {
    TaskClassEvaluation {
        task_class: "coding".to_string(),
        candidates: vec![
            candidate("cheap", 0.76, 0.01, 500.0),
            candidate("strong", 0.92, 0.08, 1_500.0),
        ],
        pareto_candidate_ids: vec!["cheap".to_string(), "strong".to_string()],
        recommendations: Vec::new(),
    }
}

fn request(objective: ObjectiveProfile, risk_level: &str) -> ContextualPolicyRequest {
    ContextualPolicyRequest {
        schema_version: CONTEXTUAL_POLICY_SCHEMA_VERSION.to_string(),
        request_id: "request-1".to_string(),
        task_class: "coding".to_string(),
        objective,
        risk_level: risk_level.to_string(),
        exploration_seed: 7,
    }
}

fn observation(
    id: &str,
    candidate_id: &str,
    sequence: u64,
    quality: f64,
) -> ContextualBanditObservation {
    ContextualBanditObservation {
        schema_version: CONTEXTUAL_POLICY_SCHEMA_VERSION.to_string(),
        observation_id: id.to_string(),
        run_id: format!("run-{id}"),
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        candidate_id: candidate_id.to_string(),
        sequence,
        success: quality >= 0.8,
        quality_score: quality,
        tool_success_score: quality,
        cost_efficiency_score: 0.5,
        latency_efficiency_score: 0.5,
        human_score: None,
    }
}

#[test]
fn efficient_and_quality_objectives_select_different_candidates() {
    let disabled = AdaptiveExplorationGate::from_flags(false, false, false, 0.05);
    let efficient = ContextualBanditEngine::decide(
        &request(ObjectiveProfile::Efficient, "low"),
        &evaluation(),
        &[],
        &disabled,
    )
    .unwrap();
    let quality = ContextualBanditEngine::decide(
        &request(ObjectiveProfile::Quality, "low"),
        &evaluation(),
        &[],
        &disabled,
    )
    .unwrap();

    assert_eq!(efficient.selected_candidate_id, "cheap");
    assert_eq!(quality.selected_candidate_id, "strong");
    assert!(efficient.shadow_only);
    assert!(!efficient.live_execution_authority);
    assert!(efficient.requires_explicit_adaptive_plan);
}

#[test]
fn recent_observations_are_weighted_more_than_old_observations() {
    let disabled = AdaptiveExplorationGate::from_flags(false, false, false, 0.05);
    let observations = vec![
        observation("old-strong", "strong", 1, 0.95),
        observation("recent-cheap-1", "cheap", 99, 0.99),
        observation("recent-cheap-2", "cheap", 100, 0.99),
    ];
    let decision = ContextualBanditEngine::decide(
        &request(ObjectiveProfile::Quality, "low"),
        &evaluation(),
        &observations,
        &disabled,
    )
    .unwrap();

    assert_eq!(decision.selected_candidate_id, "cheap");
}

#[test]
fn exploration_is_disabled_by_default_and_excludes_high_risk() {
    let active = AdaptiveExplorationGate::from_flags(true, true, false, 1.0);
    let high_risk = ContextualBanditEngine::decide(
        &request(ObjectiveProfile::Quality, "high"),
        &evaluation(),
        &[],
        &active,
    )
    .unwrap();
    let inactive = ContextualBanditEngine::decide(
        &request(ObjectiveProfile::Quality, "low"),
        &evaluation(),
        &[],
        &AdaptiveExplorationGate::from_flags(false, true, false, 1.0),
    )
    .unwrap();

    assert_eq!(high_risk.exploration_rate, 0.0);
    assert!(!high_risk.exploration_assigned);
    assert_eq!(inactive.exploration_rate, 0.0);
    assert!(!inactive.exploration_assigned);
}

#[test]
fn duplicate_observations_are_rejected() {
    let disabled = AdaptiveExplorationGate::from_flags(false, false, false, 0.05);
    let err = ContextualBanditEngine::decide(
        &request(ObjectiveProfile::Quality, "low"),
        &evaluation(),
        &[
            observation("duplicate", "strong", 1, 0.9),
            observation("duplicate", "strong", 2, 0.9),
        ],
        &disabled,
    )
    .unwrap_err();

    assert!(err
        .violations
        .contains(&"duplicate_observation_id".to_string()));
}

#[test]
fn observations_must_reference_evaluation_candidates() {
    let disabled = AdaptiveExplorationGate::from_flags(false, false, false, 0.05);
    let err = ContextualBanditEngine::decide(
        &request(ObjectiveProfile::Quality, "low"),
        &evaluation(),
        &[observation("unknown-candidate", "missing", 1, 0.9)],
        &disabled,
    )
    .unwrap_err();

    assert!(err
        .violations
        .contains(&"unknown_observation_candidate".to_string()));
}

fn promotion() -> ContextualPolicyPromotion {
    ContextualPolicyPromotion {
        schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        candidate_id: "strong".to_string(),
        baseline_candidate_id: "cheap".to_string(),
        sample_count: 30,
        confidence: 0.9,
        mean_quality_delta: 0.1,
        mean_cost_reduction: 0.02,
        failure_rate_delta: 0.0,
        evidence_run_ids: (0..30).map(|index| format!("run-{index}")).collect(),
        risk_level: "low".to_string(),
        confirm_adaptive_policy_promotion: true,
    }
}

#[test]
fn promotion_requires_dual_gates_confirmation_and_sufficient_evidence() {
    let denied = ContextualPolicyPromotionGate::from_flags(false, true).evaluate(&promotion());
    assert!(!denied.eligible);
    assert!(denied
        .blocked_reasons
        .contains(&"adaptive_policy_promotion_gates_disabled".to_string()));

    let allowed = ContextualPolicyPromotionGate::from_flags(true, true).evaluate(&promotion());
    assert!(allowed.eligible);
    let policy = allowed.policy.unwrap();
    assert_eq!(policy.policy_key, "coding:quality");
    assert!(policy.shadow_first);
    assert!(!policy.live_execution_authority);
    assert!(policy.requires_explicit_adaptive_plan);
    assert!(policy.is_valid());
}

#[test]
fn promoted_policy_hash_detects_live_authority_tampering() {
    let mut policy = ContextualPolicyPromotionGate::from_flags(true, true)
        .evaluate(&promotion())
        .policy
        .unwrap();
    policy.live_execution_authority = true;
    assert!(!policy.is_valid());
}

#[test]
fn promotion_rejects_duplicate_evidence_and_high_risk_context() {
    let mut request = promotion();
    request.risk_level = "critical".to_string();
    request.evidence_run_ids[1] = request.evidence_run_ids[0].clone();
    let verdict = ContextualPolicyPromotionGate::from_flags(true, true).evaluate(&request);

    assert!(!verdict.eligible);
    assert!(verdict
        .blocked_reasons
        .contains(&"invalid_evidence_id".to_string()));
    assert!(verdict
        .blocked_reasons
        .contains(&"high_risk_context_excluded".to_string()));
}

#[test]
fn local_store_applies_and_rolls_back_promoted_policy() {
    let dir = tempfile::tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let verdict = ContextualPolicyPromotionGate::from_flags(true, true).evaluate(&promotion());

    let applied = store
        .apply_adaptive_fusion_policy(&verdict, "operator")
        .unwrap();
    assert_eq!(applied["applied"], true);
    let adjustment_id = applied["adjustment_id"].as_str().unwrap().to_string();
    assert_eq!(store.active_adaptive_fusion_policies().unwrap().len(), 1);
    assert_eq!(store.adaptive_fusion_policy_snapshots().unwrap().len(), 1);

    let rolled_back = store
        .rollback_adaptive_fusion_policy(&adjustment_id, true, "operator")
        .unwrap();
    assert_eq!(rolled_back["rolled_back"], true);
    assert!(store.active_adaptive_fusion_policies().unwrap().is_empty());
}

#[test]
fn local_store_concurrent_promotions_preserve_unique_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let store = std::sync::Arc::new(LocalProductStore::new(dir.path().join("team.db")).unwrap());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles = [ObjectiveProfile::Efficient, ObjectiveProfile::Quality]
        .into_iter()
        .enumerate()
        .map(|(index, objective)| {
            let store = std::sync::Arc::clone(&store);
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                let mut request = promotion();
                request.objective = objective;
                request.candidate_id = format!("candidate-{index}");
                request.baseline_candidate_id = format!("baseline-{index}");
                request.evidence_run_ids = (0..30)
                    .map(|evidence_index| format!("run-{index}-{evidence_index}"))
                    .collect();
                let verdict =
                    ContextualPolicyPromotionGate::from_flags(true, true).evaluate(&request);
                barrier.wait();
                store
                    .apply_adaptive_fusion_policy(&verdict, "operator")
                    .unwrap()
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        assert_eq!(handle.join().unwrap()["applied"], true);
    }
    let policies = store.active_adaptive_fusion_policies().unwrap();
    let snapshots = store.adaptive_fusion_policy_snapshots().unwrap();
    let adjustment_ids = snapshots
        .iter()
        .map(|snapshot| snapshot.adjustment_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(policies.len(), 2);
    assert_eq!(snapshots.len(), 2);
    assert_eq!(adjustment_ids.len(), 2);
}

#[test]
fn local_store_ignores_tampered_live_authority_policy() {
    let dir = tempfile::tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let mut policy = ContextualPolicyPromotionGate::from_flags(true, true)
        .evaluate(&promotion())
        .policy
        .unwrap();
    policy.live_execution_authority = true;
    store
        .set_config_value(
            "adaptive_fusion_active_policies",
            json!([policy]),
            "test-tamper",
        )
        .unwrap();

    assert!(store.active_adaptive_fusion_policies().unwrap().is_empty());
}
