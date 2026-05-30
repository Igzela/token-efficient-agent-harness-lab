use std::collections::HashMap;

use super::types::{DAGEdge, DAGMutationProposal, DAGNode, DAGState};

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
