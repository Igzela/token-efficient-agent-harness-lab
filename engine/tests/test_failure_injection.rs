use std::sync::Arc;
use std::thread;

use engine::node_executor::{CommandNodeExecutor, FailNodeExecutor};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::tempdir;

fn make_single_node_plan(ids: &engine::storage::local_product_store::WorkflowPlanIds) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-0001", "task_domain": "test"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-06-05T00:00:00Z",
            "updated_at": "2026-06-05T00:00:00Z",
            "nodes": [
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-a",
                    "workflow_id": ids.workflow_id,
                    "task_type": "command",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": [],
                    "output_ref": null,
                    "budget": 0.1,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                }
            ],
            "edges": [],
            "started_at": null,
            "completed_at": null,
            "result": null
        },
        "boundaries": {
            "execution": "disabled",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled",
        },
    })
}

fn make_plan_and_run(store: &LocalProductStore) -> (String, String) {
    let plan = store
        .create_workflow_plan("test run", "api", "actor", |ids, _| {
            Ok(make_single_node_plan(ids))
        })
        .unwrap();
    let plan_id = plan["plan_id"].as_str().unwrap().to_string();
    let run = store
        .create_workflow_run_from_plan(&plan_id, "actor")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap().to_string();
    (plan_id, run_id)
}

// 1. test_executor_timeout_recovery: tick with "sleep 30" + 200ms timeout -> "command_timeout"
#[test]
fn test_executor_timeout_recovery() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let plan = store
        .create_workflow_plan("timeout test", "api", "actor", |ids, _| {
            let mut plan = make_single_node_plan(ids);
            // Inject command into node metadata
            if let Some(nodes) = plan["graph"]["nodes"].as_array_mut() {
                if let Some(node) = nodes.first_mut() {
                    node["command"] = json!("sleep 30");
                }
            }
            Ok(plan)
        })
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();

    let executor = CommandNodeExecutor {
        timeout_ms: 200,
        allowed_commands: vec!["sleep".to_string()],
        allowed_binaries: vec!["sleep".to_string()],
        env_vars: Vec::new(),
    };

    let result = store
        .tick_with_executor(run_id, "actor", 0, &executor)
        .unwrap();

    assert_eq!(result["action"], "node_executed");
    assert_eq!(result["result"]["status"], "failed");
    assert_eq!(result["result"]["error_domain"], "command_timeout");
    assert!(result["result"]["error_message"]
        .as_str()
        .unwrap()
        .contains("timeout"));

    // After timeout, the run should transition to failed since all nodes are terminal
    let final_run = store.get_workflow_run(run_id).unwrap().unwrap();
    assert_eq!(final_run["status"], "failed");
}

// 2. test_node_retry_exhaustion: FailNodeExecutor + max_retries=2, tick 3x -> run "failed"
#[test]
fn test_node_retry_exhaustion() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let (_, run_id) = make_plan_and_run(&store);

    let executor = FailNodeExecutor::default();

    // Tick 1: attempt 1 fails, retry scheduled (max_retries=2, attempt <= 2)
    let r1 = store
        .tick_with_executor(&run_id, "actor", 2, &executor)
        .unwrap();
    assert_eq!(r1["action"], "node_retry");
    assert_eq!(r1["attempt"], 1);

    // Tick 2: attempt 2 fails, retry scheduled
    let r2 = store
        .tick_with_executor(&run_id, "actor", 2, &executor)
        .unwrap();
    assert_eq!(r2["action"], "node_retry");
    assert_eq!(r2["attempt"], 2);

    // Tick 3: attempt 3 fails, no more retries (3 > 2), node fails
    let r3 = store
        .tick_with_executor(&run_id, "actor", 2, &executor)
        .unwrap();
    assert_eq!(r3["action"], "node_executed");
    assert_eq!(r3["result"]["status"], "failed");
    assert_eq!(r3["attempt"], 3);

    // Run should be failed
    let run = store.get_workflow_run(&run_id).unwrap().unwrap();
    assert_eq!(run["status"], "failed");
}

// 3. test_workspace_cleanup_missing_dir: create workspace, delete dir, cleanup -> graceful
#[test]
fn test_workspace_cleanup_missing_dir() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let target_dir = tempdir().unwrap();
    let workspace_dir = dir.path().join("workspaces").join("ws-missing");

    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-0001",
                "run_id": "run-0001",
                "target_id": "target-001",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_dir.to_string_lossy(),
                "source_revision": "abc123",
                "status": "workspace_created",
            }),
            "operator",
        )
        .unwrap();
    let ws_id = workspace["workspace_id"].as_str().unwrap();

    // Create the directory so cleanup has something to try to remove
    std::fs::create_dir_all(&workspace_dir).unwrap();
    assert!(workspace_dir.exists());

    // Delete it externally to simulate a missing directory
    std::fs::remove_dir_all(&workspace_dir).unwrap();
    assert!(!workspace_dir.exists());

    // Cleanup should succeed gracefully (path.exists() check skips remove_dir_all)
    let cleaned = store.cleanup_workspace(ws_id, "operator").unwrap();
    assert_eq!(cleaned["status"], "cleaned");
}

// 4. test_approval_expiry_blocks_export: approval with expires_at in past -> export_eligible false
#[test]
fn test_approval_expiry_blocks_export() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let target_dir = tempdir().unwrap();
    let workspace_dir = dir.path().join("workspaces").join("ws-expiry");

    // Create a plan + run for the workflow run approval
    let plan = store
        .create_workflow_plan("expiry test", "api", "actor", |ids, _| {
            Ok(make_single_node_plan(ids))
        })
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap();

    // Record an approval with expires_at in the past
    let _approval = store
        .record_workflow_run_approval(
            run_id,
            "node-a",
            "approved",
            "reviewer",
            Some("approved with expiry"),
            Some("sha256:abc123"),
            Some("rev-001"),
            Some(&["file.txt".to_string()]),
            Some("2020-01-01T00:00:00Z"),
        )
        .unwrap();

    // Create workspace + artifact with matching hash/files
    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": plan["plan_id"].as_str().unwrap(),
                "run_id": run_id,
                "target_id": "target-001",
                "target_repo_path": target_dir.path().to_string_lossy(),
                "workspace_path": workspace_dir.to_string_lossy(),
                "source_revision": "rev-001",
                "status": "patch_prepared",
            }),
            "operator",
        )
        .unwrap();

    let artifact = store
        .record_supervised_patch_artifact(
            &json!({
                "workspace_id": workspace["workspace_id"].as_str().unwrap(),
                "patch_hash": "sha256:abc123",
                "changed_files": ["file.txt"],
            }),
            "operator",
        )
        .unwrap();

    // Validate binding: expired approval should make export_eligible false
    let binding = store
        .validate_approval_binding(run_id, artifact["artifact_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(binding["export_eligible"], false);

    let checks = binding["binding_checks"].as_array().unwrap();
    let first_check = &checks[0];
    assert_eq!(first_check["not_expired"], false);
}

// 5. test_artifact_integrity_tamper: capture patch, modify file -> integrity detects change
#[test]
fn test_artifact_integrity_tamper() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    // Create a target repo directory with a file
    let target_dir = dir.path().join("target_repo");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("hello.txt"), "original content").unwrap();

    // Use create_workspace_directory to make a proper workspace with manifest
    let ws_path = store
        .create_workspace_directory("ws-tamper", target_dir.to_str().unwrap())
        .unwrap();
    let ws_path = std::path::PathBuf::from(&ws_path);
    assert!(ws_path.exists());
    assert!(ws_path.join(".source_manifest.json").exists());

    // Record the workspace in the store
    let workspace = store
        .record_supervised_patch_workspace(
            &json!({
                "plan_id": "plan-0001",
                "run_id": "run-0001",
                "target_id": "target-001",
                "target_repo_path": target_dir.to_string_lossy(),
                "workspace_path": ws_path.to_string_lossy(),
                "source_revision": "abc123",
                "status": "workspace_created",
            }),
            "operator",
        )
        .unwrap();
    let ws_id = workspace["workspace_id"].as_str().unwrap();

    // Modify a file in the workspace to create a diff
    std::fs::write(ws_path.join("hello.txt"), "modified content").unwrap();

    // Capture the patch
    let artifact = store.capture_patch(ws_id, "operator").unwrap();
    let artifact_id = artifact["artifact_id"].as_str().unwrap();
    assert_eq!(artifact["schema_version"], "supervised_patch_artifact.v1");

    // Integrity should pass now
    let integrity = store.validate_artifact_integrity(artifact_id).unwrap();
    assert_eq!(
        integrity["integrity_ok"], true,
        "integrity should pass before tamper"
    );

    // Tamper: add a new file after capture, which changes the diff set
    std::fs::write(ws_path.join("tampered.txt"), "injected content").unwrap();

    // Integrity should now detect the change
    let integrity_after = store.validate_artifact_integrity(artifact_id).unwrap();
    assert_eq!(
        integrity_after["integrity_ok"], false,
        "integrity should fail after tamper"
    );

    let checks = integrity_after["checks"].as_array().unwrap();
    let hash_check = checks
        .iter()
        .find(|c| c["check"] == "patch_hash_unchanged")
        .unwrap();
    assert_eq!(hash_check["passed"], false);
}

// 6. test_concurrent_tick_no_double_execute: 2 threads tick same run -> no double exec
#[test]
fn test_concurrent_tick_no_double_execute() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());
    let (_, run_id) = make_plan_and_run(&store);

    let executor = Arc::new(FailNodeExecutor::default());
    let run_id = Arc::new(run_id);

    let mut handles = Vec::new();
    for _ in 0..2 {
        let store = Arc::clone(&store);
        let executor = Arc::clone(&executor);
        let run_id = Arc::clone(&run_id);
        handles.push(thread::spawn(move || {
            store.tick_with_executor(&run_id, "actor", 0, &*executor)
        }));
    }

    let mut executed = 0;
    let mut no_ready = 0;
    let mut errors = 0;
    for handle in handles {
        match handle.join() {
            Ok(Ok(result)) => {
                let action = result["action"].as_str().unwrap_or("");
                if action == "node_executed" || action == "node_retry" {
                    executed += 1;
                } else if action == "no_ready_node" {
                    no_ready += 1;
                }
            }
            _ => errors += 1,
        }
    }

    // Exactly one thread should have executed the node; the other should have found no ready node
    assert_eq!(
        executed, 1,
        "exactly one thread should execute the node, got {executed}"
    );
    assert_eq!(
        no_ready + errors,
        1,
        "the other thread should see no ready node or error"
    );

    // Verify node status: node-a should be failed (FailNodeExecutor), not double-processed
    let run = store
        .get_workflow_run(&run_id.to_string())
        .unwrap()
        .unwrap();
    let nodes = run["nodes"].as_array().unwrap();
    let node_a = nodes.iter().find(|n| n["node_id"] == "node-a").unwrap();
    let db_status = node_a["db_status"].as_str().unwrap();
    assert!(
        db_status == "failed" || db_status == "pending",
        "node should be failed or pending (retry), not double-executed: {db_status}"
    );
}
