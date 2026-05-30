use super::types::DAGMutationProposal;

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
