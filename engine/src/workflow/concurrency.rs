use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// DAG types (minimal subset needed by concurrency controller)
// ---------------------------------------------------------------------------

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

fn default_edge_status() -> String {
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

// ---------------------------------------------------------------------------
// Concurrency types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileOverlap {
    pub item_a_id: String,
    pub item_b_id: String,
    pub files: Vec<String>,
}

impl Default for FileOverlap {
    fn default() -> Self {
        Self {
            item_a_id: String::new(),
            item_b_id: String::new(),
            files: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScheduleBatch {
    pub scheduled_items: Vec<Value>,
    pub blocked_items: Vec<Value>,
    pub file_overlaps: Vec<FileOverlap>,
    pub warnings: Vec<String>,
}

impl Default for ScheduleBatch {
    fn default() -> Self {
        Self {
            scheduled_items: Vec::new(),
            blocked_items: Vec::new(),
            file_overlaps: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

impl ScheduleBatch {
    pub fn item_ids(&self) -> Vec<String> {
        self.scheduled_items.iter().map(item_id).collect()
    }
}

// ---------------------------------------------------------------------------
// ConcurrencyController
// ---------------------------------------------------------------------------

pub struct ConcurrencyController {
    pub max_concurrent: usize,
}

impl Default for ConcurrencyController {
    fn default() -> Self {
        Self { max_concurrent: 4 }
    }
}

impl ConcurrencyController {
    pub fn new(max_concurrent: usize) -> Self {
        if max_concurrent < 1 {
            panic!("max_concurrent must be at least 1");
        }
        Self { max_concurrent }
    }

    pub fn schedule(
        &self,
        ready_items: &[Value],
        dag: &DagState,
        active_claims: &[Value],
    ) -> ScheduleBatch {
        if ready_items.is_empty() {
            return ScheduleBatch::default();
        }

        let overlaps = self.detect_file_overlaps(ready_items);
        let mut blocked: Vec<Value> = Vec::new();
        let mut eligible: Vec<Value> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();

        let mut sorted_items = ready_items.to_vec();
        sorted_items.sort_by_key(item_id);

        for item in sorted_items {
            if let Some(reason) = blocking_reason(&item, dag, active_claims) {
                blocked.push(item.clone());
                warnings.push(reason);
            } else {
                eligible.push(item);
            }
        }

        let mut scheduled: Vec<Value> = Vec::new();
        for item in eligible {
            if scheduled.len() >= self.max_concurrent {
                warnings.push(format!("{} exceeds max_concurrent", item_id(&item)));
                blocked.push(item);
                continue;
            }
            if scheduled
                .iter()
                .all(|existing| self.can_run_parallel(existing, &item, &overlaps))
            {
                scheduled.push(item);
            } else {
                warnings.push(format!(
                    "{} conflicts with scheduled file claims",
                    item_id(&item)
                ));
                blocked.push(item);
            }
        }

        ScheduleBatch {
            scheduled_items: scheduled,
            blocked_items: blocked,
            file_overlaps: overlaps,
            warnings,
        }
    }

    pub fn detect_file_overlaps(&self, items: &[Value]) -> Vec<FileOverlap> {
        let mut overlaps = Vec::new();
        let mut sorted_items = items.to_vec();
        sorted_items.sort_by_key(item_id);

        for (index, item_a) in sorted_items.iter().enumerate() {
            for item_b in sorted_items.iter().skip(index + 1) {
                let files = conflicting_files(item_a, item_b);
                if !files.is_empty() {
                    overlaps.push(FileOverlap {
                        item_a_id: item_id(item_a),
                        item_b_id: item_id(item_b),
                        files,
                    });
                }
            }
        }
        overlaps
    }

    pub fn can_run_parallel(
        &self,
        item_a: &Value,
        item_b: &Value,
        overlaps: &[FileOverlap],
    ) -> bool {
        let mut pair = vec![item_id(item_a), item_id(item_b)];
        pair.sort();
        for overlap in overlaps {
            let mut overlap_pair = vec![overlap.item_a_id.clone(), overlap.item_b_id.clone()];
            overlap_pair.sort();
            if pair == overlap_pair {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
    // Fallback: try as string (shouldn't happen in practice)
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
    // a writes & (b writes | b reads)
    for f in a_writes.iter() {
        if b_writes.contains(f) || b_reads.contains(f) {
            conflicts.insert(f.clone());
        }
    }
    // b writes & a reads
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

    // Check incoming edges for dependency blocks
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

    // Check active file claims
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_item(node_id: &str, read_files: Vec<&str>, write_files: Vec<&str>) -> Value {
        json!({
            "node_id": node_id,
            "metadata": {
                "read_files": read_files,
                "write_files": write_files
            }
        })
    }

    fn empty_dag() -> DagState {
        DagState::default()
    }

    #[test]
    fn test_item_id_from_node_id() {
        let item = json!({"node_id": "task-1"});
        assert_eq!(item_id(&item), "task-1");
    }

    #[test]
    fn test_item_id_fallback_task_id() {
        let item = json!({"task_id": "task-2"});
        assert_eq!(item_id(&item), "task-2");
    }

    #[test]
    fn test_read_files_sorted() {
        let item = make_item("n1", vec!["b.txt", "a.txt"], vec![]);
        assert_eq!(read_files(&item), vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn test_write_files_sorted() {
        let item = make_item("n1", vec![], vec!["z.rs", "a.rs"]);
        assert_eq!(write_files(&item), vec!["a.rs", "z.rs"]);
    }

    #[test]
    fn test_conflicting_files_write_write() {
        let a = make_item("a", vec![], vec!["shared.txt"]);
        let b = make_item("b", vec![], vec!["shared.txt"]);
        assert_eq!(conflicting_files(&a, &b), vec!["shared.txt"]);
    }

    #[test]
    fn test_conflicting_files_write_read() {
        let a = make_item("a", vec![], vec!["lib.rs"]);
        let b = make_item("b", vec!["lib.rs"], vec![]);
        assert_eq!(conflicting_files(&a, &b), vec!["lib.rs"]);
    }

    #[test]
    fn test_conflicting_files_none() {
        let a = make_item("a", vec!["a.txt"], vec![]);
        let b = make_item("b", vec!["b.txt"], vec![]);
        assert!(conflicting_files(&a, &b).is_empty());
    }

    #[test]
    fn test_schedule_empty_items() {
        let ctrl = ConcurrencyController::default();
        let batch = ctrl.schedule(&[], &empty_dag(), &[]);
        assert!(batch.scheduled_items.is_empty());
        assert!(batch.blocked_items.is_empty());
    }

    #[test]
    fn test_schedule_no_conflicts() {
        let ctrl = ConcurrencyController::default();
        let items = vec![
            make_item("a", vec!["a.txt"], vec!["a_out.txt"]),
            make_item("b", vec!["b.txt"], vec!["b_out.txt"]),
        ];
        let batch = ctrl.schedule(&items, &empty_dag(), &[]);
        assert_eq!(batch.scheduled_items.len(), 2);
        assert!(batch.blocked_items.is_empty());
    }

    #[test]
    fn test_schedule_write_conflict_blocks() {
        let ctrl = ConcurrencyController::default();
        let items = vec![
            make_item("a", vec![], vec!["shared.txt"]),
            make_item("b", vec!["shared.txt"], vec![]),
        ];
        let batch = ctrl.schedule(&items, &empty_dag(), &[]);
        assert_eq!(batch.scheduled_items.len(), 1);
        assert_eq!(batch.blocked_items.len(), 1);
        assert!(!batch.warnings.is_empty());
    }

    #[test]
    fn test_schedule_max_concurrent_limits() {
        let ctrl = ConcurrencyController::new(1);
        let items = vec![
            make_item("a", vec!["a.txt"], vec![]),
            make_item("b", vec!["b.txt"], vec![]),
        ];
        let batch = ctrl.schedule(&items, &empty_dag(), &[]);
        assert_eq!(batch.scheduled_items.len(), 1);
        assert_eq!(batch.blocked_items.len(), 1);
    }

    #[test]
    fn test_schedule_item_ids_property() {
        let ctrl = ConcurrencyController::default();
        let items = vec![
            make_item("x", vec!["x.txt"], vec![]),
            make_item("y", vec!["y.txt"], vec![]),
        ];
        let batch = ctrl.schedule(&items, &empty_dag(), &[]);
        let ids = batch.item_ids();
        assert!(ids.contains(&"x".to_string()));
        assert!(ids.contains(&"y".to_string()));
    }

    #[test]
    fn test_detect_file_overlaps() {
        let ctrl = ConcurrencyController::default();
        let items = vec![
            make_item("a", vec![], vec!["shared.txt"]),
            make_item("b", vec!["shared.txt"], vec![]),
            make_item("c", vec!["c.txt"], vec![]),
        ];
        let overlaps = ctrl.detect_file_overlaps(&items);
        assert_eq!(overlaps.len(), 1);
        assert_eq!(overlaps[0].files, vec!["shared.txt"]);
    }

    #[test]
    fn test_can_run_parallel_no_overlap() {
        let ctrl = ConcurrencyController::default();
        let a = make_item("a", vec!["a.txt"], vec![]);
        let b = make_item("b", vec!["b.txt"], vec![]);
        assert!(ctrl.can_run_parallel(&a, &b, &[]));
    }

    #[test]
    fn test_can_run_parallel_with_overlap() {
        let ctrl = ConcurrencyController::default();
        let a = make_item("a", vec![], vec!["shared.txt"]);
        let b = make_item("b", vec!["shared.txt"], vec![]);
        let overlaps = vec![FileOverlap {
            item_a_id: "a".to_string(),
            item_b_id: "b".to_string(),
            files: vec!["shared.txt".to_string()],
        }];
        assert!(!ctrl.can_run_parallel(&a, &b, &overlaps));
    }

    #[test]
    fn test_edge_blocks_hard_unsatisfied() {
        let edge = DagEdge {
            edge_id: "e1".to_string(),
            from_node: "a".to_string(),
            to_node: "b".to_string(),
            dependency_type: "hard".to_string(),
            status: "pending".to_string(),
        };
        let mut node_status = HashMap::new();
        node_status.insert("a".to_string(), "running".to_string());
        let item = json!({"node_id": "b"});
        assert!(edge_blocks(&edge, &item, &node_status));
    }

    #[test]
    fn test_edge_blocks_soft_never_blocks() {
        let edge = DagEdge {
            edge_id: "e1".to_string(),
            from_node: "a".to_string(),
            to_node: "b".to_string(),
            dependency_type: "soft".to_string(),
            status: "pending".to_string(),
        };
        let node_status = HashMap::new();
        let item = json!({"node_id": "b"});
        assert!(!edge_blocks(&edge, &item, &node_status));
    }

    #[test]
    fn test_edge_blocks_satisfied_never_blocks() {
        let edge = DagEdge {
            edge_id: "e1".to_string(),
            from_node: "a".to_string(),
            to_node: "b".to_string(),
            dependency_type: "hard".to_string(),
            status: "satisfied".to_string(),
        };
        let node_status = HashMap::new();
        let item = json!({"node_id": "b"});
        assert!(!edge_blocks(&edge, &item, &node_status));
    }

    #[test]
    fn test_blocking_reason_with_dag_dependency() {
        let dag = DagState {
            dag_id: "d1".to_string(),
            version: 1,
            nodes: vec![DagNode {
                node_id: "a".to_string(),
                status: "running".to_string(),
                ..DagNode::default()
            }],
            edges: vec![DagEdge {
                edge_id: "e1".to_string(),
                from_node: "a".to_string(),
                to_node: "b".to_string(),
                dependency_type: "hard".to_string(),
                status: "pending".to_string(),
            }],
            ..DagState::default()
        };
        let item = json!({"node_id": "b", "metadata": {}});
        let reason = blocking_reason(&item, &dag, &[]);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("blocked by hard dependency"));
    }

    #[test]
    fn test_blocking_reason_with_active_claim() {
        let item = make_item("b", vec![], vec!["shared.txt"]);
        let claim = json!({"file_path": "shared.txt", "released": false});
        let reason = blocking_reason(&item, &empty_dag(), &[claim]);
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("active write claim"));
    }

    #[test]
    fn test_schedule_blocks_on_dag_dependency() {
        let dag = DagState {
            dag_id: "d1".to_string(),
            version: 1,
            nodes: vec![DagNode {
                node_id: "a".to_string(),
                status: "running".to_string(),
                ..DagNode::default()
            }],
            edges: vec![DagEdge {
                edge_id: "e1".to_string(),
                from_node: "a".to_string(),
                to_node: "b".to_string(),
                dependency_type: "hard".to_string(),
                status: "pending".to_string(),
            }],
            ..DagState::default()
        };
        let ctrl = ConcurrencyController::default();
        let items = vec![make_item("b", vec!["b.txt"], vec![])];
        let batch = ctrl.schedule(&items, &dag, &[]);
        assert!(batch.scheduled_items.is_empty());
        assert_eq!(batch.blocked_items.len(), 1);
    }

    #[test]
    fn test_controller_default_max_concurrent() {
        let ctrl = ConcurrencyController::default();
        assert_eq!(ctrl.max_concurrent, 4);
    }

    #[test]
    fn test_file_overlap_default() {
        let fo = FileOverlap::default();
        assert!(fo.files.is_empty());
    }
}
