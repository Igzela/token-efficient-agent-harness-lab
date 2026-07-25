//! G1 focused tests: canonical product task intake and worktree-first binding.

use engine::product_golden_path::{
    product_gate_enabled, validate_intake, ProductExecutorPolicy, ProductTaskIntakeRequest,
    ProductTaskStatus, ProductVerificationCommand, PRODUCT_TASK_GATE,
};
use engine::storage::local_product_store::LocalProductStore;
use std::path::PathBuf;
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
    run_git(root, &["config", "user.email", "g1@example.com"]);
    run_git(root, &["config", "user.name", "G1 Tester"]);
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", "init"]);
    let rev = run_git(root, &["rev-parse", "HEAD"]);
    rev.trim().to_string()
}

fn run_git(cwd: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git");
    if !output.status.success() {
        panic!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn sample_intake(target: &std::path::Path, rev: &str, key: &str) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "Add a short docs note for golden path acceptance.".to_string(),
        target_id: "disposable-target".to_string(),
        target_repo_path: target.to_string_lossy().into_owned(),
        source_revision: rev.to_string(),
        source_tree_hash: None,
        allowed_paths: vec!["README.md".to_string(), "docs/note.md".to_string()],
        verification_commands: vec![ProductVerificationCommand {
            command: "test -f README.md".to_string(),
            timeout_ms: 5_000,
        }],
        output_intent: "artifact_only".to_string(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["deterministic".to_string()],
            prefer: Some("deterministic".to_string()),
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
fn schema_includes_product_tasks_at_v30() {
    let (_dir, store) = temp_store();
    assert_eq!(store.schema_version().unwrap(), 34);
}

#[test]
fn gate_defaults_off() {
    let _guard = env_lock().lock().unwrap();
    std::env::remove_var(PRODUCT_TASK_GATE);
    assert!(!product_gate_enabled());
}

#[test]
fn admit_binds_worktree_before_execution() {
    with_gates(|| {
        let (store_dir, store) = temp_store();
        let repo = store_dir.path().join("target-repo");
        let rev = init_git_repo(&repo);
        let intake = sample_intake(&repo, &rev, "idem-success-1");
        let validated = validate_intake(&intake, "local", "default").expect("validate");
        let task = store
            .admit_product_task(&validated, "tester")
            .expect("admit");
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::WorkspaceBound.as_str())
        );
        assert_eq!(task["execution_admitted"].as_bool(), Some(false));
        assert!(task["workspace_record_id"].as_str().is_some());
        let binding = task.get("workspace_binding").expect("binding");
        let ws_path = binding["workspace_path"].as_str().expect("path");
        assert!(PathBuf::from(ws_path).is_dir());
        assert_eq!(binding["workspace_mode"].as_str(), Some("git_worktree"));
        assert_eq!(binding["source_revision"].as_str(), Some(rev.as_str()));
        // No real workflow run was created for execution.
        assert!(task["plan_id"].is_null() || task["plan_id"].as_str().is_none());
    });
}

#[test]
fn duplicate_idempotency_returns_same_task() {
    with_gates(|| {
        let (store_dir, store) = temp_store();
        let repo = store_dir.path().join("target-repo");
        let rev = init_git_repo(&repo);
        let intake = sample_intake(&repo, &rev, "idem-dup-1");
        let validated = validate_intake(&intake, "local", "default").unwrap();
        let first = store.admit_product_task(&validated, "tester").unwrap();
        let second = store.admit_product_task(&validated, "tester").unwrap();
        assert_eq!(first["task_id"], second["task_id"]);
        assert_eq!(first["workspace_record_id"], second["workspace_record_id"]);
    });
}

#[test]
fn idempotency_conflict_on_different_contract() {
    with_gates(|| {
        let (store_dir, store) = temp_store();
        let repo = store_dir.path().join("target-repo");
        let rev = init_git_repo(&repo);
        let mut intake = sample_intake(&repo, &rev, "idem-conflict-1");
        let validated = validate_intake(&intake, "local", "default").unwrap();
        store.admit_product_task(&validated, "tester").unwrap();
        intake.objective = "Completely different objective text.".to_string();
        let validated2 = validate_intake(&intake, "local", "default").unwrap();
        let err = store.admit_product_task(&validated2, "tester").unwrap_err();
        assert!(err.contains("idempotency key already bound"));
    });
}

#[test]
fn rejects_missing_target_repo() {
    with_gates(|| {
        let (_store_dir, store) = temp_store();
        let intake = sample_intake(
            std::path::Path::new("/tmp/does-not-exist-g1-target"),
            "deadbeef",
            "idem-missing-1",
        );
        let validated = validate_intake(&intake, "local", "default").unwrap();
        let err = store.admit_product_task(&validated, "tester").unwrap_err();
        assert!(
            err.contains("not a directory") || err.contains("prepare_git_worktree"),
            "unexpected error: {err}"
        );
        let task = store
            .get_product_task_by_idempotency("local", "default", "idem-missing-1")
            .unwrap()
            .expect("task reserved");
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::Failed.as_str())
        );
        assert_eq!(task["execution_admitted"].as_bool(), Some(false));
    });
}

#[test]
fn rejects_stale_source_revision() {
    with_gates(|| {
        let (store_dir, store) = temp_store();
        let repo = store_dir.path().join("target-repo");
        let _rev = init_git_repo(&repo);
        let intake = sample_intake(
            &repo,
            "0123456789abcdef0123456789abcdef01234567",
            "idem-stale-rev",
        );
        let validated = validate_intake(&intake, "local", "default").unwrap();
        let err = store.admit_product_task(&validated, "tester").unwrap_err();
        assert!(
            err.contains("prepare_git_worktree")
                || err.contains("rev-parse")
                || err.contains("revision"),
            "unexpected: {err}"
        );
    });
}

#[test]
fn rejects_path_escape_allowed_paths() {
    with_gates(|| {
        let mut intake = sample_intake(std::path::Path::new("/tmp/x"), "abc", "idem-escape");
        intake.allowed_paths = vec!["../../etc/passwd".to_string()];
        assert!(validate_intake(&intake, "local", "default").is_err());
    });
}

#[test]
fn rejects_tenant_scope_mismatch() {
    with_gates(|| {
        let mut intake = sample_intake(std::path::Path::new("/tmp/x"), "abc", "idem-tenant");
        intake.tenant_id = Some("other-tenant".to_string());
        let err = validate_intake(&intake, "local", "default").unwrap_err();
        assert!(err.contains("tenant_id"));
    });
}

#[test]
fn rejects_noop_only_executors() {
    with_gates(|| {
        let mut intake = sample_intake(std::path::Path::new("/tmp/x"), "abc", "idem-noop");
        intake.executor_policy = ProductExecutorPolicy {
            allowed_executors: vec!["noop".to_string()],
            prefer: None,
        };
        assert!(validate_intake(&intake, "local", "default").is_err());
    });
}

#[test]
fn rejects_absolute_verification_binary() {
    with_gates(|| {
        let mut intake = sample_intake(std::path::Path::new("/tmp/x"), "abc", "idem-bin");
        intake.verification_commands = vec![ProductVerificationCommand {
            command: "/bin/sh -c evil".to_string(),
            timeout_ms: 1000,
        }];
        assert!(validate_intake(&intake, "local", "default").is_err());
    });
}

#[test]
fn empty_v30_rollback_works() {
    let (_dir, store) = temp_store();
    assert_eq!(store.schema_version().unwrap(), 34);
    store.rollback_v34_to_v33("tester", true).unwrap();
    store.rollback_v33_to_v32("tester", true).unwrap();
    store
        .rollback_v32_to_v31("tester", true)
        .expect("empty v32 rollback");
    store
        .rollback_v31_to_v30("tester", true)
        .expect("empty v31 rollback");
    store
        .rollback_v30_to_v29("tester", true)
        .expect("empty rollback");
    assert_eq!(store.schema_version().unwrap(), 29);
}

#[test]
fn occupied_v30_rollback_blocked() {
    with_gates(|| {
        let (store_dir, store) = temp_store();
        let repo = store_dir.path().join("target-repo");
        let rev = init_git_repo(&repo);
        let intake = sample_intake(&repo, &rev, "idem-rollback-block");
        let validated = validate_intake(&intake, "local", "default").unwrap();
        store.admit_product_task(&validated, "tester").unwrap();
        store.rollback_v34_to_v33("tester", true).unwrap();
        store.rollback_v33_to_v32("tester", true).unwrap();
        store
            .rollback_v32_to_v31("tester", true)
            .expect("empty managed acceptance rollback");
        store
            .rollback_v31_to_v30("tester", true)
            .expect("empty terminal evidence rollback");
        let err = store.rollback_v30_to_v29("tester", true).unwrap_err();
        assert!(err.contains("blocked"));
        assert_eq!(store.schema_version().unwrap(), 30);
    });
}

#[test]
fn workspace_bound_task_does_not_admit_execution() {
    with_gates(|| {
        let (store_dir, store) = temp_store();
        let repo = store_dir.path().join("target-repo");
        let rev = init_git_repo(&repo);
        let intake = sample_intake(&repo, &rev, "idem-no-exec");
        let validated = validate_intake(&intake, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        assert!(!ProductTaskStatus::WorkspaceBound.admits_execution());
        assert_eq!(task["execution_admitted"], false);
    });
}
