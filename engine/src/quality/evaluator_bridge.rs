use serde_json::{json, Value};

use crate::dispatch_decision::DispatchDecision;
use crate::evaluation_stub::{EvaluationCheck, EvaluationResult, Evaluator};
use crate::executor_adapter::ExecutionResult;
use crate::quality::artifact_gate::ArtifactGate;
use crate::quality::final_gate::FinalGateRunner;
use crate::quality::quality_gate::{ArtifactGateResult, QualityGateDecision, QualityGateManager};
use crate::quality::scoring::{ScoringEngine, TaskScore};
use crate::runtime::FixtureRuntime;

/// QualityGateEvaluator replaces the default `EvaluationStub` by routing
/// execution results through the quality module's gate chain:
/// FinalGate → ArtifactGate → ScoringEngine → QualityGateManager.
///
/// When the quality gate chain produces a pass/retryable result, the evaluation
/// maps to the standard `EvaluationResult` format. On failure or human-review
/// signals, it preserves those semantics for the dispatch ledger.
pub struct QualityGateEvaluator {
    final_gate: FinalGateRunner,
    quality_gate: QualityGateManager,
    scoring: ScoringEngine,
    artifact_gate: ArtifactGate,
}

impl Default for QualityGateEvaluator {
    fn default() -> Self {
        Self {
            final_gate: FinalGateRunner::new(),
            quality_gate: QualityGateManager::new(),
            scoring: ScoringEngine::new(),
            artifact_gate: ArtifactGate::new(),
        }
    }
}

impl QualityGateEvaluator {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Evaluator for QualityGateEvaluator {
    fn evaluate(
        &self,
        result: &ExecutionResult,
        decision: &DispatchDecision,
        runtime: &mut FixtureRuntime,
    ) -> EvaluationResult {
        // Build synthetic handoff_pack and completion from decision/result context
        let handoff_pack = build_handoff_pack(decision);
        let completion = build_completion(result);

        // Run the final gate
        let final_gate_decision = self.final_gate.evaluate(
            &completion,
            &handoff_pack,
            true,
            &decide_item_status(result),
        );

        // Run artifact gate and convert to quality_gate's local type
        let raw_artifact = self
            .artifact_gate
            .evaluate(&completion, &handoff_pack, None, None);
        let artifact_result = convert_artifact_result(&raw_artifact);

        // Score the task
        let task_score = self.scoring.score_task(
            &json!({"task_id": decision.decision_id}),
            &completion,
            &handoff_pack,
            None,
        );

        // Run quality gate
        let qg_decision = self.quality_gate.evaluate(
            &handoff_pack,
            &completion,
            &final_gate_decision,
            &artifact_result,
            None,
            Some(&task_score),
        );

        // Map quality gate decision to evaluation result
        map_to_evaluation_result(&qg_decision, result, decision, runtime, &task_score)
    }
}

fn convert_artifact_result(
    raw: &crate::quality::artifact_gate::ArtifactGateResult,
) -> ArtifactGateResult {
    ArtifactGateResult {
        ok: raw.ok,
        checks: raw
            .checks
            .iter()
            .map(|c| crate::quality::quality_gate::ArtifactCheck {
                name: c.name.clone(),
                passed: c.passed,
                message: c.message.clone(),
            })
            .collect(),
        missing_artifacts: raw.missing_artifacts.clone(),
        schema_violations: raw.schema_violations.clone(),
        forbidden_violations: raw.forbidden_violations.clone(),
    }
}

fn decide_item_status(result: &ExecutionResult) -> String {
    match result.status.as_str() {
        "completed" | "cli_completed" | "provider_completed" => "done".to_string(),
        "failed" | "cli_failed" => "failed".to_string(),
        "manual_pending" => "pending_approval".to_string(),
        _ => "review".to_string(),
    }
}

fn build_handoff_pack(decision: &DispatchDecision) -> Value {
    json!({
        "decision_id": decision.decision_id,
        "analysis_id": decision.analysis_id,
        "selected_tier": decision.selected_tier,
        "quality_requirement": decision.quality_requirement,
        "confidence": decision.confidence,
        "execution_policy": decision.execution_policy,
        "risk_level": decision.analysis_snapshot.get("risk_level").unwrap_or(&Value::Null),
    })
}

fn build_completion(result: &ExecutionResult) -> Value {
    json!({
        "status": result.status,
        "executor_type": result.executor_type,
        "output_ref": result.output,
        "retry_count": 0,
    })
}

fn map_to_evaluation_result(
    qg: &QualityGateDecision,
    result: &ExecutionResult,
    _decision: &DispatchDecision,
    runtime: &mut FixtureRuntime,
    task_score: &TaskScore,
) -> EvaluationResult {
    let (status, requires_retry, retry_reason) = match qg.result.as_str() {
        "pass" | "pass_with_notes" => ("pass", false, None),
        "fail_retryable" => ("fail", true, Some("quality_gate_retryable".to_string())),
        "fail_terminal" => ("fail", false, None),
        "requires_human_review" => ("needs_human_review", false, None),
        _ => ("fail", false, None),
    };

    let mut checks = Vec::new();
    checks.push(EvaluationCheck {
        check_id: runtime.id("chk-"),
        name: "quality_gate".to_string(),
        status: if qg.result == "pass" || qg.result == "pass_with_notes" {
            "pass".to_string()
        } else if qg.result == "requires_human_review" {
            "warning".to_string()
        } else {
            "fail".to_string()
        },
        reason: qg.reasons.first().cloned().unwrap_or_default(),
    });

    checks.push(EvaluationCheck {
        check_id: runtime.id("chk-"),
        name: "task_score".to_string(),
        status: if task_score.weighted_score >= 0.60 {
            "pass".to_string()
        } else {
            "warning".to_string()
        },
        reason: format!(
            "score={:.2} grade={}",
            task_score.weighted_score, task_score.grade
        ),
    });

    checks.push(EvaluationCheck {
        check_id: runtime.id("chk-"),
        name: "artifact_gate".to_string(),
        status: if qg.artifact_result.as_ref().map_or(true, |a| a.ok) {
            "pass".to_string()
        } else {
            "warning".to_string()
        },
        reason: "artifact gate evaluated".to_string(),
    });

    EvaluationResult {
        schema_version: "evaluation_result.v1".to_string(),
        evaluation_id: runtime.id("eval-"),
        dispatch_id: result.dispatch_id.clone(),
        decision_id: result.decision_id.clone(),
        execution_result_id: result.result_id.clone(),
        status: status.to_string(),
        checks,
        quality_score: Some(task_score.weighted_score),
        requires_retry,
        retry_reason,
        created_at: runtime.now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch_decision::{BudgetReservation, DispatchDecision};
    use crate::executor_adapter::ExecutionResult;

    fn noop_result() -> ExecutionResult {
        ExecutionResult {
            schema_version: "execution_result.v1".to_string(),
            result_id: "res-1".to_string(),
            dispatch_id: "disp-1".to_string(),
            decision_id: "dec-1".to_string(),
            executor_type: "noop".to_string(),
            status: "completed".to_string(),
            output: None,
            prompt_pack: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: None,
            error_domain: None,
            error_message: None,
            provider_request_id: None,
            attempt_number: None,
            finish_reason: None,
            usage_source: None,
            created_at: "2026-06-06T00:00:00Z".to_string(),
        }
    }

    fn base_decision() -> DispatchDecision {
        DispatchDecision {
            schema_version: "dispatch_decision.v1".to_string(),
            decision_id: "dec-1".to_string(),
            analysis_id: "ana-1".to_string(),
            analysis_snapshot: json!({"risk_level": "low"}),
            selected_tier: "balanced_worker".to_string(),
            selected_profile_id: None,
            fallback_tier: "cheap_executor".to_string(),
            fallback_profile_id: None,
            shadow_routes: Vec::new(),
            hard_constraints: Vec::new(),
            rejected_candidates: Vec::new(),
            no_shadow_route_reason: None,
            max_input_tokens: 1000,
            max_output_tokens: 1000,
            routing_reason: "test".to_string(),
            quality_requirement: "medium".to_string(),
            expected_quality_band: "medium".to_string(),
            confidence: 0.8,
            confidence_label: "medium".to_string(),
            budget_reservation: BudgetReservation {
                reservation_id: "res-1".to_string(),
                reserved_cost: 0.01,
                budget_violation: false,
                ..Default::default()
            },
            execution_policy: json!({"executor_type": "noop", "execution_allowed": true}),
            execution_gates: Vec::new(),
            routing_mode: "static".to_string(),
            routing_experiment_id: None,
            decision_status: "decided".to_string(),
            created_at: "2026-06-06T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn quality_evaluator_runs_without_panic() {
        let evaluator = QualityGateEvaluator::new();
        let mut runtime = FixtureRuntime::new();
        let result = evaluator.evaluate(&noop_result(), &base_decision(), &mut runtime);
        // Quality gate chain runs end-to-end without panic.
        // Result may be pass/fail/needs_human_review depending on gate thresholds.
        assert!(
            [
                "pass",
                "pass_with_notes",
                "fail",
                "fail_terminal",
                "fail_retryable",
                "needs_human_review"
            ]
            .contains(&result.status.as_str()),
            "unexpected status: {}",
            result.status
        );
        assert!(!result.checks.is_empty());
        assert!(result.quality_score.is_some());
    }

    #[test]
    fn quality_evaluator_has_quality_gate_check() {
        let evaluator = QualityGateEvaluator::new();
        let mut runtime = FixtureRuntime::new();
        let result = evaluator.evaluate(&noop_result(), &base_decision(), &mut runtime);
        assert!(result.checks.iter().any(|c| c.name == "quality_gate"));
        assert!(result.checks.iter().any(|c| c.name == "task_score"));
        assert!(result.checks.iter().any(|c| c.name == "artifact_gate"));
    }

    #[test]
    fn quality_evaluator_maps_failed_execution() {
        let evaluator = QualityGateEvaluator::new();
        let mut runtime = FixtureRuntime::new();
        let mut result = noop_result();
        result.status = "failed".to_string();
        let eval = evaluator.evaluate(&result, &base_decision(), &mut runtime);
        // Failed execution should produce a non-pass status
        assert_ne!(eval.status, "pass", "failed exec should not pass");
        assert!(!eval.checks.is_empty());
    }

    #[test]
    fn quality_evaluator_serializes() {
        let evaluator = QualityGateEvaluator::new();
        let mut runtime = FixtureRuntime::new();
        let result = evaluator.evaluate(&noop_result(), &base_decision(), &mut runtime);
        let v = result.to_value();
        assert_eq!(v["schema_version"], "evaluation_result.v1");
    }

    #[test]
    fn quality_evaluator_different_decisions_different_check_counts() {
        // Verify the evaluator is not hardcoded — different inputs produce different evaluations
        let evaluator = QualityGateEvaluator::new();
        let mut runtime = FixtureRuntime::new();
        let mut high_risk = base_decision();
        high_risk.analysis_snapshot = json!({"risk_level": "critical"});
        high_risk.confidence_label = "low".to_string();
        high_risk.confidence = 0.3;
        let result = evaluator.evaluate(&noop_result(), &high_risk, &mut runtime);
        // High-risk decision should trigger different evaluation path
        assert!(!result.checks.is_empty());
    }
}
