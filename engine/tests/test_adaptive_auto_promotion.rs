use engine::feedback::offline_replay_report_sha256;
use engine::feedback::{
    AdaptiveAutoPromotionController, AdaptiveAutoPromotionEvidence, AdaptiveAutoPromotionGate,
    AdaptiveAutoPromotionPolicy, AdaptiveAutoPromotionRequest, AdaptiveCanaryRequest,
    AdaptiveExperimentController, AdaptiveExperimentGate, AdaptivePromotionEvidenceChain,
    ContextualPolicyPromotion, ContextualPolicyPromotionGate, ObjectiveProfile,
    OfflineCounterfactualEstimate, OfflineObservedFacts, OfflinePolicyComparison,
    OfflinePolicyDefinition, OfflinePolicySelection, OfflineReplayOutcome, OfflineReplayReport,
    OfflineReplayStatus, ShadowRouter, ADAPTIVE_CANARY_SCHEMA_VERSION,
    ADAPTIVE_PROMOTION_EVIDENCE_CHAIN_SCHEMA_VERSION, CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION,
    OFFLINE_REPLAY_SCHEMA_VERSION,
};
use engine::storage::local_product_store::{
    AdaptiveObservationInput, LocalProductStore, ADAPTIVE_OBSERVATION_SCHEMA_VERSION,
};
use std::collections::BTreeMap;
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
fn caller_only_evidence_is_no_longer_authoritative() {
    let active = baseline_verdict().policy.unwrap();
    let verdict = AdaptiveAutoPromotionController::evaluate(
        &request(Some(active.policy_hash.clone())),
        &winning_evidence(),
        Some(&active),
        &AdaptiveAutoPromotionPolicy::default(),
        &AdaptiveAutoPromotionGate::from_flags(true, true, false),
    );

    assert!(!verdict.eligible);
    assert!(verdict
        .blocked_reasons
        .contains(&"complete_evidence_chain_required".to_string()));
    assert!(verdict.policy.is_none());
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
    assert_eq!(applied["applied"], false);
    assert_eq!(applied["status"], "blocked");
    assert_eq!(
        applied["blocked_reasons"][0],
        "complete_evidence_chain_required"
    );
    let restored = store.active_adaptive_fusion_policies().unwrap().remove(0);
    assert_eq!(restored.policy_hash, active.policy_hash);
}

fn replay_report_for_promotion() -> OfflineReplayReport {
    let definition_hash = format!("{:064x}", 3);
    let current_policy = OfflinePolicyDefinition::new(
        "current",
        "policy-current-v1",
        BTreeMap::from([(
            "coding".to_string(),
            OfflinePolicySelection {
                candidate_id: "cheap".to_string(),
                candidate_version: "candidate-v1".to_string(),
                candidate_definition_sha256: definition_hash.clone(),
            },
        )]),
    )
    .unwrap();
    let candidate_policy = OfflinePolicyDefinition::new(
        "candidate-policy",
        "policy-candidate-v1",
        BTreeMap::from([(
            "coding".to_string(),
            OfflinePolicySelection {
                candidate_id: "strong".to_string(),
                candidate_version: "candidate-v2".to_string(),
                candidate_definition_sha256: definition_hash.clone(),
            },
        )]),
    )
    .unwrap();
    let observed = OfflineObservedFacts {
        task_class: "coding".to_string(),
        candidate_id: "cheap".to_string(),
        candidate_version: "candidate-v1".to_string(),
        candidate_definition_sha256: definition_hash.clone(),
        member_endpoint_ids: vec!["endpoint-cheap".to_string()],
        trace_ids: vec![
            "trace-1".to_string(),
            "trace-2".to_string(),
            "trace-3".to_string(),
        ],
        evidence_content_sha256: vec![format!("{:064x}", 4), format!("{:064x}", 5)],
        sample_count: 3,
        success_rate: 0.8,
        average_quality_score: 0.8,
        average_tool_success_score: 0.9,
        average_cost_usd: 0.08,
        average_latency_ms: 800.0,
        average_total_tokens: 100.0,
        average_retry_count: 0.1,
    };
    let predicted = OfflineCounterfactualEstimate {
        policy_id: candidate_policy.policy_id.clone(),
        policy_version: candidate_policy.policy_version.clone(),
        policy_hash: candidate_policy.policy_hash.clone(),
        task_class: "coding".to_string(),
        selection: candidate_policy.selections["coding"].clone(),
        source_candidate_id: "strong".to_string(),
        source_candidate_version: "candidate-v2".to_string(),
        source_candidate_definition_sha256: definition_hash,
        source_trace_ids: vec![
            "trace-4".to_string(),
            "trace-5".to_string(),
            "trace-6".to_string(),
        ],
        source_evidence_content_sha256: vec![format!("{:064x}", 6), format!("{:064x}", 7)],
        sample_count: 3,
        estimated_success_rate: 0.9,
        estimated_quality_score: 0.85,
        estimated_tool_success_score: 0.92,
        estimated_cost_usd: 0.06,
        estimated_latency_ms: 700.0,
        estimated_total_tokens: 90.0,
        estimated_retry_count: 0.1,
        estimation_method: "observed_comparable_candidate_cohort".to_string(),
    };
    let comparison = OfflinePolicyComparison {
        policy_id: candidate_policy.policy_id.clone(),
        policy_version: candidate_policy.policy_version.clone(),
        policy_hash: candidate_policy.policy_hash.clone(),
        task_class: "coding".to_string(),
        current_observed_candidate_id: "cheap".to_string(),
        candidate_selection: predicted.selection.clone(),
        current_observed: observed.clone(),
        counterfactual: predicted.clone(),
        success_rate_delta: 0.1,
        quality_score_delta: 0.05,
        tool_success_score_delta: 0.02,
        cost_usd_delta: -0.02,
        latency_ms_delta: -100.0,
        total_tokens_delta: -10.0,
        retry_count_delta: 0.0,
    };
    let mut report = OfflineReplayReport {
        schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
        status: OfflineReplayStatus::Sufficient,
        reason_codes: Vec::new(),
        current_policy,
        candidate_policies: vec![candidate_policy],
        observed_facts: vec![observed],
        counterfactual_estimates: vec![predicted],
        comparisons: vec![comparison],
        outcomes: vec![OfflineReplayOutcome {
            status: OfflineReplayStatus::Sufficient,
            policy_id: Some("candidate-policy".to_string()),
            task_class: Some("coding".to_string()),
            reason_codes: Vec::new(),
        }],
        eligibility_content_sha256: format!("{:064x}", 8),
        replay_judge_calibrations: Vec::new(),
        source_trace_ids: vec![
            "trace-1".to_string(),
            "trace-2".to_string(),
            "trace-3".to_string(),
        ],
        source_evidence_content_sha256: vec![format!("{:064x}", 4), format!("{:064x}", 5)],
        shadow_only: true,
        influence_selected_tier: false,
        influence_executor_type: false,
        influence_retry_path: false,
        influence_routing_policy: false,
        content_sha256: String::new(),
    };
    report.content_sha256 = offline_replay_report_sha256(&report).unwrap();
    report
}

#[test]
fn complete_evidence_chain_is_required_for_promotion() {
    let report = replay_report_for_promotion();
    let shadow = ShadowRouter::compare_replay_report(&report).unwrap();
    let candidate = &report.candidate_policies[0];
    let canary = AdaptiveExperimentController::start_canary(
        &AdaptiveCanaryRequest {
            canary_id: "canary-1".to_string(),
            task_class: "coding".to_string(),
            scope: "team-coding".to_string(),
            policy_version: candidate.policy_version.clone(),
            policy_hash: candidate.policy_hash.clone(),
            candidate_id: "strong".to_string(),
            candidate_version: "candidate-v2".to_string(),
            candidate_definition_sha256: candidate.selections["coding"]
                .candidate_definition_sha256
                .clone(),
            rollout_percentage: 5,
            duration_seconds: 3_600,
            minimum_evidence: 3,
            confirm_canary: true,
            permission_granted: true,
            idempotency_key: "canary-idempotency-1".to_string(),
        },
        &shadow,
        &AdaptiveExperimentGate::from_flags(true, true, false, false),
    )
    .unwrap();
    assert_eq!(canary.schema_version, ADAPTIVE_CANARY_SCHEMA_VERSION);
    let active = baseline_verdict().policy.unwrap();
    let mut chain = AdaptivePromotionEvidenceChain {
        schema_version: ADAPTIVE_PROMOTION_EVIDENCE_CHAIN_SCHEMA_VERSION.to_string(),
        offline: report,
        shadow,
        canary,
        rollout_scope: "team-coding".to_string(),
        rollback_target: active.policy_hash.clone(),
        content_sha256: String::new(),
    };
    chain.finalize();
    let promotion_request = request(Some(active.policy_hash.clone()));
    let verdict = AdaptiveAutoPromotionController::evaluate_with_evidence_chain(
        &promotion_request,
        &chain,
        Some(&active),
        &AdaptiveAutoPromotionPolicy {
            min_samples_per_candidate: 3,
            min_confidence: 0.7,
            ..Default::default()
        },
        &AdaptiveAutoPromotionGate::from_flags(true, true, false),
        true,
        true,
    );
    assert!(verdict.eligible, "{verdict:?}");
    let promoted = verdict.policy.unwrap();
    assert_eq!(
        promoted.evidence_chain_sha256.as_deref(),
        Some(chain.content_sha256.as_str())
    );
    assert!(promoted.is_valid());

    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("promotion-chain.db")).unwrap();
    store
        .apply_adaptive_fusion_policy(&baseline_verdict(), "operator")
        .unwrap();
    let applied = store
        .promote_adaptive_fusion_policy_with_evidence_chain(
            &promotion_request,
            &chain,
            &AdaptiveAutoPromotionPolicy {
                min_samples_per_candidate: 3,
                min_confidence: 0.7,
                ..Default::default()
            },
            &AdaptiveAutoPromotionGate::from_flags(true, true, false),
            "operator",
            true,
            true,
        )
        .unwrap();
    assert_eq!(applied["applied"], true);
    assert_eq!(
        store.active_adaptive_fusion_policies().unwrap()[0]
            .evidence_chain_sha256
            .as_deref(),
        Some(chain.content_sha256.as_str())
    );

    let mut tampered = chain;
    tampered.shadow.content_sha256 = "0".repeat(64);
    assert!(
        !AdaptiveAutoPromotionController::evaluate_with_evidence_chain(
            &promotion_request,
            &tampered,
            Some(&active),
            &AdaptiveAutoPromotionPolicy {
                min_samples_per_candidate: 3,
                min_confidence: 0.7,
                ..Default::default()
            },
            &AdaptiveAutoPromotionGate::from_flags(true, true, false),
            true,
            true,
        )
        .eligible
    );
}
