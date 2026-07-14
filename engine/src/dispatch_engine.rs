use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Value};

use crate::budget_manager::BudgetManager;
use crate::dispatch_decision::{
    BudgetReservation, DispatchDecision, ExecutionGate, DISPATCH_DECISION_SCHEMA_VERSION,
};
use crate::dispatch_ledger::{DispatchBundle, DispatchLedger};
use crate::evaluation_stub::{EvaluationResult, EvaluationStub, Evaluator};
use crate::executor_adapter::{ExecutionResult, Executor, NoopExecutor};
use crate::harness::advisor::{AdvisorBroker, AdvisorContextPack};
use crate::infrastructure::structured_events;
use crate::model_selector::{DispatchRoutingPolicy, ModelSelector};
use crate::provider::executor::make_not_executed_result;
use crate::runtime::FixtureRuntime;
use crate::task_analyzer::{RuleBasedTaskAnalyzer, TaskAnalysis};

pub struct DispatchEngine {
    analyzer: RuleBasedTaskAnalyzer,
    selector: ModelSelector,
    budget_manager: BudgetManager,
    executor: Box<dyn Executor>,
    evaluator: Box<dyn Evaluator>,
    advisor: Option<AdvisorBroker>,
    ledger: DispatchLedger,
    executor_type_name: String,
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
            advisor: None,
            ledger: DispatchLedger::new(),
            executor_type_name: "noop".to_string(),
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

    pub fn with_advisor(advisor: AdvisorBroker) -> Self {
        Self {
            advisor: Some(advisor),
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

    pub fn executor_type(&self) -> &str {
        &self.executor_type_name
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

    pub fn dispatch_with_policy(
        &self,
        raw_request: &str,
        request_source: &str,
        policy: DispatchRoutingPolicy,
    ) -> Value {
        self.dispatch_bundle_with_policy(raw_request, request_source, policy)
            .to_value()
    }

    fn execute_with_fallback(
        &self,
        tier: &str,
        original_decision: &DispatchDecision,
        raw_request: &str,
        dispatch_id: &str,
        runtime: &mut FixtureRuntime,
    ) -> ExecutionResult {
        let mut modified = original_decision.clone();
        modified.selected_tier = tier.to_string();
        self.executor
            .execute(&modified, raw_request, dispatch_id, runtime)
    }

    pub fn dispatch_bundle(&self, raw_request: &str, request_source: &str) -> DispatchBundle {
        self.dispatch_bundle_with_selector(raw_request, request_source, &self.selector)
    }

    pub fn dispatch_bundle_with_policy(
        &self,
        raw_request: &str,
        request_source: &str,
        policy: DispatchRoutingPolicy,
    ) -> DispatchBundle {
        let selector = ModelSelector::new(Some(policy));
        self.dispatch_bundle_with_selector(raw_request, request_source, &selector)
    }

    fn dispatch_bundle_with_selector(
        &self,
        raw_request: &str,
        request_source: &str,
        selector: &ModelSelector,
    ) -> DispatchBundle {
        let mut runtime = FixtureRuntime::new();
        let dispatch_id = format!(
            "disp-{:04}",
            self.dispatch_counter.fetch_add(1, Ordering::Relaxed) + 1
        );
        let decision_id = runtime.id("dec-");

        structured_events::log_dispatch_start("", &dispatch_id, request_source, raw_request.len());

        let analysis =
            self.analyzer
                .analyze_with_runtime(raw_request, request_source, &mut runtime);
        let selection = selector.select(&analysis);

        structured_events::log_dispatch_analysis(
            "",
            &dispatch_id,
            &analysis.analysis_id,
            &analysis.task_domain,
            &analysis.task_intent,
            &analysis.risk_level,
            analysis.confidence,
        );
        structured_events::log_dispatch_selection(
            "",
            &dispatch_id,
            &selection.selected_tier,
            &self.executor_type_name,
        );

        let budget_reservation = self.budget_manager.create_reservation(
            &decision_id,
            &analysis,
            &selection.selected_tier,
            &mut runtime,
        );
        let effective_executor_type = self.executor_type_name.clone();
        let mut execution_policy = build_execution_policy(&analysis, &effective_executor_type);

        // Phase 3: Activate advisor as dispatch advisory layer
        if let Some(ref advisor) = self.advisor {
            let ctx = AdvisorContextPack {
                task_description: raw_request.to_string(),
                context: format!(
                    "domain={} intent={}",
                    analysis.task_domain, analysis.task_intent
                ),
                constraints: analysis.risk_flags.clone(),
                budget_tokens: analysis.context_budget_estimate,
            };
            let advice = advisor.request_advice(&ctx);
            execution_policy["advisory"] = json!({
                "recommendation": advice.recommendation,
                "confidence": advice.confidence,
                "reasoning": advice.reasoning,
                "alternatives": advice.alternatives,
            });
        }
        let execution_gates = build_execution_gates(
            &analysis,
            &budget_reservation,
            &execution_policy,
            &effective_executor_type,
            &mut runtime,
        );
        let hard_constraints = derive_hard_constraints(&analysis, &execution_policy);
        let decision_status = determine_decision_status(&execution_gates);

        structured_events::log_dispatch_decision(
            "",
            &dispatch_id,
            &decision_status,
            execution_gates.len(),
        );

        let mut decision = DispatchDecision {
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

        let effective_type = self.executor_type_name.clone();
        let is_provider = effective_type == "provider";
        let provider_blocked = is_provider
            && (decision.decision_status != "decided"
                || decision
                    .hard_constraints
                    .contains(&"no_provider_call".to_string()));

        let mut execution_result = if provider_blocked {
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

        structured_events::log_dispatch_execution(
            "",
            &dispatch_id,
            &effective_type,
            &decision.selected_tier,
            &execution_result.status,
        );

        let mut evaluation_result =
            self.evaluator
                .evaluate(&execution_result, &decision, &mut runtime);

        // Quality retry: if evaluation fails and retry is suggested, upgrade tier and retry once.
        // Guard against: (a) non-decided status, (b) no_provider_call constraint,
        // (c) needs_human_review on the original evaluation.
        let should_retry = evaluation_result.requires_retry
            && evaluation_result.status == "fail"
            && decision.decision_status == "decided"
            && !decision
                .hard_constraints
                .iter()
                .any(|c| c == "no_provider_call");

        if should_retry {
            let upgraded_tier = upgrade_tier(&decision.selected_tier);
            if upgraded_tier != decision.selected_tier {
                // Ensure upgraded tier does not itself violate no_provider_call
                let upgraded_type = self.executor_type_name.clone();
                let upgraded_blocked = upgraded_type == "provider"
                    && decision
                        .hard_constraints
                        .iter()
                        .any(|c| c == "no_provider_call");
                if !upgraded_blocked {
                    let retry_execution = self.execute_with_fallback(
                        &upgraded_tier,
                        &decision,
                        raw_request,
                        &dispatch_id,
                        &mut runtime,
                    );
                    let retry_eval =
                        self.evaluator
                            .evaluate(&retry_execution, &decision, &mut runtime);
                    structured_events::log_dispatch_retry(
                        "",
                        &dispatch_id,
                        &decision.selected_tier,
                        &upgraded_tier,
                        evaluation_result
                            .retry_reason
                            .as_deref()
                            .unwrap_or("quality_fail"),
                    );
                    if retry_eval.status != "fail" {
                        execution_result = retry_execution;
                        evaluation_result = retry_eval;
                        // Record the upgraded tier in the decision so the ledger
                        // reflects the tier that actually executed.
                        decision.selected_tier = upgraded_tier;
                    }
                }
            }
        }

        let final_status = derive_final_status(&execution_result, &evaluation_result);

        structured_events::log_dispatch_complete("", &dispatch_id, final_status);

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

fn upgrade_tier(tier: &str) -> String {
    match tier {
        "cheap_executor" => "balanced_worker".to_string(),
        "balanced_worker" => "claude_code_cli".to_string(),
        "codex_cli" => "claude_code_cli".to_string(),
        other => other.to_string(),
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
        reason: "process/container/VM sandbox isolation is not implemented for this local runtime boundary".to_string(),
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
    use crate::harness::advisor::StubAdvisorProvider;

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

    #[test]
    fn advisor_enriches_dispatch_decision_policy() {
        let advisor = AdvisorBroker::new(Box::new(StubAdvisorProvider::new()));
        let engine = DispatchEngine::with_advisor(advisor);
        let bundle = engine.dispatch_bundle("fix auth bug", "api");
        let policy: Value =
            serde_json::from_str(&serde_json::to_string(&bundle.decision).unwrap()).unwrap();
        let advisory = &policy["execution_policy"]["advisory"];
        assert!(
            advisory.is_object(),
            "advisory should be present in execution_policy"
        );
        assert!(advisory["recommendation"].as_str().is_some());
        assert!(advisory["confidence"].as_f64().is_some());
    }

    #[test]
    fn advisor_not_present_by_default() {
        let engine = DispatchEngine::new();
        let bundle = engine.dispatch_bundle("test request", "api");
        let policy: Value =
            serde_json::from_str(&serde_json::to_string(&bundle.decision).unwrap()).unwrap();
        assert!(
            policy["execution_policy"]["advisory"].is_null(),
            "advisory should not be present without advisor"
        );
    }

    #[test]
    fn with_advisor_and_evaluator_together() {
        // Build engine with both advisor and quality evaluator
        let engine = DispatchEngine {
            advisor: Some(AdvisorBroker::new(Box::new(StubAdvisorProvider::new()))),
            evaluator: Box::new(crate::quality::evaluator_bridge::QualityGateEvaluator::new()),
            ..DispatchEngine::default()
        };
        let bundle = engine.dispatch_bundle("analyze code quality", "api");
        // Both advisor and quality evaluator should be active
        let policy: Value =
            serde_json::from_str(&serde_json::to_string(&bundle.decision).unwrap()).unwrap();
        assert!(policy["execution_policy"]["advisory"].is_object());
        assert!(bundle.evaluation_result["quality_score"].is_number());
    }

    #[test]
    fn test_retry_does_not_violate_no_provider_call() {
        // A mock evaluator that always fails
        struct FailEvaluator;
        impl Evaluator for FailEvaluator {
            fn evaluate(
                &self,
                result: &ExecutionResult,
                _decision: &DispatchDecision,
                runtime: &mut FixtureRuntime,
            ) -> EvaluationResult {
                EvaluationResult {
                    schema_version: "evaluation_result.v1".to_string(),
                    evaluation_id: runtime.id("eval-"),
                    dispatch_id: result.dispatch_id.clone(),
                    decision_id: result.decision_id.clone(),
                    execution_result_id: result.result_id.clone(),
                    status: "fail".to_string(),
                    checks: vec![],
                    quality_score: None,
                    requires_retry: true,
                    retry_reason: Some("test".to_string()),
                    created_at: runtime.now(),
                }
            }
        }

        let engine = DispatchEngine::with_evaluator(Box::new(FailEvaluator));
        // Dispatch a request — the noop executor will succeed but FailEvaluator will mark it as fail
        // The retry should NOT upgrade to a provider tier (default engine has no_provider_call constraint)
        let bundle = engine.dispatch_bundle("test request", "api");
        // The final status should reflect the original execution, not a provider retry
        let final_status = bundle.record["final_status"].as_str().unwrap_or("");
        assert!(
            final_status == "failed" || final_status == "not_executed",
            "final_status should be failed or not_executed, got: {final_status}"
        );
    }
}
