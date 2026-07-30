//! Recovery, concurrency, and fail-closed matrix for product golden path.

use engine::node_executor::{
    NodeExecutionInput, NodeExecutionOutput, NodeExecutor, ProcessOutcome,
};
use engine::product_golden_path::{
    validate_intake, ProductExecutorPolicy, ProductTaskBudget, ProductTaskIntakeRequest,
    ProductTaskStatus, ProductVerificationCommand, ProductVerificationRuntimeAuthority,
    PRODUCT_TASK_GATE,
};
use engine::storage::local_product_store::LocalProductStore;
use engine::tool_policy_executor::ToolPolicyNodeExecutor;
#[cfg(unix)]
use std::ffi::OsString;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_gates<R>(f: impl FnOnce() -> R) -> R {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let workspace_root = tempfile::tempdir().unwrap();
    std::env::set_var(PRODUCT_TASK_GATE, "1");
    std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
    std::env::set_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", "0");
    std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", workspace_root.path());
    let result = f();
    std::env::remove_var(PRODUCT_TASK_GATE);
    std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
    std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
    result
}

#[cfg(unix)]
struct ScopedEnv {
    previous: Vec<(&'static str, Option<OsString>)>,
}

#[cfg(unix)]
impl ScopedEnv {
    fn set(values: Vec<(&'static str, OsString)>) -> Self {
        let previous = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect::<Vec<_>>();
        for (key, value) in values {
            std::env::set_var(key, value);
        }
        Self { previous }
    }
}

#[cfg(unix)]
impl Drop for ScopedEnv {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..) {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}

#[cfg(unix)]
struct FileReleaseGuard {
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl Drop for FileReleaseGuard {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.path, "release\n");
    }
}

#[cfg(unix)]
fn git_binary_from_path() -> std::path::PathBuf {
    let path = std::env::var_os("PATH").expect("PATH must be set for git fixtures");
    std::env::split_paths(&path)
        .map(|root| root.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git must be available for product fixture")
}

#[cfg(unix)]
fn wait_for_workspace_prepare_lock_contention(store: &LocalProductStore, task_id: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let contended = store
            .audit_events(10_000)
            .expect("read workspace preparation audit events")
            .into_iter()
            .any(|event| {
                event["action"] == "product_task.workspace_prepare_lock_contended"
                    && event["resource"] == task_id
                    && event["details"]["synchronization_only"] == true
                    && event["details"]["authority_owner"] == "product_task"
            });
        if contended {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "recovery did not reach the shared workspace-preparation lock contention boundary"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn temp_store() -> (tempfile::TempDir, LocalProductStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("store.db")).unwrap();
    (dir, store)
}

#[cfg(unix)]
fn workspace_preparation_receipt_sha256(
    task_id: &str,
    workspace_root: &std::path::Path,
    workspace_path: &std::path::Path,
    marker_sha256: &str,
    marker_state: &str,
) -> String {
    use sha2::{Digest, Sha256};

    hex::encode(Sha256::digest(
        serde_json::to_vec(&serde_json::json!({
            "schema_version": "product_task_workspace_preparation.v1",
            "task_id": task_id,
            "workspace_root": workspace_root,
            "workspace_path": workspace_path,
            "marker_sha256": marker_sha256,
            "marker_state": marker_state,
        }))
        .unwrap(),
    ))
}

fn tool_policy_pass_count(store: &LocalProductStore) -> usize {
    store
        .audit_events(100_000)
        .expect("read tool-policy audit events")
        .into_iter()
        .filter(|event| event["action"] == "tool_execution.pre_policy_passed")
        .count()
}

fn wait_for_new_tool_policy_pass(store: &LocalProductStore, baseline: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tool_policy_pass_count(store) > baseline {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "managed verification did not reach the pre-policy execution boundary"
        );
        thread::sleep(Duration::from_millis(20));
    }
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
        .args([
            "remote",
            "add",
            "origin",
            "https://example.invalid/recovery-product.git",
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

fn run_git_head(root: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn git_status_paths(root: &std::path::Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "-uall"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn intake(target: &std::path::Path, rev: &str, key: &str) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "recovery matrix fixture task".to_string(),
        target_id: "disposable".to_string(),
        target_repo_path: target.to_string_lossy().into_owned(),
        source_kind: None,
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

struct OverBudgetManagedExecutor;

impl NodeExecutor for OverBudgetManagedExecutor {
    fn executor_type_name(&self) -> &str {
        "codex_cli"
    }

    fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "codex_cli".to_string(),
            output: Some("bounded managed fixture".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens: Some(41),
            output_tokens: Some(10),
            estimated_cost: None,
            latency_ms: Some(10),
            process_outcome: Some(ProcessOutcome::exited(0)),
            resolved_model: None,
        }
    }
}

struct CumulativeManagedExecutor;

impl NodeExecutor for CumulativeManagedExecutor {
    fn executor_type_name(&self) -> &str {
        "codex_cli"
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        match input
            .node_metadata
            .get("execution_attempt")
            .and_then(serde_json::Value::as_u64)
        {
            Some(1) => NodeExecutionOutput {
                status: "failed".to_string(),
                executor_type: "codex_cli".to_string(),
                output: Some("bounded retry fixture".to_string()),
                error_domain: Some("retryable_fixture_failure".to_string()),
                error_message: Some("retryable fixture".to_string()),
                input_tokens: Some(30),
                output_tokens: Some(10),
                estimated_cost: None,
                latency_ms: Some(10),
                process_outcome: Some(ProcessOutcome::exited(7)),
                resolved_model: None,
            },
            Some(2) => NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "codex_cli".to_string(),
                output: Some("bounded retry fixture".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: Some(15),
                output_tokens: Some(5),
                estimated_cost: None,
                latency_ms: Some(10),
                process_outcome: Some(ProcessOutcome::exited(0)),
                resolved_model: None,
            },
            attempt => panic!("unexpected managed attempt: {attempt:?}"),
        }
    }
}

struct MissingUsageManagedExecutor;

impl NodeExecutor for MissingUsageManagedExecutor {
    fn executor_type_name(&self) -> &str {
        "codex_cli"
    }

    fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "codex_cli".to_string(),
            output: Some("bounded result without authoritative usage".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: Some(10),
            process_outcome: Some(ProcessOutcome::exited(0)),
            resolved_model: None,
        }
    }
}

struct RetryableManagedExecutor;

impl NodeExecutor for RetryableManagedExecutor {
    fn executor_type_name(&self) -> &str {
        "codex_cli"
    }

    fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "failed".to_string(),
            executor_type: "codex_cli".to_string(),
            output: Some("bounded retryable result".to_string()),
            error_domain: Some("retryable_fixture_failure".to_string()),
            error_message: Some("retryable fixture".to_string()),
            input_tokens: Some(10),
            output_tokens: Some(2),
            estimated_cost: None,
            latency_ms: Some(10),
            process_outcome: Some(ProcessOutcome::exited(7)),
            resolved_model: None,
        }
    }
}

struct CountingManagedExecutor {
    calls: Arc<AtomicUsize>,
}

impl NodeExecutor for CountingManagedExecutor {
    fn executor_type_name(&self) -> &str {
        "codex_cli"
    }

    fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
        self.calls.fetch_add(1, Ordering::SeqCst);
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "codex_cli".to_string(),
            output: Some("unexpected second call".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens: Some(1),
            output_tokens: Some(1),
            estimated_cost: None,
            latency_ms: Some(1),
            process_outcome: Some(ProcessOutcome::exited(0)),
            resolved_model: None,
        }
    }
}

fn prepare_cumulative_managed_task(
    store: &LocalProductStore,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
) -> (String, String) {
    let mut request = intake(repo, rev, key);
    request.executor_policy = ProductExecutorPolicy {
        allowed_executors: vec!["codex_cli".to_string()],
        prefer: Some("codex_cli".to_string()),
    };
    request.budget = Some(ProductTaskBudget {
        total_tokens: Some(50),
        total_calls: Some(2),
        max_retries: Some(1),
        max_repairs: Some(0),
        ..ProductTaskBudget::default()
    });
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store.admit_product_task(&validated, "tester").unwrap();
    let task_id = task["task_id"].as_str().unwrap().to_string();
    let compiled = store
        .compile_and_schedule_product_task(&task_id, "tester", &["codex_cli".to_string()])
        .unwrap();
    let run_id = compiled["task"]["run_id"].as_str().unwrap().to_string();
    let first = store
        .tick_with_executor(&run_id, "scheduler", 1, &CumulativeManagedExecutor)
        .unwrap();
    assert_eq!(first["action"], "node_retry");
    let run = store.get_workflow_run(&run_id).unwrap().unwrap();
    assert_eq!(
        run["nodes"][0]["product_managed_usage"]["cumulative_tokens"],
        40
    );
    assert_eq!(run["nodes"][0]["product_managed_usage"]["last_attempt"], 1);
    (task_id, run_id)
}

fn finish_cumulative_managed_task(store: &LocalProductStore, task_id: &str, run_id: &str) {
    let second = store
        .tick_with_executor(run_id, "scheduler", 1, &CumulativeManagedExecutor)
        .unwrap();
    assert_eq!(second["run"]["status"], "failed");
    assert_eq!(
        second["result"]["error_domain"],
        "product_token_budget_exhausted"
    );
    let run = store.get_workflow_run(run_id).unwrap().unwrap();
    assert_eq!(
        run["nodes"][0]["product_managed_usage"]["cumulative_tokens"],
        60
    );
    assert_eq!(run["nodes"][0]["product_managed_usage"]["last_attempt"], 2);
    let finalized = store
        .finalize_product_task_after_execution_with_authority(task_id, "verifier", &|| {
            Ok(running_scheduler_authority())
        })
        .unwrap();
    assert_eq!(finalized["task"]["status"], "budget_exhausted");
    assert!(finalized["verification"].is_null());
    assert!(finalized["artifact_id"].is_null());
}

fn assert_managed_token_budget_exhaustion(
    store: &LocalProductStore,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
) {
    let mut request = intake(repo, rev, key);
    request.executor_policy = ProductExecutorPolicy {
        allowed_executors: vec!["codex_cli".to_string()],
        prefer: Some("codex_cli".to_string()),
    };
    request.budget = Some(ProductTaskBudget {
        total_tokens: Some(50),
        total_calls: Some(1),
        max_retries: Some(0),
        max_repairs: Some(0),
        ..ProductTaskBudget::default()
    });
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store.admit_product_task(&validated, "tester").unwrap();
    let task_id = task["task_id"].as_str().unwrap();
    let compiled = store
        .compile_and_schedule_product_task(task_id, "tester", &["codex_cli".to_string()])
        .unwrap();
    let run_id = compiled["task"]["run_id"].as_str().unwrap();
    let tick = store
        .tick_with_executor(run_id, "scheduler", 1, &OverBudgetManagedExecutor)
        .unwrap();
    assert_eq!(tick["run"]["status"], "failed");
    assert_eq!(
        tick["result"]["error_domain"],
        "product_token_budget_exhausted"
    );
    assert_eq!(tick["result"]["input_tokens"], 41);
    assert_eq!(tick["result"]["output_tokens"], 10);

    let finalized = store
        .finalize_product_task_after_execution_with_authority(task_id, "verifier", &|| {
            Ok(running_scheduler_authority())
        })
        .unwrap();
    assert_eq!(finalized["task"]["status"], "budget_exhausted");
    assert!(matches!(
        finalized["phase"].as_str(),
        Some("terminal_failure" | "execution_failed")
    ));
    assert!(finalized["task"]["artifact_id"].is_null());
    assert!(finalized["verification"].is_null());
}

fn prepare_managed_negative_task(
    store: &LocalProductStore,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
    total_calls: u64,
    max_retries: u64,
) -> (String, String) {
    let mut request = intake(repo, rev, key);
    request.executor_policy = ProductExecutorPolicy {
        allowed_executors: vec!["codex_cli".to_string()],
        prefer: Some("codex_cli".to_string()),
    };
    request.budget = Some(ProductTaskBudget {
        total_tokens: Some(50),
        total_calls: Some(total_calls),
        max_retries: Some(max_retries),
        max_repairs: Some(0),
        ..ProductTaskBudget::default()
    });
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store.admit_product_task(&validated, "tester").unwrap();
    let task_id = task["task_id"].as_str().unwrap().to_string();
    let compiled = store
        .compile_and_schedule_product_task(&task_id, "tester", &["codex_cli".to_string()])
        .unwrap();
    (
        task_id,
        compiled["task"]["run_id"].as_str().unwrap().to_string(),
    )
}

fn assert_no_product_effects(
    store: &LocalProductStore,
    task_id: &str,
    repo: &std::path::Path,
    rev: &str,
) {
    let finalized = store
        .finalize_product_task_after_execution_with_authority(task_id, "verifier", &|| {
            Ok(running_scheduler_authority())
        })
        .unwrap();
    assert!(matches!(
        finalized["task"]["status"].as_str(),
        Some("failed" | "budget_exhausted")
    ));
    assert!(finalized["verification"].is_null());
    assert!(finalized["artifact_id"].is_null());
    assert!(finalized["task"]["artifact_id"].is_null());
    assert_eq!(run_git_head(repo), rev);
    assert!(git_status_paths(repo).is_empty());
}

fn assert_managed_missing_usage_fails_closed(
    store: &LocalProductStore,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
) {
    let (task_id, run_id) = prepare_managed_negative_task(store, repo, rev, key, 1, 0);
    let tick = store
        .tick_with_executor(&run_id, "scheduler", 1, &MissingUsageManagedExecutor)
        .unwrap();
    assert_eq!(tick["run"]["status"], "failed");
    assert_eq!(
        tick["result"]["error_domain"],
        "product_token_usage_unavailable"
    );
    let run = store.get_workflow_run(&run_id).unwrap().unwrap();
    assert_managed_usage_state(&run, "unavailable", 0, 1);
    assert_no_product_effects(store, &task_id, repo, rev);
}

fn assert_managed_call_budget_denies_second_call(
    store: Arc<LocalProductStore>,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
) {
    let (task_id, run_id) = prepare_managed_negative_task(&store, repo, rev, key, 1, 1);
    let first = store
        .tick_with_executor(&run_id, "scheduler", 1, &RetryableManagedExecutor)
        .unwrap();
    assert_eq!(first["action"], "node_retry");

    let calls = Arc::new(AtomicUsize::new(0));
    let policy = ToolPolicyNodeExecutor::cli(
        Arc::new(CountingManagedExecutor {
            calls: Arc::clone(&calls),
        }),
        Arc::clone(&store),
        "codex_cli",
    );
    let second = store
        .tick_with_executor(&run_id, "scheduler", 1, &policy)
        .unwrap();
    assert_eq!(second["run"]["status"], "failed");
    assert_eq!(
        second["result"]["error_domain"],
        "product_call_budget_exhausted"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_no_product_effects(&store, &task_id, repo, rev);
}

fn assert_managed_usage_state(
    run: &serde_json::Value,
    status: &str,
    cumulative: u64,
    attempt: u64,
) {
    assert_eq!(run["nodes"][0]["product_managed_usage"]["status"], status);
    assert_eq!(
        run["nodes"][0]["product_managed_usage"]["cumulative_tokens"],
        cumulative
    );
    assert_eq!(
        run["nodes"][0]["product_managed_usage"]["last_attempt"],
        attempt
    );
}

fn running_scheduler_authority() -> ProductVerificationRuntimeAuthority {
    ProductVerificationRuntimeAuthority {
        scheduler_attached: true,
        scheduler_running: true,
        scheduler_paused: false,
        scheduler_killed: false,
        global_kill_active: false,
        manual_operational_tick: false,
    }
}

fn ready_for_slow_verification(
    store: &LocalProductStore,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
    command: &str,
) -> (String, String, String, String) {
    let mut request = intake(repo, rev, key);
    request.verification_commands = vec![ProductVerificationCommand {
        command: command.to_string(),
        timeout_ms: 700,
    }];
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store.admit_product_task(&validated, "tester").unwrap();
    let task_id = task["task_id"].as_str().unwrap().to_string();
    let compiled = compile(store, &task_id);
    let run_id = compiled["task"]["run_id"].as_str().unwrap().to_string();
    complete_run(store, &run_id);
    let current = store.get_product_task(&task_id).unwrap().unwrap();
    (
        task_id,
        run_id,
        current["workspace_record_id"].as_str().unwrap().to_string(),
        current["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap()
            .to_string(),
    )
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
        let ids = handles
            .into_iter()
            .map(|handle| {
                let task = handle
                    .join()
                    .expect("concurrent admit thread must not panic")
                    .expect("concurrent duplicate intake must wait for the canonical task");
                task["task_id"].as_str().unwrap().to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), 4);
        assert!(
            ids.iter().all(|task_id| task_id == &ids[0]),
            "concurrent intake must collapse to one task: {ids:?}"
        );
        let final_task = store
            .get_product_task_by_idempotency("local", "default", "rec-concurrent-1")
            .unwrap()
            .expect("idempotent task");
        assert_eq!(final_task["task_id"].as_str().unwrap(), ids[0]);
        assert_eq!(
            final_task["status"].as_str(),
            Some(ProductTaskStatus::WorkspaceBound.as_str())
        );
    });
}

#[cfg(unix)]
#[test]
fn recovery_waits_for_active_worktree_prepare() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = Arc::new(
            validate_intake(
                &intake(&repo, &rev, "rec-recover-race-1"),
                "local",
                "default",
            )
            .unwrap(),
        );

        let wrapper_dir = dir.path().join("git-wrapper");
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let worktree_add_started = dir.path().join("worktree-add-started");
        let worktree_add_release = dir.path().join("worktree-add-release");
        let worktree_add_log = dir.path().join("worktree-add.log");
        let _release_guard = FileReleaseGuard {
            path: worktree_add_release.clone(),
        };
        let real_git = git_binary_from_path();
        let shell_quote = |path: &std::path::Path| {
            format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
        };
        let wrapper = wrapper_dir.join("git");
        std::fs::write(
            &wrapper,
            format!(
                "#!/bin/sh\n\
previous=''\n\
for arg in \"$@\"; do\n\
  if [ \"$previous\" = \"worktree\" ] && [ \"$arg\" = \"add\" ]; then\n\
    : > {started}\n\
    printf '%s\\n' worktree-add >> {log}\n\
    while [ ! -f {release} ]; do\n\
      sleep 0.01\n\
    done\n\
    break\n\
  fi\n\
  previous=\"$arg\"\n\
done\n\
exec {git} \"$@\"\n",
                started = shell_quote(&worktree_add_started),
                log = shell_quote(&worktree_add_log),
                release = shell_quote(&worktree_add_release),
                git = shell_quote(&real_git),
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&wrapper).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&wrapper, permissions).unwrap();

        let old_path = std::env::var_os("PATH").expect("PATH must be set for git fixtures");
        let mut path_entries = vec![wrapper_dir];
        path_entries.extend(std::env::split_paths(&old_path));
        let _env = ScopedEnv::set(vec![("PATH", std::env::join_paths(path_entries).unwrap())]);

        let admitting_store = store.clone();
        let admitting_intake = validated.clone();
        let admit =
            thread::spawn(move || admitting_store.admit_product_task(&admitting_intake, "tester"));

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !worktree_add_started.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "admit did not reach the delayed worktree preparation boundary",
            );
            thread::sleep(Duration::from_millis(10));
        }
        let preparation_deadline = std::time::Instant::now() + Duration::from_secs(1);
        while std::fs::read_to_string(&worktree_add_log)
            .unwrap_or_default()
            .lines()
            .count()
            == 0
        {
            assert!(
                std::time::Instant::now() < preparation_deadline,
                "admit did not enter the delayed git worktree add"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let preparing_task = store
            .get_product_task_by_idempotency("local", "default", "rec-recover-race-1")
            .unwrap()
            .expect("reserved task");
        assert_eq!(
            preparing_task["status"].as_str(),
            Some(ProductTaskStatus::WorkspacePreparing.as_str())
        );
        let task_id = preparing_task
            .get("task_id")
            .and_then(|value| value.as_str())
            .expect("task_id")
            .to_string();

        let recovering_store = store.clone();
        let recovering_task_id = task_id.clone();
        let recover = thread::spawn(move || {
            recovering_store.recover_product_task_workspace(&recovering_task_id, "tester")
        });
        wait_for_workspace_prepare_lock_contention(&store, &task_id);

        let worktree_add_count = std::fs::read_to_string(&worktree_add_log)
            .unwrap_or_default()
            .lines()
            .count();
        assert_eq!(
            worktree_add_count, 1,
            "recovery must contend on the shared lock before a second git worktree add"
        );
        std::fs::write(&worktree_add_release, "release\n").unwrap();

        let admitted = admit.join().unwrap().expect("admit must succeed");
        let recovered = recover.join().unwrap().expect("recovery must succeed");
        let completed_worktree_add_count = std::fs::read_to_string(&worktree_add_log)
            .unwrap_or_default()
            .lines()
            .count();
        assert_eq!(
            completed_worktree_add_count, 1,
            "recovery must not enter a second git worktree add while admit owns preparation"
        );
        assert_eq!(admitted["task_id"], task_id);
        assert_eq!(recovered["task_id"], task_id);
        assert_eq!(
            recovered["status"].as_str(),
            Some(ProductTaskStatus::WorkspaceBound.as_str())
        );
    });
}

#[cfg(unix)]
#[test]
fn timed_out_worktree_add_requires_reconciliation_and_reuses_pinned_receipt() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "rec-worktree-add-timeout-1"),
            "local",
            "default",
        )
        .unwrap();

        let wrapper_dir = dir.path().join("git-wrapper");
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let real_git = git_binary_from_path();
        let shell_quote = |path: &std::path::Path| {
            format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
        };
        let wrapper = wrapper_dir.join("git");
        std::fs::write(
            &wrapper,
            format!(
                r#"#!/bin/sh
previous=''
for arg in "$@"; do
  if [ "$previous" = "worktree" ] && [ "$arg" = "add" ]; then
    {git} "$@"
    status=$?
    while :; do :; done
    exit "$status"
  fi
  previous="$arg"
done
exec {git} "$@"
"#,
                git = shell_quote(&real_git),
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&wrapper).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&wrapper, permissions).unwrap();

        let old_path = std::env::var_os("PATH").expect("PATH must be set for git fixtures");
        let mut path_entries = vec![wrapper_dir];
        path_entries.extend(std::env::split_paths(&old_path));
        let error = {
            let _env = ScopedEnv::set(vec![
                ("PATH", std::env::join_paths(path_entries).unwrap()),
                ("ACP_TARGET_REPO_GIT_TIMEOUT_MS", OsString::from("100")),
            ]);
            store
                .admit_product_task(&validated, "tester")
                .expect_err("a timed-out worktree add must retain reconciliation state")
        };
        assert!(
            error.starts_with("product task workspace preparation requires reconciliation"),
            "{error}"
        );
        assert!(
            error.contains("git worktree creation outcome is unknown"),
            "{error}"
        );

        let task = store
            .get_product_task_by_idempotency("local", "default", "rec-worktree-add-timeout-1")
            .unwrap()
            .expect("timed-out task must remain durable");
        let task_id = task["task_id"].as_str().unwrap().to_string();
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::WorkspacePreparing.as_str())
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let workspace_path: String = connection
            .query_row(
                "SELECT workspace_path FROM product_task_workspace_preparations WHERE task_id = ?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id = ?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 1);
        drop(connection);

        let worktree_list = Command::new(&real_git)
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(worktree_list.status.success());
        assert!(
            String::from_utf8_lossy(&worktree_list.stdout)
                .lines()
                .any(|line| line == format!("worktree {workspace_path}")),
            "the timed-out add must leave a physical result for explicit recovery"
        );

        let recovered = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect("recovery must reuse the pinned worktree after an ambiguous add");
        assert_eq!(
            recovered["status"].as_str(),
            Some(ProductTaskStatus::WorkspaceBound.as_str())
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id = ?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 0);
        assert_eq!(run_git_head(&repo), rev);
    });
}

#[cfg(unix)]
#[test]
fn recovery_refuses_foreign_worktree_with_a_matching_source_commit() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "rec-foreign-worktree-recovery-1"),
            "local",
            "default",
        )
        .unwrap();

        // Publish the durable preparation receipt without creating the target
        // worktree. The failed post-add command is outcome-unknown and must
        // therefore remain recoverable rather than terminalized.
        let wrapper_dir = dir.path().join("git-wrapper");
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let real_git = git_binary_from_path();
        let shell_quote = |path: &std::path::Path| {
            format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
        };
        let wrapper = wrapper_dir.join("git");
        std::fs::write(
            &wrapper,
            format!(
                r#"#!/bin/sh
previous=''
for arg in "$@"; do
  if [ "$previous" = "worktree" ] && [ "$arg" = "add" ]; then
    exit 99
  fi
  previous="$arg"
done
exec {git} "$@"
"#,
                git = shell_quote(&real_git),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let old_path = std::env::var_os("PATH").expect("PATH must be set for git fixtures");
        let mut path_entries = vec![wrapper_dir];
        path_entries.extend(std::env::split_paths(&old_path));
        let admission_error = {
            let _env = ScopedEnv::set(vec![("PATH", std::env::join_paths(path_entries).unwrap())]);
            store
                .admit_product_task(&validated, "tester")
                .expect_err("failed worktree add must retain a receipt for reconciliation")
        };
        assert!(
            admission_error
                .starts_with("product task workspace preparation requires reconciliation"),
            "{admission_error}"
        );

        let task = store
            .get_product_task_by_idempotency("local", "default", "rec-foreign-worktree-recovery-1")
            .unwrap()
            .expect("preparing task");
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let workspace_path: String = connection
            .query_row(
                "SELECT workspace_path FROM product_task_workspace_preparations WHERE task_id=?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        drop(connection);
        let workspace_path = std::path::PathBuf::from(workspace_path);
        assert!(!workspace_path.exists());

        // A clone of the target contains the same commit object, but only the
        // foreign repository registers this path. A HEAD-only recovery check
        // would incorrectly bind it as the target worktree.
        let foreign_root = tempfile::tempdir().unwrap();
        let foreign = foreign_root.path().join("foreign");
        let clone = Command::new(&real_git)
            .args([
                "clone",
                "--no-hardlinks",
                repo.to_str().unwrap(),
                foreign.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            clone.status.success(),
            "foreign clone failed: {}",
            String::from_utf8_lossy(&clone.stderr)
        );
        let foreign_add = Command::new(&real_git)
            .args([
                "worktree",
                "add",
                "--detach",
                workspace_path.to_str().unwrap(),
                &rev,
            ])
            .current_dir(&foreign)
            .output()
            .unwrap();
        assert!(
            foreign_add.status.success(),
            "foreign worktree add failed: {}",
            String::from_utf8_lossy(&foreign_add.stderr)
        );

        let recovery_error = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect_err("foreign worktree must require reconciliation");
        assert!(
            recovery_error
                .starts_with("product task workspace preparation requires reconciliation"),
            "{recovery_error}"
        );
        assert!(
            recovery_error
                .contains("existing receipt workspace registration or source identity is unproved"),
            "{recovery_error}"
        );
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"],
            ProductTaskStatus::WorkspacePreparing.as_str()
        );
        assert_eq!(
            store.supervised_patch_workspaces(10).unwrap().len(),
            0,
            "foreign workspace must not produce a target supervised-workspace record"
        );
        let foreign_worktrees = Command::new(&real_git)
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&foreign)
            .output()
            .unwrap();
        assert!(foreign_worktrees.status.success());
        assert!(
            String::from_utf8_lossy(&foreign_worktrees.stdout)
                .lines()
                .any(|line| line == format!("worktree {}", workspace_path.display())),
            "recovery must not delete a foreign registered worktree"
        );
    });
}

#[cfg(unix)]
#[test]
fn target_path_drift_blocks_recovery_before_root_marker_or_lock_recreation() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "rec-target-path-drift-1"),
            "local",
            "default",
        )
        .unwrap();

        let wrapper_dir = dir.path().join("git-wrapper");
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let real_git = git_binary_from_path();
        let shell_quote = |path: &std::path::Path| {
            format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
        };
        let wrapper = wrapper_dir.join("git");
        std::fs::write(
            &wrapper,
            format!(
                r#"#!/bin/sh
previous=''
for arg in "$@"; do
  if [ "$previous" = "worktree" ] && [ "$arg" = "add" ]; then
    exit 99
  fi
  previous="$arg"
done
exec {git} "$@"
"#,
                git = shell_quote(&real_git),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let old_path = std::env::var_os("PATH").expect("PATH must be set for git fixtures");
        let mut path_entries = vec![wrapper_dir];
        path_entries.extend(std::env::split_paths(&old_path));
        let admission_error = {
            let _env = ScopedEnv::set(vec![("PATH", std::env::join_paths(path_entries).unwrap())]);
            store
                .admit_product_task(&validated, "tester")
                .expect_err("failed worktree add must retain a preparation receipt")
        };
        assert!(
            admission_error
                .starts_with("product task workspace preparation requires reconciliation"),
            "{admission_error}"
        );

        let task = store
            .get_product_task_by_idempotency("local", "default", "rec-target-path-drift-1")
            .unwrap()
            .expect("preparing task");
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let workspace_root = std::path::PathBuf::from(
            std::env::var("ACP_PRODUCT_WORKSPACE_ROOT").expect("workspace root"),
        );
        let marker_path = workspace_root.join(format!(".pt-{task_id}.prepare.marker"));
        assert!(workspace_root.is_dir());
        assert!(marker_path.is_file());

        // The original target was valid when the receipt was published. After
        // a restart its configured path drifts to the parent of that pinned
        // workspace root. Remove the old root so any new root/marker/lock is
        // attributable to the recovery under test.
        let drifted_target = workspace_root.parent().unwrap().to_path_buf();
        std::fs::remove_dir_all(&workspace_root).unwrap();
        assert!(!workspace_root.exists());
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "UPDATE product_tasks SET target_repo_path=?1 WHERE task_id=?2",
                rusqlite::params![drifted_target.to_string_lossy(), &task_id],
            )
            .unwrap();
        drop(connection);

        let recovery_error = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect_err("target/root overlap must require reconciliation before effects");
        assert!(
            recovery_error
                .starts_with("product task workspace preparation requires reconciliation"),
            "{recovery_error}"
        );
        assert!(
            recovery_error.contains("pinned workspace root overlaps current target repository"),
            "{recovery_error}"
        );
        assert!(
            !workspace_root.exists(),
            "recovery must not recreate a workspace root inside the drifted target"
        );
        assert!(
            !marker_path.exists(),
            "recovery must not recreate the receipt marker after target drift"
        );
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"],
            ProductTaskStatus::WorkspacePreparing.as_str()
        );
    });
}

#[cfg(unix)]
#[test]
fn unproven_worktree_compensation_requires_reconciliation() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "rec-worktree-remove-failure-1"),
            "local",
            "default",
        )
        .unwrap();

        let wrapper_dir = dir.path().join("git-wrapper");
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let real_git = git_binary_from_path();
        let shell_quote = |path: &std::path::Path| {
            format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
        };
        let wrapper = wrapper_dir.join("git");
        std::fs::write(
            &wrapper,
            format!(
                r#"#!/bin/sh
previous=''
for arg in "$@"; do
  if [ "$previous" = "worktree" ] && [ "$arg" = "remove" ]; then
    exit 1
  fi
  previous="$arg"
done
exec {git} "$@"
"#,
                git = shell_quote(&real_git),
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&wrapper).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&wrapper, permissions).unwrap();

        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_product_workspace_bind_audit
                 BEFORE INSERT ON audit_log
                 WHEN NEW.action = 'product_task.transition'
                  AND NEW.details_json LIKE '%\"to\":\"workspace_bound\"%'
                 BEGIN SELECT RAISE(ABORT, 'injected workspace bind audit failure'); END;",
            )
            .unwrap();
        drop(connection);

        let old_path = std::env::var_os("PATH").expect("PATH must be set for git fixtures");
        let mut path_entries = vec![wrapper_dir];
        path_entries.extend(std::env::split_paths(&old_path));
        let error = {
            let _env = ScopedEnv::set(vec![("PATH", std::env::join_paths(path_entries).unwrap())]);
            store
                .admit_product_task(&validated, "tester")
                .expect_err("an unproven cleanup must not terminalize the task")
        };
        assert!(
            error.starts_with("product task workspace preparation requires reconciliation"),
            "{error}"
        );

        let task = store
            .get_product_task_by_idempotency("local", "default", "rec-worktree-remove-failure-1")
            .unwrap()
            .expect("task must remain durable for reconciliation");
        let task_id = task["task_id"].as_str().unwrap().to_string();
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::WorkspacePreparing.as_str())
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let workspace_path: String = connection
            .query_row(
                "SELECT workspace_path FROM product_task_workspace_preparations WHERE task_id = ?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id = ?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 1);
        let workspaces = store.supervised_patch_workspaces(10).unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_ne!(workspaces[0]["status"], "cleaned");
        drop(connection);

        let worktree_list = Command::new(&real_git)
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(worktree_list.status.success());
        assert!(
            String::from_utf8_lossy(&worktree_list.stdout)
                .lines()
                .any(|line| line == format!("worktree {workspace_path}")),
            "cleanup failure must retain the exact registered worktree for reconciliation"
        );
    });
}

#[cfg(unix)]
#[test]
fn recovery_waits_for_locked_worktree_compensation() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = Arc::new(
            validate_intake(
                &intake(&repo, &rev, "rec-recover-compensation-race-1"),
                "local",
                "default",
            )
            .unwrap(),
        );

        let wrapper_dir = dir.path().join("git-wrapper");
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let cleanup_started = dir.path().join("worktree-cleanup-started");
        let cleanup_release = dir.path().join("worktree-cleanup-release");
        let _release_guard = FileReleaseGuard {
            path: cleanup_release.clone(),
        };
        let real_git = git_binary_from_path();
        let shell_quote = |path: &std::path::Path| {
            format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
        };
        let wrapper = wrapper_dir.join("git");
        std::fs::write(
            &wrapper,
            format!(
                r#"#!/bin/sh
previous=''
for arg in "$@"; do
  if [ "$previous" = "worktree" ] && [ "$arg" = "remove" ]; then
    : > {started}
    while [ ! -f {release} ]; do
      sleep 0.01
    done
    break
  fi
  previous="$arg"
done
exec {git} "$@"
"#,
                started = shell_quote(&cleanup_started),
                release = shell_quote(&cleanup_release),
                git = shell_quote(&real_git),
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&wrapper).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&wrapper, permissions).unwrap();

        let old_path = std::env::var_os("PATH").expect("PATH must be set for git fixtures");
        let mut path_entries = vec![wrapper_dir];
        path_entries.extend(std::env::split_paths(&old_path));
        let _env = ScopedEnv::set(vec![("PATH", std::env::join_paths(path_entries).unwrap())]);

        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_product_workspace_bind_audit
                 BEFORE INSERT ON audit_log
                 WHEN NEW.action = 'product_task.transition'
                  AND NEW.details_json LIKE '%\"to\":\"workspace_bound\"%'
                 BEGIN SELECT RAISE(ABORT, 'injected workspace bind audit failure'); END;",
            )
            .unwrap();
        drop(connection);

        let admitting_store = Arc::clone(&store);
        let admitting_intake = Arc::clone(&validated);
        let admit = thread::spawn(move || {
            admitting_store.admit_product_task(&admitting_intake, "active-admit")
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !cleanup_started.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "admit did not reach the delayed worktree compensation boundary",
            );
            thread::sleep(Duration::from_millis(10));
        }
        let preparing_task = store
            .get_product_task_by_idempotency("local", "default", "rec-recover-compensation-race-1")
            .unwrap()
            .expect("reserved task");
        assert_eq!(
            preparing_task["status"].as_str(),
            Some(ProductTaskStatus::WorkspacePreparing.as_str())
        );
        let task_id = preparing_task["task_id"].as_str().unwrap().to_string();

        let recovering_store = Arc::clone(&store);
        let recovering_task_id = task_id.clone();
        let recover = thread::spawn(move || {
            recovering_store.recover_product_task_workspace(&recovering_task_id, "recovery")
        });
        wait_for_workspace_prepare_lock_contention(&store, &task_id);
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"],
            ProductTaskStatus::WorkspacePreparing.as_str(),
            "recovery must not bind while the active owner compensates"
        );

        std::fs::write(&cleanup_release, "release\n").unwrap();

        let admit_error = admit
            .join()
            .expect("admit thread must not panic")
            .expect_err("injected workspace-bind failure must fail active admit");
        assert!(admit_error.contains("injected workspace bind audit failure"));
        let recovered = recover
            .join()
            .expect("recovery thread must not panic")
            .expect("waiting recovery must observe terminal task");
        assert_eq!(
            recovered["status"].as_str(),
            Some(ProductTaskStatus::Failed.as_str())
        );
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"],
            ProductTaskStatus::Failed.as_str()
        );
        let workspaces = store.supervised_patch_workspaces(10).unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0]["status"], "cleaned");

        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute_batch("DROP TRIGGER fail_product_workspace_bind_audit")
            .unwrap();
    });
}

#[cfg(unix)]
#[test]
fn invalid_workspace_root_and_legacy_preparation_fail_closed_without_cleanup() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let unavailable_workspace_root = dir.path().join("workspace-root-file");
        std::fs::write(&unavailable_workspace_root, "not a directory\n").unwrap();
        let _env = ScopedEnv::set(vec![(
            "ACP_PRODUCT_WORKSPACE_ROOT",
            unavailable_workspace_root.into_os_string(),
        )]);
        let request = intake(&repo, &rev, "rec-workspace-lock-setup-failure");
        let validated = validate_intake(&request, "local", "default").unwrap();

        let error = store
            .admit_product_task(&validated, "tester")
            .expect_err("unavailable worktree lock root must reject fresh intake");
        assert!(error.contains("workspace root is unavailable"));
        let admitted = store
            .get_product_task_by_idempotency("local", "default", "rec-workspace-lock-setup-failure")
            .unwrap()
            .expect("fresh task reservation");
        assert_eq!(
            admitted["status"].as_str(),
            Some(ProductTaskStatus::Admitted.as_str()),
            "lock setup happens before durable workspace preparation"
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id = ?1",
                [admitted["task_id"].as_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            receipt_count, 0,
            "fresh preflight must not publish a receipt"
        );
        drop(connection);
        assert!(store.supervised_patch_workspaces(10).unwrap().is_empty());

        // Simulate a persisted interrupted preparation from an earlier
        // process.  An unavailable lock must not terminalize or clean it
        // without proving that no physical owner remains.
        let task_id = admitted["task_id"].as_str().unwrap().to_string();
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "UPDATE product_tasks
                 SET status = 'workspace_preparing', version = version + 1
                 WHERE task_id = ?1",
                [&task_id],
            )
            .unwrap();
        drop(connection);

        let recovery_error = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect_err("legacy prepare must require reconciliation");
        assert!(recovery_error.contains("legacy preparing task has no receipt"));
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"],
            ProductTaskStatus::WorkspacePreparing.as_str()
        );
        assert!(store.supervised_patch_workspaces(10).unwrap().is_empty());

        let recovery_workspace_root = dir.path().join("recovery-workspaces");
        std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", &recovery_workspace_root);
        let root_drift_error = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect_err("a legacy prepare must not adopt a newly configured root");
        assert!(root_drift_error.contains("legacy preparing task has no receipt"));
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"].as_str(),
            Some(ProductTaskStatus::WorkspacePreparing.as_str())
        );
        assert!(
            !recovery_workspace_root.exists(),
            "legacy recovery must not synthesize a new workspace root"
        );
    });
}

#[cfg(unix)]
#[test]
fn workspace_root_relative_traversal_is_rejected_before_receipt_or_marker_creation() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let escaped_root = dir
            .path()
            .join("untrusted-workspace-root")
            .join("..")
            .join("escaped-workspace-root");
        let _root = ScopedEnv::set(vec![(
            "ACP_PRODUCT_WORKSPACE_ROOT",
            escaped_root.clone().into_os_string(),
        )]);
        let key = "rec-workspace-root-traversal";
        let validated = validate_intake(&intake(&repo, &rev, key), "local", "default").unwrap();
        let error = store
            .admit_product_task(&validated, "tester")
            .expect_err("relative traversal in the workspace root must fail closed");
        assert!(error.contains("workspace root contains relative traversal"));
        let task = store
            .get_product_task_by_idempotency("local", "default", key)
            .unwrap()
            .expect("preflight failure still reserves an idempotency row");
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::Admitted.as_str())
        );
        assert!(
            !dir.path().join("escaped-workspace-root").exists(),
            "a traversal-rejected root must not be created"
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id=?1",
                [task["task_id"].as_str().unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 0);
    });
}

#[cfg(unix)]
#[test]
fn disabled_product_gate_recovery_creates_no_workspace_effect() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "rec-disabled-gate-recovery"),
            "local",
            "default",
        )
        .unwrap();

        // Reserve an otherwise valid idempotency row without publishing a
        // receipt, then model an interrupted legacy prepare.
        std::env::remove_var("ACP_ENABLE_TARGET_REPO_OUTPUT");
        let admission_error = store
            .admit_product_task(&validated, "tester")
            .expect_err("disabled target output must leave a task admitted");
        assert!(admission_error.contains("ACP_ENABLE_TARGET_REPO_OUTPUT=1"));
        std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
        let task = store
            .get_product_task_by_idempotency("local", "default", "rec-disabled-gate-recovery")
            .unwrap()
            .expect("reserved task");
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "UPDATE product_tasks
                 SET status='workspace_preparing', version=version+1
                 WHERE task_id=?1",
                [&task_id],
            )
            .unwrap();
        drop(connection);

        let disabled_root = dir.path().join("disabled-recovery-root");
        let _root = ScopedEnv::set(vec![(
            "ACP_PRODUCT_WORKSPACE_ROOT",
            disabled_root.clone().into_os_string(),
        )]);
        std::env::remove_var(PRODUCT_TASK_GATE);
        let error = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect_err("disabled product gate must stop recovery before physical preparation");
        assert!(error.contains("product golden path intake is disabled"));
        assert!(
            !disabled_root.exists(),
            "disabled recovery must not create a workspace root, marker, or lock"
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id=?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 0);
        std::env::set_var(PRODUCT_TASK_GATE, "1");
    });
}

#[cfg(unix)]
#[test]
fn restart_retires_a_crash_left_preparation_receipt_after_workspace_binding() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task_id = admit(&store, &repo, &rev, "rec-retire-crash-residue");
        let workspace_root = std::fs::canonicalize(
            std::env::var_os("ACP_PRODUCT_WORKSPACE_ROOT").expect("workspace root"),
        )
        .unwrap();
        let workspace_path = workspace_root.join(format!("pt-{task_id}"));
        assert!(workspace_path.is_dir());
        let marker_sha256 = "a".repeat(64);
        let marker_state = "marker_ready";
        let receipt_sha256 = workspace_preparation_receipt_sha256(
            &task_id,
            &workspace_root,
            &workspace_path,
            &marker_sha256,
            marker_state,
        );
        let marker_path = workspace_root.join(format!(".pt-{task_id}.prepare.marker"));
        std::fs::write(&marker_path, format!("{marker_sha256}\n")).unwrap();
        let mut permissions = std::fs::metadata(&marker_path).unwrap().permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(&marker_path, permissions).unwrap();

        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "INSERT INTO product_task_workspace_preparations (
                    task_id, workspace_root, workspace_path, marker_sha256, marker_state,
                    receipt_sha256, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                rusqlite::params![
                    &task_id,
                    workspace_root.to_string_lossy(),
                    workspace_path.to_string_lossy(),
                    &marker_sha256,
                    marker_state,
                    &receipt_sha256,
                    "2026-07-28T00:00:00Z",
                ],
            )
            .unwrap();
        drop(connection);

        let recovered = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect("bound task must repair crash-left retirement residue");
        assert_eq!(
            recovered["status"].as_str(),
            Some(ProductTaskStatus::WorkspaceBound.as_str())
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id=?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 0);
        assert!(
            !marker_path.exists(),
            "receipt retirement must remove only the exact durable marker"
        );
    });
}

#[cfg(unix)]
#[test]
fn compensation_cleans_exact_workspace_beyond_global_listing_limit() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task_id = admit(&store, &repo, &rev, "rec-compensation-exact-run-lookup-1");
        let provisional = engine::product_golden_path::provisional_run_id_for_task(&task_id);
        let original_workspace = store
            .get_supervised_patch_workspace_for_run(&provisional)
            .unwrap()
            .expect("bound task workspace record");
        let original_workspace_id = original_workspace["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Simulate a crash after a physical workspace binding but before
        // receipt retirement. Force the resumed prepare to fail before it can
        // create a second record, so compensation must find this original
        // provisional-run record exactly.
        let workspace_root = std::fs::canonicalize(
            std::env::var_os("ACP_PRODUCT_WORKSPACE_ROOT").expect("workspace root"),
        )
        .unwrap();
        let workspace_path = workspace_root.join(format!("pt-{task_id}"));
        let marker_sha256 = "b".repeat(64);
        let marker_state = "marker_ready";
        let receipt_sha256 = workspace_preparation_receipt_sha256(
            &task_id,
            &workspace_root,
            &workspace_path,
            &marker_sha256,
            marker_state,
        );
        let marker_path = workspace_root.join(format!(".pt-{task_id}.prepare.marker"));
        std::fs::write(&marker_path, format!("{marker_sha256}\n")).unwrap();
        std::fs::set_permissions(&marker_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "INSERT INTO product_task_workspace_preparations (
                    task_id, workspace_root, workspace_path, marker_sha256, marker_state,
                    receipt_sha256, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                rusqlite::params![
                    &task_id,
                    workspace_root.to_string_lossy(),
                    workspace_path.to_string_lossy(),
                    &marker_sha256,
                    marker_state,
                    &receipt_sha256,
                    "2026-07-28T00:00:00Z",
                ],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE product_tasks
                 SET status='workspace_preparing', version=version+1, source_tree_hash=?1
                 WHERE task_id=?2",
                rusqlite::params!["f".repeat(64), &task_id],
            )
            .unwrap();
        drop(connection);

        // These rows sort ahead of the original ProductTask workspace. A
        // global `LIMIT 50` scan would miss it; exact run lookup must not.
        let unrelated_root = dir.path().join("unrelated-workspaces");
        for sequence in 0..51 {
            let unrelated_path = unrelated_root.join(format!("workspace-{sequence:02}"));
            store
                .record_supervised_patch_workspace(
                    &serde_json::json!({
                        "run_id": format!("unrelated-run-{sequence:02}"),
                        "target_id": format!("unrelated-target-{sequence:02}"),
                        "target_repo_path": repo.to_string_lossy(),
                        "workspace_path": unrelated_path.to_string_lossy(),
                        "source_revision": rev,
                        "status": "requested",
                    }),
                    "fixture",
                )
                .unwrap();
        }
        assert!(
            store
                .supervised_patch_workspaces(50)
                .unwrap()
                .iter()
                .all(|workspace| workspace["run_id"] != provisional),
            "the original workspace must be outside the old global-listing window"
        );

        let error = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect_err("mismatched source tree must enter compensation");
        assert!(error.contains("source_tree_hash mismatch against prepared workspace"));
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"],
            ProductTaskStatus::Failed.as_str()
        );
        assert_eq!(
            store
                .get_supervised_patch_workspace(&original_workspace_id)
                .unwrap()
                .unwrap()["status"],
            "cleaned",
            "exact provisional-run lookup must clean the original workspace record"
        );
        assert!(
            !workspace_path.exists(),
            "compensation must prove the receipt worktree path is absent"
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id=?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 0);
    });
}

#[cfg(unix)]
#[test]
fn compensation_refuses_ambiguous_duplicate_provisional_workspace_records() {
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task_id = admit(&store, &repo, &rev, "rec-compensation-duplicate-run-1");
        let provisional = engine::product_golden_path::provisional_run_id_for_task(&task_id);
        let original_workspace = store
            .get_supervised_patch_workspace_for_run(&provisional)
            .unwrap()
            .expect("bound task workspace record");
        let original_workspace_id = original_workspace["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();
        let workspace_root = std::fs::canonicalize(
            std::env::var_os("ACP_PRODUCT_WORKSPACE_ROOT").expect("workspace root"),
        )
        .unwrap();
        let workspace_path = workspace_root.join(format!("pt-{task_id}"));
        let marker_sha256 = "c".repeat(64);
        let marker_state = "marker_ready";
        let receipt_sha256 = workspace_preparation_receipt_sha256(
            &task_id,
            &workspace_root,
            &workspace_path,
            &marker_sha256,
            marker_state,
        );
        let marker_path = workspace_root.join(format!(".pt-{task_id}.prepare.marker"));
        std::fs::write(&marker_path, format!("{marker_sha256}\n")).unwrap();
        std::fs::set_permissions(&marker_path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "INSERT INTO product_task_workspace_preparations (
                    task_id, workspace_root, workspace_path, marker_sha256, marker_state,
                    receipt_sha256, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                rusqlite::params![
                    &task_id,
                    workspace_root.to_string_lossy(),
                    workspace_path.to_string_lossy(),
                    &marker_sha256,
                    marker_state,
                    &receipt_sha256,
                    "2026-07-28T00:00:00Z",
                ],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE product_tasks
                 SET status='workspace_preparing', version=version+1, source_tree_hash=?1
                 WHERE task_id=?2",
                rusqlite::params!["e".repeat(64), &task_id],
            )
            .unwrap();
        drop(connection);

        let duplicate = store
            .record_supervised_patch_workspace(
                &serde_json::json!({
                    "run_id": provisional,
                    "target_id": "duplicate-target",
                    "target_repo_path": repo.to_string_lossy(),
                    "workspace_path": workspace_path.to_string_lossy(),
                    "source_revision": rev,
                    "workspace_mode": "git_worktree",
                    "git": {
                        "default_branch": "main",
                        "source_revision": rev,
                    },
                    "status": "workspace_created",
                }),
                "fixture",
            )
            .unwrap();
        assert_ne!(duplicate["workspace_id"], original_workspace_id);
        assert_eq!(
            store
                .supervised_patch_workspaces(10)
                .unwrap()
                .into_iter()
                .filter(|workspace| workspace["run_id"] == provisional)
                .count(),
            2
        );

        let recovery_error = store
            .recover_product_task_workspace(&task_id, "recovery")
            .expect_err("ambiguous provisional workspace records must block compensation");
        assert!(
            recovery_error
                .starts_with("product task workspace preparation requires reconciliation"),
            "{recovery_error}"
        );
        assert!(
            recovery_error.contains("multiple supervised workspaces match the provisional run"),
            "{recovery_error}"
        );
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"],
            ProductTaskStatus::WorkspacePreparing.as_str()
        );
        assert!(
            workspace_path.is_dir(),
            "ambiguous metadata must not trigger destructive worktree cleanup"
        );
        assert_ne!(
            store
                .get_supervised_patch_workspace(&original_workspace_id)
                .unwrap()
                .unwrap()["status"],
            "cleaned"
        );
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        let receipt_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM product_task_workspace_preparations WHERE task_id=?1",
                [&task_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(receipt_count, 1);
    });
}

#[test]
fn idempotent_intake_never_reprepares_a_bound_repair_pending_or_paused_task() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(&repo, &rev, "rec-bound-repair-paused"),
            "local",
            "default",
        )
        .unwrap();
        let admitted = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = admitted["task_id"].as_str().unwrap().to_string();
        let workspace_count = store.supervised_patch_workspaces(10).unwrap().len();

        for status in ["repair_pending", "paused"] {
            let connection = rusqlite::Connection::open(store.db_path()).unwrap();
            connection
                .execute(
                    "UPDATE product_tasks SET status=?1, version=version+1 WHERE task_id=?2",
                    rusqlite::params![status, &task_id],
                )
                .unwrap();
            drop(connection);
            let replay = store
                .admit_product_task(&validated, "tester")
                .expect("idempotent intake must reuse the bound workspace");
            assert_eq!(replay["task_id"].as_str(), Some(task_id.as_str()));
            assert_eq!(replay["status"].as_str(), Some(status));
            assert_eq!(
                store.supervised_patch_workspaces(10).unwrap().len(),
                workspace_count
            );
        }
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
fn restart_before_verification_reuses_persisted_execution_authority() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-restart-before-verification",
            "test -f docs/product_golden_path_fixture.md",
        );
        let db_path = store.db_path().to_path_buf();
        drop(store);

        let restarted = LocalProductStore::new(db_path).unwrap();
        let finalized = restarted
            .finalize_product_task_after_execution_with_authority(&task_id, "verifier", &|| {
                Ok(running_scheduler_authority())
            })
            .unwrap();
        assert_eq!(finalized["phase"], "awaiting_approval");
        assert_eq!(finalized["task"]["status"], "awaiting_approval");
        assert!(finalized["artifact_id"].is_string());
    });
}

#[test]
fn restart_during_verification_persists_pause_and_rejects_late_result() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, run_id, workspace_id, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-restart-during-verification",
            "tail -f README.md",
        );
        let db_path = store.db_path().to_path_buf();
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task_id = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task_id,
                "verifier",
                &|| Ok(running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        let restarted = LocalProductStore::new(&db_path).unwrap();
        restarted
            .update_run_pause_reason(&run_id, Some("operator_hold_after_restart"))
            .unwrap();
        drop(restarted);

        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "paused");
        assert!(finalized["artifact_id"].is_null());

        drop(store);
        let restarted = LocalProductStore::new(db_path).unwrap();
        let persisted = restarted.get_product_task(&task_id).unwrap().unwrap();
        assert_eq!(persisted["status"], "paused");
        let workspace = restarted
            .get_supervised_patch_workspace(&workspace_id)
            .unwrap()
            .unwrap();
        assert_eq!(workspace["verification"]["status"], "authority_lost");
        assert_eq!(
            workspace["verification"]["verification_attempts"][0]["late_result_rejected"],
            true
        );
        let approval = restarted.approve_product_task(
            &task_id,
            "operator",
            persisted["version"].as_u64().unwrap(),
        );
        assert!(approval.unwrap_err().contains("awaiting_approval"));
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
            .approve_and_output_product_task(&task_id, "tester", true)
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
            .approve_and_output_product_task(&task_id, "tester", true)
            .unwrap();
        let again = store
            .approve_and_output_product_task(&task_id, "tester", true)
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

#[test]
fn verification_workflow_result_never_persists_repository_command_output() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        init_git_repo(&repo);
        let marker = "private-repository-content-never-persist-7f5a";
        std::fs::write(repo.join("README.md"), format!("{marker}\n")).unwrap();
        for args in [
            &["add", "README.md"][..],
            &["commit", "--amend", "--no-edit"][..],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap()
                .status
                .success());
        }
        let revision = String::from_utf8_lossy(
            &Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let (task_id, _, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &revision,
            "rec-redacted-workflow-output",
            "cat README.md",
        );
        let finalized = store
            .finalize_product_task_after_execution(&task_id, "tester")
            .unwrap();
        assert_eq!(finalized["phase"], "awaiting_approval");
        let verification_run_id = finalized["verification"]["verification_attempts"][0]
            ["verification_run_id"]
            .as_str()
            .unwrap();
        let persisted_run = store
            .get_workflow_run(verification_run_id)
            .unwrap()
            .unwrap()
            .to_string();
        assert!(!persisted_run.contains(marker));
        assert!(persisted_run.contains("redacted_command_output_sha256"));
    });
}

#[test]
fn concurrent_finalizers_consume_one_managed_verification_effect() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-concurrent-finalize",
            "tail -f README.md",
        );
        let mut handles = Vec::new();
        for _ in 0..2 {
            let store = Arc::clone(&store);
            let task_id = task_id.clone();
            handles.push(thread::spawn(move || {
                store.finalize_product_task_after_execution_with_authority(
                    &task_id,
                    "verifier",
                    &|| Ok(running_scheduler_authority()),
                )
            }));
        }
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert!(results.iter().any(Result::is_ok));
        let allowed_effects = store
            .audit_events(10_000)
            .unwrap()
            .into_iter()
            .filter(|event| event["action"] == "tool_execution.pre_policy_passed")
            .count();
        assert_eq!(allowed_effects, 1, "only one tool effect may pass policy");
        let artifacts = store.supervised_patch_artifacts(100).unwrap();
        assert!(
            artifacts.is_empty(),
            "late-writing verification must capture no artifact"
        );
    });
}

#[test]
fn restart_after_persisted_effect_rejects_changed_pre_patch_binding() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, workspace_id, workspace_path) =
            ready_for_slow_verification(&store, &repo, &rev, "rec-crash-after-effect", "true");
        let db_path = store.db_path().to_path_buf();
        let calls = Arc::new(AtomicUsize::new(0));
        let finalizer_store = Arc::clone(&store);
        let finalizer_task = task_id.clone();
        let finalizer_calls = Arc::clone(&calls);
        let crash_workspace = workspace_path.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task,
                "verifier-before-crash",
                &|| {
                    if finalizer_calls.fetch_add(1, Ordering::SeqCst) == 1 {
                        std::fs::write(
                            std::path::Path::new(&crash_workspace).join("README.md"),
                            "changed in crash window\n",
                        )
                        .unwrap();
                        panic!("simulated process loss after durable managed effect");
                    }
                    Ok(running_scheduler_authority())
                },
            )
        });
        assert!(handle.join().is_err());
        drop(store);

        let restarted = LocalProductStore::new(db_path).unwrap();
        let finalized = restarted
            .finalize_product_task_after_execution_with_authority(
                &task_id,
                "verifier-after-restart",
                &|| Ok(running_scheduler_authority()),
            )
            .unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "blocked");
        assert!(finalized["artifact_id"].is_null());
        assert!(finalized["verification"]["authority_loss_reason"]
            .as_str()
            .unwrap()
            .contains("pre_patch_binding_superseded"));
        assert_eq!(
            restarted
                .get_supervised_patch_workspace(&workspace_id)
                .unwrap()
                .unwrap()["status"],
            "quarantined"
        );
    });
}

#[test]
fn product_verification_rejects_writable_or_absolute_path_commands_at_intake() {
    with_gates(|| {
        let (dir, _) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let protected = dir.path().join("target-main-protected.txt");
        std::fs::write(&protected, "unchanged\n").unwrap();

        for (index, (command, expected_error)) in [
            (
                format!("tee {}", protected.display()),
                "read-only admitted binary",
            ),
            ("python3 mutate.py".to_string(), "read-only admitted binary"),
            (
                "cat /etc/passwd".to_string(),
                "relative to the bound workspace",
            ),
            (
                "test -f ../outside".to_string(),
                "relative to the bound workspace",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let mut request = intake(&repo, &rev, &format!("reject-command-{index}"));
            request.verification_commands = vec![ProductVerificationCommand {
                command,
                timeout_ms: 5_000,
            }];
            let error = validate_intake(&request, "local", "default").unwrap_err();
            assert!(
                error.contains(expected_error),
                "unexpected rejection: {error}"
            );
        }
        assert_eq!(std::fs::read_to_string(protected).unwrap(), "unchanged\n");
    });
}

#[test]
fn tracked_ignored_directory_write_changes_authoritative_patch_identity() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        init_git_repo(&repo);
        std::fs::create_dir_all(repo.join("target")).unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        std::fs::write(repo.join("target/tracked.txt"), "before\n").unwrap();
        for args in [
            &["add", ".gitignore"][..],
            &["add", "-f", "target/tracked.txt"][..],
            &["commit", "-m", "add tracked ignored fixture"][..],
        ] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap()
                .status
                .success());
        }
        let rev = String::from_utf8_lossy(
            &Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let (task_id, _, _, workspace_path) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-tracked-ignored-write",
            "tail -f README.md",
        );
        let store = Arc::new(store);
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task,
                "verifier",
                &|| Ok(running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        std::fs::write(
            std::path::Path::new(&workspace_path).join("target/tracked.txt"),
            "after\n",
        )
        .unwrap();
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert!(finalized["artifact_id"].is_null());
        assert!(finalized["verification"]["authority_loss_reason"]
            .as_str()
            .unwrap()
            .contains("late_filesystem_write"));
    });
}

#[test]
fn total_elapsed_budget_exhaustion_blocks_verification_effect_and_artifact() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let mut request = intake(&repo, &rev, "rec-verification-elapsed-budget");
        request.budget = Some(ProductTaskBudget {
            total_elapsed_ms: Some(1),
            ..ProductTaskBudget::default()
        });
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let compiled = compile(&store, &task_id);
        complete_run(&store, compiled["task"]["run_id"].as_str().unwrap());
        thread::sleep(Duration::from_millis(1_100));
        let finalized = store
            .finalize_product_task_after_execution_with_authority(&task_id, "verifier", &|| {
                Ok(running_scheduler_authority())
            })
            .unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "budget_exhausted");
        assert!(finalized["artifact_id"].is_null());
        assert!(finalized["verification"]["authority_loss_reason"]
            .as_str()
            .unwrap()
            .contains("budget_exhausted"));
    });
}

#[test]
fn sqlite_managed_token_budget_exhaustion_blocks_verification_and_artifact() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        assert_managed_token_budget_exhaustion(
            &store,
            &repo,
            &rev,
            "rec-managed-token-budget-sqlite",
        );
    });
}

#[test]
fn sqlite_managed_missing_usage_blocks_all_product_effects() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        assert_managed_missing_usage_fails_closed(
            &store,
            &repo,
            &rev,
            "rec-managed-missing-usage-sqlite",
        );
    });
}

#[test]
fn sqlite_managed_call_budget_denies_second_executor_call() {
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(LocalProductStore::new(dir.path().join("store.db")).unwrap());
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        assert_managed_call_budget_denies_second_call(
            store,
            &repo,
            &rev,
            "rec-managed-call-budget-sqlite",
        );
    });
}

#[test]
fn sqlite_product_admission_audit_failure_rolls_back_private_objective() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let key = "rec-product-admit-audit-sqlite";
        let request = intake(&repo, &rev, key);
        let validated = validate_intake(&request, "local", "default").unwrap();
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_product_admit_audit
                 BEFORE INSERT ON audit_log
                 WHEN NEW.action = 'product_task.admit'
                 BEGIN SELECT RAISE(ABORT, 'injected product admission audit failure'); END;",
            )
            .unwrap();
        let error = store
            .admit_product_task(&validated, "tester")
            .expect_err("audit failure must roll back admission");
        assert!(error.contains("injected product admission audit failure"));
        assert!(store
            .get_product_task_by_idempotency("local", "default", key)
            .unwrap()
            .is_none());
        connection
            .execute_batch("DROP TRIGGER fail_product_admit_audit")
            .unwrap();
    });
}

#[test]
fn sqlite_restart_accumulates_failed_managed_attempt_usage() {
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("store.db");
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let store = LocalProductStore::new(&db_path).unwrap();
        let (task_id, run_id) =
            prepare_cumulative_managed_task(&store, &repo, &rev, "rec-managed-cumulative-sqlite");
        drop(store);

        let restarted = LocalProductStore::new(&db_path).unwrap();
        finish_cumulative_managed_task(&restarted, &task_id, &run_id);
        assert_eq!(run_git_head(&repo), rev);
        assert!(git_status_paths(&repo).is_empty());
    });
}

#[cfg(all(feature = "pg-tests", unix))]
#[test]
fn postgres_worktree_recovery_rejects_root_drift_and_reuses_pinned_root() {
    use std::os::unix::fs::PermissionsExt;

    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let workspace_root_a = dir.path().join("workspace-root-a");
        let workspace_root_b = dir.path().join("workspace-root-b");
        let wrapper_dir = dir.path().join("git-wrapper");
        std::fs::create_dir_all(&wrapper_dir).unwrap();
        let worktree_add_started = dir.path().join("worktree-add-started");
        let worktree_add_release = dir.path().join("worktree-add-release");
        let worktree_add_log = dir.path().join("worktree-add.log");
        let _release_guard = FileReleaseGuard {
            path: worktree_add_release.clone(),
        };
        let real_git = git_binary_from_path();
        let shell_quote = |path: &std::path::Path| {
            format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
        };
        let wrapper = wrapper_dir.join("git");
        std::fs::write(
            &wrapper,
            format!(
                r#"#!/bin/sh
previous=''
for arg in "$@"; do
  if [ "$previous" = "worktree" ] && [ "$arg" = "add" ]; then
    : > {started}
    printf '%s\n' worktree-add >> {log}
    while [ ! -f {release} ]; do
      sleep 0.01
    done
    break
  fi
  previous="$arg"
done
exec {git} "$@"
"#,
                started = shell_quote(&worktree_add_started),
                log = shell_quote(&worktree_add_log),
                release = shell_quote(&worktree_add_release),
                git = shell_quote(&real_git),
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&wrapper).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&wrapper, permissions).unwrap();

        let old_path = std::env::var_os("PATH").expect("PATH must be set for git fixtures");
        let mut path_entries = vec![wrapper_dir];
        path_entries.extend(std::env::split_paths(&old_path));
        let _env = ScopedEnv::set(vec![
            ("PATH", std::env::join_paths(path_entries).unwrap()),
            (
                "ACP_PRODUCT_WORKSPACE_ROOT",
                workspace_root_a.clone().into_os_string(),
            ),
        ]);

        let suffix = uuid::Uuid::new_v4();
        let store_a = Arc::new(
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:00Z".to_string()).unwrap(),
        );
        let store_b = Arc::new(
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:01Z".to_string()).unwrap(),
        );
        let key = format!("rec-pg-worktree-lock-{suffix}");
        let validated =
            Arc::new(validate_intake(&intake(&repo, &rev, &key), "local", "default").unwrap());

        let admitting_store = Arc::clone(&store_a);
        let admitting_intake = Arc::clone(&validated);
        let admit = thread::spawn(move || {
            admitting_store.admit_product_task(&admitting_intake, "pg-active-admit")
        });

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !worktree_add_started.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "PostgreSQL admit did not reach delayed worktree creation"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let preparing_task = store_a
            .get_product_task_by_idempotency("local", "default", &key)
            .unwrap()
            .expect("reserved PostgreSQL task");
        let task_id = preparing_task["task_id"].as_str().unwrap().to_string();
        assert_eq!(
            preparing_task["status"].as_str(),
            Some(ProductTaskStatus::WorkspacePreparing.as_str())
        );

        // A second worker may have a different local configuration, but it
        // cannot adopt it: the durable receipt pins this preparation to A.
        // The active owner has already passed its final read-only validation
        // before the wrapper observes `git worktree add`, so this switch
        // cannot redirect that in-flight physical operation.
        std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", &workspace_root_b);
        let recovering_store = Arc::clone(&store_b);
        let recovering_task_id = task_id.clone();
        let recover = thread::spawn(move || {
            recovering_store.recover_product_task_workspace(&recovering_task_id, "pg-recovery")
        });
        let recovery_error = recover
            .join()
            .expect("PostgreSQL recovery thread must not panic")
            .expect_err("root drift must require reconciliation before a second worktree effect");
        assert!(recovery_error.contains("configured root does not match the receipt"));
        assert!(
            !workspace_root_b.exists(),
            "root drift must not create the newly configured root"
        );
        let worktree_add_count = std::fs::read_to_string(&worktree_add_log)
            .unwrap_or_default()
            .lines()
            .count();
        assert_eq!(
            worktree_add_count, 1,
            "root drift must not enter a second local-root worktree mutation"
        );

        std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", &workspace_root_a);
        std::fs::write(&worktree_add_release, "release\n").unwrap();
        let admitted = admit
            .join()
            .unwrap()
            .expect("PostgreSQL admit must succeed");
        let recovered = store_b
            .recover_product_task_workspace(&task_id, "pg-recovery")
            .expect("matching root must reuse the bound task");
        let completed_worktree_add_count = std::fs::read_to_string(&worktree_add_log)
            .unwrap_or_default()
            .lines()
            .count();
        assert_eq!(completed_worktree_add_count, 1);
        assert_eq!(admitted["task_id"], task_id);
        assert_eq!(recovered["task_id"], task_id);
        assert_eq!(
            recovered["status"].as_str(),
            Some(ProductTaskStatus::WorkspaceBound.as_str())
        );
    });
}

#[cfg(feature = "pg-tests")]
#[test]
fn postgres_managed_token_budget_exhaustion_blocks_verification_and_artifact() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let store =
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:00Z".to_string()).unwrap();
        assert_managed_token_budget_exhaustion(
            &store,
            &repo,
            &rev,
            &format!("rec-managed-token-budget-pg-{}", uuid::Uuid::new_v4()),
        );
    });
}

#[cfg(feature = "pg-tests")]
#[test]
fn postgres_managed_missing_usage_blocks_all_product_effects() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let store =
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:00Z".to_string()).unwrap();
        assert_managed_missing_usage_fails_closed(
            &store,
            &repo,
            &rev,
            &format!("rec-managed-missing-usage-pg-{}", uuid::Uuid::new_v4()),
        );
    });
}

#[cfg(feature = "pg-tests")]
#[test]
fn postgres_managed_call_budget_denies_second_executor_call() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let store = Arc::new(
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:00Z".to_string()).unwrap(),
        );
        assert_managed_call_budget_denies_second_call(
            store,
            &repo,
            &rev,
            &format!("rec-managed-call-budget-pg-{}", uuid::Uuid::new_v4()),
        );
    });
}

#[cfg(feature = "pg-tests")]
#[test]
fn postgres_product_admission_audit_failure_rolls_back_private_objective() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let key = format!("rec-product-admit-audit-pg-{}", uuid::Uuid::new_v4());
        let request = intake(&repo, &rev, &key);
        let validated = validate_intake(&request, "local", "default").unwrap();
        let store =
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:00Z".to_string()).unwrap();
        let suffix: String = uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(12)
            .collect();
        let function_name = format!("reject_product_admit_audit_{suffix}");
        let trigger_name = format!("reject_product_admit_audit_trigger_{suffix}");
        let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        client
            .batch_execute(&format!(
                "CREATE FUNCTION {function_name}() RETURNS trigger
                 LANGUAGE plpgsql AS $$
                 BEGIN
                   IF NEW.action = 'product_task.admit' THEN
                     RAISE EXCEPTION 'injected product admission audit failure';
                   END IF;
                   RETURN NEW;
                 END;
                 $$;
                 CREATE TRIGGER {trigger_name}
                 BEFORE INSERT ON audit_log
                 FOR EACH ROW EXECUTE FUNCTION {function_name}();"
            ))
            .unwrap();
        let result = store.admit_product_task(&validated, "tester");
        let persisted = store
            .get_product_task_by_idempotency("local", "default", &key)
            .unwrap();
        client
            .batch_execute(&format!(
                "DROP TRIGGER {trigger_name} ON audit_log;
                 DROP FUNCTION {function_name}();"
            ))
            .unwrap();
        let error = result.expect_err("audit failure must roll back admission");
        assert!(
            error.contains("injected product admission audit failure")
                || error.contains("db error"),
            "unexpected PostgreSQL admission audit error: {error}"
        );
        assert!(persisted.is_none());
    });
}

#[cfg(feature = "pg-tests")]
#[test]
fn postgres_workspace_transition_audit_failure_keeps_task_admitted() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let key = format!("rec-product-transition-audit-pg-{}", uuid::Uuid::new_v4());
        let validated = validate_intake(&intake(&repo, &rev, &key), "local", "default").unwrap();
        let store =
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:00Z".to_string()).unwrap();
        let suffix: String = uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(12)
            .collect();
        let function_name = format!("reject_product_transition_audit_{suffix}");
        let trigger_name = format!("reject_product_transition_audit_trigger_{suffix}");
        let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        client
            .batch_execute(&format!(
                "CREATE FUNCTION {function_name}() RETURNS trigger
                 LANGUAGE plpgsql AS $$
                 BEGIN
                   IF NEW.action = 'product_task.transition' THEN
                     RAISE EXCEPTION 'injected product transition audit failure';
                   END IF;
                   RETURN NEW;
                 END;
                 $$;
                 CREATE TRIGGER {trigger_name}
                 BEFORE INSERT ON audit_log
                 FOR EACH ROW EXECUTE FUNCTION {function_name}();"
            ))
            .unwrap();
        let result = store.admit_product_task(&validated, "tester");
        let persisted = store
            .get_product_task_by_idempotency("local", "default", &key)
            .unwrap()
            .expect("admission reservation survives failed transition");
        client
            .batch_execute(&format!(
                "DROP TRIGGER {trigger_name} ON audit_log;
                 DROP FUNCTION {function_name}();"
            ))
            .unwrap();
        let error = result.expect_err("transition audit failure must reject worktree preparation");
        assert!(
            error.contains("injected product transition audit failure")
                || error.contains("db error"),
            "unexpected PostgreSQL transition audit error: {error}"
        );
        assert_eq!(
            persisted["status"].as_str(),
            Some(ProductTaskStatus::Admitted.as_str()),
            "task transition and transition audit must commit atomically"
        );
        let task_id = persisted["task_id"]
            .as_str()
            .expect("admission reservation task id")
            .to_string();
        let provisional = engine::product_golden_path::provisional_run_id_for_task(&task_id);
        assert!(
            store
                .get_supervised_patch_workspace_for_run(&provisional)
                .unwrap()
                .is_none(),
            "a failed admitted-to-preparing transition must not leave a workspace for its task"
        );
    });
}

#[cfg(feature = "pg-tests")]
#[test]
fn postgres_restart_accumulates_failed_managed_attempt_usage() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let suffix = uuid::Uuid::new_v4();
        let store =
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:00Z".to_string()).unwrap();
        let (task_id, run_id) = prepare_cumulative_managed_task(
            &store,
            &repo,
            &rev,
            &format!("rec-managed-cumulative-pg-{suffix}"),
        );
        drop(store);

        let restarted =
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:01Z".to_string()).unwrap();
        finish_cumulative_managed_task(&restarted, &task_id, &run_id);
        assert_eq!(run_git_head(&repo), rev);
        assert!(git_status_paths(&repo).is_empty());
    });
}

#[test]
fn remaining_elapsed_budget_caps_the_running_command_timeout() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let mut request = intake(&repo, &rev, "rec-verification-running-budget");
        request.verification_commands = vec![ProductVerificationCommand {
            command: "tail -f README.md".to_string(),
            timeout_ms: 3_600_000,
        }];
        request.budget = Some(ProductTaskBudget {
            total_elapsed_ms: Some(2_500),
            ..ProductTaskBudget::default()
        });
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let compiled = compile(&store, &task_id);
        complete_run(&store, compiled["task"]["run_id"].as_str().unwrap());
        let started = std::time::Instant::now();
        let finalized = store
            .finalize_product_task_after_execution_with_authority(&task_id, "verifier", &|| {
                Ok(running_scheduler_authority())
            })
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        let attempt = &finalized["verification"]["verification_attempts"][0];
        assert!(attempt["effective_timeout_ms"].as_u64().unwrap() <= 2_500);
        assert_eq!(attempt["declared_timeout_ms"], 3_600_000);
        assert!(finalized["artifact_id"].is_null());
    });
}

#[test]
fn scheduler_kill_at_artifact_commit_boundary_prevents_artifact() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, _, _) =
            ready_for_slow_verification(&store, &repo, &rev, "rec-kill-during-artifact", "true");
        let calls = AtomicUsize::new(0);
        let finalized = store
            .finalize_product_task_after_execution_with_authority(&task_id, "verifier", &|| {
                let mut authority = running_scheduler_authority();
                authority.scheduler_killed = calls.fetch_add(1, Ordering::SeqCst) >= 3;
                Ok(authority)
            })
            .unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "killed");
        assert!(finalized["artifact_id"].is_null());
        assert!(store.supervised_patch_artifacts(100).unwrap().is_empty());
    });
}

#[test]
fn sqlite_artifact_audit_failure_rolls_back_artifact_workspace_and_task() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, workspace_id, _) =
            ready_for_slow_verification(&store, &repo, &rev, "rec-artifact-audit-rollback", "true");
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_product_artifact_audit
                 BEFORE INSERT ON audit_log
                 WHEN NEW.action = 'supervised_patch.artifact_record'
                 BEGIN SELECT RAISE(ABORT, 'injected artifact audit failure'); END;",
            )
            .unwrap();
        let error = store
            .finalize_product_task_after_execution_with_authority(&task_id, "verifier", &|| {
                Ok(running_scheduler_authority())
            })
            .expect_err("artifact audit failure must abort the whole transaction");
        assert!(error.contains("injected artifact audit failure"));
        assert_eq!(
            store.get_product_task(&task_id).unwrap().unwrap()["status"],
            "verifying"
        );
        assert_ne!(
            store
                .get_supervised_patch_workspace(&workspace_id)
                .unwrap()
                .unwrap()["status"],
            "patch_prepared"
        );
        assert!(store.supervised_patch_artifacts(100).unwrap().is_empty());
        connection
            .execute_batch("DROP TRIGGER fail_product_artifact_audit")
            .unwrap();
        let retry = store
            .finalize_product_task_after_execution_with_authority(&task_id, "verifier", &|| {
                Ok(running_scheduler_authority())
            })
            .unwrap();
        assert_eq!(retry["phase"], "awaiting_approval");
        assert_eq!(store.supervised_patch_artifacts(100).unwrap().len(), 1);
    });
}

#[test]
fn pause_during_verification_rejects_late_result_without_artifact() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, run_id, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-pause-verification",
            "tail -f README.md",
        );
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task_id = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task_id,
                "verifier",
                &|| Ok(running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        store
            .update_run_pause_reason(&run_id, Some("operator_hold"))
            .unwrap();
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "paused");
        assert!(finalized["artifact_id"].is_null());
        assert_eq!(
            finalized["verification"]["verification_attempts"][0]["result_status"],
            "stale_rejected"
        );
        assert_eq!(
            finalized["verification"]["verification_attempts"][0]["late_result_rejected"],
            true
        );
    });
}

#[test]
fn scheduler_kill_during_verification_rejects_late_result() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-kill-verification",
            "tail -f README.md",
        );
        let killed = Arc::new(AtomicBool::new(false));
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task_id = task_id.clone();
        let finalizer_killed = Arc::clone(&killed);
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task_id,
                "verifier",
                &|| {
                    let mut authority = running_scheduler_authority();
                    authority.scheduler_killed = finalizer_killed.load(Ordering::SeqCst);
                    Ok(authority)
                },
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        killed.store(true, Ordering::SeqCst);
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "killed");
        assert!(finalized["artifact_id"].is_null());
    });
}

#[test]
fn lease_attempt_change_during_verification_blocks_late_result() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, run_id, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-lease-loss-verification",
            "tail -f README.md",
        );
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task_id = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task_id,
                "verifier",
                &|| Ok(running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "UPDATE workflow_run_nodes SET attempt_count = attempt_count + 1 WHERE run_id = ?1",
                [&run_id],
            )
            .unwrap();
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "blocked");
        assert!(finalized["artifact_id"].is_null());
        assert!(finalized["verification"]["authority_loss_reason"]
            .as_str()
            .unwrap()
            .contains("node_attempt_or_lease_superseded"));
    });
}

#[test]
fn task_version_change_during_verification_blocks_late_result() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-version-change-verification",
            "tail -f README.md",
        );
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task_id = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task_id,
                "verifier",
                &|| Ok(running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "UPDATE product_tasks SET version = version + 1 WHERE task_id = ?1",
                [&task_id],
            )
            .unwrap();
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "blocked");
        assert!(finalized["artifact_id"].is_null());
    });
}

#[test]
fn task_kill_during_verification_preserves_killed_state_and_rejects_result() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-task-kill-verification",
            "tail -f README.md",
        );
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task_id = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task_id,
                "verifier",
                &|| Ok(running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "UPDATE product_tasks SET status = 'killed', version = version + 1 WHERE task_id = ?1",
                [&task_id],
            )
            .unwrap();
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "killed");
        assert!(finalized["artifact_id"].is_null());
    });
}

#[test]
fn workspace_replacement_during_verification_is_quarantined() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, workspace_id, workspace_path) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-workspace-replaced-verification",
            "tail -f README.md",
        );
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task_id = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task_id,
                "verifier",
                &|| Ok(running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        let moved = format!("{workspace_path}.replaced");
        std::fs::rename(&workspace_path, &moved).unwrap();
        std::fs::create_dir(&workspace_path).unwrap();
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "blocked");
        assert!(finalized["artifact_id"].is_null());
        assert_eq!(
            store
                .get_supervised_patch_workspace(&workspace_id)
                .unwrap()
                .unwrap()["status"],
            "quarantined"
        );
    });
}

#[test]
fn verification_filesystem_write_is_quarantined_and_never_captured() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let store = Arc::new(store);
        let (task_id, _, workspace_id, workspace_path) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-late-write-verification",
            "tail -f README.md",
        );
        let tool_passes_before = tool_policy_pass_count(&store);
        let finalizer_store = Arc::clone(&store);
        let finalizer_task = task_id.clone();
        let handle = thread::spawn(move || {
            finalizer_store.finalize_product_task_after_execution_with_authority(
                &finalizer_task,
                "verifier",
                &|| Ok(running_scheduler_authority()),
            )
        });
        wait_for_new_tool_policy_pass(&store, tool_passes_before);
        std::fs::write(
            std::path::Path::new(&workspace_path).join("README.md"),
            "late write\n",
        )
        .unwrap();
        let finalized = handle.join().unwrap().unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "blocked");
        assert!(finalized["artifact_id"].is_null());
        assert_eq!(
            store
                .get_supervised_patch_workspace(&workspace_id)
                .unwrap()
                .unwrap()["status"],
            "quarantined"
        );
    });
}

#[test]
fn global_kill_before_verification_runs_no_command_or_capture() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (task_id, _, _, _) = ready_for_slow_verification(
            &store,
            &repo,
            &rev,
            "rec-global-kill-verification",
            "true",
        );
        let finalized = store
            .finalize_product_task_after_execution_with_authority(&task_id, "verifier", &|| {
                let mut authority = running_scheduler_authority();
                authority.global_kill_active = true;
                Ok(authority)
            })
            .unwrap();
        assert_eq!(finalized["phase"], "verification_authority_lost");
        assert_eq!(finalized["task"]["status"], "killed");
        assert!(finalized["artifact_id"].is_null());
        assert_eq!(
            finalized["verification"]["verification_attempts"][0]["late_result_rejected"],
            false
        );
    });
}
