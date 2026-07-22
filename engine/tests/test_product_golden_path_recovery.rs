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

fn temp_store() -> (tempfile::TempDir, LocalProductStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("store.db")).unwrap();
    (dir, store)
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
        let mut ids = Vec::new();
        let mut errors = Vec::new();
        for h in handles {
            match h.join().unwrap() {
                Ok(task) => ids.push(task["task_id"].as_str().unwrap().to_string()),
                Err(e) => errors.push(e),
            }
        }
        // At least one admit must succeed; all successes share one task_id.
        assert!(
            !ids.is_empty(),
            "at least one concurrent admit must succeed; errors={errors:?}"
        );
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 1, "concurrent intake must collapse to one task");
        // Losers may surface as CAS conflicts only; not unrelated failures.
        for e in &errors {
            assert!(
                e.contains("stale")
                    || e.contains("conflict")
                    || e.contains("expected-current")
                    || e.contains("already exists")
                    || e.contains("retry exhausted"),
                "unexpected concurrent admit error: {e}"
            );
        }
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
