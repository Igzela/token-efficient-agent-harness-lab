use serde_json::{json, Value};

use crate::budget_manager::BudgetManager;
use crate::dispatch_decision::{
    BudgetReservation, DispatchDecision, ExecutionGate, DISPATCH_DECISION_SCHEMA_VERSION,
};
use crate::dispatch_ledger::{DispatchBundle, DispatchLedger};
use crate::evaluation_stub::{EvaluationResult, EvaluationStub};
use crate::executor_adapter::{Executor, NoopExecutor};
use crate::model_selector::ModelSelector;
use crate::runtime::FixtureRuntime;
use crate::task_analyzer::{RuleBasedTaskAnalyzer, TaskAnalysis};

pub struct DispatchEngine {
    analyzer: RuleBasedTaskAnalyzer,
    selector: ModelSelector,
    budget_manager: BudgetManager,
    executor: Box<dyn Executor>,
    evaluator: EvaluationStub,
    ledger: DispatchLedger,
}

impl Default for DispatchEngine {
    fn default() -> Self {
        Self {
            analyzer: RuleBasedTaskAnalyzer::new(),
            selector: ModelSelector::new(None),
            budget_manager: BudgetManager::new(),
            executor: Box::new(NoopExecutor),
            evaluator: EvaluationStub,
            ledger: DispatchLedger::new(),
        }
    }
}

impl DispatchEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_executor(executor: Box<dyn Executor>) -> Self {
        Self {
            executor,
            ..Self::default()
        }
    }

    pub fn dispatch(&self, raw_request: &str, request_source: &str) -> Value {
        self.dispatch_bundle(raw_request, request_source).to_value()
    }

    pub fn dispatch_bundle(&self, raw_request: &str, request_source: &str) -> DispatchBundle {
        let mut runtime = FixtureRuntime::new();
        let dispatch_id = runtime.id("disp-");
        let decision_id = runtime.id("dec-");

        let analysis =
            self.analyzer
                .analyze_with_runtime(raw_request, request_source, &mut runtime);
        let selection = self.selector.select(&analysis);
        let budget_reservation = self.budget_manager.create_reservation(
            &decision_id,
            &analysis,
            &selection.selected_tier,
            &mut runtime,
        );
        let execution_policy = build_execution_policy(&analysis);
        let execution_gates = build_execution_gates(
            &analysis,
            &budget_reservation,
            &execution_policy,
            &mut runtime,
        );
        let hard_constraints = derive_hard_constraints(&analysis);
        let decision_status = determine_decision_status(&execution_gates);

        let decision = DispatchDecision {
            schema_version: DISPATCH_DECISION_SCHEMA_VERSION.to_string(),
            decision_id: decision_id.clone(),
            analysis_id: analysis.analysis_id.clone(),
            analysis_snapshot: analysis.to_value(),
            selected_tier: selection.selected_tier.clone(),
            selected_profile_id: selection.selected_profile_id,
            fallback_tier: selection.fallback_tier,
            fallback_profile_id: selection.fallback_profile_id,
            shadow_routes: selection.shadow_routes,
            hard_constraints,
            rejected_candidates: selection.rejected_candidates,
            no_shadow_route_reason: None,
            max_input_tokens: analysis.context_budget_estimate,
            max_output_tokens: analysis.execution_budget_estimate,
            routing_reason: selection.routing_reason,
            quality_requirement: analysis.quality_requirement.clone(),
            expected_quality_band: quality_band(&selection.selected_tier).to_string(),
            confidence: analysis.confidence,
            confidence_label: analysis.confidence_label.clone(),
            budget_reservation: budget_reservation.clone(),
            execution_policy,
            execution_gates,
            routing_mode: "static".to_string(),
            routing_experiment_id: None,
            decision_status,
            created_at: runtime.now(),
        };

        let record = self.ledger.create_record(
            &dispatch_id,
            raw_request,
            &analysis.analysis_id,
            &decision_id,
            Some(decision.budget_reservation.reservation_id.clone()),
            &runtime,
        );
        let execution_result =
            self.executor
                .execute(&decision, raw_request, &dispatch_id, &mut runtime);
        let evaluation_result = self
            .evaluator
            .evaluate(&execution_result, &decision, &mut runtime);
        let final_status = derive_final_status(&execution_result, &evaluation_result);
        let record = self.ledger.update_record(
            record,
            final_status,
            Some(execution_result.result_id.clone()),
            Some(evaluation_result.evaluation_id.clone()),
            None,
            &runtime,
        );

        self.ledger.store_bundle(
            record,
            analysis,
            decision,
            execution_result,
            evaluation_result,
        )
    }
}

pub fn build_dispatch_bundle(raw_request: &str, request_source: &str) -> Value {
    DispatchEngine::new().dispatch(raw_request, request_source)
}

fn build_execution_policy(analysis: &TaskAnalysis) -> Value {
    let requires_human_review = ["critical", "high"].contains(&analysis.risk_level.as_str())
        || analysis.confidence_label == "low";
    json!({
        "executor_type": "noop",
        "execution_allowed": true,
        "requires_human_review": requires_human_review,
        "max_retries": 0
    })
}

fn build_execution_gates(
    analysis: &TaskAnalysis,
    reservation: &BudgetReservation,
    execution_policy: &Value,
    runtime: &mut FixtureRuntime,
) -> Vec<ExecutionGate> {
    let mut gates = Vec::new();
    gates.push(ExecutionGate {
        gate_id: runtime.id("gate-"),
        gate_type: "provider_disabled".to_string(),
        severity: "info".to_string(),
        reason: "real provider calls disabled \u{2014} non-provider executor".to_string(),
        evidence_refs: Vec::new(),
        clearance_required: "policy".to_string(),
        cleared: false,
        cleared_by: None,
        cleared_at: None,
    });
    gates.push(ExecutionGate {
        gate_id: runtime.id("gate-"),
        gate_type: "sandbox_disabled".to_string(),
        severity: "info".to_string(),
        reason: "sandbox execution disabled in Phase 1".to_string(),
        evidence_refs: Vec::new(),
        clearance_required: "policy".to_string(),
        cleared: false,
        cleared_by: None,
        cleared_at: None,
    });

    if ["critical", "high"].contains(&analysis.risk_level.as_str()) {
        gates.push(ExecutionGate {
            gate_id: runtime.id("gate-"),
            gate_type: "risk".to_string(),
            severity: "block".to_string(),
            reason: format!("risk_level={}", analysis.risk_level),
            evidence_refs: Vec::new(),
            clearance_required: "human".to_string(),
            cleared: false,
            cleared_by: None,
            cleared_at: None,
        });
    }
    if analysis
        .risk_flags
        .iter()
        .any(|flag| flag == "target_write")
    {
        gates.push(ExecutionGate {
            gate_id: runtime.id("gate-"),
            gate_type: "target_write".to_string(),
            severity: "block".to_string(),
            reason: "target_write risk flag detected".to_string(),
            evidence_refs: Vec::new(),
            clearance_required: "human".to_string(),
            cleared: false,
            cleared_by: None,
            cleared_at: None,
        });
    }
    if analysis.confidence_label == "low" {
        gates.push(ExecutionGate {
            gate_id: runtime.id("gate-"),
            gate_type: "confidence".to_string(),
            severity: "warning".to_string(),
            reason: format!("confidence={:.2} below threshold", analysis.confidence),
            evidence_refs: Vec::new(),
            clearance_required: "none".to_string(),
            cleared: false,
            cleared_by: None,
            cleared_at: None,
        });
    }
    if reservation.budget_violation {
        gates.push(ExecutionGate {
            gate_id: runtime.id("gate-"),
            gate_type: "budget".to_string(),
            severity: "block".to_string(),
            reason: "budget reservation violated".to_string(),
            evidence_refs: Vec::new(),
            clearance_required: "human".to_string(),
            cleared: false,
            cleared_by: None,
            cleared_at: None,
        });
    }
    if analysis
        .risk_flags
        .iter()
        .any(|flag| flag == "provider_call" || flag == "sandbox_execution")
    {
        gates.push(ExecutionGate {
            gate_id: runtime.id("gate-"),
            gate_type: "boundary".to_string(),
            severity: "block".to_string(),
            reason: "boundary violation detected (provider/sandbox)".to_string(),
            evidence_refs: Vec::new(),
            clearance_required: "human".to_string(),
            cleared: false,
            cleared_by: None,
            cleared_at: None,
        });
    }
    if execution_policy["requires_human_review"] == true {
        gates.push(ExecutionGate {
            gate_id: runtime.id("gate-"),
            gate_type: "manual_review".to_string(),
            severity: "block".to_string(),
            reason: "high risk or low confidence requires human review".to_string(),
            evidence_refs: Vec::new(),
            clearance_required: "human".to_string(),
            cleared: false,
            cleared_by: None,
            cleared_at: None,
        });
    }
    gates
}

fn derive_hard_constraints(analysis: &TaskAnalysis) -> Vec<String> {
    let mut constraints = vec![
        "no_target_write".to_string(),
        "no_provider_call".to_string(),
    ];
    if analysis.risk_level == "critical" {
        constraints.push("requires_human_approval".to_string());
    }
    constraints
}

fn determine_decision_status(gates: &[ExecutionGate]) -> String {
    if gates
        .iter()
        .any(|gate| gate.severity == "block" || gate.severity == "critical")
    {
        "needs_approval".to_string()
    } else {
        "decided".to_string()
    }
}

fn quality_band(tier: &str) -> &'static str {
    match tier {
        "cheap_executor" => "low",
        "balanced_worker" => "medium",
        "strong_planner" | "verifier" | "advisor" => "high",
        _ => "unknown",
    }
}

fn derive_final_status(
    execution_result: &crate::executor_adapter::ExecutionResult,
    evaluation_result: &EvaluationResult,
) -> &'static str {
    if execution_result.status == "not_executed" {
        "not_executed"
    } else if execution_result.status == "failed" || evaluation_result.status == "fail" {
        "failed"
    } else if execution_result.status == "manual_pending" {
        "manual_pending"
    } else if evaluation_result.status == "needs_human_review" {
        "escalated"
    } else {
        "completed"
    }
}
