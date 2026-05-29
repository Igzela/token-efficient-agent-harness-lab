use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

pub const SUPPORTED_DAG_MUTATIONS: &[&str] = &[
    "add_node",
    "remove_node",
    "split_node",
    "retry_node",
    "pause_node",
    "resume_node",
    "replace_edge",
    "rollback",
];

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DAGMutationLimits {
    pub max_nodes: i64,
    pub max_edges: i64,
}

impl Default for DAGMutationLimits {
    fn default() -> Self {
        Self {
            max_nodes: 1000,
            max_edges: 5000,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DAGMutation {
    pub mutation_id: String,
    pub dag_id: String,
    pub mutation_type: String,
    pub target_node_id: Option<String>,
    pub target_edge_id: Option<String>,
    pub payload: HashMap<String, Value>,
    pub reason: String,
    pub requires_approval: bool,
    pub status: String,
}

impl Default for DAGMutation {
    fn default() -> Self {
        Self {
            mutation_id: String::new(),
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
pub struct DAGMutationAuditEvent {
    pub event_type: String,
    pub mutation_id: String,
    pub dag_id: String,
    pub mutation_type: String,
    pub payload: HashMap<String, Value>,
    pub reason: String,
}

impl Default for DAGMutationAuditEvent {
    fn default() -> Self {
        Self {
            event_type: String::new(),
            mutation_id: String::new(),
            dag_id: String::new(),
            mutation_type: String::new(),
            payload: HashMap::new(),
            reason: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DAGMutationValidation {
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Default for DAGMutationValidation {
    fn default() -> Self {
        Self {
            ok: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn node_delta(mutation: &DAGMutation) -> i64 {
    match mutation.mutation_type.as_str() {
        "add_node" => 1,
        "remove_node" => -1,
        "split_node" => {
            let replacement = mutation
                .payload
                .get("replacement_node_count")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            replacement - 1
        }
        _ => 0,
    }
}

fn edge_delta(mutation: &DAGMutation) -> i64 {
    match mutation.mutation_type.as_str() {
        "split_node" => mutation
            .payload
            .get("added_edge_count")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        "replace_edge" => 0,
        _ => mutation
            .payload
            .get("edge_delta")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn validate_dag_mutation(
    mutation: &DAGMutation,
    current_node_count: i64,
    current_edge_count: i64,
    limits: &DAGMutationLimits,
) -> DAGMutationValidation {
    let mut errors = Vec::new();

    if !SUPPORTED_DAG_MUTATIONS.contains(&mutation.mutation_type.as_str()) {
        errors.push(format!(
            "unsupported mutation_type: {}",
            mutation.mutation_type
        ));
    }

    let next_node_count = current_node_count + node_delta(mutation);
    let next_edge_count = current_edge_count + edge_delta(mutation);
    if next_node_count > limits.max_nodes {
        errors.push(format!(
            "mutation would exceed max_nodes: {next_node_count} > {}",
            limits.max_nodes
        ));
    }
    if next_edge_count > limits.max_edges {
        errors.push(format!(
            "mutation would exceed max_edges: {next_edge_count} > {}",
            limits.max_edges
        ));
    }

    DAGMutationValidation {
        ok: errors.is_empty(),
        errors,
        warnings: Vec::new(),
    }
}

pub fn dag_mutation_requires_approval(
    mutation: &DAGMutation,
    target_node_status: Option<&str>,
    source_node_status: Option<&str>,
    affects_artifacts: bool,
) -> bool {
    if mutation.requires_approval {
        return true;
    }
    if affects_artifacts {
        return true;
    }
    let needs_approval_on_active = matches!(
        mutation.mutation_type.as_str(),
        "remove_node" | "split_node" | "retry_node" | "pause_node" | "replace_edge"
    );
    if needs_approval_on_active && matches!(target_node_status, Some("running") | Some("completed"))
    {
        return true;
    }
    if mutation.mutation_type == "replace_edge" && source_node_status == Some("completed") {
        return true;
    }
    false
}

pub fn create_compensating_mutation(
    mutation: &DAGMutation,
    previous_payload: Option<&HashMap<String, Value>>,
) -> DAGMutation {
    let inverse_types: HashMap<&str, &str> = [
        ("add_node", "remove_node"),
        ("remove_node", "add_node"),
        ("split_node", "rollback"),
        ("retry_node", "rollback"),
        ("pause_node", "resume_node"),
        ("resume_node", "pause_node"),
        ("replace_edge", "replace_edge"),
        ("rollback", "rollback"),
    ]
    .iter()
    .cloned()
    .collect();

    let mut payload = previous_payload
        .cloned()
        .unwrap_or_else(|| mutation.payload.clone());
    payload.insert(
        "compensates".to_string(),
        Value::String(mutation.mutation_id.clone()),
    );

    let inverse = inverse_types
        .get(mutation.mutation_type.as_str())
        .copied()
        .unwrap_or(&mutation.mutation_type);

    DAGMutation {
        mutation_id: format!("comp_{}", mutation.mutation_id),
        dag_id: mutation.dag_id.clone(),
        mutation_type: inverse.to_string(),
        target_node_id: mutation.target_node_id.clone(),
        target_edge_id: mutation.target_edge_id.clone(),
        payload,
        reason: format!("compensate {}: {}", mutation.mutation_id, mutation.reason)
            .trim_end()
            .to_string(),
        requires_approval: false,
        status: "pending".to_string(),
    }
}

pub fn mutation_to_audit_event(mutation: &DAGMutation) -> DAGMutationAuditEvent {
    let mut inner = HashMap::new();
    inner.insert(
        "payload".to_string(),
        serde_json::to_value(&mutation.payload).unwrap_or(Value::Null),
    );
    inner.insert(
        "requires_approval".to_string(),
        Value::Bool(mutation.requires_approval),
    );
    inner.insert("status".to_string(), Value::String(mutation.status.clone()));
    inner.insert(
        "target_edge_id".to_string(),
        mutation
            .target_edge_id
            .as_deref()
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
    );
    inner.insert(
        "target_node_id".to_string(),
        mutation
            .target_node_id
            .as_deref()
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
    );

    DAGMutationAuditEvent {
        event_type: "dag_mutation_recorded".to_string(),
        mutation_id: mutation.mutation_id.clone(),
        dag_id: mutation.dag_id.clone(),
        mutation_type: mutation.mutation_type.clone(),
        payload: inner,
        reason: mutation.reason.clone(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_validate_mutation_ok() {
        let m = DAGMutation {
            mutation_type: "add_node".to_string(),
            ..Default::default()
        };
        let v = validate_dag_mutation(&m, 10, 10, &DAGMutationLimits::default());
        assert!(v.ok);
        assert!(v.errors.is_empty());
    }

    #[test]
    fn test_validate_unsupported_type() {
        let m = DAGMutation {
            mutation_type: "bogus".to_string(),
            ..Default::default()
        };
        let v = validate_dag_mutation(&m, 0, 0, &DAGMutationLimits::default());
        assert!(!v.ok);
        assert!(v.errors[0].contains("unsupported mutation_type"));
    }

    #[test]
    fn test_validate_exceeds_max_nodes() {
        let m = DAGMutation {
            mutation_type: "add_node".to_string(),
            ..Default::default()
        };
        let limits = DAGMutationLimits {
            max_nodes: 5,
            max_edges: 5000,
        };
        let v = validate_dag_mutation(&m, 5, 0, &limits);
        assert!(!v.ok);
        assert!(v.errors[0].contains("max_nodes"));
    }

    #[test]
    fn test_validate_exceeds_max_edges() {
        let mut payload = HashMap::new();
        payload.insert("edge_delta".to_string(), json!(100));
        let m = DAGMutation {
            mutation_type: "add_node".to_string(),
            payload,
            ..Default::default()
        };
        let limits = DAGMutationLimits {
            max_nodes: 1000,
            max_edges: 5,
        };
        let v = validate_dag_mutation(&m, 0, 5, &limits);
        assert!(!v.ok);
        assert!(v.errors.iter().any(|e| e.contains("max_edges")));
    }

    #[test]
    fn test_requires_approval_explicit() {
        let m = DAGMutation {
            requires_approval: true,
            ..Default::default()
        };
        assert!(dag_mutation_requires_approval(&m, None, None, false));
    }

    #[test]
    fn test_requires_approval_artifacts() {
        let m = DAGMutation::default();
        assert!(dag_mutation_requires_approval(&m, None, None, true));
    }

    #[test]
    fn test_requires_approval_remove_running() {
        let m = DAGMutation {
            mutation_type: "remove_node".to_string(),
            ..Default::default()
        };
        assert!(dag_mutation_requires_approval(
            &m,
            Some("running"),
            None,
            false
        ));
        assert!(!dag_mutation_requires_approval(
            &m,
            Some("pending"),
            None,
            false
        ));
    }

    #[test]
    fn test_requires_approval_replace_edge_completed_source() {
        let m = DAGMutation {
            mutation_type: "replace_edge".to_string(),
            ..Default::default()
        };
        assert!(dag_mutation_requires_approval(
            &m,
            None,
            Some("completed"),
            false
        ));
    }

    #[test]
    fn test_create_compensating_mutation() {
        let m = DAGMutation {
            mutation_id: "m1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_node".to_string(),
            reason: "test".to_string(),
            ..Default::default()
        };
        let comp = create_compensating_mutation(&m, None);
        assert_eq!(comp.mutation_id, "comp_m1");
        assert_eq!(comp.mutation_type, "remove_node");
        assert_eq!(comp.status, "pending");
        assert!(!comp.requires_approval);
        assert!(comp.payload.contains_key("compensates"));
    }

    #[test]
    fn test_compensating_with_previous_payload() {
        let m = DAGMutation {
            mutation_id: "m2".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "pause_node".to_string(),
            reason: "pausing".to_string(),
            ..Default::default()
        };
        let mut prev = HashMap::new();
        prev.insert("key".to_string(), json!("val"));
        let comp = create_compensating_mutation(&m, Some(&prev));
        assert_eq!(comp.mutation_type, "resume_node");
        assert_eq!(comp.payload.get("key").unwrap(), &json!("val"));
    }

    #[test]
    fn test_mutation_to_audit_event() {
        let m = DAGMutation {
            mutation_id: "m1".to_string(),
            dag_id: "d1".to_string(),
            mutation_type: "add_node".to_string(),
            reason: "adding".to_string(),
            target_node_id: Some("n1".to_string()),
            ..Default::default()
        };
        let event = mutation_to_audit_event(&m);
        assert_eq!(event.event_type, "dag_mutation_recorded");
        assert_eq!(event.mutation_id, "m1");
        assert_eq!(event.dag_id, "d1");
        assert_eq!(event.payload["target_node_id"], json!("n1"));
        assert_eq!(event.payload["requires_approval"], json!(false));
    }

    #[test]
    fn test_node_delta_split_node() {
        let mut payload = HashMap::new();
        payload.insert("replacement_node_count".to_string(), json!(3));
        let m = DAGMutation {
            mutation_type: "split_node".to_string(),
            payload,
            ..Default::default()
        };
        assert_eq!(node_delta(&m), 2);
    }

    #[test]
    fn test_edge_delta_replace_edge_is_zero() {
        let m = DAGMutation {
            mutation_type: "replace_edge".to_string(),
            ..Default::default()
        };
        assert_eq!(edge_delta(&m), 0);
    }
}
