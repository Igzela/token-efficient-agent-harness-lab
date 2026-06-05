use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::orchestration::{DependencyResolver, TaskDecomposer};
use crate::runtime::FixtureRuntime;
use crate::task_analyzer::RuleBasedTaskAnalyzer;

pub const READ_ONLY_PLAN_SCHEMA_VERSION: &str = "read_only_plan.v1";

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
            "boundaries": read_only_boundaries(),
        }))
    }
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
