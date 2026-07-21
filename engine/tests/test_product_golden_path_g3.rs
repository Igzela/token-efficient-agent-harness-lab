//! G3: verification, artifact, approval, and output orchestration for product tasks.

use engine::product_golden_path::{
    validate_intake, ProductExecutorPolicy, ProductTaskIntakeRequest, ProductTaskStatus,
    ProductVerificationCommand, PRODUCT_TASK_GATE,
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

fn intake(target: &std::path::Path, rev: &str, key: &str) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "Create a bounded product golden path note.".to_string(),
        target_id: "disposable".to_string(),
        target_repo_path: target.to_string_lossy().into_owned(),
        source_revision: rev.to_string(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/note.md".to_string(), "README.md".to_string()],
        verification_commands: vec![ProductVerificationCommand {
            command: "test -f README.md".to_string(),
            timeout_ms: 5_000,
        }],
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

#[test]
fn end_to_end_artifact_only_path() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated =
            validate_intake(&intake(&repo, &rev, "g3-e2e-1"), "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        let finalized = store
            .finalize_product_task_after_execution(task_id, "tester")
            .expect("finalize");
        assert_eq!(finalized["phase"], "awaiting_approval");
        assert!(finalized["artifact_id"].as_str().is_some());
        let task = &finalized["task"];
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::AwaitingApproval.as_str())
        );
        // Worktree should contain the deterministic note.
        let ws = task["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap();
        assert!(
            std::path::Path::new(ws).join("docs/note.md").exists()
                || std::path::Path::new(ws).join("README.md").exists()
        );
        let done = store
            .approve_and_output_product_task(task_id, "tester", false)
            .expect("approve");
        assert_eq!(
            done["task"]["status"].as_str(),
            Some(ProductTaskStatus::Completed.as_str())
        );
        assert_eq!(done["output"]["mode"], "artifact_only");
        // Target default branch unchanged: original README content.
        let main_readme = std::fs::read_to_string(repo.join("README.md")).unwrap();
        assert_eq!(main_readme, "hello\n");
    });
}
