use std::collections::{HashMap, HashSet};

use super::schemas::WorkflowGraph;
use crate::workflow::graph_operations::GraphOperations;

#[derive(Default)]
pub struct DependencyResolver;

impl DependencyResolver {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, graph: &WorkflowGraph) -> (bool, Vec<String>) {
        let mut errors: Vec<String> = Vec::new();
        let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.node_id.as_str()).collect();

        for edge in &graph.edges {
            if !node_ids.contains(edge.from_node_id.as_str()) {
                errors.push(format!("missing_source:{}", edge.from_node_id));
            }
            if !node_ids.contains(edge.to_node_id.as_str()) {
                errors.push(format!("missing_target:{}", edge.to_node_id));
            }
        }

        if self.has_cycle(graph) {
            errors.push("cycle_detected".to_string());
        }

        (errors.is_empty(), errors)
    }

    pub fn execution_order(&self, graph: &WorkflowGraph) -> Vec<Vec<String>> {
        let (valid, _) = self.validate(graph);
        if !valid {
            return Vec::new();
        }

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for node in &graph.nodes {
            in_degree.entry(node.node_id.clone()).or_insert(0);
            dependents.entry(node.node_id.clone()).or_default();
        }

        for edge in &graph.edges {
            *in_degree.entry(edge.to_node_id.clone()).or_insert(0) += 1;
            dependents
                .entry(edge.from_node_id.clone())
                .or_default()
                .push(edge.to_node_id.clone());
        }

        let mut waves: Vec<Vec<String>> = Vec::new();
        let mut ready: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(nid, _)| nid.clone())
            .collect();
        ready.sort();

        while !ready.is_empty() {
            waves.push(ready.clone());
            let mut next_ready: Vec<String> = Vec::new();
            for nid in &ready {
                if let Some(deps) = dependents.get(nid) {
                    for dep in deps {
                        if let Some(deg) = in_degree.get_mut(dep) {
                            *deg -= 1;
                            if *deg == 0 {
                                next_ready.push(dep.clone());
                            }
                        }
                    }
                }
            }
            next_ready.sort();
            ready = next_ready;
        }

        waves
    }

    pub fn ready_nodes(&self, graph: &WorkflowGraph) -> Vec<String> {
        let completed: HashSet<&str> = graph
            .nodes
            .iter()
            .filter(|n| n.status == "completed")
            .map(|n| n.node_id.as_str())
            .collect();

        let mut ready: Vec<String> = Vec::new();
        for node in &graph.nodes {
            if node.status != "pending" {
                continue;
            }
            let deps = self.dependencies_of(graph, &node.node_id);
            if deps.iter().all(|d| completed.contains(d.as_str())) {
                ready.push(node.node_id.clone());
            }
        }
        ready.sort();
        ready
    }

    fn dependencies_of(&self, graph: &WorkflowGraph, node_id: &str) -> Vec<String> {
        graph
            .edges
            .iter()
            .filter(|e| e.to_node_id == node_id && e.edge_type == "dependency")
            .map(|e| e.from_node_id.clone())
            .collect()
    }

    fn has_cycle(&self, graph: &WorkflowGraph) -> bool {
        const WHITE: u8 = 0;
        const GRAY: u8 = 1;
        const BLACK: u8 = 2;

        let mut color: HashMap<String, u8> = HashMap::new();
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();

        for node in &graph.nodes {
            color.insert(node.node_id.clone(), WHITE);
            adj.insert(node.node_id.clone(), Vec::new());
        }

        for edge in &graph.edges {
            adj.entry(edge.from_node_id.clone())
                .or_default()
                .push(edge.to_node_id.clone());
        }

        fn dfs(
            nid: &str,
            color: &mut HashMap<String, u8>,
            adj: &HashMap<String, Vec<String>>,
        ) -> bool {
            color.insert(nid.to_string(), GRAY);
            if let Some(neighbors) = adj.get(nid) {
                for dep in neighbors {
                    match color.get(dep).copied() {
                        Some(GRAY) => return true,
                        Some(WHITE) if dfs(dep, color, adj) => return true,
                        _ => {}
                    }
                }
            }
            color.insert(nid.to_string(), BLACK);
            false
        }

        let white_nodes: Vec<String> = color
            .iter()
            .filter(|(_, &c)| c == WHITE)
            .map(|(k, _)| k.clone())
            .collect();

        for nid in &white_nodes {
            if dfs(nid, &mut color, &adj) {
                return true;
            }
        }
        false
    }
}

/// Pairs a `DependencyResolver` with a `WorkflowGraph` so `GraphOperations`
/// can be implemented without changing the resolver's method signatures.
pub struct ResolvableGraph<'a> {
    resolver: &'a DependencyResolver,
    graph: &'a WorkflowGraph,
}

impl<'a> ResolvableGraph<'a> {
    pub fn new(resolver: &'a DependencyResolver, graph: &'a WorkflowGraph) -> Self {
        Self { resolver, graph }
    }
}

impl<'a> GraphOperations for ResolvableGraph<'a> {
    fn validate(&self) -> (bool, Vec<String>) {
        self.resolver.validate(self.graph)
    }

    fn topological_order(&self) -> Vec<String> {
        self.resolver
            .execution_order(self.graph)
            .into_iter()
            .flatten()
            .collect()
    }

    fn ready_nodes(&self, completed: &HashSet<String>) -> Vec<String> {
        // DependencyResolver.ready_nodes reads from graph status, but the trait
        // passes completed explicitly. Filter to nodes whose deps are all completed.
        let mut ready = Vec::new();
        for node in &self.graph.nodes {
            if node.status != "pending" {
                continue;
            }
            let deps: Vec<String> = self
                .graph
                .edges
                .iter()
                .filter(|e| e.to_node_id == node.node_id && e.edge_type == "dependency")
                .map(|e| e.from_node_id.clone())
                .collect();
            if deps.iter().all(|d| completed.contains(d.as_str())) {
                ready.push(node.node_id.clone());
            }
        }
        ready.sort();
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph_node(id: &str, status: &str) -> super::super::schemas::WorkflowNode {
        super::super::schemas::WorkflowNode {
            schema_version: "workflow_node.v1".to_string(),
            node_id: id.to_string(),
            workflow_id: "wf1".to_string(),
            task_type: "task".to_string(),
            assigned_agent_id: None,
            status: status.to_string(),
            input_refs: vec![],
            output_ref: None,
            budget: 0.0,
            cost_incurred: 0.0,
            error: None,
            created_at: "t0".to_string(),
            started_at: None,
            completed_at: None,
        }
    }

    fn make_graph_edge(from: &str, to: &str) -> super::super::schemas::WorkflowEdge {
        super::super::schemas::WorkflowEdge {
            schema_version: "workflow_edge.v1".to_string(),
            edge_id: format!("{}->{}", from, to),
            from_node_id: from.to_string(),
            to_node_id: to.to_string(),
            edge_type: "dependency".to_string(),
        }
    }

    fn make_graph(
        nodes: Vec<super::super::schemas::WorkflowNode>,
        edges: Vec<super::super::schemas::WorkflowEdge>,
    ) -> WorkflowGraph {
        WorkflowGraph {
            schema_version: "workflow_graph.v1".to_string(),
            workflow_id: "wf1".to_string(),
            dispatch_id: "disp-0001".to_string(),
            nodes,
            edges,
            status: "created".to_string(),
            created_at: "t0".to_string(),
            updated_at: "t0".to_string(),
            started_at: None,
            completed_at: None,
            result: None,
        }
    }

    #[test]
    fn resolvable_graph_validate_ok() {
        let graph = make_graph(
            vec![
                make_graph_node("a", "pending"),
                make_graph_node("b", "pending"),
            ],
            vec![make_graph_edge("a", "b")],
        );
        let resolver = DependencyResolver::new();
        let rg = ResolvableGraph::new(&resolver, &graph);
        let (ok, errs) = rg.validate();
        assert!(ok, "errors: {:?}", errs);
    }

    #[test]
    fn resolvable_graph_validate_cycle() {
        let graph = make_graph(
            vec![
                make_graph_node("a", "pending"),
                make_graph_node("b", "pending"),
            ],
            vec![make_graph_edge("a", "b"), make_graph_edge("b", "a")],
        );
        let resolver = DependencyResolver::new();
        let rg = ResolvableGraph::new(&resolver, &graph);
        let (ok, errs) = rg.validate();
        assert!(!ok);
        assert!(errs.iter().any(|e| e.contains("cycle")));
    }

    #[test]
    fn resolvable_graph_ready_nodes_with_completed() {
        let graph = make_graph(
            vec![
                make_graph_node("a", "completed"),
                make_graph_node("b", "pending"),
                make_graph_node("c", "pending"),
            ],
            vec![make_graph_edge("a", "b"), make_graph_edge("a", "c")],
        );
        let resolver = DependencyResolver::new();
        let rg = ResolvableGraph::new(&resolver, &graph);
        let mut completed = HashSet::new();
        completed.insert("a".to_string());
        let ready = rg.ready_nodes(&completed);
        assert_eq!(ready, vec!["b", "c"]);
    }

    #[test]
    fn resolvable_graph_dyn_dispatch() {
        let graph = make_graph(vec![make_graph_node("a", "pending")], vec![]);
        let resolver = DependencyResolver::new();
        let rg = ResolvableGraph::new(&resolver, &graph);
        let ops: &dyn GraphOperations = &rg;
        let (ok, _) = ops.validate();
        assert!(ok);
    }
}
