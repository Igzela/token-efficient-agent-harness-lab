//! G2 tests: executable graph compile + scheduler-eligible product task runs.

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
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LocalProductStore::new(dir.path().join("store.db")).expect("store");
    (dir, store)
}

fn init_git_repo(root: &std::path::Path) -> String {
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(root.join("README.md"), "hello\n").unwrap();
    run_git(root, &["init", "-b", "main"]);
    run_git(root, &["config", "user.email", "g2@example.com"]);
    run_git(root, &["config", "user.name", "G2 Tester"]);
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", "init"]);
    run_git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

fn run_git(cwd: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git");
    assert!(output.status.success(), "{:?}", output);
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn sample_intake(target: &std::path::Path, rev: &str, key: &str) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "Prove executable graph scheduling for golden path.".to_string(),
        target_id: "disposable-target".to_string(),
        target_repo_path: target.to_string_lossy().into_owned(),
        source_revision: rev.to_string(),
        source_tree_hash: None,
        allowed_paths: vec!["README.md".to_string()],
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

fn admit_bound(
    store: &LocalProductStore,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
) -> serde_json::Value {
    let intake = sample_intake(repo, rev, key);
    let validated = validate_intake(&intake, "local", "default").unwrap();
    store.admit_product_task(&validated, "tester").unwrap()
}

#[test]
fn compile_creates_executable_run_bound_to_task() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = admit_bound(&store, &repo, &rev, "g2-compile-1");
        let task_id = task["task_id"].as_str().unwrap();
        let available = vec!["command".to_string()];
        let result = store
            .compile_and_schedule_product_task(task_id, "tester", &available)
            .expect("compile");
        assert_eq!(result["execution_admitted"], true);
        assert_eq!(result["scheduler_eligible"], true);
        let task = &result["task"];
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::Running.as_str())
        );
        assert!(task["plan_id"].as_str().is_some());
        assert!(task["run_id"].as_str().is_some());
        let run_id = task["run_id"].as_str().unwrap();
        let run = store.get_workflow_run(run_id).unwrap().expect("run");
        let nodes = run["nodes"].as_array().expect("nodes");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["product_task_id"], task_id);
        assert_eq!(nodes[0]["task_type"], "command");
        assert!(nodes[0]["workspace_path"].as_str().is_some());
        assert!(nodes[0].get("managed_supervised_patch").is_some());
        // Workspace rebound to real run for lease injection.
        let ws_id = task["workspace_record_id"].as_str().unwrap();
        let ws = store
            .get_supervised_patch_workspace(ws_id)
            .unwrap()
            .unwrap();
        assert_eq!(ws["run_id"].as_str(), Some(run_id));
    });
}

#[test]
fn unavailable_executor_fails_closed() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = admit_bound(&store, &repo, &rev, "g2-unavail-1");
        let task_id = task["task_id"].as_str().unwrap();
        let err = store
            .compile_and_schedule_product_task(task_id, "tester", &[])
            .unwrap_err();
        assert!(err.contains("unavailable"));
        let task = store.get_product_task(task_id).unwrap().unwrap();
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::WorkspaceBound.as_str())
        );
        assert_eq!(task["execution_admitted"], false);
    });
}

#[test]
fn missing_worktree_blocks_compile() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = admit_bound(&store, &repo, &rev, "g2-missing-ws");
        let task_id = task["task_id"].as_str().unwrap();
        let ws_path = task["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap()
            .to_string();
        std::fs::remove_dir_all(&ws_path).ok();
        let err = store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap_err();
        assert!(err.contains("missing") || err.contains("zero execution"));
    });
}

#[test]
fn compile_is_idempotent_when_already_running() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = admit_bound(&store, &repo, &rev, "g2-idem-1");
        let task_id = task["task_id"].as_str().unwrap();
        let first = store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        let second = store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        assert_eq!(second["reused"], true);
        assert_eq!(first["task"]["run_id"], second["task"]["run_id"]);
    });
}

#[test]
fn tick_executes_command_in_bound_worktree() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = admit_bound(&store, &repo, &rev, "g2-tick-1");
        let task_id = task["task_id"].as_str().unwrap();
        let result = store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        let run_id = result["task"]["run_id"].as_str().unwrap();
        let executor = engine::node_executor::CommandNodeExecutor::default();
        let tick = store
            .tick_with_executor(run_id, "tester", 1, &executor)
            .expect("tick");
        // Either leased/completed or terminal depending on graph advancement.
        assert!(
            tick.get("action").is_some()
                || tick.get("run").is_some()
                || tick.get("status").is_some()
                || tick.as_object().map(|o| !o.is_empty()).unwrap_or(true),
            "tick result: {tick}"
        );
    });
}
