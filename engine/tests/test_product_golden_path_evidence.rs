//! Terminal evidence and export_patch path for product golden path.

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
    let _guard = env_lock().lock().unwrap_or_else(|p| p.into_inner());
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
        &["config", "user.email", "ev@example.com"][..],
        &["config", "user.name", "Evidence"][..],
        &["add", "README.md"][..],
        &["commit", "-m", "init"][..],
    ] {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(out.status.success());
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
    intent: &str,
) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "evidence path fixture".to_string(),
        target_id: "disposable".to_string(),
        target_repo_path: target.to_string_lossy().into_owned(),
        source_revision: rev.to_string(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
        verification_commands: vec![ProductVerificationCommand {
            command: "test -f docs/product_golden_path_fixture.md".to_string(),
            timeout_ms: 5_000,
        }],
        output_intent: intent.to_string(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["command".to_string()],
            prefer: Some("command".to_string()),
        },
        budget: None,
        risk_class: "low".to_string(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: Some(true),
        idempotency_key: key.to_string(),
        expected_version: None,
        tenant_id: Some("local".to_string()),
        workspace_id: Some("default".to_string()),
        workspace_mode: Some("git_worktree".to_string()),
    }
}

fn complete_to_approval(store: &LocalProductStore, task_id: &str) {
    let compiled = store
        .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
        .unwrap();
    let run_id = compiled["task"]["run_id"].as_str().unwrap();
    let executor = engine::node_executor::CommandNodeExecutor::default();
    for _ in 0..8 {
        let tick = store
            .tick_with_executor(run_id, "tester", 1, &executor)
            .unwrap();
        let status = tick
            .pointer("/run/status")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if matches!(status, "completed" | "failed") {
            break;
        }
    }
    store
        .finalize_product_task_after_execution(task_id, "tester")
        .unwrap();
}

#[test]
fn terminal_evidence_links_task_owners_without_fabricated_cost() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "ev-terminal-1", "artifact_only"),
            "local",
            "default",
        )
        .unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        complete_to_approval(&store, task_id);
        let done = store
            .approve_and_output_product_task(task_id, "tester", false)
            .unwrap();
        assert_eq!(
            done["task"]["status"].as_str(),
            Some(ProductTaskStatus::Completed.as_str())
        );
        let evidence = done["terminal_evidence"].as_object().expect("evidence");
        assert_eq!(evidence["product_task_id"], task_id);
        assert!(evidence["run_id"].as_str().is_some());
        assert!(evidence["workspace_record_id"].as_str().is_some());
        assert!(evidence["artifact_id"].as_str().is_some());
        assert!(evidence["approval_id"].as_str().is_some());
        assert_eq!(evidence["verification_trustworthy"], true);
        assert_eq!(evidence["usage"]["status"], "unavailable");
        assert_eq!(evidence["cost"]["status"], "unavailable");
        assert!(evidence["usage"]["reason"].as_str().is_some());
        assert!(evidence["cost"]["reason"].as_str().is_some());
        // Idempotent re-read
        let again = store.get_product_task_terminal_evidence(task_id).unwrap();
        assert_eq!(again["product_task_id"], task_id);
        assert_eq!(again["run_id"], evidence["run_id"]);
        // Target main unchanged
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "hello\n"
        );
    });
}

#[test]
fn export_patch_writes_approved_patch_without_touching_main() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "ev-export-1", "export_patch"),
            "local",
            "default",
        )
        .unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        complete_to_approval(&store, task_id);
        let done = store
            .approve_and_output_product_task(task_id, "tester", true)
            .expect("export");
        assert_eq!(done["output"]["mode"], "export_patch");
        assert_eq!(done["output"]["status"], "exported");
        let export_path = done["output"]["export_path"].as_str().unwrap();
        let patch = std::fs::read_to_string(export_path).unwrap();
        assert!(
            patch.contains("product_golden_path_fixture")
                || patch.contains("diff")
                || !patch.is_empty(),
            "export patch must be non-empty: {patch}"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "hello\n"
        );
        assert!(!repo.join("docs/product_golden_path_fixture.md").exists());
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), rev);
    });
}

#[test]
fn draft_pr_without_network_gate_is_explicitly_unavailable() {
    with_gates(|| {
        std::env::remove_var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT");
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "ev-draft-1", "draft_pr"),
            "local",
            "default",
        )
        .unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        complete_to_approval(&store, task_id);
        let done = store
            .approve_and_output_product_task(task_id, "tester", true)
            .expect("draft unavailable path");
        assert_eq!(done["output"]["mode"], "draft_pr");
        assert_eq!(done["output"]["status"], "network_output_unavailable");
        assert!(done["output"]["reason"].as_str().is_some());
        assert_eq!(done["output"]["export_eligible"], true);
        // Still completes task with explicit unavailable network output.
        assert_eq!(
            done["task"]["status"].as_str(),
            Some(ProductTaskStatus::Completed.as_str())
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "hello\n"
        );
    });
}

#[test]
fn draft_pr_pushes_acp_branch_to_local_origin_without_touching_main() {
    with_gates(|| {
        std::env::set_var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT", "1");
        std::env::set_var("ACP_TARGET_REPO_ALLOW_LOCAL_REMOTE", "1");
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let bare = dir.path().join("origin.git");
        let rev = init_git_repo(&repo);
        let out = Command::new("git")
            .args(["init", "--bare", "-b", "main"])
            .arg(&bare)
            .output()
            .unwrap();
        assert!(out.status.success(), "{:?}", out);
        let out = Command::new("git")
            .args(["remote", "add", "origin"])
            .arg(&bare)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(out.status.success(), "{:?}", out);
        let out = Command::new("git")
            .args(["push", "-u", "origin", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(out.status.success(), "{:?}", out);

        let validated = validate_intake(
            &intake(&repo, &rev, "ev-acp-push-1", "draft_pr"),
            "local",
            "default",
        )
        .unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        complete_to_approval(&store, task_id);
        let done = store
            .approve_and_output_product_task(task_id, "tester", true)
            .expect("acp push");
        assert_eq!(done["output"]["mode"], "draft_pr");
        assert_eq!(done["output"]["status"], "branch_pushed");
        let branch = done["output"]["branch_name"].as_str().unwrap();
        assert!(branch.starts_with("acp/"), "branch must be acp/*: {branch}");
        let refs = Command::new("git")
            .args(["show-ref"])
            .current_dir(&bare)
            .output()
            .unwrap();
        let refs_txt = String::from_utf8_lossy(&refs.stdout);
        assert!(
            refs_txt.contains("acp/"),
            "bare remote missing acp branch: {refs_txt}"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "hello\n"
        );
        let head = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), rev);
        let main_ref = Command::new("git")
            .args(["rev-parse", "refs/heads/main"])
            .current_dir(&bare)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&main_ref.stdout).trim(), rev);

        std::env::remove_var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT");
        std::env::remove_var("ACP_TARGET_REPO_ALLOW_LOCAL_REMOTE");
    });
}
