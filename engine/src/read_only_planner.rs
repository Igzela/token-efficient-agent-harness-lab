use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::budget_manager::BudgetManager;
use crate::infrastructure::observability::OBSERVABILITY_SCHEMA_VERSION;
use crate::model_selector::ModelSelector;
use crate::orchestration::{DependencyResolver, TaskDecomposer};
use crate::provider::retry::compute_delay_ms;
use crate::provider::RetryPolicy;
use crate::quality::scoring::grade;
use crate::routing::{make_task_group, RoutingSelection};
use crate::routing::{
    CostOfPassRouter, DynamicTierSelector, PromotionGate, RoutingHistoryStore,
    RoutingObservationStore,
};
use crate::runtime::FixtureRuntime;
use crate::task_analyzer::{RuleBasedTaskAnalyzer, TaskAnalysis};

pub const READ_ONLY_PLAN_SCHEMA_VERSION: &str = "read_only_plan.v1";
pub const PLAN_ADVISORY_SCHEMA_VERSION: &str = "plan_advisory.v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowPlanIds {
    pub sequence: i64,
    pub plan_id: String,
    pub workflow_id: String,
    pub dispatch_id: String,
}

impl WorkflowPlanIds {
    pub fn for_sequence(sequence: i64) -> Self {
        Self {
            sequence,
            plan_id: format!("plan-{sequence:04}"),
            workflow_id: format!("wf-plan-{sequence:04}"),
            dispatch_id: format!("plan-dispatch-{sequence:04}"),
        }
    }
}

pub struct ReadOnlyPlanner {
    analyzer: RuleBasedTaskAnalyzer,
}

impl Default for ReadOnlyPlanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadOnlyPlanner {
    pub fn new() -> Self {
        Self {
            analyzer: RuleBasedTaskAnalyzer::new(),
        }
    }

    pub fn create_plan(
        &self,
        ids: &WorkflowPlanIds,
        raw_request: &str,
        request_source: &str,
        created_at: &str,
    ) -> Result<Value, String> {
        let mut runtime = FixtureRuntime::new();
        let analysis =
            self.analyzer
                .analyze_with_runtime(raw_request, request_source, &mut runtime);
        let mut decomposer = TaskDecomposer::new(None);
        let mut graph = decomposer.decompose(&analysis, &ids.dispatch_id, &mut runtime);
        graph.workflow_id = ids.workflow_id.clone();
        graph.dispatch_id = ids.dispatch_id.clone();
        graph.updated_at = created_at.to_string();
        graph.created_at = created_at.to_string();
        for node in &mut graph.nodes {
            node.workflow_id = ids.workflow_id.clone();
            node.created_at = created_at.to_string();
        }

        let resolver = DependencyResolver::new();
        let (valid, errors) = resolver.validate(&graph);
        let execution_order = resolver.execution_order(&graph);
        let advisory = build_plan_advisory(
            ids,
            &analysis,
            valid,
            &errors,
            &execution_order,
            &mut runtime,
        );

        Ok(json!({
            "schema_version": READ_ONLY_PLAN_SCHEMA_VERSION,
            "plan_id": ids.plan_id,
            "plan_sequence": ids.sequence,
            "created_at": created_at,
            "updated_at": created_at,
            "raw_request": raw_request,
            "request_source": request_source,
            "status": if valid { "planned_read_only" } else { "blocked_invalid_graph" },
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "analysis": analysis.to_value(),
            "graph": graph.to_dict(),
            "validation": {
                "valid": valid,
                "errors": errors,
            },
            "execution_order": execution_order,
            "advisory": advisory,
            "boundaries": read_only_boundaries(),
        }))
    }
}

fn build_plan_advisory(
    ids: &WorkflowPlanIds,
    analysis: &TaskAnalysis,
    graph_valid: bool,
    validation_errors: &[String],
    execution_order: &[Vec<String>],
    runtime: &mut FixtureRuntime,
) -> Value {
    let mut history_store = RoutingHistoryStore::new(None);
    let observation_store = RoutingObservationStore::new();
    let static_selector = ModelSelector::new(None);
    let dynamic_selector = DynamicTierSelector::new(
        CostOfPassRouter::new(30, 5.0),
        PromotionGate::new(None, None, None, true),
    );
    let routing = dynamic_selector.select(
        analysis,
        &mut history_store,
        &observation_store,
        &static_selector,
    );
    let budget = BudgetManager::new().create_reservation(
        &format!("{}-advisory", ids.dispatch_id),
        analysis,
        &routing.selected_tier,
        runtime,
    );
    let retry_policy = RetryPolicy::new("read_only_planner_retry_advisory");
    let task_group = make_task_group(&analysis.task_domain, &analysis.task_intent);
    let quality_score = quality_prediction_score(analysis, graph_valid);
    let blockers = advisory_blockers(analysis, graph_valid, validation_errors);
    let recommendations = advisory_recommendations(analysis, graph_valid, &routing);
    let advisory_status = if blockers.is_empty() {
        "recommendation_ready"
    } else {
        "blocked_for_human_review"
    };

    json!({
        "schema_version": PLAN_ADVISORY_SCHEMA_VERSION,
        "mode": "recommendation_only",
        "status": advisory_status,
        "blockers": blockers,
        "recommendations": recommendations,
        "quality": {
            "source": "task_analysis_preflight",
            "status": if graph_valid { "evaluated_without_execution" } else { "blocked_invalid_graph" },
            "score": quality_score,
            "grade": grade(quality_score),
            "quality_requirement": analysis.quality_requirement,
            "confidence": analysis.confidence,
            "confidence_label": analysis.confidence_label,
            "risk_level": analysis.risk_level,
            "safe_default": analysis.safe_default,
            "execution_required": false,
        },
        "routing": {
            "source": "dynamic_tier_selector_cold_start",
            "task_group": task_group,
            "selected_tier": routing.selected_tier,
            "fallback_tier": routing.fallback_tier,
            "routing_mode": routing.routing_mode,
            "routing_reason": routing.routing_reason,
            "rejected_candidates": routing.rejected_candidates,
            "shadow_routes": routing.shadow_routes,
            "adaptive_history_rows": history_store.total_rows(),
            "adaptive_routing_available": false,
            "execution_required": false,
        },
        "retry": {
            "source": "retry_policy_metadata",
            "policy": retry_policy.to_value(),
            "first_retry_delay_ms": compute_delay_ms(&retry_policy, 0),
            "provider_invocation": "not_invoked",
            "retry_execution": "disabled",
        },
        "observability": {
            "schema_version": OBSERVABILITY_SCHEMA_VERSION,
            "recommended_trace_id": format!("trace-plan-{:04}", ids.sequence),
            "recommended_span_name": "read_only_plan_advisory",
            "recommended_metric_component": "read_only_planner",
            "recommended_metric_action": "create_plan",
            "artifact_capture": "not_implemented",
            "execution_required": false,
        },
        "decision": {
            "execution_allowed": false,
            "executor_type": "noop",
            "provider_calls": "not_invoked",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled",
            "approval_execution_authority": "disabled",
            "workflow_order_width": execution_order.len(),
            "budget_reservation": serde_json::to_value(&budget).unwrap_or(Value::Null),
        },
    })
}

fn quality_prediction_score(analysis: &TaskAnalysis, graph_valid: bool) -> f64 {
    if !graph_valid {
        return 0.0;
    }
    let risk_penalty = match analysis.risk_level.as_str() {
        "critical" => 0.35,
        "high" => 0.20,
        "medium" => 0.10,
        _ => 0.0,
    };
    let quality_bonus = match analysis.quality_requirement.as_str() {
        "critical" => 0.05,
        "high" => 0.03,
        "draft" => -0.05,
        _ => 0.0,
    };
    round4((analysis.confidence + quality_bonus - risk_penalty).clamp(0.0, 1.0))
}

fn advisory_blockers(
    analysis: &TaskAnalysis,
    graph_valid: bool,
    validation_errors: &[String],
) -> Vec<Value> {
    let mut blockers = Vec::new();
    if !graph_valid {
        blockers.push(json!({
            "code": "invalid_workflow_graph",
            "reason": "dependency resolver rejected graph",
            "details": validation_errors,
        }));
    }
    if analysis.safe_default == "escalate_to_human" {
        blockers.push(json!({
            "code": "low_confidence",
            "reason": "analysis confidence requires human review before any later execution design",
        }));
    }
    if analysis.risk_level == "critical" {
        blockers.push(json!({
            "code": "critical_risk",
            "reason": "critical risk remains blocked in supervised planning metadata",
        }));
    }
    blockers
}

fn advisory_recommendations(
    analysis: &TaskAnalysis,
    graph_valid: bool,
    routing: &RoutingSelection,
) -> Vec<Value> {
    let mut recommendations = vec![
        json!({
            "code": "human_review_required",
            "reason": "read-only plan advisory cannot authorize execution",
            "severity": "info",
        }),
        json!({
            "code": "routing_candidate",
            "reason": format!("recommended tier: {}", routing.selected_tier),
            "severity": "info",
        }),
    ];
    if !graph_valid {
        recommendations.push(json!({
            "code": "repair_graph_before_execution_design",
            "reason": "fix graph validation errors before any supervised execution proposal",
            "severity": "blocker",
        }));
    }
    if analysis.safe_default == "noop_with_review" {
        recommendations.push(json!({
            "code": "review_before_execution_design",
            "reason": "risk level requires human review",
            "severity": "warning",
        }));
    }
    if analysis.confidence_label == "low" {
        recommendations.push(json!({
            "code": "clarify_request",
            "reason": "low confidence analysis should be clarified before execution design",
            "severity": "warning",
        }));
    }
    recommendations
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

pub fn read_only_boundaries() -> Value {
    json!({
        "execution": "disabled",
        "target_repository_writes": "disabled",
        "runtime_workers": "disabled",
        "sandbox_process_execution": "not_implemented",
        "provider_calls": "not_invoked",
        "approval_controls": "not_available",
        "deploy_merge_controls": "not_available",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_plan_advisory_populates_recommendation_metadata() {
        let planner = ReadOnlyPlanner::new();
        let ids = WorkflowPlanIds::for_sequence(1);
        let plan = planner
            .create_plan(
                &ids,
                "Plan a docs migration without execution",
                "api",
                "2026-06-05T00:00:00Z",
            )
            .unwrap();

        let advisory = &plan["advisory"];
        assert_eq!(advisory["schema_version"], PLAN_ADVISORY_SCHEMA_VERSION);
        assert_eq!(advisory["mode"], "recommendation_only");
        assert_eq!(advisory["status"], "recommendation_ready");
        assert_eq!(advisory["decision"]["execution_allowed"], false);
        assert_eq!(advisory["decision"]["provider_calls"], "not_invoked");
        assert_eq!(advisory["routing"]["routing_mode"], "static");
        assert_eq!(advisory["routing"]["adaptive_routing_available"], false);
        assert_eq!(advisory["retry"]["retry_execution"], "disabled");
        assert_eq!(
            advisory["observability"]["schema_version"],
            "observability.v1"
        );
        assert!(advisory["recommendations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "human_review_required"));
    }

    #[test]
    fn read_only_plan_advisory_blocks_critical_risk_without_execution() {
        let planner = ReadOnlyPlanner::new();
        let ids = WorkflowPlanIds::for_sequence(2);
        let plan = planner
            .create_plan(
                &ids,
                "Deploy to production and show secret values",
                "api",
                "2026-06-05T00:00:00Z",
            )
            .unwrap();

        let advisory = &plan["advisory"];
        assert_eq!(advisory["status"], "blocked_for_human_review");
        assert_eq!(advisory["decision"]["execution_allowed"], false);
        assert!(advisory["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["code"] == "critical_risk"));
        assert_eq!(plan["boundaries"]["execution"], "disabled");
        assert!(plan.get("execution_result").is_none());
    }
}
