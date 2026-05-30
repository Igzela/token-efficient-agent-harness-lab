use serde_json::Value;

use super::helpers::{find_edge, find_node, has_cycle};
use super::types::{DAGEdge, DAGMutationProposal, DAGNode, DAGState};

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
