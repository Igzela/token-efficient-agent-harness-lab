use engine::feedback::{
    AdaptiveExperimentController, AdaptiveExperimentGate, AdaptiveExperimentLimits,
    AdaptiveExperimentPolicy, AdaptiveExperimentRequest, AdaptiveExplorationGate,
    ContextualPolicyPromotion, ContextualPolicyPromotionGate, ObjectiveProfile,
    CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION, CONTEXTUAL_POLICY_SCHEMA_VERSION,
};
use engine::node_executor::{NodeExecutionInput, NodeExecutor};
use engine::provider::adaptive_execution::{
    AdaptiveExecutionExecutor, AdaptiveExecutionGate, AdaptiveExecutionKillSwitch,
    AdaptiveProviderNodeExecutor,
};
use engine::provider::{Provider, ProviderAuditRecorder};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;

fn request(request_id: &str, risk_level: &str) -> AdaptiveExperimentRequest {
    AdaptiveExperimentRequest {
        request_id: request_id.to_string(),
        exploration_seed: 17,
        risk_level: risk_level.to_string(),
    }
}

fn policy(traffic_rate: f64) -> AdaptiveExperimentPolicy {
    AdaptiveExperimentPolicy {
        traffic_rate,
        max_cost_usd: 0.2,
        max_total_tokens: 4_096,
        max_calls: 5,
        max_elapsed_ms: 10_000,
        max_concurrency: 3,
    }
}

fn limits() -> AdaptiveExperimentLimits {
    AdaptiveExperimentLimits {
        reserved_cost_usd: 0.1,
        max_cost_usd: 0.2,
        max_total_tokens: 2_048,
        max_calls: 4,
        max_elapsed_ms: 5_000,
        max_concurrency: 2,
    }
}

#[test]
fn experiments_are_disabled_by_default() {
    let decision = AdaptiveExperimentController::decide(
        &request("request-1", "low"),
        &policy(0.05),
        &AdaptiveExperimentGate::from_flags(false, false, false, false),
    )
    .unwrap();

    assert!(!decision.assigned);
    assert_eq!(
        decision.blocked_reasons,
        vec!["adaptive_experiment_gates_disabled"]
    );
}

#[test]
fn deterministic_traffic_assignment_is_stable_and_rate_bounded() {
    let gate = AdaptiveExperimentGate::from_flags(true, true, false, false);
    let policy = policy(0.05);
    let first = AdaptiveExperimentController::decide(&request("request-42", "low"), &policy, &gate)
        .unwrap();
    let second =
        AdaptiveExperimentController::decide(&request("request-42", "low"), &policy, &gate)
            .unwrap();

    assert_eq!(first, second);
    assert!((0.0..1.0).contains(&first.bucket));
    assert_eq!(first.assigned, first.bucket < policy.traffic_rate);
    assert_eq!(first.traffic_rate, 0.05);
}

#[test]
fn pause_kill_and_high_risk_block_experiments() {
    let active = policy(0.05);
    let paused = AdaptiveExperimentController::decide(
        &request("request-1", "low"),
        &active,
        &AdaptiveExperimentGate::from_flags(true, true, true, false),
    )
    .unwrap();
    let killed = AdaptiveExperimentController::decide(
        &request("request-1", "low"),
        &active,
        &AdaptiveExperimentGate::from_flags(true, true, false, true),
    )
    .unwrap();
    let high_risk = AdaptiveExperimentController::decide(
        &request("request-1", "high"),
        &active,
        &AdaptiveExperimentGate::from_flags(true, true, false, false),
    )
    .unwrap();

    assert_eq!(paused.blocked_reasons, vec!["adaptive_experiment_paused"]);
    assert_eq!(
        killed.blocked_reasons,
        vec!["adaptive_experiment_kill_switch_active"]
    );
    assert_eq!(
        high_risk.blocked_reasons,
        vec!["adaptive_experiment_risk_blocked"]
    );
}

#[test]
fn budget_token_call_time_and_concurrency_caps_are_all_enforced() {
    let policy = policy(0.05);
    assert!(AdaptiveExperimentController::validate_limits(&limits(), &policy).is_ok());

    let cases = [
        (
            AdaptiveExperimentLimits {
                reserved_cost_usd: 0.21,
                ..limits()
            },
            "adaptive_experiment_cost_cap_exceeded",
        ),
        (
            AdaptiveExperimentLimits {
                max_cost_usd: 0.21,
                ..limits()
            },
            "adaptive_experiment_cost_cap_exceeded",
        ),
        (
            AdaptiveExperimentLimits {
                max_total_tokens: 4_097,
                ..limits()
            },
            "adaptive_experiment_token_cap_exceeded",
        ),
        (
            AdaptiveExperimentLimits {
                max_calls: 6,
                ..limits()
            },
            "adaptive_experiment_call_cap_exceeded",
        ),
        (
            AdaptiveExperimentLimits {
                max_elapsed_ms: 10_001,
                ..limits()
            },
            "adaptive_experiment_time_cap_exceeded",
        ),
        (
            AdaptiveExperimentLimits {
                max_concurrency: 4,
                ..limits()
            },
            "adaptive_experiment_concurrency_cap_exceeded",
        ),
    ];

    for (limits, expected) in cases {
        assert_eq!(
            AdaptiveExperimentController::validate_limits(&limits, &policy).unwrap_err(),
            expected
        );
    }
}

#[test]
fn invalid_policy_and_request_are_rejected_without_assignment() {
    let gate = AdaptiveExperimentGate::from_flags(true, true, false, false);
    let mut invalid_policy = policy(0.06);
    invalid_policy.max_cost_usd = f64::NAN;
    let error = AdaptiveExperimentController::decide(
        &request("/private/repo", "low"),
        &invalid_policy,
        &gate,
    )
    .unwrap_err();

    assert_eq!(error.code, "adaptive_experiment_validation_failed");
    assert!(error.violations.contains(&"invalid_request_id".to_string()));
    assert!(error
        .violations
        .contains(&"invalid_traffic_rate".to_string()));
    assert!(error
        .violations
        .contains(&"invalid_experiment_cost_cap".to_string()));
}

fn promoted_policy() -> engine::feedback::PromotedAdaptivePolicy {
    ContextualPolicyPromotionGate::from_flags(true, true)
        .evaluate(&ContextualPolicyPromotion {
            schema_version: CONTEXTUAL_POLICY_PROMOTION_SCHEMA_VERSION.to_string(),
            task_class: "coding".to_string(),
            objective: ObjectiveProfile::Quality,
            candidate_id: "strong".to_string(),
            baseline_candidate_id: "cheap".to_string(),
            sample_count: 30,
            confidence: 0.9,
            mean_quality_delta: 0.1,
            mean_cost_reduction: 0.01,
            failure_rate_delta: 0.0,
            evidence_run_ids: (0..30).map(|index| format!("run-{index}")).collect(),
            risk_level: "low".to_string(),
            confirm_adaptive_policy_promotion: true,
        })
        .policy
        .unwrap()
}

fn assigned_request_id(policy: &AdaptiveExperimentPolicy, gate: &AdaptiveExperimentGate) -> String {
    (0..10_000)
        .map(|index| format!("experiment-request-{index}"))
        .find(|request_id| {
            AdaptiveExperimentController::decide(&request(request_id, "low"), policy, gate)
                .unwrap()
                .assigned
        })
        .expect("5% traffic should assign a bounded test request")
}

fn contextual_input(request_id: &str) -> NodeExecutionInput {
    NodeExecutionInput {
        node_id: "node-1".to_string(),
        task_type: "generate".to_string(),
        run_id: "run-experiment".to_string(),
        workflow_id: "workflow-1".to_string(),
        node_metadata: json!({
            "prompt": "solve",
            "adaptive_policy_execution": {
                "request": {
                    "schema_version": CONTEXTUAL_POLICY_SCHEMA_VERSION,
                    "request_id": request_id,
                    "task_class": "coding",
                    "objective": "quality",
                    "risk_level": "low",
                    "exploration_seed": 17
                },
                "evaluation": {
                    "task_class": "coding",
                    "candidates": [
                        {
                            "candidate_id": "cheap",
                            "candidate_kind": "endpoint",
                            "member_endpoint_ids": ["cheap"],
                            "sample_count": 30,
                            "evidence_run_ids": ["cheap-evidence"],
                            "success_rate": 0.76,
                            "average_quality_score": 0.76,
                            "average_tool_success_score": 0.76,
                            "average_cost_usd": 0.01,
                            "average_latency_ms": 400.0
                        },
                        {
                            "candidate_id": "strong",
                            "candidate_kind": "endpoint",
                            "member_endpoint_ids": ["strong"],
                            "sample_count": 30,
                            "evidence_run_ids": ["strong-evidence"],
                            "success_rate": 0.92,
                            "average_quality_score": 0.92,
                            "average_tool_success_score": 0.92,
                            "average_cost_usd": 0.08,
                            "average_latency_ms": 1_200.0
                        }
                    ],
                    "pareto_candidate_ids": ["cheap", "strong"],
                    "recommendations": []
                },
                "observations": [],
                "candidate_plans": {
                    "cheap": {
                        "plan": {
                            "mode": "single",
                            "endpoint": {
                                "endpoint_id": "cheap",
                                "model": "test-model",
                                "reserved_cost_usd": 0.01
                            }
                        },
                        "limits": {
                            "max_calls": 1,
                            "max_cost_usd": 0.1,
                            "max_elapsed_ms": 1000,
                            "max_concurrency": 1,
                            "max_total_tokens": 2048
                        }
                    },
                    "strong": {
                        "plan": {
                            "mode": "single",
                            "endpoint": {
                                "endpoint_id": "strong",
                                "model": "test-model",
                                "reserved_cost_usd": 0.02
                            }
                        },
                        "limits": {
                            "max_calls": 1,
                            "max_cost_usd": 0.1,
                            "max_elapsed_ms": 1000,
                            "max_concurrency": 1,
                            "max_total_tokens": 2048
                        }
                    }
                }
            }
        }),
    }
}

fn online_executor(
    policy: AdaptiveExperimentPolicy,
    gate: AdaptiveExperimentGate,
    audit: Arc<ProviderAuditRecorder>,
) -> AdaptiveProviderNodeExecutor {
    let providers = ["cheap", "strong"]
        .into_iter()
        .map(|endpoint_id| {
            (
                endpoint_id.to_string(),
                Arc::new(
                    engine::provider::stub::StubProvider::new(endpoint_id)
                        .with_default_model("test-model"),
                ) as Arc<dyn Provider>,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let execution = Arc::new(AdaptiveExecutionExecutor::new(
        providers,
        audit,
        AdaptiveExecutionKillSwitch::from_flags(false),
    ));
    AdaptiveProviderNodeExecutor::new(
        execution,
        AdaptiveExecutionGate::from_flags(true, true, true),
    )
    .with_contextual_policies(
        vec![promoted_policy()],
        AdaptiveExplorationGate::from_flags(false, false, false, 0.0),
    )
    .with_online_experiments(policy, gate)
}

#[test]
fn assigned_experiment_executes_alternative_and_creates_observation() {
    let policy = policy(0.05);
    let gate = AdaptiveExperimentGate::from_flags(true, true, false, false);
    let request_id = assigned_request_id(&policy, &gate);
    let audit = Arc::new(ProviderAuditRecorder::new());
    let executor = online_executor(policy, gate, audit.clone());

    let output = executor.execute_node(&contextual_input(&request_id));

    assert_eq!(output.status, "completed");
    let observation = executor.take_observation().unwrap();
    assert_eq!(observation.candidate_id, "cheap");
    assert!(observation.success);
    assert!(audit
        .list_events("workflow:run-experiment:node-1")
        .iter()
        .any(|event| event.event_type == "adaptive_experiment_assigned"));
}

#[test]
fn experiment_cap_block_falls_back_to_promoted_candidate() {
    let mut policy = policy(0.05);
    policy.max_cost_usd = 0.005;
    let gate = AdaptiveExperimentGate::from_flags(true, true, false, false);
    let request_id = assigned_request_id(&policy, &gate);
    let audit = Arc::new(ProviderAuditRecorder::new());
    let executor = online_executor(policy, gate, audit.clone());

    let output = executor.execute_node(&contextual_input(&request_id));

    assert_eq!(output.status, "completed");
    let observation = executor.take_observation().unwrap();
    assert_eq!(observation.candidate_id, "strong");
    assert!(audit
        .list_events("workflow:run-experiment:node-1")
        .iter()
        .any(|event| {
            event.event_type == "adaptive_experiment_blocked"
                && event.error_domain.as_deref() == Some("adaptive_experiment_cost_cap_exceeded")
        }));
}
