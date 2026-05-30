use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::dag_types::{DagEdge, DagState};

pub fn item_id(item: &Value) -> String {
    if let Some(obj) = item.as_object() {
        for key in &["node_id", "task_id", "item_id"] {
            if let Some(v) = obj.get(*key).and_then(|v| v.as_str()) {
                if !v.is_empty() {
                    return v.to_string();
                }
            }
        }
    }
    item.as_str().unwrap_or("").to_string()
}

pub fn metadata(item: &Value) -> Value {
    if let Some(obj) = item.as_object() {
        if let Some(meta) = obj.get("metadata") {
            return meta.clone();
        }
    }
    item.clone()
}

pub fn read_files(item: &Value) -> Vec<String> {
    let meta = metadata(item);
    let mut files: Vec<String> = meta
        .get("read_files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

pub fn write_files(item: &Value) -> Vec<String> {
    let meta = metadata(item);
    let mut files: Vec<String> = meta
        .get("write_files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

pub fn conflicting_files(item_a: &Value, item_b: &Value) -> Vec<String> {
    let a_writes: HashSet<String> = write_files(item_a).into_iter().collect();
    let b_writes: HashSet<String> = write_files(item_b).into_iter().collect();
    let a_reads: HashSet<String> = read_files(item_a).into_iter().collect();
    let b_reads: HashSet<String> = read_files(item_b).into_iter().collect();

    let mut conflicts: HashSet<String> = HashSet::new();
    for f in a_writes.iter() {
        if b_writes.contains(f) || b_reads.contains(f) {
            conflicts.insert(f.clone());
        }
    }
    for f in b_writes.iter() {
        if a_reads.contains(f) {
            conflicts.insert(f.clone());
        }
    }

    let mut result: Vec<String> = conflicts.into_iter().collect();
    result.sort();
    result
}

pub fn blocking_reason(item: &Value, state: &DagState, active_claims: &[Value]) -> Option<String> {
    let id = item_id(item);

    let mut incoming: Vec<&DagEdge> = state.edges.iter().filter(|e| e.to_node == id).collect();
    incoming.sort_by(|a, b| a.edge_id.cmp(&b.edge_id));

    let node_status: HashMap<String, String> = state
        .nodes
        .iter()
        .map(|n| (n.node_id.clone(), n.status.clone()))
        .collect();

    for edge in &incoming {
        if edge_blocks(edge, item, &node_status) {
            return Some(format!(
                "{} blocked by {} dependency {}",
                id, edge.dependency_type, edge.edge_id
            ));
        }
    }

    let item_writes: HashSet<String> = write_files(item).into_iter().collect();
    let mut active_files: HashSet<String> = HashSet::new();
    for claim in active_claims {
        if let Some(obj) = claim.as_object() {
            let released = obj
                .get("released")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !released {
                if let Some(path) = obj.get("file_path").and_then(|v| v.as_str()) {
                    active_files.insert(path.to_string());
                }
            }
        }
    }
    active_files.remove("");

    let mut conflict: Vec<String> = active_files.intersection(&item_writes).cloned().collect();
    conflict.sort();
    if let Some(first) = conflict.first() {
        return Some(format!("{} blocked by active write claim on {}", id, first));
    }

    None
}

pub fn edge_blocks(edge: &DagEdge, item: &Value, node_status: &HashMap<String, String>) -> bool {
    if edge.dependency_type == "soft" {
        return false;
    }
    if edge.status == "satisfied" {
        return false;
    }
    if edge.dependency_type == "artifact" {
        let meta = metadata(item);
        let verified: HashSet<String> = meta
            .get("verified_artifacts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        return !verified.contains(&edge.edge_id) && !verified.contains(&edge.from_node);
    }
    node_status.get(&edge.from_node).map(|s| s.as_str()) != Some("completed")
}
