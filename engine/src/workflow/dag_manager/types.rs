use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
