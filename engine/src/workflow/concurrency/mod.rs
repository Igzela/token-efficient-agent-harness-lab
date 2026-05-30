mod controller;
mod dag_types;
mod helpers;
mod types;

pub use controller::ConcurrencyController;
pub use dag_types::{DagEdge, DagNode, DagState};
pub use helpers::{
    blocking_reason, conflicting_files, edge_blocks, item_id, metadata, read_files, write_files,
};
pub use types::{FileOverlap, ScheduleBatch};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_item(
        node_id: &str,
        read_files: Vec<&str>,
        write_files: Vec<&str>,
    ) -> serde_json::Value {
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
        let mut node_status = std::collections::HashMap::new();
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
        let node_status = std::collections::HashMap::new();
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
        let node_status = std::collections::HashMap::new();
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
