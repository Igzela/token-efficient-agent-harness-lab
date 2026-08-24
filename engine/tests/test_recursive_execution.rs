use engine::recursive_execution::{
    recursive_root_creation_receipt_sha256, RecursiveBudget, RecursiveScope, RecursiveTree,
    RECURSIVE_ROOT_AUTHORITY_VERSION, RECURSIVE_SCHEMA_VERSION,
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

fn establish_v36_fixture(store: &LocalProductStore) {
    let connection = rusqlite::Connection::open(store.db_path()).expect("open sqlite fixture");
    connection
        .execute_batch(
            "DROP TABLE harness_evolution_ec2_prediction_outcomes;
             PRAGMA user_version = 36;",
        )
        .expect("establish v36 fixture");
}

fn bind_test_workflow(store: &LocalProductStore, tree: &mut RecursiveTree) {
    let root_node_id = tree.root_node_id.clone();
    tree.bind_root_identity(
        "test-agent",
        &root_node_id,
        &recursive_root_creation_receipt_sha256(
            "test-root-receipt",
            &tree.root_run_id,
            &tree.workflow_id,
            &root_node_id,
            "test-agent",
        ),
    )
    .expect("bind root identity");
    store
        .import_workflow_run(&json!({
            "run_id": tree.root_run_id,
            "workflow_id": tree.workflow_id,
            "status": "created",
            "boundaries": {
                "execution_authority": "disabled",
                "repository": tree.root_scope.repository,
                "allowed_paths": tree.root_scope.allowed_paths
            },
            "nodes": [{
                "node_id": tree.root_node_id,
                "task_type": "agent_step",
                "status": "pending",
                "recursive_root_node_id": tree.root_node_id,
                "agent_id": "test-agent",
                "capability_profile": tree.root_capabilities,
                "recursive_root_authority": {
                    "schema_version": RECURSIVE_ROOT_AUTHORITY_VERSION,
                    "scope": tree.root_scope,
                    "capabilities": tree.root_capabilities,
                    "tree_budget": tree.root_budget_limit,
                    "child_budget": tree.root_child_budget_limit,
                    "usage_contract": {"kind": "fixture", "calls": 1, "tokens": 1, "cost_micros": 1, "time_ms": 1}
                },
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
    bind_test_workflow(&store, &mut tree);

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
    bind_test_workflow(&occupied, &mut tree);
    occupied.save_recursive_tree(&tree).expect("save");
    // Peel delegated autonomy (v36), workspace preparation (v35), RWE (v34), managed
    // acceptance (v33/v32), terminal evidence (v31), product-task (v30),
    // then PR_READY/evolution so v26 recursive rollback can be attempted.
    establish_v36_fixture(&occupied);
    occupied
        .rollback_v36_to_v35("test", true)
        .expect("empty delegated autonomy rollback");
    occupied
        .rollback_v35_to_v34("test", true)
        .expect("empty workspace preparation rollback");
    occupied
        .rollback_v34_to_v33("test", true)
        .expect("empty rwe surface rollback");
    occupied
        .rollback_v33_to_v32("test", true)
        .expect("empty managed acceptance spend rollback");
    occupied
        .rollback_v32_to_v31("test", true)
        .expect("empty managed acceptance surface rollback");
    occupied
        .rollback_v31_to_v30("test", true)
        .expect("empty terminal evidence surface rollback");
    occupied
        .rollback_v30_to_v29("test", true)
        .expect("empty product_tasks surface rollback");
    occupied
        .rollback_v29_to_v28("test", true)
        .expect("empty PR_READY surface rollback");
    occupied
        .rollback_v28_to_v27("test", true)
        .expect("empty evaluation surface rollback");
    occupied
        .rollback_v27_to_v26("test", true)
        .expect("empty evolution surface rollback");
    let error = occupied
        .rollback_v26_to_v25("test", true)
        .expect_err("persisted tree must block destructive rollback");
    assert!(error.contains("authoritative recursive execution data"));
    assert_eq!(occupied.schema_version().expect("version"), 26);

    let empty_path = tempfile::NamedTempFile::new().expect("path");
    let empty = LocalProductStore::new(empty_path.path()).expect("store");
    establish_v36_fixture(&empty);
    empty
        .rollback_v36_to_v35("test", true)
        .expect("rollback v36");
    empty
        .rollback_v35_to_v34("test", true)
        .expect("rollback v35");
    empty
        .rollback_v34_to_v33("test", true)
        .expect("rollback v34");
    empty
        .rollback_v33_to_v32("test", true)
        .expect("rollback v33");
    empty
        .rollback_v32_to_v31("test", true)
        .expect("rollback v32");
    empty
        .rollback_v31_to_v30("test", true)
        .expect("rollback v31");
    empty
        .rollback_v30_to_v29("test", true)
        .expect("rollback v30");
    empty
        .rollback_v29_to_v28("test", true)
        .expect("rollback v29");
    empty
        .rollback_v28_to_v27("test", true)
        .expect("rollback v28");
    empty
        .rollback_v27_to_v26("test", true)
        .expect("rollback v27");
    empty
        .rollback_v26_to_v25("test", true)
        .expect("rollback v26");
    assert_eq!(empty.schema_version().expect("version"), 25);
    drop(empty);
    let reapplied = LocalProductStore::new(empty_path.path()).expect("reopen");
    assert_eq!(reapplied.schema_version().expect("version"), 38);
}
