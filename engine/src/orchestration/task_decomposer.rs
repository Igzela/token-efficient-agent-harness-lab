use super::agent_role_registry::AgentRoleRegistry;
use super::schemas::{
    WorkflowEdge, WorkflowGraph, WorkflowNode, WORKFLOW_EDGE_SCHEMA_VERSION,
    WORKFLOW_NODE_SCHEMA_VERSION, WORKFLOW_SCHEMA_VERSION,
};
use crate::task_analyzer::TaskAnalysis;

pub struct TaskDecomposer {
    registry: Option<AgentRoleRegistry>,
}

#[allow(clippy::cloned_ref_to_slice_refs)]
impl TaskDecomposer {
    pub fn new(registry: Option<AgentRoleRegistry>) -> Self {
        Self { registry }
    }

    pub fn decompose(
        &mut self,
        analysis: &TaskAnalysis,
        dispatch_id: &str,
        runtime: &mut crate::runtime::FixtureRuntime,
    ) -> WorkflowGraph {
        let workflow_id = runtime.id("wf");
        let (nodes, edges) = self.build_graph(&workflow_id, analysis, runtime);

        WorkflowGraph {
            schema_version: WORKFLOW_SCHEMA_VERSION.to_string(),
            workflow_id,
            dispatch_id: dispatch_id.to_string(),
            nodes,
            edges,
            status: "decomposed".to_string(),
            created_at: runtime.now(),
            updated_at: runtime.now(),
            started_at: None,
            completed_at: None,
            result: None,
        }
    }

    fn build_graph(
        &mut self,
        workflow_id: &str,
        analysis: &TaskAnalysis,
        runtime: &mut crate::runtime::FixtureRuntime,
    ) -> (Vec<WorkflowNode>, Vec<WorkflowEdge>) {
        let complexity = analysis.complexity_score;

        if complexity < 0.3 && analysis.risk_flags.is_empty() {
            return self.simple_graph(workflow_id, analysis, runtime);
        }

        if complexity >= 0.6 || analysis.risk_flags.len() >= 2 {
            return self.complex_graph(workflow_id, analysis, runtime);
        }

        self.medium_graph(workflow_id, analysis, runtime)
    }

    fn simple_graph(
        &mut self,
        workflow_id: &str,
        analysis: &TaskAnalysis,
        runtime: &mut crate::runtime::FixtureRuntime,
    ) -> (Vec<WorkflowNode>, Vec<WorkflowEdge>) {
        let node = self.make_node(workflow_id, &analysis.task_domain, analysis, &[], runtime);
        (vec![node], vec![])
    }

    fn medium_graph(
        &mut self,
        workflow_id: &str,
        analysis: &TaskAnalysis,
        runtime: &mut crate::runtime::FixtureRuntime,
    ) -> (Vec<WorkflowNode>, Vec<WorkflowEdge>) {
        let analyze = self.make_node(
            workflow_id,
            &format!("{}_analyze", analysis.task_domain),
            analysis,
            &[],
            runtime,
        );
        let execute = self.make_node(
            workflow_id,
            &format!("{}_execute", analysis.task_domain),
            analysis,
            &[analyze.node_id.clone()],
            runtime,
        );
        let edge = WorkflowEdge {
            schema_version: WORKFLOW_EDGE_SCHEMA_VERSION.to_string(),
            edge_id: runtime.id("edge"),
            from_node_id: analyze.node_id.clone(),
            to_node_id: execute.node_id.clone(),
            edge_type: "dependency".to_string(),
        };
        (vec![analyze, execute], vec![edge])
    }

    fn complex_graph(
        &mut self,
        workflow_id: &str,
        analysis: &TaskAnalysis,
        runtime: &mut crate::runtime::FixtureRuntime,
    ) -> (Vec<WorkflowNode>, Vec<WorkflowEdge>) {
        let analyze = self.make_node(
            workflow_id,
            &format!("{}_analyze", analysis.task_domain),
            analysis,
            &[],
            runtime,
        );
        let plan = self.make_node(
            workflow_id,
            &format!("{}_plan", analysis.task_domain),
            analysis,
            &[analyze.node_id.clone()],
            runtime,
        );
        let execute = self.make_node(
            workflow_id,
            &format!("{}_execute", analysis.task_domain),
            analysis,
            &[plan.node_id.clone()],
            runtime,
        );
        let review = self.make_node(
            workflow_id,
            &format!("{}_review", analysis.task_domain),
            analysis,
            &[execute.node_id.clone()],
            runtime,
        );
        let edges = vec![
            WorkflowEdge {
                schema_version: WORKFLOW_EDGE_SCHEMA_VERSION.to_string(),
                edge_id: runtime.id("edge"),
                from_node_id: analyze.node_id.clone(),
                to_node_id: plan.node_id.clone(),
                edge_type: "dependency".to_string(),
            },
            WorkflowEdge {
                schema_version: WORKFLOW_EDGE_SCHEMA_VERSION.to_string(),
                edge_id: runtime.id("edge"),
                from_node_id: plan.node_id.clone(),
                to_node_id: execute.node_id.clone(),
                edge_type: "dependency".to_string(),
            },
            WorkflowEdge {
                schema_version: WORKFLOW_EDGE_SCHEMA_VERSION.to_string(),
                edge_id: runtime.id("edge"),
                from_node_id: execute.node_id.clone(),
                to_node_id: review.node_id.clone(),
                edge_type: "dependency".to_string(),
            },
        ];
        (vec![analyze, plan, execute, review], edges)
    }

    fn make_node(
        &mut self,
        workflow_id: &str,
        task_type: &str,
        analysis: &TaskAnalysis,
        input_refs: &[String],
        runtime: &mut crate::runtime::FixtureRuntime,
    ) -> WorkflowNode {
        let node_id = runtime.id("node");

        let agent_id = self
            .registry
            .as_mut()
            .and_then(|r| r.assign_agent(workflow_id, &node_id, task_type));

        let risk_divisor = (analysis.risk_flags.len() + 1) as f64;
        let budget = analysis.execution_budget_estimate as f64 / risk_divisor;

        WorkflowNode {
            schema_version: WORKFLOW_NODE_SCHEMA_VERSION.to_string(),
            node_id,
            workflow_id: workflow_id.to_string(),
            task_type: task_type.to_string(),
            assigned_agent_id: agent_id,
            status: "pending".to_string(),
            input_refs: input_refs.to_vec(),
            output_ref: None,
            budget: (budget * 100.0).round() / 100.0,
            cost_incurred: 0.0,
            error: None,
            created_at: runtime.now(),
            started_at: None,
            completed_at: None,
        }
    }
}
