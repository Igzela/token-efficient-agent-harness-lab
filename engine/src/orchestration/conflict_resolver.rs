use super::schemas::{ConflictRecord, WorkflowGraph, CONFLICT_RECORD_SCHEMA_VERSION};

#[derive(Default)]
pub struct ConflictResolver;

impl ConflictResolver {
    pub fn new() -> Self {
        Self
    }

    pub fn detect_conflicts(&self, graph: &WorkflowGraph) -> Vec<ConflictRecord> {
        let mut conflicts: Vec<ConflictRecord> = Vec::new();

        let failed: Vec<&str> = graph
            .nodes
            .iter()
            .filter(|n| n.status == "failed")
            .map(|n| n.node_id.as_str())
            .collect();

        if !failed.is_empty() {
            conflicts.push(ConflictRecord {
                schema_version: CONFLICT_RECORD_SCHEMA_VERSION.to_string(),
                conflict_id: format!("conflict-{}", short_id()),
                workflow_id: graph.workflow_id.clone(),
                conflict_type: "dependency_violation".to_string(),
                involved_nodes: failed.into_iter().map(String::from).collect(),
                resolution_strategy: None,
                resolution_result: None,
                resolved_at: None,
            });
        }

        // Output conflicts
        let completed: Vec<&super::schemas::WorkflowNode> = graph
            .nodes
            .iter()
            .filter(|n| n.status == "completed")
            .collect();
        let mut output_groups: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for node in &completed {
            if let Some(ref output_ref) = node.output_ref {
                output_groups
                    .entry(output_ref.clone())
                    .or_default()
                    .push(node.node_id.clone());
            }
        }
        for node_ids in output_groups.values() {
            if node_ids.len() > 1 {
                conflicts.push(ConflictRecord {
                    schema_version: CONFLICT_RECORD_SCHEMA_VERSION.to_string(),
                    conflict_id: format!("conflict-{}", short_id()),
                    workflow_id: graph.workflow_id.clone(),
                    conflict_type: "output_conflict".to_string(),
                    involved_nodes: node_ids.clone(),
                    resolution_strategy: None,
                    resolution_result: None,
                    resolved_at: None,
                });
            }
        }

        // Resource conflicts
        let running: Vec<&super::schemas::WorkflowNode> = graph
            .nodes
            .iter()
            .filter(|n| n.status == "running")
            .collect();
        let mut agent_running: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for node in &running {
            if let Some(ref agent_id) = node.assigned_agent_id {
                agent_running
                    .entry(agent_id.clone())
                    .or_default()
                    .push(node.node_id.clone());
            }
        }
        for node_ids in agent_running.values() {
            if node_ids.len() > 1 {
                conflicts.push(ConflictRecord {
                    schema_version: CONFLICT_RECORD_SCHEMA_VERSION.to_string(),
                    conflict_id: format!("conflict-{}", short_id()),
                    workflow_id: graph.workflow_id.clone(),
                    conflict_type: "resource_conflict".to_string(),
                    involved_nodes: node_ids.clone(),
                    resolution_strategy: None,
                    resolution_result: None,
                    resolved_at: None,
                });
            }
        }

        // Budget overrun
        let total_cost: f64 = graph.nodes.iter().map(|n| n.cost_incurred).sum();
        let total_budget: f64 = graph.nodes.iter().map(|n| n.budget).sum();
        if total_budget > 0.0 && total_cost > total_budget {
            conflicts.push(ConflictRecord {
                schema_version: CONFLICT_RECORD_SCHEMA_VERSION.to_string(),
                conflict_id: format!("conflict-{}", short_id()),
                workflow_id: graph.workflow_id.clone(),
                conflict_type: "budget_overrun".to_string(),
                involved_nodes: graph.nodes.iter().map(|n| n.node_id.clone()).collect(),
                resolution_strategy: None,
                resolution_result: None,
                resolved_at: None,
            });
        }

        conflicts
    }

    pub fn resolve(&self, conflict: &ConflictRecord) -> ConflictRecord {
        let strategy = pick_strategy(&conflict.conflict_type);
        let result = match conflict.conflict_type.as_str() {
            "output_conflict" => "latest_output_wins",
            "resource_conflict" => "serialized_execution",
            "dependency_violation" => "failed_node_skipped",
            "budget_overrun" => "workflow_cancelled",
            _ => "unresolved",
        };
        ConflictRecord {
            resolution_strategy: Some(strategy.to_string()),
            resolution_result: Some(result.to_string()),
            resolved_at: Some("now".to_string()),
            ..conflict.clone()
        }
    }
}

fn pick_strategy(conflict_type: &str) -> &str {
    match conflict_type {
        "output_conflict" => "latest_wins",
        "resource_conflict" => "priority_wins",
        "budget_overrun" => "human_decides",
        _ => "skip",
    }
}

fn short_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:08x}", (t & 0xFFFF_FFFF) as u32)
}
