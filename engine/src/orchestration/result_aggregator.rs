use std::collections::HashMap;

use super::schemas::WorkflowGraph;
use serde_json::Value;

#[derive(Default)]
pub struct ResultAggregator;

impl ResultAggregator {
    pub fn new() -> Self {
        Self
    }

    pub fn is_complete(&self, graph: &WorkflowGraph) -> bool {
        graph
            .nodes
            .iter()
            .all(|n| matches!(n.status.as_str(), "completed" | "failed" | "cancelled"))
    }

    pub fn aggregate(&self, graph: &WorkflowGraph) -> HashMap<String, Value> {
        let mut node_results: HashMap<String, Value> = HashMap::new();
        for node in &graph.nodes {
            node_results.insert(
                node.node_id.clone(),
                serde_json::json!({
                    "task_type": node.task_type,
                    "status": node.status,
                    "output_ref": node.output_ref,
                    "error": node.error,
                    "agent_id": node.assigned_agent_id,
                }),
            );
        }

        let total_cost: f64 = graph.nodes.iter().map(|n| n.cost_incurred).sum();
        let completed_count = graph
            .nodes
            .iter()
            .filter(|n| n.status == "completed")
            .count();
        let failed_count = graph.nodes.iter().filter(|n| n.status == "failed").count();

        let mut result = HashMap::new();
        result.insert(
            "workflow_id".to_string(),
            Value::String(graph.workflow_id.clone()),
        );
        result.insert(
            "dispatch_id".to_string(),
            Value::String(graph.dispatch_id.clone()),
        );
        result.insert(
            "total_nodes".to_string(),
            Value::Number(graph.nodes.len().into()),
        );
        result.insert(
            "completed_nodes".to_string(),
            Value::Number(completed_count.into()),
        );
        result.insert(
            "failed_nodes".to_string(),
            Value::Number(failed_count.into()),
        );
        result.insert("total_cost".to_string(), serde_json::json!(total_cost));
        result.insert("node_results".to_string(), serde_json::json!(node_results));
        result
    }
}
