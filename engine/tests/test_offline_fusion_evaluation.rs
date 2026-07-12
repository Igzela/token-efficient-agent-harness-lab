use engine::feedback::{
    offline_replay_report_sha256, CandidateKind, JudgeEvidence, ObjectiveProfile,
    OfflineEvaluationEngine, OfflinePolicyDefinition, OfflinePolicySelection,
    OfflineReplayObservation, OfflineReplayRequest, OfflineReplayStatus, ReplayEligibilityRequest,
    ReplayEvidenceScope, RunTrace, RunTraceRecorder, OFFLINE_EVALUATION_SCHEMA_VERSION,
    OFFLINE_REPLAY_SCHEMA_VERSION, RUN_TRACE_SCHEMA_VERSION,
};
use engine::storage::LocalProductStore;
use serde_json::json;
use std::collections::BTreeMap;
use tempfile::tempdir;

#[allow(clippy::too_many_arguments)]
fn observation(
    observation_id: &str,
    task_class: &str,
    candidate_id: &str,
    candidate_kind: CandidateKind,
    quality_score: f64,
    success: bool,
    tool_success_score: f64,
    cost_usd: f64,
    latency_ms: u64,
) -> OfflineReplayObservation {
    OfflineReplayObservation {
        schema_version: OFFLINE_EVALUATION_SCHEMA_VERSION.to_string(),
        observation_id: observation_id.to_string(),
        run_id: format!("run-{observation_id}"),
        task_class: task_class.to_string(),
        candidate_id: candidate_id.to_string(),
        candidate_kind,
        member_endpoint_ids: match candidate_kind {
            CandidateKind::Endpoint => vec![candidate_id.to_string()],
            CandidateKind::Portfolio => vec!["alpha".to_string(), "beta".to_string()],
        },
        success,
        quality_score,
        tool_success_score,
        cost_usd,
        latency_ms,
        judge_evidence: None,
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "actual={actual}, expected={expected}"
    );
}

fn replay_hash(value: char) -> String {
    std::iter::repeat_n(value, 64).collect()
}

fn trace(candidate_id: &str, index: usize, policy_hash: &str) -> RunTrace {
    let candidate_definition_sha256 = if candidate_id == "candidate-a" {
        replay_hash('b')
    } else {
        replay_hash('c')
    };
    let (quality_score, cost_usd) = if candidate_id == "candidate-a" {
        (0.8, 0.1)
    } else {
        (0.95, 0.3)
    };
    RunTrace {
        schema_version: RUN_TRACE_SCHEMA_VERSION.to_string(),
        trace_id: format!("trace-{candidate_id}-{index}"),
        dispatch_id: format!("dispatch-{candidate_id}-{index}"),
        history_id: Some(index as i64),
        created_at: Some("2026-07-12T00:00:00Z".to_string()),
        task_class: "code_generate".to_string(),
        task_domain: Some("code".to_string()),
        task_intent: Some("generate".to_string()),
        selected_tier: candidate_id.to_string(),
        selected_profile: None,
        routing_policy: Some("routing-policy".to_string()),
        complexity_score: Some(0.5),
        constraints: Vec::new(),
        human_review_flag: false,
        retry_policy: None,
        shadow_routes: Vec::new(),
        executor_type: "test".to_string(),
        execution_status: Some("completed".to_string()),
        latency_ms: Some(100),
        input_tokens: Some(10),
        output_tokens: Some(20),
        estimated_cost_usd: None,
        reserved_cost: cost_usd,
        total_cost: cost_usd,
        retry_count: 0,
        evaluation_status: "pass".to_string(),
        final_status: "completed".to_string(),
        success: true,
        failure_domain: None,
        analysis: json!({
            "task_class": "code_generate",
            "task_domain": "code",
            "task_intent": "generate",
            "objective": "quality",
            "complexity_bucket": "medium"
        }),
        decision: json!({
            "candidate_id": candidate_id,
            "selected_tier": candidate_id,
            "candidate_version": "candidate-v1",
            "candidate_hash": candidate_definition_sha256,
            "routing_policy": "routing-policy",
            "policy_version": "current-policy-v1",
            "policy_hash": policy_hash,
            "member_endpoint_ids": [candidate_id]
        }),
        execution: json!({
            "status": "completed",
            "latency_ms": 100,
            "input_tokens": 10,
            "output_tokens": 20,
            "actual_cost_usd": cost_usd,
            "retry_count": 0,
            "success": true
        }),
        evaluation: json!({
            "status": "pass",
            "measurement_schema_version": "measurements-v1",
            "quality_score": quality_score,
            "tool_success_score": 0.95,
            "quality_score_source": "reference"
        }),
    }
}

fn replay_eligibility_request(policy_hash: &str) -> ReplayEligibilityRequest {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("replay-owner.db")).unwrap();
    let mut dispatch_ids = Vec::new();
    for candidate_id in ["candidate-a", "candidate-b"] {
        for index in 0..30 {
            let trace = trace(candidate_id, index, policy_hash);
            let dispatch_id = format!("owner-{candidate_id}-{index}");
            let bundle = json!({
                "analysis": trace.analysis,
                "decision": trace.decision,
                "execution_result": {
                    "executor_type": trace.executor_type,
                    "status": trace.execution_status,
                    "latency_ms": trace.latency_ms,
                    "input_tokens": trace.input_tokens,
                    "output_tokens": trace.output_tokens,
                    "actual_cost_usd": trace.total_cost,
                    "retry_count": trace.retry_count,
                    "success": true
                },
                "evaluation_result": trace.evaluation,
                "record": {
                    "dispatch_id": dispatch_id,
                    "created_at": trace.created_at,
                    "final_status": trace.final_status
                }
            });
            let stored = store
                .record_dispatch("{}", "test-owner", &bundle, "test")
                .unwrap();
            dispatch_ids.push(stored["dispatch_id"].as_str().unwrap().to_string());
        }
    }
    store
        .trusted_replay_eligibility_request(
            &dispatch_ids,
            "2026-07-12T00:01:00Z",
            300,
            ReplayEvidenceScope::default(),
        )
        .unwrap()
}

fn policy(
    policy_id: &str,
    candidate_id: &str,
    candidate_definition_sha256: &str,
) -> OfflinePolicyDefinition {
    OfflinePolicyDefinition::new(
        policy_id,
        if policy_id == "current" {
            "current-policy-v1"
        } else {
            "candidate-policy-v1"
        },
        BTreeMap::from([(
            "code_generate".to_string(),
            OfflinePolicySelection {
                candidate_id: candidate_id.to_string(),
                candidate_version: "candidate-v1".to_string(),
                candidate_definition_sha256: candidate_definition_sha256.to_string(),
            },
        )]),
    )
    .unwrap()
}

fn offline_replay_request() -> OfflineReplayRequest {
    let candidate_a_hash = replay_hash('b');
    let candidate_b_hash = replay_hash('c');
    let current = policy("current", "candidate-a", &candidate_a_hash);
    let candidate = policy("candidate-policy", "candidate-b", &candidate_b_hash);
    OfflineReplayRequest {
        schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
        eligibility: replay_eligibility_request(&current.policy_hash),
        current_policy: current,
        candidate_policies: vec![candidate],
    }
}

#[test]
fn run_trace_adapter_reuses_existing_quality_cost_latency_and_success_evidence() {
    let bundle = json!({
        "analysis": {"task_class": "code_generate"},
        "decision": {
            "selected_tier": "balanced_worker",
            "budget_reservation": {"reserved_cost": 0.2}
        },
        "execution_result": {
            "status": "completed",
            "latency_ms": 420,
            "estimated_cost": 0.12
        },
        "evaluation_result": {
            "status": "pass",
            "quality_score": 0.91
        },
        "final_status": "completed"
    });
    let trace = RunTraceRecorder::record_from_bundle(
        &bundle,
        "dispatch-1",
        None,
        Some("2026-06-21T00:00:00Z".to_string()),
    );

    let replay = OfflineReplayObservation::from_run_trace(
        &trace,
        "provider-a/model-a",
        CandidateKind::Endpoint,
        vec!["provider-a/model-a".to_string()],
        0.88,
        Some(JudgeEvidence {
            judge_endpoint_id: "judge-a".to_string(),
            judge_score: 0.95,
            reference_score: 0.91,
        }),
    )
    .unwrap();

    assert_eq!(replay.run_id, "dispatch-1");
    assert_eq!(replay.task_class, "code_generate");
    assert!(replay.success);
    assert_close(replay.quality_score, 0.91);
    assert_close(replay.cost_usd, 0.12);
    assert_eq!(replay.latency_ms, 420);
    assert_eq!(
        replay.observation_id,
        "replay-dispatch-1-provider-a/model-a"
    );
}

#[test]
fn offline_aggregation_preserves_negative_quality_samples() {
    let observations = vec![
        observation(
            "negative-sample",
            "code_review",
            "endpoint-a",
            CandidateKind::Endpoint,
            0.1,
            false,
            0.2,
            0.1,
            200,
        ),
        observation(
            "positive-sample",
            "code_review",
            "endpoint-a",
            CandidateKind::Endpoint,
            0.9,
            true,
            0.8,
            0.1,
            200,
        ),
    ];
    let report = OfflineEvaluationEngine::evaluate(&observations).unwrap();
    let aggregate = &report.task_classes[0].candidates[0];
    assert_close(aggregate.success_rate, 0.5);
    assert_close(aggregate.average_quality_score, 0.5);
    assert_close(aggregate.average_tool_success_score, 0.5);
}

#[test]
fn evaluation_aggregates_endpoint_and_portfolio_metrics_deterministically() {
    let observations = vec![
        observation(
            "1",
            "code_generate",
            "endpoint-a",
            CandidateKind::Endpoint,
            0.8,
            true,
            0.9,
            0.1,
            200,
        ),
        observation(
            "2",
            "code_generate",
            "endpoint-a",
            CandidateKind::Endpoint,
            1.0,
            false,
            0.7,
            0.3,
            400,
        ),
        observation(
            "3",
            "code_generate",
            "portfolio-ab",
            CandidateKind::Portfolio,
            0.95,
            true,
            0.95,
            0.4,
            600,
        ),
    ];
    let mut reversed = observations.clone();
    reversed.reverse();

    let first = OfflineEvaluationEngine::evaluate(&observations).unwrap();
    let second = OfflineEvaluationEngine::evaluate(&reversed).unwrap();

    assert_eq!(first, second);
    let task = &first.task_classes[0];
    assert_eq!(task.task_class, "code_generate");
    assert_eq!(task.candidates.len(), 2);
    let endpoint = task
        .candidates
        .iter()
        .find(|candidate| candidate.candidate_id == "endpoint-a")
        .unwrap();
    assert_eq!(endpoint.sample_count, 2);
    assert_close(endpoint.success_rate, 0.5);
    assert_close(endpoint.average_quality_score, 0.9);
    assert_close(endpoint.average_tool_success_score, 0.8);
    assert_close(endpoint.average_cost_usd, 0.2);
    assert_close(endpoint.average_latency_ms, 300.0);
}

#[test]
fn pareto_frontier_and_objective_recommendations_preserve_tradeoffs() {
    let observations = vec![
        observation(
            "cheap",
            "code_generate",
            "cheap",
            CandidateKind::Endpoint,
            0.75,
            true,
            0.9,
            0.02,
            100,
        ),
        observation(
            "premium",
            "code_generate",
            "premium",
            CandidateKind::Endpoint,
            0.98,
            true,
            0.95,
            0.5,
            1000,
        ),
        observation(
            "dominated",
            "code_generate",
            "dominated",
            CandidateKind::Endpoint,
            0.7,
            true,
            0.8,
            0.3,
            500,
        ),
    ];

    let report = OfflineEvaluationEngine::evaluate(&observations).unwrap();
    let task = &report.task_classes[0];

    assert_eq!(task.pareto_candidate_ids, vec!["cheap", "premium"]);
    let efficient = task
        .recommendations
        .iter()
        .find(|recommendation| recommendation.objective == ObjectiveProfile::Efficient)
        .unwrap();
    let quality = task
        .recommendations
        .iter()
        .find(|recommendation| recommendation.objective == ObjectiveProfile::Quality)
        .unwrap();
    assert_eq!(efficient.candidate_id, "cheap");
    assert_eq!(quality.candidate_id, "premium");
    assert!(efficient.shadow_only);
    assert!(quality.shadow_only);
    assert!(!efficient.influence_routing_policy);
    assert!(!quality.influence_routing_policy);
}

#[test]
fn judge_calibration_reports_signed_bias_and_requires_three_samples() {
    let mut observations = Vec::new();
    for (index, (judge_score, reference_score)) in
        [(0.9, 0.8), (0.8, 0.7), (0.7, 0.6)].into_iter().enumerate()
    {
        let mut item = observation(
            &format!("judge-a-{index}"),
            "code_review",
            "endpoint-a",
            CandidateKind::Endpoint,
            reference_score,
            true,
            1.0,
            0.1,
            100,
        );
        item.judge_evidence = Some(JudgeEvidence {
            judge_endpoint_id: "judge-a".to_string(),
            judge_score,
            reference_score,
        });
        observations.push(item);
    }
    for (index, score) in [0.7, 0.8].into_iter().enumerate() {
        let mut item = observation(
            &format!("judge-b-{index}"),
            "code_review",
            "endpoint-a",
            CandidateKind::Endpoint,
            score,
            true,
            1.0,
            0.1,
            100,
        );
        item.judge_evidence = Some(JudgeEvidence {
            judge_endpoint_id: "judge-b".to_string(),
            judge_score: score,
            reference_score: score,
        });
        observations.push(item);
    }

    let report = OfflineEvaluationEngine::evaluate(&observations).unwrap();
    let judge_a = report
        .judge_calibrations
        .iter()
        .find(|calibration| calibration.judge_endpoint_id == "judge-a")
        .unwrap();
    let judge_b = report
        .judge_calibrations
        .iter()
        .find(|calibration| calibration.judge_endpoint_id == "judge-b")
        .unwrap();

    assert_eq!(judge_a.sample_count, 3);
    assert_eq!(judge_a.status, "within_tolerance");
    assert_close(judge_a.mean_signed_bias, 0.1);
    assert_close(judge_a.mean_absolute_error, 0.1);
    assert_close(judge_a.recommended_score_offset, -0.1);
    assert_eq!(judge_b.status, "insufficient_data");
}

#[test]
fn invalid_secret_and_duplicate_observations_are_rejected_without_id_disclosure() {
    let valid = observation(
        "valid",
        "code_generate",
        "endpoint-a",
        CandidateKind::Endpoint,
        0.8,
        true,
        0.9,
        0.1,
        200,
    );
    let duplicate = valid.clone();
    let mut secret = valid.clone();
    secret.observation_id = "sk-abcdefghijklmnopqrstuvwxyz".to_string();

    let report = OfflineEvaluationEngine::evaluate(&[valid, duplicate, secret]).unwrap();
    let serialized = serde_json::to_string(&report).unwrap();

    assert_eq!(report.accepted_observation_count, 1);
    assert_eq!(report.rejected_observation_count, 2);
    assert_eq!(report.rejection_reasons["duplicate_observation_id"], 1);
    assert_eq!(report.rejection_reasons["sensitive_pattern_detected"], 1);
    assert!(!serialized.contains("sk-abcdefghijklmnopqrstuvwxyz"));
}

#[test]
fn rejected_observation_count_is_not_inflated_by_multiple_violations() {
    let mut invalid = observation(
        "invalid",
        "code_generate",
        "endpoint-a",
        CandidateKind::Endpoint,
        0.8,
        true,
        0.9,
        0.1,
        200,
    );
    invalid.task_class = "invalid task class".to_string();
    invalid.quality_score = 2.0;
    invalid.cost_usd = -1.0;

    let report = OfflineEvaluationEngine::evaluate(&[invalid]).unwrap();

    assert_eq!(report.accepted_observation_count, 0);
    assert_eq!(report.rejected_observation_count, 1);
    assert_eq!(report.rejection_reasons.len(), 3);
}

#[test]
fn extreme_finite_cost_is_rejected_before_aggregation() {
    let mut invalid = observation(
        "invalid-cost",
        "code_generate",
        "endpoint-a",
        CandidateKind::Endpoint,
        0.8,
        true,
        0.9,
        f64::MAX,
        200,
    );

    let report = OfflineEvaluationEngine::evaluate(&[invalid.clone()]).unwrap();
    invalid.cost_usd = 1_000_000.0;
    let boundary = OfflineEvaluationEngine::evaluate(&[invalid]).unwrap();

    assert_eq!(report.accepted_observation_count, 0);
    assert_eq!(report.rejection_reasons["invalid_cost_usd"], 1);
    assert_eq!(boundary.accepted_observation_count, 1);
}

#[test]
fn evaluation_rejects_unbounded_input_before_processing() {
    let observations = (0..10_001)
        .map(|index| {
            observation(
                &format!("observation-{index}"),
                "code_generate",
                "endpoint-a",
                CandidateKind::Endpoint,
                0.8,
                true,
                0.9,
                0.1,
                200,
            )
        })
        .collect::<Vec<_>>();

    let error = OfflineEvaluationEngine::evaluate(&observations).unwrap_err();

    assert_eq!(error.code, "observation_limit_exceeded");
}

#[test]
fn evaluation_bounds_candidates_per_task_class_before_pareto_analysis() {
    let observations = (0..513)
        .map(|index| {
            let candidate_id = format!("candidate-{index:03}");
            observation(
                &format!("observation-{index:03}"),
                "code_generate",
                &candidate_id,
                CandidateKind::Endpoint,
                0.8,
                true,
                0.9,
                0.1,
                200,
            )
        })
        .collect::<Vec<_>>();

    let report = OfflineEvaluationEngine::evaluate(&observations).unwrap();

    assert_eq!(report.accepted_observation_count, 512);
    assert_eq!(report.rejected_observation_count, 1);
    assert_eq!(report.rejection_reasons["task_candidate_limit_exceeded"], 1);
}

#[test]
fn report_is_always_shadow_only_with_zero_live_influence() {
    let report = OfflineEvaluationEngine::evaluate(&[observation(
        "1",
        "code_generate",
        "endpoint-a",
        CandidateKind::Endpoint,
        0.8,
        true,
        0.9,
        0.1,
        200,
    )])
    .unwrap();

    assert_eq!(report.schema_version, OFFLINE_EVALUATION_SCHEMA_VERSION);
    assert!(report.shadow_only);
    assert!(!report.influence_selected_tier);
    assert!(!report.influence_executor_type);
    assert!(!report.influence_retry_path);
    assert!(!report.influence_routing_policy);
}

#[test]
fn trace_replay_separates_observed_facts_and_counterfactual_estimates() {
    let request = offline_replay_request();
    let report = OfflineEvaluationEngine::replay_policies(&request).unwrap();

    assert_eq!(report.status, OfflineReplayStatus::Sufficient);
    assert_eq!(report.observed_facts.len(), 2);
    assert_eq!(report.counterfactual_estimates.len(), 1);
    assert_eq!(report.comparisons.len(), 1);
    assert_eq!(
        report.counterfactual_estimates[0].policy_id,
        "candidate-policy"
    );
    assert_eq!(
        report.counterfactual_estimates[0].estimation_method,
        "observed_comparable_candidate_cohort"
    );
    assert_eq!(
        report.counterfactual_estimates[0].source_candidate_id,
        "candidate-b"
    );
    assert_close(report.comparisons[0].quality_score_delta, 0.15);
    assert_close(report.comparisons[0].cost_usd_delta, 0.2);
    assert!(report.shadow_only);
    assert!(!report.influence_routing_policy);
    assert_eq!(report.source_trace_ids.len(), 60);
    assert!(report
        .source_trace_ids
        .contains(&"trace-owner-candidate-a-0".to_string()));
}

#[test]
fn trace_replay_is_deterministic_and_policy_hash_bound() {
    let request = offline_replay_request();
    let first = OfflineEvaluationEngine::replay_policies(&request).unwrap();
    let second = OfflineEvaluationEngine::replay_policies(&request).unwrap();

    assert_eq!(first, second);
    assert_eq!(
        offline_replay_report_sha256(&first).unwrap(),
        first.content_sha256
    );
    assert_eq!(
        first.current_policy.policy_hash,
        request.current_policy.policy_hash
    );
    assert_eq!(
        first.counterfactual_estimates[0].policy_hash,
        request.candidate_policies[0].policy_hash
    );
    assert_eq!(
        first.counterfactual_estimates[0]
            .selection
            .candidate_definition_sha256,
        replay_hash('c')
    );

    let mut reordered = request;
    reordered.candidate_policies = vec![
        policy("z-policy", "candidate-z", &replay_hash('d')),
        policy("candidate-policy", "candidate-b", &replay_hash('c')),
    ];
    let mut reversed = reordered.clone();
    reversed.candidate_policies.reverse();
    assert_eq!(
        OfflineEvaluationEngine::replay_policies(&reordered).unwrap(),
        OfflineEvaluationEngine::replay_policies(&reversed).unwrap()
    );
}

#[test]
fn trace_replay_rejects_tampered_evidence_before_estimation() {
    let mut request = offline_replay_request();
    request.eligibility.traces[0].trace.evaluation["quality_score"] = json!(0.1);

    let report = OfflineEvaluationEngine::replay_policies(&request).unwrap();

    assert_eq!(report.status, OfflineReplayStatus::TamperedEvidence);
    assert!(report
        .reason_codes
        .contains(&"tampered_evidence".to_string()));
    assert!(report.counterfactual_estimates.is_empty());
}

#[test]
fn trace_replay_exposes_stale_and_out_of_distribution_outcomes() {
    let mut stale_request = offline_replay_request();
    stale_request.eligibility.generated_at = "2026-08-12T00:01:00Z".to_string();
    let stale = OfflineEvaluationEngine::replay_policies(&stale_request).unwrap();
    assert_eq!(stale.status, OfflineReplayStatus::StaleEvidence);

    let mut ood_request = offline_replay_request();
    // Caller limits constrain the request but cannot create empirical support
    // for a candidate that is absent from the accepted owner-backed cohort.
    ood_request.eligibility.scope.max_total_tokens = Some(1_000);
    ood_request.eligibility.scope.max_latency_ms = Some(1_000);
    ood_request.candidate_policies = vec![policy("ood-policy", "candidate-z", &replay_hash('d'))];
    let ood = OfflineEvaluationEngine::replay_policies(&ood_request).unwrap();
    assert_eq!(ood.status, OfflineReplayStatus::OutOfDistribution);
    assert!(ood
        .reason_codes
        .contains(&"candidate_policy_candidate_not_observed_in_comparable_cohort".to_string()));
}
