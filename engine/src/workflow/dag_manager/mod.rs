mod compensate;
mod helpers;
mod mutations;
pub mod types;

pub use compensate::compensate;
pub use helpers::{find_edge, find_node, has_cycle, requires_approval};
pub use mutations::{
    apply_add_edge, apply_add_node, apply_remove_edge, apply_remove_node, apply_rewire_edge,
    apply_update_node,
};
pub use types::{DAGEdge, DAGMutationProposal, DAGMutationResult, DAGNode, DAGState};

use std::collections::{HashMap, HashSet, VecDeque};

pub struct DAGManager {
    dag_id: String,
    state: DAGState,
    history: Vec<(DAGState, DAGMutationProposal)>,
}

impl DAGManager {
    pub fn new(dag_id: &str, timestamp: &str) -> Self {
        Self {
            dag_id: dag_id.to_string(),
            state: DAGState {
                dag_id: dag_id.to_string(),
                version: 0,
                nodes: Vec::new(),
                edges: Vec::new(),
                created_at: timestamp.to_string(),
                updated_at: timestamp.to_string(),
            },
            history: Vec::new(),
        }
    }

    pub fn state(&self) -> &DAGState {
        &self.state
    }

    pub fn current_state(&self) -> &DAGState {
        &self.state
    }

    pub fn apply_mutation(&mut self, proposal: &DAGMutationProposal) -> DAGMutationResult {
        if proposal.dag_id != self.dag_id {
            return DAGMutationResult {
                proposal_id: proposal.proposal_id.clone(),
                applied: false,
                new_dag_version: self.state.version,
                rolled_back: false,
                errors: vec![format!(
                    "dag_id mismatch: expected {}, got {}",
                    self.dag_id, proposal.dag_id
                )],
            };
        }

        let apply_fn: fn(&DAGState, &DAGMutationProposal) -> Result<DAGState, String> =
            match proposal.mutation_type.as_str() {
                "add_node" => apply_add_node,
                "remove_node" => apply_remove_node,
                "add_edge" => apply_add_edge,
                "remove_edge" => apply_remove_edge,
                "rewire_edge" => apply_rewire_edge,
                "update_node" => apply_update_node,
                _ => {
                    return DAGMutationResult {
                        proposal_id: proposal.proposal_id.clone(),
                        applied: false,
                        new_dag_version: self.state.version,
                        rolled_back: false,
                        errors: vec![format!("unknown mutation_type: {}", proposal.mutation_type)],
                    };
                }
            };

        if helpers::requires_approval(proposal, &self.state) {
            return DAGMutationResult {
                proposal_id: proposal.proposal_id.clone(),
                applied: false,
                new_dag_version: self.state.version,
                rolled_back: false,
                errors: vec![
                    "mutation requires approval; set status=approved to proceed".to_string()
                ],
            };
        }

        match apply_fn(&self.state, proposal) {
            Ok(new_state) => {
                self.history.push((self.state.clone(), proposal.clone()));
                let version = new_state.version;
                self.state = new_state;
                DAGMutationResult {
                    proposal_id: proposal.proposal_id.clone(),
                    applied: true,
                    new_dag_version: version,
                    rolled_back: false,
                    errors: Vec::new(),
                }
            }
            Err(e) => DAGMutationResult {
                proposal_id: proposal.proposal_id.clone(),
                applied: false,
                new_dag_version: self.state.version,
                rolled_back: false,
                errors: vec![e],
            },
        }
    }

    pub fn rollback(&mut self, to_version: i64) -> &DAGState {
        while self.state.version > to_version {
            if let Some((prev_state, _)) = self.history.pop() {
                self.state = prev_state;
            } else {
                break;
            }
        }
        &self.state
    }

    pub fn validate_dag(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let node_ids: HashSet<&str> = self
            .state
            .nodes
            .iter()
            .map(|n| n.node_id.as_str())
            .collect();
        for e in &self.state.edges {
            if !node_ids.contains(e.from_node.as_str()) {
                errors.push(format!(
                    "edge {}: from_node {} not found",
                    e.edge_id, e.from_node
                ));
            }
            if !node_ids.contains(e.to_node.as_str()) {
                errors.push(format!(
                    "edge {}: to_node {} not found",
                    e.edge_id, e.to_node
                ));
            }
        }
        if has_cycle(&self.state.nodes, &self.state.edges) {
            errors.push("DAG contains a cycle".to_string());
        }
        errors
    }

    pub fn topological_order(&self) -> Vec<String> {
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut in_degree: HashMap<&str, i64> = HashMap::new();
        for n in &self.state.nodes {
            adj.entry(n.node_id.as_str()).or_default();
            in_degree.entry(n.node_id.as_str()).or_insert(0);
        }
        for e in &self.state.edges {
            if let Some(list) = adj.get_mut(e.from_node.as_str()) {
                list.push(e.to_node.as_str());
            }
            *in_degree.entry(e.to_node.as_str()).or_insert(0) += 1;
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();
        let mut sorted_queue: Vec<&str> = queue.drain(..).collect();
        sorted_queue.sort();
        for id in sorted_queue {
            queue.push_back(id);
        }

        let mut result = Vec::new();
        while let Some(node) = queue.pop_front() {
            result.push(node.to_string());
            if let Some(neighbors) = adj.get(node) {
                let mut sorted_neighbors: Vec<&str> = neighbors.clone();
                sorted_neighbors.sort();
                for neighbor in sorted_neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }
        result
    }

    pub fn nodes_ready(&self, completed: &HashSet<String>) -> Vec<&DAGNode> {
        let mut ready: Vec<&DAGNode> = Vec::new();
        for node in &self.state.nodes {
            if completed.contains(&node.node_id) || node.status != "pending" {
                continue;
            }
            let predecessors: Vec<&str> = self
                .state
                .edges
                .iter()
                .filter(|e| e.to_node == node.node_id)
                .map(|e| e.from_node.as_str())
                .collect();
            if predecessors.iter().all(|p| completed.contains(*p)) {
                ready.push(node);
            }
        }
        ready.sort_by(|a, b| a.node_id.cmp(&b.node_id));
        ready
    }

    pub fn path_between(&self, from_node: &str, to_node: &str) -> Option<Vec<String>> {
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for n in &self.state.nodes {
            adj.entry(n.node_id.as_str()).or_default();
        }
        for e in &self.state.edges {
            if let Some(list) = adj.get_mut(e.from_node.as_str()) {
                list.push(e.to_node.as_str());
            }
        }

        let mut visited: HashSet<String> = HashSet::new();
        let mut path: Vec<String> = Vec::new();

        fn dfs(
            current: &str,
            to_node: &str,
            adj: &HashMap<&str, Vec<&str>>,
            visited: &mut HashSet<String>,
            path: &mut Vec<String>,
        ) -> bool {
            if current == to_node {
                path.push(current.to_string());
                return true;
            }
            visited.insert(current.to_string());
            if let Some(neighbors) = adj.get(current) {
                let mut sorted_neighbors: Vec<&str> = neighbors.clone();
                sorted_neighbors.sort();
                for neighbor in sorted_neighbors {
                    if !visited.contains(neighbor) {
                        path.push(current.to_string());
                        if dfs(neighbor, to_node, adj, visited, path) {
                            return true;
                        }
                        path.pop();
                    }
                }
            }
            false
        }

        if dfs(from_node, to_node, &adj, &mut visited, &mut path) {
            Some(path)
        } else {
            None
        }
    }
}

impl super::graph_operations::GraphOperations for DAGManager {
    fn validate(&self) -> (bool, Vec<String>) {
        let errors = self.validate_dag();
        (errors.is_empty(), errors)
    }

    fn topological_order(&self) -> Vec<String> {
        DAGManager::topological_order(self)
    }

    fn ready_nodes(&self, completed: &HashSet<String>) -> Vec<String> {
        DAGManager::nodes_ready(self, completed)
            .into_iter()
            .map(|n| n.node_id.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_node(id: &str) -> DAGNode {
        DAGNode {
            node_id: id.to_string(),
            ..Default::default()
        }
    }

    fn make_edge(id: &str, from: &str, to: &str) -> DAGEdge {
        DAGEdge {
            edge_id: id.to_string(),
            from_node: from.to_string(),
            to_node: to.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_has_cycle_no_edges() {
        let nodes = vec![make_node("a"), make_node("b")];
        assert!(!has_cycle(&nodes, &[]));
    }

    #[test]
    fn test_has_cycle_detected() {
        let nodes = vec![make_node("a"), make_node("b"), make_node("c")];
        let edges = vec![
            make_edge("e1", "a", "b"),
            make_edge("e2", "b", "c"),
            make_edge("e3", "c", "a"),
        ];
        assert!(has_cycle(&nodes, &edges));
    }

    #[test]
    fn test_has_cycle_acyclic() {
        let nodes = vec![make_node("a"), make_node("b"), make_node("c")];
        let edges = vec![make_edge("e1", "a", "b"), make_edge("e2", "b", "c")];
        assert!(!has_cycle(&nodes, &edges));
    }

    #[test]
    fn test_find_node_present() {
        let nodes = vec![make_node("a"), make_node("b")];
        assert_eq!(find_node(&nodes, "b").unwrap().node_id, "b");
    }

    #[test]
    fn test_find_node_absent() {
        let nodes = vec![make_node("a")];
        assert!(find_node(&nodes, "z").is_none());
    }

    #[test]
    fn test_find_edge_present() {
        let edges = vec![make_edge("e1", "a", "b")];
        assert_eq!(find_edge(&edges, "e1").unwrap().edge_id, "e1");
    }

    #[test]
    fn test_requires_approval_explicit() {
        let proposal = DAGMutationProposal {
            requires_approval: true,
            ..Default::default()
        };
        let state = DAGState::default();
        assert!(helpers::requires_approval(&proposal, &state));
    }

    #[test]
    fn test_requires_approval_remove_running_node() {
        let state = DAGState {
            nodes: vec![DAGNode {
                node_id: "n1".to_string(),
                status: "running".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let proposal = DAGMutationProposal {
            mutation_type: "remove_node".to_string(),
            target_node_id: Some("n1".to_string()),
            ..Default::default()
        };
        assert!(helpers::requires_approval(&proposal, &state));
    }

    #[test]
    fn test_apply_add_node_success() {
        let state = DAGState {
            dag_id: "d1".to_string(),
            version: 0,
            ..Default::default()
        };
        let proposal = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_node".to_string(),
            payload: {
                let mut m = HashMap::new();
                m.insert("node_id".to_string(), json!("n1"));
                m
            },
            ..Default::default()
        };
        let result = mutations::apply_add_node(&state, &proposal).unwrap();
        assert_eq!(result.version, 1);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].node_id, "n1");
    }

    #[test]
    fn test_apply_add_node_duplicate() {
        let state = DAGState {
            nodes: vec![make_node("n1")],
            ..Default::default()
        };
        let proposal = DAGMutationProposal {
            payload: {
                let mut m = HashMap::new();
                m.insert("node_id".to_string(), json!("n1"));
                m
            },
            ..Default::default()
        };
        assert!(mutations::apply_add_node(&state, &proposal).is_err());
    }

    #[test]
    fn test_apply_remove_node_with_edges_fails() {
        let state = DAGState {
            nodes: vec![make_node("n1"), make_node("n2")],
            edges: vec![make_edge("e1", "n1", "n2")],
            ..Default::default()
        };
        let proposal = DAGMutationProposal {
            target_node_id: Some("n1".to_string()),
            ..Default::default()
        };
        assert!(mutations::apply_remove_node(&state, &proposal).is_err());
    }

    #[test]
    fn test_apply_add_edge_cycle_detected() {
        let state = DAGState {
            nodes: vec![make_node("a"), make_node("b")],
            edges: vec![make_edge("e1", "a", "b")],
            ..Default::default()
        };
        let proposal = DAGMutationProposal {
            payload: {
                let mut m = HashMap::new();
                m.insert("edge_id".to_string(), json!("e2"));
                m.insert("from_node".to_string(), json!("b"));
                m.insert("to_node".to_string(), json!("a"));
                m
            },
            ..Default::default()
        };
        assert!(mutations::apply_add_edge(&state, &proposal).is_err());
    }

    #[test]
    fn test_apply_rewire_edge() {
        let state = DAGState {
            nodes: vec![make_node("a"), make_node("b"), make_node("c")],
            edges: vec![make_edge("e1", "a", "b")],
            ..Default::default()
        };
        let proposal = DAGMutationProposal {
            target_edge_id: Some("e1".to_string()),
            payload: {
                let mut m = HashMap::new();
                m.insert("to_node".to_string(), json!("c"));
                m
            },
            ..Default::default()
        };
        let result = mutations::apply_rewire_edge(&state, &proposal).unwrap();
        assert_eq!(result.edges[0].to_node, "c");
    }

    #[test]
    fn test_apply_update_node() {
        let state = DAGState {
            nodes: vec![DAGNode {
                node_id: "n1".to_string(),
                status: "pending".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let proposal = DAGMutationProposal {
            target_node_id: Some("n1".to_string()),
            payload: {
                let mut m = HashMap::new();
                m.insert("status".to_string(), json!("running"));
                m
            },
            ..Default::default()
        };
        let result = mutations::apply_update_node(&state, &proposal).unwrap();
        assert_eq!(result.nodes[0].status, "running");
    }

    #[test]
    fn test_compensate_inverts() {
        let p = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            mutation_type: "add_node".to_string(),
            ..Default::default()
        };
        let inv = compensate(&p);
        assert_eq!(inv.mutation_type, "remove_node");
        assert_eq!(inv.proposal_id, "comp_p1");
    }

    #[test]
    fn test_dag_manager_new() {
        let mgr = DAGManager::new("d1", "t0");
        assert_eq!(mgr.state().dag_id, "d1");
        assert_eq!(mgr.state().version, 0);
    }

    #[test]
    fn test_dag_manager_apply_add_node() {
        let mut mgr = DAGManager::new("d1", "t0");
        let proposal = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_node".to_string(),
            payload: {
                let mut m = HashMap::new();
                m.insert("node_id".to_string(), json!("n1"));
                m
            },
            ..Default::default()
        };
        let result = mgr.apply_mutation(&proposal);
        assert!(result.applied);
        assert_eq!(result.new_dag_version, 1);
        assert_eq!(mgr.state().nodes.len(), 1);
    }

    #[test]
    fn test_dag_manager_dag_id_mismatch() {
        let mut mgr = DAGManager::new("d1", "t0");
        let proposal = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            dag_id: "wrong".to_string(),
            mutation_type: "add_node".to_string(),
            ..Default::default()
        };
        let result = mgr.apply_mutation(&proposal);
        assert!(!result.applied);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_dag_manager_unknown_mutation() {
        let mut mgr = DAGManager::new("d1", "t0");
        let proposal = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "bogus".to_string(),
            ..Default::default()
        };
        let result = mgr.apply_mutation(&proposal);
        assert!(!result.applied);
        assert!(result.errors[0].contains("unknown mutation_type"));
    }

    #[test]
    fn test_dag_manager_requires_approval_blocked() {
        let mut mgr = DAGManager::new("d1", "t0");
        let state = mgr.state().clone();
        let proposal_add = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_node".to_string(),
            payload: {
                let mut m = HashMap::new();
                m.insert("node_id".to_string(), json!("n1"));
                m
            },
            ..Default::default()
        };
        mgr.apply_mutation(&proposal_add);
        let _ = state;

        // update n1 to running
        let proposal_run = DAGMutationProposal {
            proposal_id: "p2".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "update_node".to_string(),
            target_node_id: Some("n1".to_string()),
            payload: {
                let mut m = HashMap::new();
                m.insert("status".to_string(), json!("running"));
                m
            },
            ..Default::default()
        };
        mgr.apply_mutation(&proposal_run);

        // try to remove running node — requires approval
        let proposal_rm = DAGMutationProposal {
            proposal_id: "p3".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "remove_node".to_string(),
            target_node_id: Some("n1".to_string()),
            ..Default::default()
        };
        let result = mgr.apply_mutation(&proposal_rm);
        assert!(!result.applied);
        assert!(result.errors[0].contains("requires approval"));
    }

    #[test]
    fn test_dag_manager_rollback() {
        let mut mgr = DAGManager::new("d1", "t0");
        let proposal = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_node".to_string(),
            payload: {
                let mut m = HashMap::new();
                m.insert("node_id".to_string(), json!("n1"));
                m
            },
            ..Default::default()
        };
        mgr.apply_mutation(&proposal);
        assert_eq!(mgr.state().version, 1);
        mgr.rollback(0);
        assert_eq!(mgr.state().version, 0);
        assert!(mgr.state().nodes.is_empty());
    }

    #[test]
    fn test_dag_manager_validate_dag() {
        let mgr = DAGManager::new("d1", "t0");
        let errors = mgr.validate_dag();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_dag_manager_topological_order() {
        let mut mgr = DAGManager::new("d1", "t0");
        for id in &["a", "b", "c"] {
            let proposal = DAGMutationProposal {
                proposal_id: format!("p_{id}"),
                dag_id: "d1".to_string(),
                mutation_type: "add_node".to_string(),
                payload: {
                    let mut m = HashMap::new();
                    m.insert("node_id".to_string(), json!(id));
                    m
                },
                ..Default::default()
            };
            mgr.apply_mutation(&proposal);
        }
        let edge = DAGMutationProposal {
            proposal_id: "pe1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_edge".to_string(),
            payload: {
                let mut m = HashMap::new();
                m.insert("edge_id".to_string(), json!("e1"));
                m.insert("from_node".to_string(), json!("a"));
                m.insert("to_node".to_string(), json!("b"));
                m
            },
            ..Default::default()
        };
        mgr.apply_mutation(&edge);
        let order = mgr.topological_order();
        assert!(order.iter().position(|n| n == "a") < order.iter().position(|n| n == "b"));
    }

    #[test]
    fn test_dag_manager_nodes_ready() {
        let mut mgr = DAGManager::new("d1", "t0");
        for id in &["a", "b"] {
            let proposal = DAGMutationProposal {
                proposal_id: format!("p_{id}"),
                dag_id: "d1".to_string(),
                mutation_type: "add_node".to_string(),
                payload: {
                    let mut m = HashMap::new();
                    m.insert("node_id".to_string(), json!(id));
                    m
                },
                ..Default::default()
            };
            mgr.apply_mutation(&proposal);
        }
        let edge = DAGMutationProposal {
            proposal_id: "pe1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_edge".to_string(),
            payload: {
                let mut m = HashMap::new();
                m.insert("edge_id".to_string(), json!("e1"));
                m.insert("from_node".to_string(), json!("a"));
                m.insert("to_node".to_string(), json!("b"));
                m
            },
            ..Default::default()
        };
        mgr.apply_mutation(&edge);

        let completed = HashSet::new();
        let ready = mgr.nodes_ready(&completed);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].node_id, "a");

        let mut completed = HashSet::new();
        completed.insert("a".to_string());
        let ready = mgr.nodes_ready(&completed);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].node_id, "b");
    }

    #[test]
    fn test_dag_manager_path_between() {
        let mut mgr = DAGManager::new("d1", "t0");
        for id in &["a", "b", "c"] {
            let proposal = DAGMutationProposal {
                proposal_id: format!("p_{id}"),
                dag_id: "d1".to_string(),
                mutation_type: "add_node".to_string(),
                payload: {
                    let mut m = HashMap::new();
                    m.insert("node_id".to_string(), json!(id));
                    m
                },
                ..Default::default()
            };
            mgr.apply_mutation(&proposal);
        }
        for (eid, from, to) in &[("e1", "a", "b"), ("e2", "b", "c")] {
            let edge = DAGMutationProposal {
                proposal_id: format!("pe_{eid}"),
                dag_id: "d1".to_string(),
                mutation_type: "add_edge".to_string(),
                payload: {
                    let mut m = HashMap::new();
                    m.insert("edge_id".to_string(), json!(eid));
                    m.insert("from_node".to_string(), json!(from));
                    m.insert("to_node".to_string(), json!(to));
                    m
                },
                ..Default::default()
            };
            mgr.apply_mutation(&edge);
        }
        let path = mgr.path_between("a", "c").unwrap();
        assert_eq!(path, vec!["a", "b", "c"]);
        assert!(mgr.path_between("c", "a").is_none());
    }
}
