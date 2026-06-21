use engine::feedback::{
    CandidateKind, JudgeEvidence, ObjectiveProfile, OfflineEvaluationEngine,
    OfflineReplayObservation, RunTraceRecorder, OFFLINE_EVALUATION_SCHEMA_VERSION,
};
use serde_json::json;

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
    assert_eq!(judge_a.status, "calibrated");
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
