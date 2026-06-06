use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

use crate::budget_manager::BudgetManager;
use crate::dispatch_decision::{
    BudgetReservation, DispatchDecision, ExecutionGate, DISPATCH_DECISION_SCHEMA_VERSION,
};
use crate::dispatch_ledger::{DispatchBundle, DispatchLedger};
use crate::evaluation_stub::{EvaluationResult, EvaluationStub, Evaluator};
use crate::executor_adapter::{Executor, NoopExecutor};
use crate::model_selector::ModelSelector;
use crate::provider::executor::make_not_executed_result;
use crate::runtime::FixtureRuntime;
use crate::task_analyzer::{RuleBasedTaskAnalyzer, TaskAnalysis};

pub struct DispatchEngine {
    analyzer: RuleBasedTaskAnalyzer,
    selector: ModelSelector,
    budget_manager: BudgetManager,
    executor: Box<dyn Executor>,
    evaluator: Box<dyn Evaluator>,
    ledger: DispatchLedger,
    executor_type_name: String,
    available_executor_tiers: HashSet<String>,
    dispatch_counter: AtomicUsize,
}

impl Default for DispatchEngine {
    fn default() -> Self {
        Self {
            analyzer: RuleBasedTaskAnalyzer::new(),
            selector: ModelSelector::new(None),
            budget_manager: BudgetManager::new(),
            executor: Box::new(NoopExecutor),
            evaluator: Box::new(EvaluationStub),
            ledger: DispatchLedger::new(),
            executor_type_name: "noop".to_string(),
            available_executor_tiers: HashSet::new(),
            dispatch_counter: AtomicUsize::new(0),
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

    pub fn with_evaluator(evaluator: Box<dyn Evaluator>) -> Self {
        Self {
            evaluator,
            ..Self::default()
        }
    }

    pub fn with_provider_executor(provider: std::sync::Arc<dyn crate::provider::Provider>) -> Self {
        use crate::provider::executor::ProviderExecutor;
        Self {
            executor: Box::new(ProviderExecutor::new(provider)),
            executor_type_name: "provider".to_string(),
            ..Self::default()
        }
    }

    pub fn with_provider_executor_and_audit(
        provider: std::sync::Arc<dyn crate::provider::Provider>,
        recorder: std::sync::Arc<crate::provider::ProviderAuditRecorder>,
    ) -> Self {
        use crate::provider::executor::ProviderExecutor;
        Self {
            executor: Box::new(ProviderExecutor::new(provider).with_audit_recorder(recorder)),
            executor_type_name: "provider".to_string(),
            ..Self::default()
        }
    }

    pub fn with_multi_executor(multi: crate::cli::MultiExecutor) -> Self {
        let available_executor_tiers = ["claude_code_cli", "codex_cli"]
            .into_iter()
            .filter(|tier| multi.has_executor_for_tier(tier))
            .map(String::from)
            .collect();
        Self {
            executor: Box::new(multi),
            executor_type_name: "multi".to_string(),
            available_executor_tiers,
            ..Self::default()
        }
    }

    pub fn executor_type(&self) -> &str {
        &self.executor_type_name
    }

    fn effective_executor_type(&self, tier: &str) -> String {
        if self.executor_type_name == "multi" {
            if self.available_executor_tiers.contains(tier) {
                return tier.to_string();
            }
            "noop".to_string()
        } else {
            self.executor_type_name.clone()
        }
    }

    pub fn preflight_reserved_cost(&self, raw_request: &str, request_source: &str) -> f64 {
        let mut runtime = FixtureRuntime::new();
        let decision_id = runtime.id("dec-");
        let analysis =
            self.analyzer
                .analyze_with_runtime(raw_request, request_source, &mut runtime);
        let selection = self.selector.select(&analysis);
        let reservation = self.budget_manager.create_reservation(
            &decision_id,
            &analysis,
            &selection.selected_tier,
            &mut runtime,
        );
        reservation.reserved_cost
    }

    pub fn dispatch(&self, raw_request: &str, request_source: &str) -> Value {
        self.dispatch_bundle(raw_request, request_source).to_value()
    }

    pub fn dispatch_bundle(&self, raw_request: &str, request_source: &str) -> DispatchBundle {
        let mut runtime = FixtureRuntime::new();
        let dispatch_id = format!(
            "disp-{:04}",
            self.dispatch_counter.fetch_add(1, Ordering::Relaxed) + 1
        );
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
        let effective_executor_type = self.effective_executor_type(&selection.selected_tier);
        let execution_policy = build_execution_policy(&analysis, &effective_executor_type);
        let execution_gates = build_execution_gates(
            &analysis,
            &budget_reservation,
            &execution_policy,
            &effective_executor_type,
            &mut runtime,
        );
        let hard_constraints = derive_hard_constraints(&analysis, &execution_policy);
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

        let effective_type = self.effective_executor_type(&decision.selected_tier);
        let is_provider = effective_type == "provider";
        let provider_blocked = is_provider
            && (decision.decision_status != "decided"
                || decision
                    .hard_constraints
                    .contains(&"no_provider_call".to_string()));

        let execution_result = if provider_blocked {
            make_not_executed_result(
                &decision,
                &dispatch_id,
                &mut runtime,
                "execution_not_authorized",
                "provider execution blocked by constraints",
            )
        } else {
            self.executor
                .execute(&decision, raw_request, &dispatch_id, &mut runtime)
        };

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

fn build_execution_policy(analysis: &TaskAnalysis, executor_type: &str) -> Value {
    let read_only_advisory = analysis.features_detected["read_only_advisory"]
        .as_bool()
        .unwrap_or(false);
    let requires_human_review = ["critical", "high"].contains(&analysis.risk_level.as_str())
        || analysis.confidence_label == "low"
        || read_only_advisory;
    json!({
        "executor_type": executor_type,
        "execution_allowed": true,
        "requires_human_review": requires_human_review,
        "max_retries": 0
    })
}

fn build_execution_gates(
    analysis: &TaskAnalysis,
    reservation: &BudgetReservation,
    execution_policy: &Value,
    executor_type: &str,
    runtime: &mut FixtureRuntime,
) -> Vec<ExecutionGate> {
    let mut gates = Vec::new();

    if executor_type == "provider" {
        // provider execution has its own gate path
    } else if executor_type.starts_with("cli")
        || executor_type == "claude_code_cli"
        || executor_type == "codex_cli"
    {
        gates.push(ExecutionGate {
            gate_id: runtime.id("gate-"),
            gate_type: "cli_execution".to_string(),
            severity: "info".to_string(),
            reason: format!("CLI executor active: {executor_type}"),
            evidence_refs: Vec::new(),
            clearance_required: "policy".to_string(),
            cleared: true,
            cleared_by: Some("auto".to_string()),
            cleared_at: Some(runtime.now()),
        });
    } else {
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
    }
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
        let advisory_review = analysis.features_detected["read_only_advisory"]
            .as_bool()
            .unwrap_or(false);
        gates.push(ExecutionGate {
            gate_id: runtime.id("gate-"),
            gate_type: "manual_review".to_string(),
            severity: if advisory_review { "warning" } else { "block" }.to_string(),
            reason: if advisory_review {
                "read-only advisory output requires human review after provider response"
            } else {
                "high risk or low confidence requires human review"
            }
            .to_string(),
            evidence_refs: Vec::new(),
            clearance_required: "human".to_string(),
            cleared: false,
            cleared_by: None,
            cleared_at: None,
        });
    }
    gates
}

fn derive_hard_constraints(analysis: &TaskAnalysis, execution_policy: &Value) -> Vec<String> {
    let mut constraints = vec!["no_target_write".to_string()];

    let executor_type = execution_policy["executor_type"].as_str().unwrap_or("noop");
    let user_negated_provider = analysis
        .negative_evidence
        .iter()
        .any(|e| e.feature.contains("provider") || e.text.contains("no_provider_call"));

    let is_cli_executor = executor_type.starts_with("cli")
        || executor_type == "claude_code_cli"
        || executor_type == "codex_cli";

    if (executor_type != "provider" && !is_cli_executor) || user_negated_provider {
        constraints.push("no_provider_call".to_string());
    }

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
        "balanced_worker" | "codex_cli" => "medium",
        "strong_planner" | "claude_code_cli" | "verifier" | "advisor" => "high",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successive_dispatches_from_one_engine_have_distinct_dispatch_ids() {
        let engine = DispatchEngine::new();
        let first = engine.dispatch("Trial 4 alpha noop dispatch", "api");
        let second = engine.dispatch("Trial 4 beta noop dispatch", "api");

        assert_eq!(first["record"]["dispatch_id"], "disp-0001");
        assert_eq!(second["record"]["dispatch_id"], "disp-0002");
        assert_eq!(first["execution_result"]["dispatch_id"], "disp-0001");
        assert_eq!(second["execution_result"]["dispatch_id"], "disp-0002");
    }
}
