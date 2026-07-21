//! Recovery, concurrency, and fail-closed matrix for product golden path.

use engine::product_golden_path::{
    validate_intake, ProductExecutorPolicy, ProductTaskIntakeRequest, ProductTaskStatus,
    ProductVerificationCommand, PRODUCT_TASK_GATE,
};
use engine::storage::local_product_store::LocalProductStore;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

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
        &["config", "user.email", "rec@example.com"][..],
        &["config", "user.name", "Recovery"][..],
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

fn intake(target: &std::path::Path, rev: &str, key: &str) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "recovery matrix fixture task".to_string(),
        target_id: "disposable".to_string(),
        target_repo_path: target.to_string_lossy().into_owned(),
        source_revision: rev.to_string(),
        source_tree_hash: None,
        allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
        verification_commands: vec![ProductVerificationCommand {
            command: "test -f docs/product_golden_path_fixture.md".to_string(),
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

fn admit(store: &LocalProductStore, repo: &std::path::Path, rev: &str, key: &str) -> String {
    let validated = validate_intake(&intake(repo, rev, key), "local", "default").unwrap();
    let task = store.admit_product_task(&validated, "tester").unwrap();
    task["task_id"].as_str().unwrap().to_string()
}

fn compile(store: &LocalProductStore, task_id: &str) -> serde_json::Value {
    store
        .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
        .unwrap()
}

fn complete_run(store: &LocalProductStore, run_id: &str) {
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
}

#[test]
fn duplicate_intake_is_idempotent() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let a = admit(&store, &repo, &rev, "rec-dup-1");
        let b = admit(&store, &repo, &rev, "rec-dup-1");
        assert_eq!(a, b);
    });
}

#[test]
fn concurrent_duplicate_intake_one_effect() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = Arc::new(
            validate_intake(&intake(&repo, &rev, "rec-concurrent-1"), "local", "default").unwrap(),
        );
        let mut handles = Vec::new();
        for _ in 0..4 {
            let store = store.clone();
            let validated = validated.clone();
            handles.push(thread::spawn(move || {
                store.admit_product_task(&validated, "tester")
            }));
        }
        let mut ids = Vec::new();
        for h in handles {
            let task = h.join().unwrap().expect("admit");
            ids.push(task["task_id"].as_str().unwrap().to_string());
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 1, "concurrent intake must collapse to one task");
    });
}

#[test]
fn restart_after_graph_ready_is_idempotent_compile() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task_id = admit(&store, &repo, &rev, "rec-restart-graph");
        let first = compile(&store, &task_id);
        let second = compile(&store, &task_id);
        assert_eq!(second["reused"], true);
        assert_eq!(first["task"]["run_id"], second["task"]["run_id"]);
        assert_eq!(
            second["task"]["status"].as_str(),
            Some(ProductTaskStatus::GraphReady.as_str())
        );
    });
}

#[test]
fn restart_after_awaiting_approval_reuses_finalize() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task_id = admit(&store, &repo, &rev, "rec-restart-approve");
        let compiled = compile(&store, &task_id);
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        complete_run(&store, run_id);
        let first = store
            .finalize_product_task_after_execution(&task_id, "tester")
            .unwrap();
        assert_eq!(first["phase"], "awaiting_approval");
        let second = store
            .finalize_product_task_after_execution(&task_id, "tester")
            .unwrap();
        assert_eq!(second["reused"], true);
        assert_eq!(
            second["task"]["status"].as_str(),
            Some(ProductTaskStatus::AwaitingApproval.as_str())
        );
    });
}

#[test]
fn stale_approval_blocked_without_trustworthy_verification() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task_id = admit(&store, &repo, &rev, "rec-stale-approval");
        // Graph ready without verification evidence.
        compile(&store, &task_id);
        let err = store
            .approve_and_output_product_task(&task_id, "tester", false)
            .unwrap_err();
        assert!(
            err.contains("awaiting_approval")
                || err.contains("requires")
                || err.contains("blocked"),
            "{err}"
        );
    });
}

#[test]
fn finalize_idempotent_after_completion() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task_id = admit(&store, &repo, &rev, "rec-complete-idem");
        let compiled = compile(&store, &task_id);
        complete_run(&store, compiled["task"]["run_id"].as_str().unwrap());
        store
            .finalize_product_task_after_execution(&task_id, "tester")
            .unwrap();
        store
            .approve_and_output_product_task(&task_id, "tester", false)
            .unwrap();
        let again = store
            .approve_and_output_product_task(&task_id, "tester", false)
            .unwrap();
        assert_eq!(again["reused"], true);
        assert_eq!(
            again["task"]["status"].as_str(),
            Some(ProductTaskStatus::Completed.as_str())
        );
    });
}

#[test]
fn verification_records_all_commands_even_when_first_fails() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let mut req = intake(&repo, &rev, "rec-multi-verify");
        req.verification_commands = vec![
            ProductVerificationCommand {
                command: "test -f docs/missing_a.md".to_string(),
                timeout_ms: 5_000,
            },
            ProductVerificationCommand {
                command: "test -f docs/missing_b.md".to_string(),
                timeout_ms: 5_000,
            },
        ];
        let validated = validate_intake(&req, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = compile(&store, task_id);
        complete_run(&store, compiled["task"]["run_id"].as_str().unwrap());
        let finalized = store
            .finalize_product_task_after_execution(task_id, "tester")
            .unwrap();
        assert_eq!(finalized["phase"], "verification_failed");
        let attempts = finalized["verification"]["verification_attempts"]
            .as_array()
            .unwrap();
        assert_eq!(attempts.len(), 2);
    });
}
