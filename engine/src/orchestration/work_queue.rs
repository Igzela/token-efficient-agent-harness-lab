use super::schemas::{WorkflowGraph, WorkflowNode};

#[derive(Default)]
pub struct WorkQueue;

impl WorkQueue {
    pub fn new() -> Self {
        Self
    }

    pub fn enqueue(&self, graph: WorkflowGraph, _node: &WorkflowNode) -> WorkflowGraph {
        graph
    }

    pub fn dequeue_ready<'a>(&self, graph: &'a WorkflowGraph) -> Vec<&'a WorkflowNode> {
        graph.nodes.iter().filter(|n| n.status == "ready").collect()
    }

    pub fn start(&self, graph: &WorkflowGraph, node_id: &str) -> WorkflowGraph {
        self.update_node(graph, node_id, "running")
    }

    pub fn complete(
        &self,
        graph: &WorkflowGraph,
        node_id: &str,
        output_ref: &str,
    ) -> WorkflowGraph {
        let node = match find_node(graph, node_id) {
            Some(n) => n,
            None => return graph.clone(),
        };
        let updated = WorkflowNode {
            status: "completed".to_string(),
            output_ref: Some(output_ref.to_string()),
            completed_at: Some("now".to_string()),
            ..node.clone()
        };
        replace_node(graph, node_id, &updated)
    }

    pub fn fail(&self, graph: &WorkflowGraph, node_id: &str, error: &str) -> WorkflowGraph {
        let node = match find_node(graph, node_id) {
            Some(n) => n,
            None => return graph.clone(),
        };
        let updated = WorkflowNode {
            status: "failed".to_string(),
            error: Some(error.to_string()),
            completed_at: Some("now".to_string()),
            ..node.clone()
        };
        replace_node(graph, node_id, &updated)
    }

    pub fn cancel(&self, graph: &WorkflowGraph, node_id: &str) -> WorkflowGraph {
        let node = match find_node(graph, node_id) {
            Some(n) => n,
            None => return graph.clone(),
        };
        if node.status == "completed" || node.status == "failed" || node.status == "cancelled" {
            return graph.clone();
        }
        self.update_node(graph, node_id, "cancelled")
    }

    pub fn status_of(&self, graph: &WorkflowGraph, node_id: &str) -> String {
        find_node(graph, node_id)
            .map(|n| n.status.clone())
            .unwrap_or_else(|| "pending".to_string())
    }

    fn update_node(&self, graph: &WorkflowGraph, node_id: &str, status: &str) -> WorkflowGraph {
        let node = match find_node(graph, node_id) {
            Some(n) => n,
            None => return graph.clone(),
        };
        let updated = WorkflowNode {
            status: status.to_string(),
            started_at: if status == "running" && node.started_at.is_none() {
                Some("now".to_string())
            } else {
                node.started_at.clone()
            },
            completed_at: if status == "completed" || status == "failed" {
                Some("now".to_string())
            } else {
                node.completed_at.clone()
            },
            ..node.clone()
        };
        replace_node(graph, node_id, &updated)
    }
}

fn find_node<'a>(graph: &'a WorkflowGraph, node_id: &str) -> Option<&'a WorkflowNode> {
    graph.nodes.iter().find(|n| n.node_id == node_id)
}

fn replace_node(graph: &WorkflowGraph, node_id: &str, replacement: &WorkflowNode) -> WorkflowGraph {
    let updated_nodes: Vec<WorkflowNode> = graph
        .nodes
        .iter()
        .map(|n| {
            if n.node_id == node_id {
                replacement.clone()
            } else {
                n.clone()
            }
        })
        .collect();
    WorkflowGraph {
        nodes: updated_nodes,
        ..graph.clone()
    }
}
