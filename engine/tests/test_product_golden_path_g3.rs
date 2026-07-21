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
    let _guard = env_lock().lock().unwrap();
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

        let validated = validate_intake(
            &intake(&repo, &rev, "g3-e2e-1", pass_verify()),
            "local",
            "default",
        )
        .unwrap();
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
        assert!(finalized["artifact_id"].as_str().is_some());
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
        let ws = task["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap();
        let note = std::path::Path::new(ws).join("docs/product_golden_path_fixture.md");
        assert!(note.exists(), "exact fixture path must exist after apply");
        assert_eq!(
            std::fs::read_to_string(&note).unwrap(),
            FIXTURE_DETERMINISTIC_NOTE_CONTENT
        );

        let done = store
            .approve_and_output_product_task(task_id, "tester", false)
            .expect("approve");
        assert_eq!(
            done["task"]["status"].as_str(),
            Some(ProductTaskStatus::Completed.as_str())
        );
        assert_eq!(done["output"]["mode"], "artifact_only");

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
            .approve_and_output_product_task(task_id, "tester", false)
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
            .approve_and_output_product_task(task_id, "tester", false)
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
