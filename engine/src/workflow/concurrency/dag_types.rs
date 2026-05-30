use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DagNode {
    pub node_id: String,
    pub task_id: Option<String>,
    pub node_type: String,
    pub status: String,
    pub tier: String,
    #[serde(default)]
    pub metadata: Value,
}

impl Default for DagNode {
    fn default() -> Self {
        Self {
            node_id: String::new(),
            task_id: None,
            node_type: "task".to_string(),
            status: "pending".to_string(),
            tier: String::new(),
            metadata: Value::Object(serde_json::Map::new()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DagEdge {
    pub edge_id: String,
    pub from_node: String,
    pub to_node: String,
    pub dependency_type: String,
    #[serde(default = "default_edge_status")]
    pub status: String,
}

pub fn default_edge_status() -> String {
    "pending".to_string()
}

impl Default for DagEdge {
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
pub struct DagState {
    pub dag_id: String,
    pub version: i64,
    pub nodes: Vec<DagNode>,
    pub edges: Vec<DagEdge>,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for DagState {
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
