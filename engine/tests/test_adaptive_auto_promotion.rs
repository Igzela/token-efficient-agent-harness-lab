use engine::feedback::{
    AdaptiveAutoPromotionController, AdaptiveAutoPromotionEvidence, AdaptiveAutoPromotionGate,
    AdaptiveAutoPromotionPolicy, AdaptiveAutoPromotionRequest, ContextualPolicyPromotion,
    ContextualPolicyPromotionGate, ObjectiveProfile, CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION,
};
use engine::storage::local_product_store::{
    AdaptiveObservationInput, LocalProductStore, ADAPTIVE_OBSERVATION_SCHEMA_VERSION,
};
use tempfile::tempdir;

fn baseline_verdict() -> engine::feedback::ContextualPolicyPromotionVerdict {
    ContextualPolicyPromotionGate::from_flags(true, true).evaluate(&ContextualPolicyPromotion {
        schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        candidate_id: "cheap".to_string(),
        baseline_candidate_id: "initial".to_string(),
        sample_count: 30,
        confidence: 0.9,
        mean_quality_delta: 0.1,
        mean_cost_reduction: 0.01,
        failure_rate_delta: 0.0,
        evidence_run_ids: (0..30).map(|index| format!("baseline-{index}")).collect(),
        risk_level: "low".to_string(),
        confirm_adaptive_policy_promotion: true,
    })
}

fn request(expected_hash: Option<String>) -> AdaptiveAutoPromotionRequest {
    AdaptiveAutoPromotionRequest {
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        risk_level: "low".to_string(),
        candidate_id: "strong".to_string(),
        baseline_candidate_id: "cheap".to_string(),
        expected_active_policy_hash: expected_hash,
        rollout_percentage: 25,
    }
}

fn evidence(
    candidate_id: &str,
    sequence: u64,
    quality: f64,
    cost_usd: f64,
    latency_ms: u64,
    success: bool,
) -> AdaptiveAutoPromotionEvidence {
    AdaptiveAutoPromotionEvidence {
        observation_id: format!("observation-{candidate_id}-{sequence}"),
        run_id: format!("run-{candidate_id}-{sequence}"),
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        candidate_id: candidate_id.to_string(),
        sequence,
        success,
        quality_score: quality,
        cost_usd,
        latency_ms,
    }
}

fn winning_evidence() -> Vec<AdaptiveAutoPromotionEvidence> {
    let mut observations = Vec::new();
    for sequence in 1..=30 {
        observations.push(evidence("cheap", sequence, 0.8, 0.08, 800, true));
        observations.push(evidence("strong", sequence + 30, 0.92, 0.05, 500, true));
    }
    observations
}

#[test]
fn auto_promotion_is_default_off_and_killable() {
    let active = baseline_verdict().policy.unwrap();
    let disabled = AdaptiveAutoPromotionController::evaluate(
        &request(Some(active.policy_hash.clone())),
        &winning_evidence(),
        Some(&active),
        &AdaptiveAutoPromotionPolicy::default(),
        &AdaptiveAutoPromotionGate::from_flags(false, false, false),
    );
    let killed = AdaptiveAutoPromotionController::evaluate(
        &request(Some(active.policy_hash.clone())),
        &winning_evidence(),
        Some(&active),
        &AdaptiveAutoPromotionPolicy::default(),
        &AdaptiveAutoPromotionGate::from_flags(true, true, true),
    );

    assert!(!disabled.eligible);
    assert!(disabled
        .blocked_reasons
        .contains(&"adaptive_auto_promotion_gates_disabled".to_string()));
    assert!(!killed.eligible);
    assert!(killed
        .blocked_reasons
        .contains(&"adaptive_auto_promotion_kill_switch_active".to_string()));
}

#[test]
fn eligible_evidence_builds_hashed_staged_policy_without_confirmation() {
    let active = baseline_verdict().policy.unwrap();
    let verdict = AdaptiveAutoPromotionController::evaluate(
        &request(Some(active.policy_hash.clone())),
        &winning_evidence(),
        Some(&active),
        &AdaptiveAutoPromotionPolicy::default(),
        &AdaptiveAutoPromotionGate::from_flags(true, true, false),
    );

    assert!(verdict.eligible);
    let policy = verdict.policy.unwrap();
    assert!(policy.auto_promoted);
    assert_eq!(policy.rollout_percentage, 25);
    assert_eq!(
        policy.previous_policy_hash.as_deref(),
        Some(active.policy_hash.as_str())
    );
    assert!(policy.mean_quality_delta > 0.0);
    assert!(policy.mean_cost_reduction > 0.0);
    assert!(policy.mean_latency_reduction > 0.0);
    assert!(policy.is_valid());
    let round_trip: engine::feedback::PromotedAdaptivePolicy =
        serde_json::from_str(&serde_json::to_string(&policy).unwrap()).unwrap();
    assert!(round_trip.is_valid(), "{round_trip:?}");
}

#[test]
fn regression_missing_and_stale_evidence_are_blocked() {
    let active = baseline_verdict().policy.unwrap();
    let gate = AdaptiveAutoPromotionGate::from_flags(true, true, false);
    let mut regressed = winning_evidence();
    for observation in &mut regressed {
        if observation.candidate_id == "strong" {
            observation.quality_score = 0.7;
            observation.cost_usd = 0.1;
            observation.latency_ms = 1_000;
            observation.success = false;
        }
    }
    let regression = AdaptiveAutoPromotionController::evaluate(
        &request(Some(active.policy_hash.clone())),
        &regressed,
        Some(&active),
        &AdaptiveAutoPromotionPolicy::default(),
        &gate,
    );
    assert!(!regression.eligible);
    for reason in [
        "quality_regression_detected",
        "cost_regression_detected",
        "latency_regression_detected",
        "failure_rate_regression_detected",
    ] {
        assert!(regression.blocked_reasons.contains(&reason.to_string()));
    }

    let missing = AdaptiveAutoPromotionController::evaluate(
        &request(Some(active.policy_hash.clone())),
        &winning_evidence()
            .into_iter()
            .filter(|observation| observation.candidate_id == "cheap")
            .collect::<Vec<_>>(),
        Some(&active),
        &AdaptiveAutoPromotionPolicy::default(),
        &gate,
    );
    assert!(missing
        .blocked_reasons
        .contains(&"candidate_evidence_missing".to_string()));

    let mut stale = winning_evidence();
    stale.push(evidence("unrelated", 10_000, 1.0, 0.0, 1, true));
    let stale = AdaptiveAutoPromotionController::evaluate(
        &request(Some(active.policy_hash.clone())),
        &stale,
        Some(&active),
        &AdaptiveAutoPromotionPolicy {
            max_evidence_age_sequences: 100,
            ..Default::default()
        },
        &gate,
    );
    assert!(stale
        .blocked_reasons
        .contains(&"fresh_evidence_missing".to_string()));

    let stale_policy = AdaptiveAutoPromotionController::evaluate(
        &request(Some("0".repeat(64))),
        &winning_evidence(),
        Some(&active),
        &AdaptiveAutoPromotionPolicy::default(),
        &gate,
    );
    assert!(stale_policy
        .blocked_reasons
        .contains(&"active_policy_hash_stale".to_string()));
}

fn observation_input(
    run_id: &str,
    candidate_id: &str,
    quality_score: f64,
    cost_usd: f64,
    latency_ms: u64,
) -> AdaptiveObservationInput {
    AdaptiveObservationInput {
        schema_version: ADAPTIVE_OBSERVATION_SCHEMA_VERSION.to_string(),
        run_id: run_id.to_string(),
        request_id: format!("request-{run_id}"),
        task_class: "coding".to_string(),
        objective: ObjectiveProfile::Quality,
        risk_level: "low".to_string(),
        candidate_id: candidate_id.to_string(),
        candidate_hash: "a".repeat(64),
        policy_hash: None,
        candidate_kind: "single".to_string(),
        success: true,
        quality_score,
        quality_score_source: "execution_success_proxy".to_string(),
        tool_success_score: 1.0,
        cost_usd,
        latency_ms,
        input_tokens: 100,
        output_tokens: 50,
    }
}

#[test]
fn local_evidence_auto_promotion_snapshots_and_rolls_back() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("team.db")).unwrap();
    let baseline = baseline_verdict();
    store
        .apply_adaptive_fusion_policy(&baseline, "operator")
        .unwrap();
    let active = store.active_adaptive_fusion_policies().unwrap().remove(0);
    for index in 0..3 {
        store
            .record_adaptive_observation(
                &observation_input(&format!("cheap-{index}"), "cheap", 0.8, 0.08, 800),
                "operator",
            )
            .unwrap();
        store
            .record_adaptive_observation(
                &observation_input(&format!("strong-{index}"), "strong", 0.92, 0.05, 500),
                "operator",
            )
            .unwrap();
    }

    let applied = store
        .auto_promote_adaptive_fusion_policy(
            &request(Some(active.policy_hash.clone())),
            &AdaptiveAutoPromotionPolicy {
                min_samples_per_candidate: 3,
                min_confidence: 0.7,
                ..Default::default()
            },
            &AdaptiveAutoPromotionGate::from_flags(true, true, false),
            "operator",
        )
        .unwrap();
    assert_eq!(applied["applied"], true);
    let adjustment_id = applied["adjustment_id"].as_str().unwrap();
    let promoted = store.active_adaptive_fusion_policies().unwrap().remove(0);
    assert_eq!(promoted.candidate_id, "strong");
    assert_eq!(promoted.rollout_percentage, 25);
    assert_eq!(
        promoted.previous_policy_hash.as_deref(),
        Some(active.policy_hash.as_str())
    );

    let rolled_back = store
        .rollback_adaptive_fusion_policy(adjustment_id, true, "operator")
        .unwrap();
    assert_eq!(rolled_back["rolled_back"], true);
    let restored = store.active_adaptive_fusion_policies().unwrap().remove(0);
    assert_eq!(restored.policy_hash, active.policy_hash);
}
