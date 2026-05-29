use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DAGNode {
    pub node_id: String,
    pub task_id: Option<String>,
    pub node_type: String,
    pub status: String,
    pub tier: String,
    pub metadata: HashMap<String, Value>,
}

impl Default for DAGNode {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            task_id: None,
            node_type: "task".to_string(),
            status: "pending".to_string(),
            tier: "cheap_executor".to_string(),
            metadata: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DAGEdge {
    pub edge_id: String,
    pub from_node: String,
    pub to_node: String,
    pub dependency_type: String,
    pub status: String,
}

impl Default for DAGEdge {
    fn default() -> Self {
        Self {
            edge_id: String::new(),
            from_node: String::new(),
            to_node: String::new(),
            dependency_type: "hard".to_string(),
            status: "pending".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DAGState {
    pub dag_id: String,
    pub version: i64,
    pub nodes: Vec<DAGNode>,
    pub edges: Vec<DAGEdge>,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for DAGState {
    fn default() -> Self {
        Self {
            dag_id: String::new(),
            version: 0,
            nodes: Vec::new(),
            edges: Vec::new(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DAGMutationProposal {
    pub proposal_id: String,
    pub dag_id: String,
    pub mutation_type: String,
    pub target_node_id: Option<String>,
    pub target_edge_id: Option<String>,
    pub payload: HashMap<String, Value>,
    pub reason: String,
    pub requires_approval: bool,
    pub status: String,
}

impl Default for DAGMutationProposal {
    fn default() -> Self {
        Self {
            proposal_id: String::new(),
            dag_id: String::new(),
            mutation_type: String::new(),
            target_node_id: None,
            target_edge_id: None,
            payload: HashMap::new(),
            reason: String::new(),
            requires_approval: false,
            status: "pending".to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DAGMutationResult {
    pub proposal_id: String,
    pub applied: bool,
    pub new_dag_version: i64,
    pub rolled_back: bool,
    pub errors: Vec<String>,
}

impl Default for DAGMutationResult {
    fn default() -> Self {
        Self {
            proposal_id: String::new(),
            applied: false,
            new_dag_version: 0,
            rolled_back: false,
            errors: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn has_cycle(nodes: &[DAGNode], edges: &[DAGEdge]) -> bool {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for n in nodes {
        adj.entry(n.node_id.as_str()).or_default();
    }
    for e in edges {
        if let Some(list) = adj.get_mut(e.from_node.as_str()) {
            list.push(e.to_node.as_str());
        }
    }

    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;

    let mut color: HashMap<&str, u8> = HashMap::new();
    for n in nodes {
        color.insert(n.node_id.as_str(), WHITE);
    }

    let mut sorted_nodes: Vec<&str> = nodes.iter().map(|n| n.node_id.as_str()).collect();
    sorted_nodes.sort();

    for &start in &sorted_nodes {
        if color.get(start).copied() != Some(WHITE) {
            continue;
        }
        // Iterative DFS: stack of (node, next_neighbor_index)
        let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
        color.insert(start, GRAY);
        while let Some((node, idx)) = stack.pop() {
            let neighbors = adj.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
            if idx >= neighbors.len() {
                color.insert(node, BLACK);
                continue;
            }
            stack.push((node, idx + 1));
            let neighbor = neighbors[idx];
            match color.get(neighbor).copied() {
                Some(GRAY) => return true,
                Some(WHITE) => {
                    color.insert(neighbor, GRAY);
                    stack.push((neighbor, 0));
                }
                _ => {}
            }
        }
    }
    false
}

pub fn find_node<'a>(nodes: &'a [DAGNode], node_id: &str) -> Option<&'a DAGNode> {
    nodes.iter().find(|n| n.node_id == node_id)
}

pub fn find_edge<'a>(edges: &'a [DAGEdge], edge_id: &str) -> Option<&'a DAGEdge> {
    edges.iter().find(|e| e.edge_id == edge_id)
}

pub fn requires_approval(proposal: &DAGMutationProposal, state: &DAGState) -> bool {
    if proposal.requires_approval {
        return true;
    }
    if proposal.mutation_type == "remove_node" {
        if let Some(ref node_id) = proposal.target_node_id {
            if let Some(node) = find_node(&state.nodes, node_id) {
                if node.status == "running" || node.status == "completed" {
                    return true;
                }
            }
        }
    }
    if proposal.mutation_type == "rewire_edge" {
        if let Some(ref edge_id) = proposal.target_edge_id {
            if let Some(edge) = find_edge(&state.edges, edge_id) {
                if let Some(from_node) = find_node(&state.nodes, &edge.from_node) {
                    if from_node.status == "completed" {
                        return true;
                    }
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Mutation applicators
// ---------------------------------------------------------------------------

pub fn apply_add_node(
    state: &DAGState,
    proposal: &DAGMutationProposal,
) -> Result<DAGState, String> {
    let p = &proposal.payload;
    let node_id = p
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or("missing node_id")?
        .to_string();
    if find_node(&state.nodes, &node_id).is_some() {
        return Err(format!("node {node_id} already exists"));
    }
    let new_node = DAGNode {
        node_id: node_id.clone(),
        task_id: p.get("task_id").and_then(Value::as_str).map(String::from),
        node_type: p
            .get("node_type")
            .and_then(Value::as_str)
            .unwrap_or("task")
            .to_string(),
        status: p
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("pending")
            .to_string(),
        tier: p
            .get("tier")
            .and_then(Value::as_str)
            .unwrap_or("cheap_executor")
            .to_string(),
        metadata: p
            .get("metadata")
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    };
    let mut new_nodes = state.nodes.clone();
    new_nodes.push(new_node);
    Ok(DAGState {
        dag_id: state.dag_id.clone(),
        version: state.version + 1,
        nodes: new_nodes,
        edges: state.edges.clone(),
        created_at: state.created_at.clone(),
        updated_at: proposal.proposal_id.clone(),
    })
}

pub fn apply_remove_node(
    state: &DAGState,
    proposal: &DAGMutationProposal,
) -> Result<DAGState, String> {
    let node_id = proposal
        .target_node_id
        .as_deref()
        .ok_or("missing target_node_id")?;
    if find_node(&state.nodes, node_id).is_none() {
        return Err(format!("node {node_id} not found"));
    }
    let connected: Vec<_> = state
        .edges
        .iter()
        .filter(|e| e.from_node == node_id || e.to_node == node_id)
        .collect();
    if !connected.is_empty() {
        return Err(format!(
            "node {node_id} has {} connected edges; remove them first",
            connected.len()
        ));
    }
    Ok(DAGState {
        dag_id: state.dag_id.clone(),
        version: state.version + 1,
        nodes: state
            .nodes
            .iter()
            .filter(|n| n.node_id != node_id)
            .cloned()
            .collect(),
        edges: state.edges.clone(),
        created_at: state.created_at.clone(),
        updated_at: proposal.proposal_id.clone(),
    })
}

pub fn apply_add_edge(
    state: &DAGState,
    proposal: &DAGMutationProposal,
) -> Result<DAGState, String> {
    let p = &proposal.payload;
    let edge_id = p
        .get("edge_id")
        .and_then(Value::as_str)
        .ok_or("missing edge_id")?
        .to_string();
    let from_node = p
        .get("from_node")
        .and_then(Value::as_str)
        .ok_or("missing from_node")?
        .to_string();
    let to_node = p
        .get("to_node")
        .and_then(Value::as_str)
        .ok_or("missing to_node")?
        .to_string();
    if find_node(&state.nodes, &from_node).is_none() {
        return Err(format!("from_node {from_node} not found"));
    }
    if find_node(&state.nodes, &to_node).is_none() {
        return Err(format!("to_node {to_node} not found"));
    }
    if find_edge(&state.edges, &edge_id).is_some() {
        return Err(format!("edge {edge_id} already exists"));
    }
    let new_edge = DAGEdge {
        edge_id: edge_id.clone(),
        from_node,
        to_node,
        dependency_type: p
            .get("dependency_type")
            .and_then(Value::as_str)
            .unwrap_or("hard")
            .to_string(),
        status: p
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("pending")
            .to_string(),
    };
    let mut test_edges = state.edges.clone();
    test_edges.push(new_edge);
    if has_cycle(&state.nodes, &test_edges) {
        return Err(format!("adding edge {edge_id} would create a cycle"));
    }
    Ok(DAGState {
        dag_id: state.dag_id.clone(),
        version: state.version + 1,
        nodes: state.nodes.clone(),
        edges: test_edges,
        created_at: state.created_at.clone(),
        updated_at: proposal.proposal_id.clone(),
    })
}

pub fn apply_remove_edge(
    state: &DAGState,
    proposal: &DAGMutationProposal,
) -> Result<DAGState, String> {
    let edge_id = proposal
        .target_edge_id
        .as_deref()
        .ok_or("missing target_edge_id")?;
    if find_edge(&state.edges, edge_id).is_none() {
        return Err(format!("edge {edge_id} not found"));
    }
    Ok(DAGState {
        dag_id: state.dag_id.clone(),
        version: state.version + 1,
        nodes: state.nodes.clone(),
        edges: state
            .edges
            .iter()
            .filter(|e| e.edge_id != edge_id)
            .cloned()
            .collect(),
        created_at: state.created_at.clone(),
        updated_at: proposal.proposal_id.clone(),
    })
}

pub fn apply_rewire_edge(
    state: &DAGState,
    proposal: &DAGMutationProposal,
) -> Result<DAGState, String> {
    let p = &proposal.payload;
    let edge_id = proposal
        .target_edge_id
        .as_deref()
        .ok_or("missing target_edge_id")?;
    let edge = find_edge(&state.edges, edge_id)
        .ok_or_else(|| format!("edge {edge_id} not found"))?
        .clone();
    let new_from = p
        .get("from_node")
        .and_then(Value::as_str)
        .unwrap_or(&edge.from_node)
        .to_string();
    let new_to = p
        .get("to_node")
        .and_then(Value::as_str)
        .unwrap_or(&edge.to_node)
        .to_string();
    if find_node(&state.nodes, &new_from).is_none() {
        return Err(format!("from_node {new_from} not found"));
    }
    if find_node(&state.nodes, &new_to).is_none() {
        return Err(format!("to_node {new_to} not found"));
    }
    let rewired = DAGEdge {
        edge_id: edge.edge_id.clone(),
        from_node: new_from,
        to_node: new_to,
        dependency_type: p
            .get("dependency_type")
            .and_then(Value::as_str)
            .unwrap_or(&edge.dependency_type)
            .to_string(),
        status: edge.status.clone(),
    };
    let mut test_edges: Vec<DAGEdge> = state
        .edges
        .iter()
        .filter(|e| e.edge_id != edge.edge_id)
        .cloned()
        .collect();
    test_edges.push(rewired);
    if has_cycle(&state.nodes, &test_edges) {
        return Err(format!("rewiring edge {} would create a cycle", edge_id));
    }
    Ok(DAGState {
        dag_id: state.dag_id.clone(),
        version: state.version + 1,
        nodes: state.nodes.clone(),
        edges: test_edges,
        created_at: state.created_at.clone(),
        updated_at: proposal.proposal_id.clone(),
    })
}

pub fn apply_update_node(
    state: &DAGState,
    proposal: &DAGMutationProposal,
) -> Result<DAGState, String> {
    let node_id = proposal
        .target_node_id
        .as_deref()
        .ok_or("missing target_node_id")?;
    let node = find_node(&state.nodes, node_id)
        .ok_or_else(|| format!("node {node_id} not found"))?
        .clone();
    let p = &proposal.payload;
    let updated = DAGNode {
        node_id: node.node_id.clone(),
        task_id: p
            .get("task_id")
            .map(|v| v.as_str().unwrap_or("").to_string())
            .or(node.task_id),
        node_type: p
            .get("node_type")
            .and_then(Value::as_str)
            .unwrap_or(&node.node_type)
            .to_string(),
        status: p
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or(&node.status)
            .to_string(),
        tier: p
            .get("tier")
            .and_then(Value::as_str)
            .unwrap_or(&node.tier)
            .to_string(),
        metadata: p
            .get("metadata")
            .and_then(Value::as_object)
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or(node.metadata),
    };
    Ok(DAGState {
        dag_id: state.dag_id.clone(),
        version: state.version + 1,
        nodes: state
            .nodes
            .iter()
            .map(|n| {
                if n.node_id == node_id {
                    updated.clone()
                } else {
                    n.clone()
                }
            })
            .collect(),
        edges: state.edges.clone(),
        created_at: state.created_at.clone(),
        updated_at: proposal.proposal_id.clone(),
    })
}

// ---------------------------------------------------------------------------
// Compensate
// ---------------------------------------------------------------------------

pub fn compensate(proposal: &DAGMutationProposal) -> DAGMutationProposal {
    let inv = match proposal.mutation_type.as_str() {
        "add_node" => "remove_node",
        "remove_node" => "add_node",
        "add_edge" => "remove_edge",
        "remove_edge" => "add_edge",
        "rewire_edge" => "rewire_edge",
        "update_node" => "update_node",
        _ => &proposal.mutation_type,
    };
    DAGMutationProposal {
        proposal_id: format!("comp_{}", proposal.proposal_id),
        dag_id: proposal.dag_id.clone(),
        mutation_type: inv.to_string(),
        target_node_id: proposal.target_node_id.clone(),
        target_edge_id: proposal.target_edge_id.clone(),
        payload: proposal.payload.clone(),
        reason: format!("compensate {}", proposal.proposal_id),
        requires_approval: false,
        status: "pending".to_string(),
    }
}

// ---------------------------------------------------------------------------
// DAGManager
// ---------------------------------------------------------------------------

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

        if requires_approval(proposal, &self.state) {
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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
        assert!(requires_approval(&proposal, &state));
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
        assert!(requires_approval(&proposal, &state));
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
        let result = apply_add_node(&state, &proposal).unwrap();
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
        assert!(apply_add_node(&state, &proposal).is_err());
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
        assert!(apply_remove_node(&state, &proposal).is_err());
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
        assert!(apply_add_edge(&state, &proposal).is_err());
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
        let result = apply_rewire_edge(&state, &proposal).unwrap();
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
        let result = apply_update_node(&state, &proposal).unwrap();
        assert_eq!(result.nodes[0].status, "running");
    }

    #[test]
    fn test_compensate_add_node() {
        let proposal = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_node".to_string(),
            ..Default::default()
        };
        let comp = compensate(&proposal);
        assert_eq!(comp.mutation_type, "remove_node");
        assert_eq!(comp.proposal_id, "comp_p1");
    }

    #[test]
    fn test_dag_manager_apply_and_rollback() {
        let mut mgr = DAGManager::new("d1", "2026-01-01T00:00:00Z");
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

        mgr.rollback(0);
        assert_eq!(mgr.state().version, 0);
        assert!(mgr.state().nodes.is_empty());
    }

    #[test]
    fn test_dag_manager_dag_id_mismatch() {
        let mut mgr = DAGManager::new("d1", "2026-01-01T00:00:00Z");
        let proposal = DAGMutationProposal {
            proposal_id: "p1".to_string(),
            dag_id: "wrong".to_string(),
            mutation_type: "add_node".to_string(),
            ..Default::default()
        };
        let result = mgr.apply_mutation(&proposal);
        assert!(!result.applied);
        assert!(result.errors[0].contains("dag_id mismatch"));
    }

    #[test]
    fn test_dag_manager_validate_dag() {
        let mut mgr = DAGManager::new("d1", "2026-01-01T00:00:00Z");
        // add two nodes
        for id in &["n1", "n2"] {
            let mut p = HashMap::new();
            p.insert("node_id".to_string(), json!(id));
            mgr.apply_mutation(&DAGMutationProposal {
                proposal_id: format!("add_{id}"),
                dag_id: "d1".to_string(),
                mutation_type: "add_node".to_string(),
                payload: p,
                ..Default::default()
            });
        }
        let errors = mgr.validate_dag();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_topological_order() {
        let mut mgr = DAGManager::new("d1", "2026-01-01T00:00:00Z");
        for id in &["a", "b", "c"] {
            let mut p = HashMap::new();
            p.insert("node_id".to_string(), json!(id));
            mgr.apply_mutation(&DAGMutationProposal {
                proposal_id: format!("add_{id}"),
                dag_id: "d1".to_string(),
                mutation_type: "add_node".to_string(),
                payload: p,
                ..Default::default()
            });
        }
        let mut p = HashMap::new();
        p.insert("edge_id".to_string(), json!("e1"));
        p.insert("from_node".to_string(), json!("a"));
        p.insert("to_node".to_string(), json!("b"));
        mgr.apply_mutation(&DAGMutationProposal {
            proposal_id: "e1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_edge".to_string(),
            payload: p,
            ..Default::default()
        });
        let order = mgr.topological_order();
        let pos_a = order.iter().position(|x| x == "a").unwrap();
        let pos_b = order.iter().position(|x| x == "b").unwrap();
        assert!(pos_a < pos_b);
    }

    #[test]
    fn test_nodes_ready() {
        let mut mgr = DAGManager::new("d1", "2026-01-01T00:00:00Z");
        for id in &["a", "b"] {
            let mut p = HashMap::new();
            p.insert("node_id".to_string(), json!(id));
            mgr.apply_mutation(&DAGMutationProposal {
                proposal_id: format!("add_{id}"),
                dag_id: "d1".to_string(),
                mutation_type: "add_node".to_string(),
                payload: p,
                ..Default::default()
            });
        }
        let mut p = HashMap::new();
        p.insert("edge_id".to_string(), json!("e1"));
        p.insert("from_node".to_string(), json!("a"));
        p.insert("to_node".to_string(), json!("b"));
        mgr.apply_mutation(&DAGMutationProposal {
            proposal_id: "e1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_edge".to_string(),
            payload: p,
            ..Default::default()
        });

        let empty: HashSet<String> = HashSet::new();
        let ready = mgr.nodes_ready(&empty);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].node_id, "a");

        let mut completed = HashSet::new();
        completed.insert("a".to_string());
        let ready = mgr.nodes_ready(&completed);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].node_id, "b");
    }

    #[test]
    fn test_path_between() {
        let mut mgr = DAGManager::new("d1", "2026-01-01T00:00:00Z");
        for id in &["a", "b", "c"] {
            let mut p = HashMap::new();
            p.insert("node_id".to_string(), json!(id));
            mgr.apply_mutation(&DAGMutationProposal {
                proposal_id: format!("add_{id}"),
                dag_id: "d1".to_string(),
                mutation_type: "add_node".to_string(),
                payload: p,
                ..Default::default()
            });
        }
        let mut p = HashMap::new();
        p.insert("edge_id".to_string(), json!("e1"));
        p.insert("from_node".to_string(), json!("a"));
        p.insert("to_node".to_string(), json!("b"));
        mgr.apply_mutation(&DAGMutationProposal {
            proposal_id: "e1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_edge".to_string(),
            payload: p,
            ..Default::default()
        });
        let mut p = HashMap::new();
        p.insert("edge_id".to_string(), json!("e2"));
        p.insert("from_node".to_string(), json!("b"));
        p.insert("to_node".to_string(), json!("c"));
        mgr.apply_mutation(&DAGMutationProposal {
            proposal_id: "e2".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_edge".to_string(),
            payload: p,
            ..Default::default()
        });

        let path = mgr.path_between("a", "c").unwrap();
        assert_eq!(path, vec!["a", "b", "c"]);
        assert!(mgr.path_between("c", "a").is_none());
    }
}
