//! G3: verification, artifact, approval, and output orchestration for product tasks.
//! Verification commands must actually execute; fabricated pass is forbidden.

use engine::product_golden_path::{
    validate_intake, ProductExecutorPolicy, ProductTaskIntakeRequest, ProductTaskStatus,
    ProductVerificationCommand, FIXTURE_DETERMINISTIC_NOTE_CONTENT, PRODUCT_TASK_GATE,
};
use engine::storage::local_product_store::LocalProductStore;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_gates<R>(f: impl FnOnce() -> R) -> R {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    std::env::set_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", "0");
    let result = f();
    std::env::remove_var(PRODUCT_TASK_GATE);
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    result
}

fn temp_store() -> (tempfile::TempDir, LocalProductStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("store.db")).unwrap();
    (dir, store)
}

fn init_git_repo(root: &std::path::Path) -> String {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("README.md"), "hello\n").unwrap();
    for args in [
        &["init", "-b", "main"][..],
        &["config", "user.email", "g3@example.com"][..],
        &["config", "user.name", "G3"][..],
        &["add", "README.md"][..],
        &["commit", "-m", "init"][..],
    ] {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?} {:?}", out);
    }
    let out = Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "https://example.invalid/g3-product.git",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn intake(
    target: &std::path::Path,
    rev: &str,
    key: &str,
    verify_cmds: Vec<ProductVerificationCommand>,
) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "Create fixture note via golden path.".to_string(),
        target_id: "disposable".to_string(),
        target_repo_path: target.to_string_lossy().into_owned(),
        source_kind: None,
        source_revision: rev.to_string(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
        verification_commands: verify_cmds,
        output_intent: "artifact_only".to_string(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["command".to_string()],
            prefer: Some("command".to_string()),
        },
        budget: None,
        risk_class: "low".to_string(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: None,
        idempotency_key: key.to_string(),
        expected_version: None,
        tenant_id: Some("local".to_string()),
        workspace_id: Some("default".to_string()),
        workspace_mode: Some("git_worktree".to_string()),
    }
}

fn pass_verify() -> Vec<ProductVerificationCommand> {
    vec![
        ProductVerificationCommand {
            command: "test -f docs/product_golden_path_fixture.md".to_string(),
            timeout_ms: 5_000,
        },
        ProductVerificationCommand {
            command: "test -s docs/product_golden_path_fixture.md".to_string(),
            timeout_ms: 5_000,
        },
    ]
}

fn run_scheduler_ticks(store: &LocalProductStore, run_id: &str) {
    let executor = engine::node_executor::CommandNodeExecutor::default();
    for _ in 0..8 {
        let tick = store
            .tick_with_executor(run_id, "tester", 1, &executor)
            .expect("tick");
        let run_status = tick
            .pointer("/run/status")
            .and_then(|v| v.as_str())
            .or_else(|| tick.get("status").and_then(|v| v.as_str()))
            .unwrap_or("");
        let action = tick.get("action").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(run_status, "completed" | "failed") || matches!(action, "completed" | "failed")
        {
            break;
        }
    }
}

#[test]
fn end_to_end_artifact_only_path_with_real_verification() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        // Expected file must not exist on target main before the task.
        assert!(!repo.join("docs/product_golden_path_fixture.md").exists());

        let mut request = intake(&repo, &rev, "g3-e2e-1", pass_verify());
        request.confirm_output = Some(true);
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        assert_eq!(
            compiled["task"]["status"].as_str(),
            Some(ProductTaskStatus::GraphReady.as_str())
        );
        let run_id = compiled["task"]["run_id"].as_str().unwrap();

        // Scheduler (not finalize) advances the apply node.
        run_scheduler_ticks(&store, run_id);
        let run = store.get_workflow_run(run_id).unwrap().unwrap();
        assert_eq!(run["status"].as_str(), Some("completed"));

        let finalized = store
            .finalize_product_task_after_execution(task_id, "tester")
            .expect("finalize");
        assert_eq!(finalized["phase"], "awaiting_approval");
        let artifact_id = finalized["artifact_id"].as_str().expect("artifact id");
        let artifact = store
            .get_supervised_patch_artifact(artifact_id)
            .unwrap()
            .expect("artifact");
        assert_eq!(
            artifact["changed_files"],
            serde_json::json!(["+docs/product_golden_path_fixture.md"]),
            "fixture control files must never enter the product artifact"
        );
        let verification = &finalized["verification"];
        assert_eq!(verification["status"], "evidence_recorded");
        assert_eq!(verification["trustworthy"], true);
        let attempts = verification["verification_attempts"]
            .as_array()
            .expect("attempts");
        assert_eq!(attempts.len(), 2, "both declared commands must run");
        for attempt in attempts {
            assert_eq!(attempt["result_status"], "completed");
            assert_eq!(attempt["exit_status"], 0);
            assert_eq!(attempt["product_task_id"], task_id);
            assert!(attempt["started_at"].as_str().is_some());
            assert!(attempt["completed_at"].as_str().is_some());
            assert!(attempt.get("command").and_then(|v| v.as_str()).is_some());
        }

        let task = &finalized["task"];
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::AwaitingApproval.as_str())
        );
        let approval_phase = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect("current verification receipt must satisfy the real owner path");
        assert_eq!(approval_phase["stage"], "awaiting_approval");
        // A trustworthy-looking receipt from another ProductTask version must
        // not satisfy the approval-stage read. Change every persisted attempt
        // consistently so this proves the version relation itself, not merely
        // a mismatched nested field, is enforced.
        let db_path = dir.path().join("store.db");
        let connection = rusqlite::Connection::open(&db_path).unwrap();
        let workspace_record_id = task["workspace_record_id"].as_str().unwrap();
        let original_workspace_json: String = connection
            .query_row(
                "SELECT workspace_json FROM supervised_patch_workspaces WHERE workspace_id=?1",
                [workspace_record_id],
                |row| row.get(0),
            )
            .unwrap();
        let mut stale_version_workspace: serde_json::Value =
            serde_json::from_str(&original_workspace_json).unwrap();
        stale_version_workspace["verification"]["expected_task_version"] = serde_json::json!(0);
        for receipt in stale_version_workspace["verification"]["verification_attempts"]
            .as_array_mut()
            .unwrap()
        {
            receipt["expected_task_version"] = serde_json::json!(0);
        }
        connection
            .execute(
                "UPDATE supervised_patch_workspaces SET workspace_json=?1 WHERE workspace_id=?2",
                [
                    stale_version_workspace.to_string(),
                    workspace_record_id.to_string(),
                ],
            )
            .unwrap();
        let stale_version_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("verification from a stale ProductTask version must fail closed");
        assert!(
            stale_version_error.contains("immediately preceding task version"),
            "{stale_version_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_workspaces SET workspace_json=?1 WHERE workspace_id=?2",
                [
                    original_workspace_json.clone(),
                    workspace_record_id.to_string(),
                ],
            )
            .unwrap();
        let ws = task["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap();
        let note = std::path::Path::new(ws).join("docs/product_golden_path_fixture.md");
        assert!(note.exists(), "exact fixture path must exist after apply");
        assert!(
            !std::path::Path::new(ws)
                .join(".product_golden_path_apply.py")
                .exists(),
            "fixture control file must be removed before verification and capture"
        );
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            FIXTURE_DETERMINISTIC_NOTE_CONTENT
        );

        let done = store
            .approve_and_output_product_task(task_id, "tester", true)
            .expect("approve");
        assert_eq!(
            done["task"]["status"].as_str(),
            Some(ProductTaskStatus::Completed.as_str())
        );
        assert_eq!(done["output"]["mode"], "artifact_only");
        let terminal_phase = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect("completed task must retain exactly-bound terminal evidence");
        assert_eq!(terminal_phase["stage"], "terminal");

        // Target default branch byte-for-byte unchanged.
        let main_readme = std::fs::read_to_string(repo.join("README.md")).unwrap();
        assert_eq!(main_readme, "hello\n");
        assert!(!repo.join("docs/product_golden_path_fixture.md").exists());
        let main_head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&main_head.stdout).trim(), rev);

        // Tamper only the isolated test store after the positive real-owner
        // proof. Each call must re-read persisted receipt values and reject
        // stale/missing data rather than accepting JSON-field presence.
        let workspace_record_id = done["task"]["workspace_record_id"].as_str().unwrap();
        let mut tampered_workspace: serde_json::Value =
            serde_json::from_str(&original_workspace_json).unwrap();
        tampered_workspace["verification"]["trustworthy"] = serde_json::json!(false);
        connection
            .execute(
                "UPDATE supervised_patch_workspaces SET workspace_json=?1 WHERE workspace_id=?2",
                [
                    tampered_workspace.to_string(),
                    workspace_record_id.to_string(),
                ],
            )
            .unwrap();
        let verification_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("untrustworthy verification receipt must fail closed");
        assert!(
            verification_error.contains("verification receipt is not accepted and trustworthy"),
            "{verification_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_workspaces SET workspace_json=?1 WHERE workspace_id=?2",
                [
                    original_workspace_json.clone(),
                    workspace_record_id.to_string(),
                ],
            )
            .unwrap();

        let original_boundary_json: String = connection
            .query_row(
                "SELECT boundary_json FROM supervised_patch_workspaces WHERE workspace_id=?1",
                [workspace_record_id],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "UPDATE supervised_patch_workspaces SET workspace_json='not-json' WHERE workspace_id=?1",
                [workspace_record_id],
            )
            .unwrap();
        let workspace_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt workspace owner JSON must fail closed at the authority read");
        assert!(
            workspace_owner_error.contains("managed acceptance workspace owner is invalid JSON"),
            "{workspace_owner_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_workspaces SET workspace_json=?1 WHERE workspace_id=?2",
                [
                    original_workspace_json.clone(),
                    workspace_record_id.to_string(),
                ],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE supervised_patch_workspaces SET boundary_json='not-json' WHERE workspace_id=?1",
                [workspace_record_id],
            )
            .unwrap();
        let boundary_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt workspace boundary JSON must fail closed at the authority read");
        assert!(
            boundary_owner_error
                .contains("managed acceptance workspace boundary owner is invalid JSON"),
            "{boundary_owner_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_workspaces SET boundary_json=?1 WHERE workspace_id=?2",
                [original_boundary_json, workspace_record_id.to_string()],
            )
            .unwrap();

        let original_artifact_json: String = connection
            .query_row(
                "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id=?1",
                [artifact_id],
                |row| row.get(0),
            )
            .unwrap();
        let mut tampered_artifact: serde_json::Value =
            serde_json::from_str(&original_artifact_json).unwrap();
        tampered_artifact["product_output_receipt"]["expected_task_version"] = serde_json::json!(0);
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET artifact_json=?1 WHERE artifact_id=?2",
                [tampered_artifact.to_string(), artifact_id.to_string()],
            )
            .unwrap();
        let output_version_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("output receipt from a stale ProductTask version must fail closed");
        assert!(
            output_version_error.contains("output receipt/operation content binding"),
            "{output_version_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET artifact_json=?1 WHERE artifact_id=?2",
                [original_artifact_json.clone(), artifact_id.to_string()],
            )
            .unwrap();
        let original_changed_files_json: String = connection
            .query_row(
                "SELECT changed_files_json FROM supervised_patch_artifacts WHERE artifact_id=?1",
                [artifact_id],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET changed_files_json='not-json' WHERE artifact_id=?1",
                [artifact_id],
            )
            .unwrap();
        let changed_files_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt artifact changed-files owner JSON must fail closed");
        assert!(
            changed_files_owner_error
                .contains("managed acceptance artifact changed-files owner is invalid JSON"),
            "{changed_files_owner_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET changed_files_json=?1 WHERE artifact_id=?2",
                [original_changed_files_json, artifact_id.to_string()],
            )
            .unwrap();

        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET artifact_json='not-json' WHERE artifact_id=?1",
                [artifact_id],
            )
            .unwrap();
        let artifact_json_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt artifact owner JSON must fail closed at the authority read");
        assert!(
            artifact_json_owner_error.contains("managed acceptance artifact owner is invalid JSON"),
            "{artifact_json_owner_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET artifact_json=?1 WHERE artifact_id=?2",
                [original_artifact_json.clone(), artifact_id.to_string()],
            )
            .unwrap();

        let mut tampered_artifact: serde_json::Value =
            serde_json::from_str(&original_artifact_json).unwrap();
        tampered_artifact["product_task_id"] = serde_json::json!("other-product-task");
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET artifact_json=?1 WHERE artifact_id=?2",
                [tampered_artifact.to_string(), artifact_id.to_string()],
            )
            .unwrap();
        let artifact_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("artifact from another ProductTask must fail closed");
        assert!(
            artifact_owner_error.contains("no exact artifact")
                || artifact_owner_error.contains("artifact target binding"),
            "{artifact_owner_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET artifact_json=?1 WHERE artifact_id=?2",
                [original_artifact_json.clone(), artifact_id.to_string()],
            )
            .unwrap();

        let mut tampered_artifact: serde_json::Value =
            serde_json::from_str(&original_artifact_json).unwrap();
        tampered_artifact["product_output_receipt"]["approval_id"] =
            serde_json::json!("stale-approval-id");
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET artifact_json=?1 WHERE artifact_id=?2",
                [tampered_artifact.to_string(), artifact_id.to_string()],
            )
            .unwrap();
        let approval_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("output receipt must bind the current approval");
        assert!(
            approval_error.contains("approval receipt")
                || approval_error.contains("output receipt"),
            "{approval_error}"
        );
        connection
            .execute(
                "UPDATE supervised_patch_artifacts SET artifact_json=?1 WHERE artifact_id=?2",
                [original_artifact_json.clone(), artifact_id.to_string()],
            )
            .unwrap();

        let original_artifact: serde_json::Value =
            serde_json::from_str(&original_artifact_json).unwrap();
        let approval_id = original_artifact["product_output_receipt"]["approval_id"]
            .as_str()
            .unwrap();
        let original_approval_json: String = connection
            .query_row(
                "SELECT approval_json FROM workflow_run_approvals WHERE approval_id=?1",
                [approval_id],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "UPDATE workflow_run_approvals SET approval_json='not-json' WHERE approval_id=?1",
                [approval_id],
            )
            .unwrap();
        let approval_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt approval owner JSON must fail closed");
        assert!(
            approval_owner_error.contains("workflow run approval receipt is invalid JSON"),
            "{approval_owner_error}"
        );
        connection
            .execute(
                "UPDATE workflow_run_approvals SET approval_json=?1 WHERE approval_id=?2",
                [original_approval_json, approval_id.to_string()],
            )
            .unwrap();

        let run_id = done["task"]["run_id"].as_str().unwrap();
        let (node_id, original_node_json): (String, String) = connection
            .query_row(
                "SELECT node_id, node_json FROM workflow_run_nodes WHERE run_id=?1 LIMIT 1",
                [run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let original_run_json: String = connection
            .query_row(
                "SELECT run_json FROM workflow_runs WHERE run_id=?1",
                [run_id],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "UPDATE workflow_runs SET run_json='not-json' WHERE run_id=?1",
                [run_id],
            )
            .unwrap();
        let run_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt workflow run JSON must fail closed at the authority read");
        assert!(
            run_owner_error.contains("managed acceptance workflow run owner is invalid JSON"),
            "{run_owner_error}"
        );
        connection
            .execute(
                "UPDATE workflow_runs SET run_json=?1 WHERE run_id=?2",
                [original_run_json, run_id.to_string()],
            )
            .unwrap();
        let original_workflow_boundaries_json: String = connection
            .query_row(
                "SELECT boundaries_json FROM workflow_runs WHERE run_id=?1",
                [run_id],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "UPDATE workflow_runs SET boundaries_json='not-json' WHERE run_id=?1",
                [run_id],
            )
            .unwrap();
        let workflow_boundaries_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt workflow boundaries JSON must fail closed at the authority read");
        assert!(
            workflow_boundaries_owner_error
                .contains("managed acceptance workflow boundaries owner is invalid JSON"),
            "{workflow_boundaries_owner_error}"
        );
        connection
            .execute(
                "UPDATE workflow_runs SET boundaries_json=?1 WHERE run_id=?2",
                [original_workflow_boundaries_json, run_id.to_string()],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE workflow_run_nodes SET node_json='not-json' WHERE run_id=?1 AND node_id=?2",
                [run_id, node_id.as_str()],
            )
            .unwrap();
        let node_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt workflow node JSON must fail closed at the authority read");
        assert!(
            node_owner_error.contains("managed acceptance workflow node owner is invalid JSON"),
            "{node_owner_error}"
        );
        connection
            .execute(
                "UPDATE workflow_run_nodes SET node_json=?1 WHERE run_id=?2 AND node_id=?3",
                [
                    original_node_json.clone(),
                    run_id.to_string(),
                    node_id.clone(),
                ],
            )
            .unwrap();
        let duplicate_node_id = format!("{node_id}-duplicate-owner");
        let mut duplicate_node: serde_json::Value =
            serde_json::from_str(&original_node_json).unwrap();
        duplicate_node["node_id"] = serde_json::json!(duplicate_node_id);
        connection
            .execute(
                "INSERT INTO workflow_run_nodes
                 (run_id, node_id, task_type, status, node_json, attempt_count)
                 VALUES (?1, ?2, 'product_apply', 'completed', ?3, 0)",
                [
                    run_id.to_string(),
                    duplicate_node_id.clone(),
                    duplicate_node.to_string(),
                ],
            )
            .unwrap();
        let duplicate_node_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("multiple nodes claiming one ProductTask must fail closed");
        assert!(
            duplicate_node_error.contains("multiple workflow nodes claim one ProductTask owner"),
            "{duplicate_node_error}"
        );
        connection
            .execute(
                "DELETE FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?2",
                [run_id, duplicate_node_id.as_str()],
            )
            .unwrap();

        let original_terminal_evidence_json: String = connection
            .query_row(
                "SELECT evidence_json FROM product_task_terminal_evidence WHERE product_task_id=?1",
                [task_id],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "UPDATE product_task_terminal_evidence SET evidence_json='not-json' WHERE product_task_id=?1",
                [task_id],
            )
            .unwrap();
        let terminal_owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("corrupt terminal evidence JSON must fail closed at the authority read");
        assert!(
            terminal_owner_error.contains("product task terminal evidence is invalid JSON"),
            "{terminal_owner_error}"
        );
        connection
            .execute(
                "UPDATE product_task_terminal_evidence SET evidence_json=?1 WHERE product_task_id=?2",
                [original_terminal_evidence_json, task_id.to_string()],
            )
            .unwrap();

        connection
            .execute(
                "DELETE FROM product_task_terminal_evidence WHERE product_task_id=?1",
                [task_id],
            )
            .unwrap();
        let terminal_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("completed claim without terminal evidence must fail closed");
        assert!(
            terminal_error.contains("terminal evidence"),
            "{terminal_error}"
        );
    });
}

#[test]
fn managed_acceptance_product_task_phase_fails_closed_on_binding_boolean_and_owner_bypasses() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let mut request = intake(&repo, &rev, "g3-managed-acceptance-phase", pass_verify());
        request.confirm_output = Some(true);
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();

        let pre_execution = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect("pre-execution admission requires actual booleans and readable owners only");
        assert_eq!(pre_execution["stage"], "pre_execution_admission");

        let target_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "other-target", &rev)
            .expect_err("target identity must not fall back to a path");
        assert!(target_error.contains("target_id"), "{target_error}");
        let revision_error = store
            .validate_managed_acceptance_product_task_phase(
                "local",
                task_id,
                "disposable",
                &"f".repeat(40),
            )
            .expect_err("source revision must exactly bind spend main SHA");
        assert!(
            revision_error.contains("source_revision"),
            "{revision_error}"
        );

        // Direct SQL is test-only tampering of an isolated store. The public
        // seam must read the persisted boolean rather than treat field presence
        // as a positive confirmation.
        let db_path = dir.path().join("store.db");
        let connection = rusqlite::Connection::open(&db_path).unwrap();
        connection
            .execute(
                "UPDATE product_tasks SET confirm_output=0 WHERE task_id=?1",
                [task_id],
            )
            .unwrap();
        let boolean_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("persisted false confirmation must fail closed");
        assert!(boolean_error.contains("confirm_output"), "{boolean_error}");
        connection
            .execute(
                "UPDATE product_tasks SET confirm_output=-1 WHERE task_id=?1",
                [task_id],
            )
            .unwrap();
        let malformed_boolean_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("non-boolean confirmation storage must fail closed");
        assert!(
            malformed_boolean_error.contains("confirm_output is not a persisted boolean"),
            "{malformed_boolean_error}"
        );
    });

    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo-owner-error");
        let rev = init_git_repo(&repo);
        let mut request = intake(
            &repo,
            &rev,
            "g3-managed-acceptance-owner-error",
            pass_verify(),
        );
        request.confirm_output = Some(true);
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();

        // Isolated destructive fixture: prove an evidence-owner read error is
        // propagated rather than converted into an absent/valid receipt.
        let connection = rusqlite::Connection::open(dir.path().join("store.db")).unwrap();
        connection
            .execute_batch("DROP TABLE product_task_terminal_evidence")
            .unwrap();
        let owner_error = store
            .validate_managed_acceptance_product_task_phase("local", task_id, "disposable", &rev)
            .expect_err("missing evidence owner must fail closed");
        assert!(
            owner_error.contains("product_task_terminal_evidence"),
            "{owner_error}"
        );
    });
}

#[test]
fn artifact_capture_rejects_changes_outside_product_allowed_paths() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let mut request = intake(&repo, &rev, "g3-out-of-scope-artifact", pass_verify());
        request
            .allowed_paths
            .push("./admitted-subtree/".to_string());
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        run_scheduler_ticks(&store, run_id);

        let workspace = compiled["task"]["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap();
        std::fs::create_dir_all(std::path::Path::new(workspace).join("admitted-subtree/nested"))
            .unwrap();
        std::fs::write(
            std::path::Path::new(workspace).join("admitted-subtree/nested/allowed.md"),
            "admitted subtree change\n",
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(workspace).join("outside-product-scope.txt"),
            "must not enter artifact\n",
        )
        .unwrap();

        let finalized = store
            .finalize_product_task_after_execution(task_id, "tester")
            .expect("out-of-scope repository changes must produce a durable blocked result");
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(
            finalized["task"]["status"],
            ProductTaskStatus::Blocked.as_str()
        );
        assert!(finalized["artifact_id"].is_null());
        assert_eq!(finalized["verification"]["status"], "authority_lost");
        assert_eq!(finalized["verification"]["trustworthy"], false);
        assert!(finalized["verification"]["authority_loss_reason"]
            .as_str()
            .is_some_and(|reason| reason.ends_with("outside-product-scope.txt")));
        assert!(store.supervised_patch_artifacts(100).unwrap().is_empty());
        let task = store.get_product_task(task_id).unwrap().unwrap();
        assert_eq!(task["status"], ProductTaskStatus::Blocked.as_str());
        let workspace = store
            .get_supervised_patch_workspace(task["workspace_record_id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(workspace["status"], "quarantined");
        assert_eq!(workspace["verification"]["status"], "authority_lost");
    });
}

#[test]
fn verification_failure_blocks_capture_approval_and_output() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        // Mutation will create fixture file, but verification requires a path that is never written.
        let verify = vec![ProductVerificationCommand {
            command: "test -f docs/this_file_must_not_exist.md".to_string(),
            timeout_ms: 5_000,
        }];
        let validated = validate_intake(
            &intake(&repo, &rev, "g3-verify-fail", verify),
            "local",
            "default",
        )
        .unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        run_scheduler_ticks(&store, run_id);

        let finalized = store
            .finalize_product_task_after_execution(task_id, "tester")
            .expect("finalize");
        assert_eq!(finalized["phase"], "verification_failed");
        assert!(finalized["artifact_id"].is_null() || finalized.get("artifact_id").is_none());
        assert_eq!(
            finalized["task"]["status"].as_str(),
            Some(ProductTaskStatus::Failed.as_str())
        );
        let verification = &finalized["verification"];
        assert_eq!(verification["status"], "verification_failed");
        assert_eq!(verification["trustworthy"], false);
        let attempts = verification["verification_attempts"].as_array().unwrap();
        assert!(!attempts.is_empty());
        assert_ne!(attempts[0]["result_status"], "completed");
        assert_ne!(attempts[0]["exit_status"], 0);

        // No approval / output effect.
        let err = store
            .approve_and_output_product_task(task_id, "tester", true)
            .unwrap_err();
        assert!(
            err.contains("awaiting_approval") || err.contains("requires"),
            "approval must be blocked: {err}"
        );

        // Workspace verification record is failed, not fabricated pass.
        let ws_id = finalized["task"]["workspace_record_id"].as_str().unwrap();
        let ws = store
            .get_supervised_patch_workspace(ws_id)
            .unwrap()
            .unwrap();
        assert_eq!(ws["verification"]["status"], "verification_failed");
        assert_eq!(ws["verification"]["trustworthy"], false);

        // Target main unchanged.
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "hello\n"
        );
    });
}

#[test]
fn capture_without_verification_is_rejected_for_approval() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "g3-no-verify-approve", pass_verify()),
            "local",
            "default",
        )
        .unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        // Force awaiting_approval-like attempt without verification by trying approve early.
        let err = store
            .approve_and_output_product_task(task_id, "tester", true)
            .unwrap_err();
        assert!(
            err.contains("awaiting_approval") || err.contains("requires"),
            "{err}"
        );
    });
}

#[test]
fn workspace_missing_during_verification_fails_closed() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "g3-ws-gone", pass_verify()),
            "local",
            "default",
        )
        .unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        run_scheduler_ticks(&store, run_id);
        let ws_path = compiled["task"]["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap()
            .to_string();
        std::fs::remove_dir_all(&ws_path).ok();
        let err = store
            .finalize_product_task_after_execution(task_id, "tester")
            .unwrap_err();
        assert!(
            err.contains("worktree missing") || err.contains("workspace") || err.contains("zero"),
            "{err}"
        );
    });
}
