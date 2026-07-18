use engine::recursive_execution::{
    RecursiveBudget, RecursiveScope, RecursiveTree, RECURSIVE_SCHEMA_VERSION,
};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::json;
use std::collections::BTreeSet;

fn scope() -> RecursiveScope {
    RecursiveScope {
        repository: Some("fixture".to_string()),
        allowed_paths: ["docs/".to_string()].into_iter().collect(),
        capabilities: ["read".to_string()].into_iter().collect(),
    }
}

fn budget() -> RecursiveBudget {
    RecursiveBudget {
        calls_remaining: 12,
        tokens_remaining: 120,
        cost_micros_remaining: 120,
        time_ms_remaining: 1200,
    }
}

fn bind_test_workflow(store: &LocalProductStore, run_id: &str, workflow_id: &str, node_id: &str) {
    store
        .import_workflow_run(&json!({
            "run_id": run_id,
            "workflow_id": workflow_id,
            "status": "created",
            "boundaries": {"execution_authority": "disabled"},
            "nodes": [{
                "node_id": node_id,
                "task_type": "agent_step",
                "status": "pending",
                "recursive_node_id": node_id,
                "agent_id": "test-agent",
                "creation_receipt_sha256": "test-root-receipt"
            }],
            "edges": [],
            "events": [],
            "approvals": []
        }))
        .expect("bind test workflow");
}

#[test]
fn recursive_tree_round_trips_through_local_store() {
    let store = LocalProductStore::new(":memory:").expect("store");
    let mut tree = RecursiveTree::new(
        "recursive-run-1",
        "fixture-workflow",
        "root objective",
        scope(),
        BTreeSet::from(["read".to_string()]),
        budget(),
    );
    bind_test_workflow(
        &store,
        &tree.root_run_id,
        &tree.workflow_id,
        &tree.root_node_id,
    );
    tree.bind_root_identity(
        "test-agent",
        &tree.root_node_id.clone(),
        "test-root-receipt",
    )
    .expect("bind root identity");

    store.save_recursive_tree(&tree).expect("save");
    let loaded = store
        .load_recursive_tree("recursive-run-1")
        .expect("load")
        .expect("tree exists");
    assert_eq!(loaded, tree);
    assert_eq!(loaded.schema_version, RECURSIVE_SCHEMA_VERSION);
    assert_eq!(loaded.nodes.len(), 1);

    let evidence = store
        .recursive_tree_operator_evidence("recursive-run-1")
        .expect("evidence");
    assert_eq!(evidence["node_count"], 1);
    assert!(!evidence.to_string().contains("root objective"));
}

#[test]
fn recursive_schema_rollback_refuses_persisted_tree_and_reapplies_empty_state() {
    let occupied_path = tempfile::NamedTempFile::new().expect("path");
    let occupied = LocalProductStore::new(occupied_path.path()).expect("store");
    let mut tree = RecursiveTree::new(
        "recursive-run-rollback",
        "fixture-workflow",
        "root objective",
        scope(),
        BTreeSet::from(["read".to_string()]),
        budget(),
    );
    bind_test_workflow(
        &occupied,
        &tree.root_run_id,
        &tree.workflow_id,
        &tree.root_node_id,
    );
    tree.bind_root_identity(
        "test-agent",
        &tree.root_node_id.clone(),
        "test-root-receipt",
    )
    .expect("bind root identity");
    occupied.save_recursive_tree(&tree).expect("save");
    let error = occupied
        .rollback_v26_to_v25("test", true)
        .expect_err("persisted tree must block destructive rollback");
    assert!(error.contains("authoritative recursive execution data"));
    assert_eq!(occupied.schema_version().expect("version"), 26);

    let empty_path = tempfile::NamedTempFile::new().expect("path");
    let empty = LocalProductStore::new(empty_path.path()).expect("store");
    empty.rollback_v26_to_v25("test", true).expect("rollback");
    assert_eq!(empty.schema_version().expect("version"), 25);
    drop(empty);
    let reapplied = LocalProductStore::new(empty_path.path()).expect("reopen");
    assert_eq!(reapplied.schema_version().expect("version"), 26);
}
