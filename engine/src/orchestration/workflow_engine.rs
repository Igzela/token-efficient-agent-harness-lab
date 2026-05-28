use super::conflict_resolver::ConflictResolver;
use super::dependency_resolver::DependencyResolver;
use super::human_approval_gate::HumanApprovalGate;
use super::multi_agent_budget::MultiAgentBudgetManager;
use super::result_aggregator::ResultAggregator;
use super::schemas::{WorkflowGraph, WorkflowNode};
use super::task_decomposer::TaskDecomposer;
use super::work_queue::WorkQueue;
use crate::task_analyzer::TaskAnalysis;

pub struct WorkflowEngine {
    decomposer: TaskDecomposer,
    resolver: DependencyResolver,
    queue: WorkQueue,
    conflict_resolver: ConflictResolver,
    aggregator: ResultAggregator,
    approval_gate: HumanApprovalGate,
    budget_manager: MultiAgentBudgetManager,
}

impl WorkflowEngine {
    pub fn new(decomposer: TaskDecomposer) -> Self {
        Self {
            decomposer,
            resolver: DependencyResolver::new(),
            queue: WorkQueue::new(),
            conflict_resolver: ConflictResolver::new(),
            aggregator: ResultAggregator::new(),
            approval_gate: HumanApprovalGate::new(0.7),
            budget_manager: MultiAgentBudgetManager::new("cancel"),
        }
    }

    pub fn create_workflow(
        &mut self,
        analysis: &TaskAnalysis,
        dispatch_id: &str,
        budget_limit: f64,
        decision_status: &str,
        runtime: &mut crate::runtime::FixtureRuntime,
    ) -> Result<WorkflowGraph, String> {
        if decision_status != "decided" {
            return Err(format!(
                "Cannot create workflow: decision_status={decision_status:?}, requires 'decided'"
            ));
        }

        let graph = self.decomposer.decompose(analysis, dispatch_id, runtime);

        let (valid, _errors) = self.resolver.validate(&graph);
        let graph = if !valid {
            set_status(&graph, "failed")
        } else {
            graph
        };

        self.budget_manager
            .create_workflow_budget(&graph.workflow_id, budget_limit);

        Ok(graph)
    }

    pub fn tick(&mut self, graph: &WorkflowGraph) -> WorkflowGraph {
        if matches!(graph.status.as_str(), "completed" | "failed" | "cancelled") {
            return graph.clone();
        }

        let mut graph = self.start_ready_nodes(graph);

        if self.aggregator.is_complete(&graph) {
            let has_failed = graph
                .nodes
                .iter()
                .any(|n| n.status == "failed" || n.status == "cancelled");

            if has_failed {
                graph = self.handle_failed_nodes(&graph);
            }

            let conflicts = self.conflict_resolver.detect_conflicts(&graph);
            let unresolved: Vec<_> = conflicts
                .iter()
                .filter(|c| c.resolution_strategy.is_none())
                .collect();
            if !unresolved.is_empty() {
                graph = self.resolve_conflicts(&graph, &unresolved);
                if graph.status == "cancelled" {
                    return graph;
                }
            }

            let needs_approval = self.check_approval_needed(&graph);
            if needs_approval {
                return set_status(&graph, "waiting_human");
            }

            if has_failed && graph.status != "failed" && graph.status != "cancelled" {
                return set_status(&graph, "failed");
            }

            let result = self.aggregator.aggregate(&graph);
            return WorkflowGraph {
                status: "completed".to_string(),
                completed_at: Some("now".to_string()),
                result: Some(serde_json::json!(result)),
                ..graph
            };
        }

        if graph.status == "created" || graph.status == "decomposed" {
            return set_status(&graph, "running");
        }

        graph
    }

    pub fn resume_after_approval(&self, graph: &WorkflowGraph, _node_id: &str) -> WorkflowGraph {
        if graph.status != "waiting_human" {
            return graph.clone();
        }
        // Note: approval_gate is behind &self, so we can't mutate here.
        // In a real implementation this would use interior mutability.
        set_status(graph, "running")
    }

    pub fn cancel(&mut self, graph: &WorkflowGraph) -> WorkflowGraph {
        let node_ids: Vec<String> = graph
            .nodes
            .iter()
            .filter(|n| !matches!(n.status.as_str(), "completed" | "failed" | "cancelled"))
            .map(|n| n.node_id.clone())
            .collect();
        let mut g = graph.clone();
        for node_id in node_ids {
            g = self.queue.cancel(&g, &node_id);
        }
        set_status(&g, "cancelled")
    }

    pub fn complete_node(
        &mut self,
        graph: &WorkflowGraph,
        node_id: &str,
        output_ref: &str,
        cost: f64,
    ) -> WorkflowGraph {
        let mut graph = self.queue.complete(graph, node_id, output_ref);

        if cost > 0.0 {
            if let Some(node) = graph.nodes.iter().find(|n| n.node_id == node_id) {
                let agent_id = node.assigned_agent_id.clone().unwrap_or_default();
                self.budget_manager
                    .record_cost(&graph.workflow_id, node_id, &agent_id, cost);

                let (ok, _) = self
                    .budget_manager
                    .check_workflow_budget(&graph.workflow_id);
                if !ok && self.budget_manager.overrun_strategy == "cancel" {
                    return set_status(&graph, "failed");
                }
            }

            // Update cost_incurred
            graph = update_node_field(&graph, node_id, cost);
        }

        graph
    }

    pub fn fail_node(
        &mut self,
        graph: &WorkflowGraph,
        node_id: &str,
        error: &str,
    ) -> WorkflowGraph {
        self.queue.fail(graph, node_id, error)
    }

    fn start_ready_nodes(&self, graph: &WorkflowGraph) -> WorkflowGraph {
        let ready_ids = self.resolver.ready_nodes(graph);
        let mut g = graph.clone();
        for node_id in ready_ids {
            g = self.queue.start(&g, &node_id);
        }
        g
    }

    fn check_approval_needed(&self, graph: &WorkflowGraph) -> bool {
        graph.nodes.iter().any(|node| {
            if matches!(
                node.status.as_str(),
                "completed" | "failed" | "waiting_human"
            ) {
                self.approval_gate.requires_approval(graph, node)
            } else {
                false
            }
        })
    }

    fn handle_failed_nodes(&self, graph: &WorkflowGraph) -> WorkflowGraph {
        let failed_or_cancelled: Vec<&WorkflowNode> = graph
            .nodes
            .iter()
            .filter(|n| n.status == "failed" || n.status == "cancelled")
            .collect();
        if failed_or_cancelled.is_empty() {
            return graph.clone();
        }
        for node in failed_or_cancelled {
            if self.approval_gate.requires_approval(graph, node) {
                return set_status(graph, "waiting_human");
            }
        }
        graph.clone()
    }

    fn resolve_conflicts(
        &self,
        graph: &WorkflowGraph,
        conflicts: &[&super::schemas::ConflictRecord],
    ) -> WorkflowGraph {
        for conflict in conflicts {
            let resolved = self.conflict_resolver.resolve(conflict);
            if resolved.resolution_result.as_deref() == Some("workflow_cancelled") {
                return set_status(graph, "cancelled");
            }
        }
        graph.clone()
    }
}

fn set_status(graph: &WorkflowGraph, status: &str) -> WorkflowGraph {
    let started = if status == "running" && graph.started_at.is_none() {
        Some("now".to_string())
    } else {
        graph.started_at.clone()
    };
    let completed = if matches!(status, "completed" | "failed" | "cancelled") {
        Some("now".to_string())
    } else {
        None
    };
    WorkflowGraph {
        status: status.to_string(),
        updated_at: "now".to_string(),
        started_at: started,
        completed_at: completed,
        ..graph.clone()
    }
}

fn update_node_field(graph: &WorkflowGraph, node_id: &str, cost_incurred: f64) -> WorkflowGraph {
    let nodes: Vec<WorkflowNode> = graph
        .nodes
        .iter()
        .map(|n| {
            if n.node_id == node_id {
                WorkflowNode {
                    cost_incurred,
                    ..n.clone()
                }
            } else {
                n.clone()
            }
        })
        .collect();
    WorkflowGraph {
        nodes,
        ..graph.clone()
    }
}
