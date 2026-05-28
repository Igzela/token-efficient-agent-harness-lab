use std::collections::{HashMap, HashSet};

use super::schemas::{WorkflowGraph, WorkflowNode};

pub struct HumanApprovalGate {
    risk_threshold: f64,
    approved: HashSet<String>,
    rejected: HashMap<String, String>,
}

impl HumanApprovalGate {
    pub fn new(risk_threshold: f64) -> Self {
        Self {
            risk_threshold,
            approved: HashSet::new(),
            rejected: HashMap::new(),
        }
    }

    pub fn requires_approval(&self, _graph: &WorkflowGraph, node: &WorkflowNode) -> bool {
        if self.approved.contains(&node.node_id) || self.rejected.contains_key(&node.node_id) {
            return false;
        }

        if node.budget > 0.0 && node.cost_incurred > node.budget * self.risk_threshold {
            return true;
        }

        if node.status == "failed" {
            return true;
        }

        false
    }

    pub fn approve(&mut self, node_id: &str) -> bool {
        if self.rejected.contains_key(node_id) {
            return false;
        }
        self.approved.insert(node_id.to_string());
        true
    }

    pub fn reject(&mut self, node_id: &str, reason: &str) -> bool {
        if self.approved.contains(node_id) {
            return false;
        }
        self.rejected
            .insert(node_id.to_string(), reason.to_string());
        true
    }

    pub fn is_approved(&self, node_id: &str) -> bool {
        self.approved.contains(node_id)
    }

    pub fn is_rejected(&self, node_id: &str) -> bool {
        self.rejected.contains_key(node_id)
    }

    pub fn rejection_reason(&self, node_id: &str) -> Option<&str> {
        self.rejected.get(node_id).map(|s| s.as_str())
    }
}
