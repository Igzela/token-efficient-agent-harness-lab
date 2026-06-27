use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// Schema versions
pub const WORKFLOW_SCHEMA_VERSION: &str = "workflow_graph.v1";
pub const WORKFLOW_NODE_SCHEMA_VERSION: &str = "workflow_node.v1";
pub const WORKFLOW_EDGE_SCHEMA_VERSION: &str = "workflow_edge.v1";
pub const AGENT_MESSAGE_SCHEMA_VERSION: &str = "agent_message.v1";
pub const AGENT_STATE_SCHEMA_VERSION: &str = "agent_state.v1";
pub const CONFLICT_RECORD_SCHEMA_VERSION: &str = "conflict_record.v1";
pub const AGENT_ROLE_SCHEMA_VERSION: &str = "agent_role.v1";
pub const CHILD_TASK_PROPOSAL_SCHEMA_VERSION: &str = "child_task_proposal.v1";
pub const HANDOFF_REQUEST_SCHEMA_VERSION: &str = "handoff_request.v1";

pub const MAILBOX_STATUSES: &[&str] = &["pending", "read", "acked", "replied", "cancelled"];
pub const PROPOSAL_STATUSES: &[&str] = &["pending", "accepted", "rejected", "cancelled"];
pub const PROPOSAL_TYPES: &[&str] = &[
    "child_task",
    "handoff",
    "review_request",
    "review_verdict",
    "debate_request",
    "debate_position",
    "debate_resolution",
];

pub const AGENT_STATUSES: &[&str] = &["idle", "busy", "blocked", "completed", "failed"];

// Constants
pub const WORKFLOW_STATUSES: &[&str] = &[
    "created",
    "decomposed",
    "running",
    "waiting_human",
    "aggregating",
    "completed",
    "failed",
    "cancelled",
];
pub const NODE_STATUSES: &[&str] = &[
    "pending",
    "ready",
    "running",
    "completed",
    "failed",
    "cancelled",
    "waiting_human",
];
pub const EDGE_TYPES: &[&str] = &["dependency", "data_flow"];
pub const MESSAGE_TYPES: &[&str] = &[
    "task_assign",
    "result",
    "conflict",
    "approval_request",
    "status_update",
];
pub const CONFLICT_TYPES: &[&str] = &[
    "output_conflict",
    "resource_conflict",
    "dependency_violation",
    "budget_overrun",
];
pub const RESOLUTION_STRATEGIES: &[&str] = &[
    "latest_wins",
    "priority_wins",
    "merge",
    "human_decides",
    "skip",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub schema_version: String,
    pub node_id: String,
    pub workflow_id: String,
    pub task_type: String,
    pub assigned_agent_id: Option<String>,
    pub status: String,
    pub input_refs: Vec<String>,
    pub output_ref: Option<String>,
    pub budget: f64,
    pub cost_incurred: f64,
    pub error: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

impl WorkflowNode {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub schema_version: String,
    pub edge_id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub edge_type: String,
}

impl WorkflowEdge {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowGraph {
    pub schema_version: String,
    pub workflow_id: String,
    pub dispatch_id: String,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub result: Option<Value>,
}

impl WorkflowGraph {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub schema_version: String,
    pub message_id: String,
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub workflow_id: String,
    pub message_type: String,
    pub payload: HashMap<String, Value>,
    pub timestamp: String,
}

impl AgentMessage {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentState {
    pub schema_version: String,
    pub agent_id: String,
    pub run_id: String,
    pub role: String,
    pub capability_profile: Vec<String>,
    pub objective: Option<String>,
    pub status: String,
    pub scratchpad_summary: Option<String>,
    pub redaction_filter: Option<String>,
    pub metadata: HashMap<String, Value>,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentState {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MailboxMessage {
    pub schema_version: String,
    pub message_id: String,
    pub correlation_id: Option<String>,
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub run_id: Option<String>,
    pub node_id: Option<String>,
    pub message_type: String,
    pub status: String,
    pub body: Option<String>,
    pub body_summary: Option<String>,
    pub redaction_status: String,
    pub created_at: String,
    pub read_at: Option<String>,
    pub ack_at: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub metadata: HashMap<String, Value>,
}

impl MailboxMessage {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub from_agent_id: String,
    pub to_agent_id: String,
    pub run_id: Option<String>,
    pub node_id: Option<String>,
    pub message_type: String,
    pub body: Option<String>,
    pub correlation_id: Option<String>,
    pub reply_to_message_id: Option<String>,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListMailboxQuery {
    pub agent_id: Option<String>,
    pub run_id: Option<String>,
    pub node_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub schema_version: String,
    pub conflict_id: String,
    pub workflow_id: String,
    pub conflict_type: String,
    pub involved_nodes: Vec<String>,
    pub resolution_strategy: Option<String>,
    pub resolution_result: Option<String>,
    pub resolved_at: Option<String>,
}

impl ConflictRecord {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRole {
    pub schema_version: String,
    pub role_id: String,
    pub role_name: String,
    pub capabilities: Vec<String>,
    pub max_concurrent_nodes: usize,
    pub budget_limit: f64,
}

impl AgentRole {
    pub fn to_dict(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildTaskProposal {
    pub schema_version: String,
    pub correlation_id: String,
    pub objective: String,
    pub context_summary: String,
    pub proposed_node_id: Option<String>,
    pub proposed_edge_id: Option<String>,
    pub parent_node_id: String,
    pub run_id: String,
    pub agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandoffRequest {
    pub schema_version: String,
    pub correlation_id: String,
    pub objective: String,
    pub context_summary: String,
    pub target_agent_id: String,
    pub source_agent_id: String,
    pub run_id: String,
    pub node_id: String,
}

// ── AR-5: Bounded review/debate primitives ────────────────────────────────────

pub const MAX_DEBATE_PARTICIPANTS: usize = 8;
pub const MAX_DEBATE_ROUNDS: usize = 10;
pub const MAX_REVIEW_DEBATE_TEXT_BYTES: usize = 4096;

pub const REVIEW_VERDICTS: &[&str] = &["accepted", "rejected"];
pub const DEBATE_POSITION_STATUSES: &[&str] = &["pending", "accepted", "rejected"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub schema_version: String,
    pub correlation_id: String,
    pub subject_summary: String,
    pub rationale_summary: String,
    pub target_agent_id: String,
    pub run_id: String,
    pub node_id: String,
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewVerdict {
    pub schema_version: String,
    pub correlation_id: String,
    pub review_request_id: String,
    pub verdict: String,
    pub rationale_summary: String,
    pub run_id: String,
    pub node_id: String,
    pub blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebateRequest {
    pub schema_version: String,
    pub correlation_id: String,
    pub subject_summary: String,
    pub participant_agent_ids: Vec<String>,
    pub max_rounds: usize,
    pub run_id: String,
    pub node_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebatePosition {
    pub schema_version: String,
    pub correlation_id: String,
    pub debate_id: String,
    pub position: String,
    pub rationale_summary: String,
    pub run_id: String,
    pub node_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DebateResolution {
    pub schema_version: String,
    pub correlation_id: String,
    pub debate_id: String,
    pub resolution: String,
    pub winning_position: Option<String>,
    pub unresolved_risks: Option<String>,
    pub run_id: String,
    pub node_id: String,
}
