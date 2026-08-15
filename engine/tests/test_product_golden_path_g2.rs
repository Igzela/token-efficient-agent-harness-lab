//! G2 tests: executable graph compile + scheduler-eligible product task runs.
//! Finalize must not drive executor ticks; the existing tick/scheduler path does.

use engine::node_executor::{
    NodeExecutionInput, NodeExecutionOutput, NodeExecutor, ProcessBoundaryMapping,
    ProcessEffectState, ProcessOutcome, ProcessOutcomeState,
};
use engine::product_golden_path::{
    validate_intake, ProductExecutorPolicy, ProductTaskBudget, ProductTaskIntakeRequest,
    ProductTaskStatus, ProductVerificationCommand, FIXTURE_DETERMINISTIC_NOTE_CONTENT,
    PRODUCT_TASK_GATE,
};
use engine::storage::local_product_store::LocalProductStore;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_gates<R>(f: impl FnOnce() -> R) -> R {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let runtime_dir = tempfile::tempdir().expect("runtime fixture directory");
    let _runtime = admit_fake_codex_runtime(runtime_dir.path());
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

struct EnvironmentGuard(Vec<(&'static str, Option<OsString>)>);

impl EnvironmentGuard {
    fn set(values: &[(&'static str, String)]) -> Self {
        let prior = values
            .iter()
            .map(|(key, _)| (*key, std::env::var_os(key)))
            .collect();
        for (key, value) in values {
            std::env::set_var(key, value);
        }
        Self(prior)
    }
}

impl Drop for EnvironmentGuard {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

fn admit_fake_codex_runtime(root: &std::path::Path) -> EnvironmentGuard {
    let binary = root.join("codex-fixture");
    std::fs::write(
        &binary,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'codex-cli 0.146.7'; else echo '--json --sandbox workspace-write --ask-for-approval --model'; fi\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let binary_sha256 = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
    EnvironmentGuard::set(&[
        ("ACP_ENABLE_CLI_EXECUTION", "1".to_string()),
        ("ACP_CODEX_BIN", binary.to_string_lossy().into_owned()),
        ("ACP_CODEX_SHA256", binary_sha256),
        ("ACP_CODEX_VERSION_POLICY", ">=0.146.0,<0.147.0".to_string()),
        ("ACP_CODEX_REQUIRED_CAPABILITIES", "--sandbox".to_string()),
        (
            "ACP_CODEX_RUNTIME_PROFILE_ID",
            "codex-g2-fixture.v1".to_string(),
        ),
        ("ACP_CODEX_MODEL", "gpt-5.6-luna".to_string()),
    ])
}

fn init_git_repo(root: &std::path::Path) -> String {
    std::fs::create_dir_all(root).unwrap();
    // Base tree has only README — expected mutation path must not pre-exist.
    std::fs::write(root.join("README.md"), "hello\n").unwrap();
    run_git(root, &["init", "-b", "main"]);
    run_git(root, &["config", "user.email", "g2@example.com"]);
    run_git(root, &["config", "user.name", "G2 Tester"]);
    run_git(root, &["add", "README.md"]);
    run_git(root, &["commit", "-m", "init"]);
    run_git(
        root,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/g2-product.git",
        ],
    );
    run_git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

fn run_git(cwd: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn sample_intake(target: &std::path::Path, rev: &str, key: &str) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "Create docs/product_golden_path_fixture.md via fixture executor.".to_string(),
        target_id: "disposable-target".to_string(),
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
fn process_boundary_mapping_is_exhaustive_and_fail_closed() {
    let cases = [
        (
            ProcessOutcome::exited(0),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownSuccess,
        ),
        (
            ProcessOutcome::exited(7),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::signaled(None),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("spawn_failed", None, "refused before spawn"),
            ProcessEffectState::NotStarted,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure(
                "process_tree_containment_unavailable",
                None,
                "containment unavailable",
            ),
            ProcessEffectState::NotStarted,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure(
                "process_tree_containment_unsupported",
                None,
                "containment unsupported",
            ),
            ProcessEffectState::NotStarted,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("invalid_output_limits", None, "invalid limits"),
            ProcessEffectState::NotStarted,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("timed_out", None, "deadline exceeded"),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("output_read_failed", Some(1), "reader failed"),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("timeout", None, "deadline exceeded"),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("wait_failed", None, "wait failed"),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("stdout_reader_failed", None, "stdout reader failed"),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("stderr_reader_failed", None, "stderr reader failed"),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("combined_reader_failed", None, "combined reader failed"),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure(
                "process_tree_cleanup_failed",
                None,
                "process cleanup failed",
            ),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("output_limit_exceeded", None, "output limit exceeded"),
            ProcessEffectState::Started,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::unavailable("provider has no process owner"),
            ProcessEffectState::Unknown,
            ProcessOutcomeState::Unknown,
        ),
        (
            ProcessOutcome::unavailable("command rejected before process spawn"),
            ProcessEffectState::NotStarted,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::unavailable("empty command rejected before process spawn"),
            ProcessEffectState::NotStarted,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::unavailable("workspace rejected before process spawn"),
            ProcessEffectState::NotStarted,
            ProcessOutcomeState::KnownFailure,
        ),
        (
            ProcessOutcome::failure("future_state", None, "not in this contract"),
            ProcessEffectState::Unknown,
            ProcessOutcomeState::Unknown,
        ),
    ];

    for (outcome, effect, state) in cases {
        let mapping = outcome.boundary_mapping();
        assert_eq!(mapping.effect, effect, "state={}", outcome.state);
        assert_eq!(mapping.outcome, state, "state={}", outcome.state);
        assert_eq!(
            mapping.is_known_success(),
            state == ProcessOutcomeState::KnownSuccess
        );
    }

    let missing = ProcessBoundaryMapping::unknown();
    assert_eq!(missing.effect, ProcessEffectState::Unknown);
    assert_eq!(missing.outcome, ProcessOutcomeState::Unknown);
    assert!(!missing.is_known_success());

    let contradictory = ProcessBoundaryMapping {
        effect: ProcessEffectState::Unknown,
        outcome: ProcessOutcomeState::KnownSuccess,
    };
    assert!(!contradictory.is_known_success());
}

#[test]
fn process_boundary_mapping_does_not_change_process_outcome_serialization() {
    let outcome = ProcessOutcome::failure("spawn_failed", None, "bounded reason");
    let encoded = serde_json::to_value(&outcome).unwrap();
    assert_eq!(
        encoded,
        serde_json::json!({
            "schema_version": "process_outcome.v1",
            "state": "spawn_failed",
            "exit_code": null,
            "signal": null,
            "unavailable_reason": "bounded reason"
        })
    );
    let decoded: ProcessOutcome = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, outcome);
}

#[test]
fn serialized_process_outcome_caller_gate_remains_fail_closed() {
    let cases = [
        (ProcessOutcome::exited(0), true),
        (
            ProcessOutcome::failure("spawn_failed", None, "did not start"),
            false,
        ),
        (
            ProcessOutcome::unavailable("provider has no process owner"),
            false,
        ),
    ];

    for (outcome, expected_success) in cases {
        let decoded: ProcessOutcome =
            serde_json::from_value(serde_json::to_value(outcome).unwrap()).unwrap();
        assert_eq!(
            decoded.boundary_mapping().is_known_success(),
            expected_success,
            "state={}",
            decoded.state
        );
    }
}

struct CapturingManagedExecutor {
    prompt: Arc<Mutex<Option<String>>>,
}

impl NodeExecutor for CapturingManagedExecutor {
    fn executor_type_name(&self) -> &str {
        "codex_cli"
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        *self.prompt.lock().unwrap() = input
            .node_metadata
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "codex_cli".to_string(),
            output: Some("bounded managed test".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens: Some(10),
            output_tokens: Some(2),
            estimated_cost: None,
            latency_ms: Some(10),
            process_outcome: Some(ProcessOutcome::exited(0)),
            resolved_model: None,
        }
    }
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
        assert_eq!(result["executor_class"], "fixture_deterministic");
        let task = &result["task"];
        // Must stay graph_ready — not running merely because a run was created.
        assert_eq!(
            task["status"].as_str(),
            Some(ProductTaskStatus::GraphReady.as_str())
        );
        assert!(task["plan_id"].as_str().is_some());
        assert!(task["run_id"].as_str().is_some());
        let run_id = task["run_id"].as_str().unwrap();
        let run = store.get_workflow_run(run_id).unwrap().expect("run");
        let nodes = run["nodes"].as_array().expect("nodes");
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["product_task_id"], task_id);
        assert_eq!(nodes[0]["task_type"], "command");
        assert_eq!(nodes[0]["executor_class"], "fixture_deterministic");
        assert_eq!(
            nodes[0]["product_apply_binding_schema_version"],
            "product_apply_binding.v2"
        );
        assert_eq!(nodes[0]["product_budget"]["total_tokens"], 50_000);
        assert!(nodes[0]["workspace_path"].as_str().is_some());
        assert!(nodes[0].get("managed_supervised_patch").is_some());
        let ws_id = task["workspace_record_id"].as_str().unwrap();
        let ws = store
            .get_supervised_patch_workspace(ws_id)
            .unwrap()
            .unwrap();
        assert_eq!(ws["run_id"].as_str(), Some(run_id));
    });
}

fn prepare_managed_long_objective(
    store: &LocalProductStore,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
) -> (String, String, String) {
    let objective = format!(
            "Create docs/product_golden_path_fixture.md and preserve this exact bounded context: {} END-OF-OBJECTIVE",
            "managed-context-".repeat(40)
        );
    assert!(objective.len() > 256);
    let mut request = sample_intake(repo, rev, key);
    request.objective = objective.clone();
    request.executor_policy = ProductExecutorPolicy {
        allowed_executors: vec!["codex_cli".to_string()],
        prefer: Some("codex_cli".to_string()),
    };
    let validated = validate_intake(&request, "local", "default").unwrap();
    let task = store.admit_product_task(&validated, "tester").unwrap();
    assert!(task["intake"].get("_execution_objective_v1").is_none());
    let objective_preview = task["intake"]["objective_preview"].clone();
    let task_id = task["task_id"].as_str().unwrap();
    let compiled = store
        .compile_and_schedule_product_task(task_id, "tester", &["codex_cli".to_string()])
        .unwrap();
    let plan = store
        .get_workflow_plan(compiled["task"]["plan_id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_ne!(plan["raw_request"], objective);
    assert_eq!(plan["raw_request"], objective_preview);

    let public_task = store.get_product_task(task_id).unwrap().unwrap();
    assert!(public_task["intake"]
        .get("_execution_objective_v1")
        .is_none());
    (
        objective,
        task_id.to_string(),
        compiled["task"]["run_id"].as_str().unwrap().to_string(),
    )
}

fn assert_managed_long_objective_delivery(
    store: &LocalProductStore,
    objective: &str,
    task_id: &str,
    run_id: &str,
) {
    let captured = Arc::new(Mutex::new(None));
    let executor = CapturingManagedExecutor {
        prompt: captured.clone(),
    };
    let tick = store
        .tick_with_executor(run_id, "scheduler", 0, &executor)
        .unwrap();
    assert_eq!(tick["run"]["status"], "completed");
    assert_eq!(captured.lock().unwrap().as_deref(), Some(objective));
    let public_task = store.get_product_task(task_id).unwrap().unwrap();
    assert!(public_task["intake"]
        .get("_execution_objective_v1")
        .is_none());
}

#[test]
fn managed_executor_receives_exact_long_objective_without_public_persistence() {
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("store.db");
        let store = LocalProductStore::new(&db_path).unwrap();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let (objective, task_id, run_id) =
            prepare_managed_long_objective(&store, &repo, &rev, "g2-long-managed-objective");
        drop(store);
        let restarted = LocalProductStore::new(&db_path).unwrap();
        assert_managed_long_objective_delivery(&restarted, &objective, &task_id, &run_id);
    });
}

#[cfg(feature = "pg-tests")]
#[test]
fn postgres_managed_executor_receives_exact_long_objective_without_public_persistence() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    with_gates(|| {
        let dir = tempfile::tempdir().unwrap();
        let workspace_root = dir.path().join("product-workspaces");
        std::env::set_var("ACP_PRODUCT_WORKSPACE_ROOT", &workspace_root);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let store =
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:00Z".to_string()).unwrap();
        let (objective, task_id, run_id) = prepare_managed_long_objective(
            &store,
            &repo,
            &rev,
            &format!("g2-long-managed-objective-pg-{}", uuid::Uuid::new_v4()),
        );
        drop(store);
        let restarted =
            LocalProductStore::new_postgres(&url, || "2026-07-22T12:00:01Z".to_string()).unwrap();
        assert_managed_long_objective_delivery(&restarted, &objective, &task_id, &run_id);
        std::env::remove_var("ACP_PRODUCT_WORKSPACE_ROOT");
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
fn compile_is_idempotent_when_already_graph_ready() {
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
        assert_eq!(
            first["task"]["status"].as_str(),
            Some(ProductTaskStatus::GraphReady.as_str())
        );
    });
}

#[test]
fn scheduler_tick_executes_command_in_bound_worktree() {
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
        let ws = result["task"]["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap()
            .to_string();
        let expected = std::path::Path::new(&ws).join("docs/product_golden_path_fixture.md");
        assert!(
            !expected.exists(),
            "mutation target must not exist before tick"
        );

        // Existing scheduler tick path — not finalize.
        let executor = engine::node_executor::CommandNodeExecutor::default();
        let mut completed = false;
        for _ in 0..8 {
            let tick = store
                .tick_with_executor(run_id, "tester", 1, &executor)
                .expect("tick");
            let action = tick.get("action").and_then(|v| v.as_str()).unwrap_or("");
            let run_status = tick
                .pointer("/run/status")
                .and_then(|v| v.as_str())
                .or_else(|| tick.get("status").and_then(|v| v.as_str()))
                .unwrap_or("");
            if matches!(action, "completed") || matches!(run_status, "completed") {
                completed = true;
                break;
            }
            if matches!(action, "failed") || matches!(run_status, "failed") {
                panic!("tick failed: {tick}");
            }
        }
        assert!(completed, "scheduler tick must reach completed run");
        let run = store.get_workflow_run(run_id).unwrap().unwrap();
        assert_eq!(run["status"].as_str(), Some("completed"));
        assert!(
            expected.exists(),
            "fixture file must be created by command node"
        );
        let content = std::fs::read_to_string(&expected).unwrap();
        assert_eq!(content, FIXTURE_DETERMINISTIC_NOTE_CONTENT);
        // Target default branch unchanged.
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "hello\n"
        );
        assert!(!repo.join("docs/product_golden_path_fixture.md").exists());
    });
}

#[cfg(unix)]
#[test]
fn claude_fixture_does_not_bypass_managed_admission() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let binary = dir.path().join("claude-2.1.217");
        std::fs::write(
            &binary,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '2.1.217 (Claude Code)\\n'; exit 0; fi\nmkdir -p docs\nprintf 'managed fixture\\n' > docs/product_golden_path_fixture.md\nprintf '%s\\n' '{\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"done\",\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"total_cost_usd\":0.01,\"modelUsage\":{\"claude-haiku-4-5-20251001\":{\"costUSD\":0.01}}}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        for (key, value) in [
            ("ACP_ENABLE_CLI_EXECUTION", "1"),
            ("ACP_ENABLE_CLAUDE_CODE_EXECUTION", "1"),
            ("ACP_CLAUDE_CODE_VERSION", "2.1.217"),
            ("ACP_CLAUDE_MODEL", "claude-haiku-4-5-20251001"),
            ("ACP_CLAUDE_MAX_TURNS", "3"),
            ("ACP_CLAUDE_MAX_BUDGET_USD", "2.16"),
        ] {
            std::env::set_var(key, value);
        }
        std::env::set_var("ACP_CLAUDE_CODE_BIN", &binary);
        std::env::set_var("ACP_CLAUDE_CODE_SHA256", &digest);

        // A fixture binary can exercise the parser shape, but it cannot prove
        // real Claude filesystem confinement. Runtime admission must therefore
        // remain disabled until provider-independent mediation is accepted.
        let admission_probe = engine::cli::CliConfig::from_env();
        if !admission_probe.claude_code_enabled {
            assert!(admission_probe.claude_code_admission.is_none());
            for key in [
                "ACP_ENABLE_CLI_EXECUTION",
                "ACP_ENABLE_CLAUDE_CODE_EXECUTION",
                "ACP_CLAUDE_CODE_BIN",
                "ACP_CLAUDE_CODE_VERSION",
                "ACP_CLAUDE_CODE_SHA256",
                "ACP_CLAUDE_MODEL",
                "ACP_CLAUDE_MAX_TURNS",
                "ACP_CLAUDE_MAX_BUDGET_USD",
            ] {
                std::env::remove_var(key);
            }
            return;
        }

        let mut request = sample_intake(&repo, &rev, "g2-claude-adapter");
        request.objective = "Create docs/product_golden_path_fixture.md".to_string();
        request.executor_policy = ProductExecutorPolicy {
            allowed_executors: vec!["claude_code_cli".to_string()],
            prefer: Some("claude_code_cli".to_string()),
        };
        request.budget = Some(ProductTaskBudget {
            total_tokens: Some(792_000),
            total_calls: Some(1),
            total_elapsed_ms: Some(60_000),
            max_retries: Some(0),
            max_repairs: Some(0),
            max_concurrency: Some(1),
            stage_budgets: None,
        });
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "tester", &["claude_code_cli".to_string()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        let config = engine::cli::CliConfig::from_env();
        let inner = engine::cli::CliNodeExecutor::from_config_for(&config, "claude_code_cli")
            .expect("exact admitted Claude executor");
        let executor = engine::tool_policy_executor::ToolPolicyNodeExecutor::cli(
            Arc::new(inner),
            Arc::clone(&store),
            "claude_code_cli",
        );
        let tick = store
            .tick_with_executor(run_id, "scheduler", 0, &executor)
            .unwrap();
        assert_eq!(tick["run"]["status"], "completed", "{tick}");
        let result = &tick["result"];
        assert_eq!(result["executor_type"], "claude_code_cli", "{tick}");
        assert_eq!(result["input_tokens"], 10);
        assert_eq!(result["estimated_cost"], 0.01);
        assert_eq!(
            result["resolved_model"].as_str(),
            Some("claude-haiku-4-5-20251001"),
            "pin-mode product path must persist resolved_model: {tick}"
        );
        let node = &tick["run"]["graph"]["nodes"][0];
        assert_eq!(node["managed_executor_identity"]["binary_sha256"], digest);
        assert_eq!(
            node["managed_executor_identity"]["model"].as_str(),
            Some("claude-haiku-4-5-20251001")
        );
        let workspace = compiled["task"]["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap();
        assert!(std::path::Path::new(workspace)
            .join("docs/product_golden_path_fixture.md")
            .is_file());
        let finalized = store
            .finalize_product_task_after_execution(task_id, "scheduler")
            .unwrap();
        assert_eq!(finalized["task"]["status"], "awaiting_approval");
        let task_version = finalized["task"]["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", task_version)
            .unwrap();
        let completed = store
            .output_product_task(
                task_id,
                "output-operator",
                task_version,
                approval["approval_id"].as_str(),
                true,
            )
            .unwrap();
        let evidence = &completed["terminal_evidence"];
        assert_eq!(evidence["node"]["executor_type"], "claude_code_cli");
        assert_eq!(evidence["usage"]["status"], "linked");
        assert_eq!(evidence["cost"]["status"], "unavailable");
        assert!(evidence["cost"]["reason"]
            .as_str()
            .unwrap()
            .contains("client-side estimate"));
        assert_eq!(
            evidence["node"]["managed_executor_identity"]["pricing_verified_at"],
            "2026-07-22"
        );
        assert_eq!(run_git(&repo, &["rev-parse", "HEAD"]).trim(), rev);

        for key in [
            "ACP_ENABLE_CLI_EXECUTION",
            "ACP_ENABLE_CLAUDE_CODE_EXECUTION",
            "ACP_CLAUDE_CODE_BIN",
            "ACP_CLAUDE_CODE_VERSION",
            "ACP_CLAUDE_CODE_SHA256",
            "ACP_CLAUDE_MODEL",
            "ACP_CLAUDE_MAX_TURNS",
            "ACP_CLAUDE_MAX_BUDGET_USD",
        ] {
            std::env::remove_var(key);
        }
    });
}

#[cfg(unix)]
#[test]
fn subscription_claude_fixture_does_not_bypass_managed_admission() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;

    with_gates(|| {
        let (dir, store) = temp_store();
        let store = Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let binary = dir.path().join("claude-subscription-2.1.217");
        std::fs::write(
            &binary,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then printf '2.1.217 (Claude Code)\\n'; exit 0; fi\nmkdir -p docs\nprintf 'managed fixture\\n' > docs/product_golden_path_fixture.md\nprintf '%s\\n' '{\"subtype\":\"success\",\"is_error\":false,\"num_turns\":1,\"result\":\"done\",\"usage\":{\"input_tokens\":10,\"output_tokens\":4},\"total_cost_usd\":0.0,\"modelUsage\":{\"subscription-claude-default\":{\"costUSD\":0.0}}}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let digest = hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()));
        // Subscription-default mode: no ACP_CLAUDE_MODEL; the admitted CLI resolves
        // its own configured default and must prove the resolved identity.
        std::env::remove_var("ACP_CLAUDE_MODEL");
        for (key, value) in [
            ("ACP_ENABLE_CLI_EXECUTION", "1"),
            ("ACP_ENABLE_CLAUDE_CODE_EXECUTION", "1"),
            ("ACP_CLAUDE_CODE_VERSION", "2.1.217"),
            ("ACP_CLAUDE_MAX_TURNS", "3"),
            ("ACP_CLAUDE_MAX_BUDGET_USD", "2.16"),
        ] {
            std::env::set_var(key, value);
        }
        std::env::set_var("ACP_CLAUDE_CODE_BIN", &binary);
        std::env::set_var("ACP_CLAUDE_CODE_SHA256", &digest);

        // Subscription/default model evidence from a fixture is not managed
        // acceptance and cannot bypass the confinement gate.
        let admission_probe = engine::cli::CliConfig::from_env();
        if !admission_probe.claude_code_enabled {
            assert!(admission_probe.claude_code_admission.is_none());
            for key in [
                "ACP_ENABLE_CLI_EXECUTION",
                "ACP_ENABLE_CLAUDE_CODE_EXECUTION",
                "ACP_CLAUDE_CODE_BIN",
                "ACP_CLAUDE_CODE_VERSION",
                "ACP_CLAUDE_CODE_SHA256",
                "ACP_CLAUDE_MODEL",
                "ACP_CLAUDE_MAX_TURNS",
                "ACP_CLAUDE_MAX_BUDGET_USD",
            ] {
                std::env::remove_var(key);
            }
            return;
        }

        let mut request = sample_intake(&repo, &rev, "g2-claude-subscription");
        request.objective = "Create docs/product_golden_path_fixture.md".to_string();
        request.executor_policy = ProductExecutorPolicy {
            allowed_executors: vec!["claude_code_cli".to_string()],
            prefer: Some("claude_code_cli".to_string()),
        };
        request.budget = Some(ProductTaskBudget {
            total_tokens: Some(792_000),
            total_calls: Some(1),
            total_elapsed_ms: Some(60_000),
            max_retries: Some(0),
            max_repairs: Some(0),
            max_concurrency: Some(1),
            stage_budgets: None,
        });
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "tester", &["claude_code_cli".to_string()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        let config = engine::cli::CliConfig::from_env();
        let inner = engine::cli::CliNodeExecutor::from_config_for(&config, "claude_code_cli")
            .expect("subscription-default admitted Claude executor");
        let executor = engine::tool_policy_executor::ToolPolicyNodeExecutor::cli(
            Arc::new(inner),
            Arc::clone(&store),
            "claude_code_cli",
        );
        let tick = store
            .tick_with_executor(run_id, "scheduler", 0, &executor)
            .unwrap();
        assert_eq!(tick["run"]["status"], "completed", "{tick}");
        let result = &tick["result"];
        assert_eq!(result["executor_type"], "claude_code_cli", "{tick}");
        assert_eq!(
            result["resolved_model"].as_str(),
            Some("subscription-claude-default"),
            "{tick}"
        );
        let node = &tick["run"]["graph"]["nodes"][0];
        assert!(node["managed_executor_identity"]["model"].is_null());
        assert_eq!(
            node["managed_executor_identity"]["model_resolution"].as_str(),
            Some("cli_subscription_default")
        );
        let finalized = store
            .finalize_product_task_after_execution(task_id, "scheduler")
            .unwrap();
        assert_eq!(finalized["task"]["status"], "awaiting_approval");
        let task_version = finalized["task"]["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", task_version)
            .unwrap();
        let completed = store
            .output_product_task(
                task_id,
                "output-operator",
                task_version,
                approval["approval_id"].as_str(),
                true,
            )
            .unwrap();
        let evidence = &completed["terminal_evidence"];
        assert_eq!(evidence["usage"]["status"], "linked");
        assert_eq!(
            evidence["usage"]["resolved_model"].as_str(),
            Some("subscription-claude-default")
        );
        assert_eq!(evidence["cost"]["status"], "unavailable");
        assert_eq!(run_git(&repo, &["rev-parse", "HEAD"]).trim(), rev);

        for key in [
            "ACP_ENABLE_CLI_EXECUTION",
            "ACP_ENABLE_CLAUDE_CODE_EXECUTION",
            "ACP_CLAUDE_CODE_BIN",
            "ACP_CLAUDE_CODE_VERSION",
            "ACP_CLAUDE_CODE_SHA256",
            "ACP_CLAUDE_MODEL",
            "ACP_CLAUDE_MAX_TURNS",
            "ACP_CLAUDE_MAX_BUDGET_USD",
        ] {
            std::env::remove_var(key);
        }
    });
}

#[test]
fn finalize_does_not_execute_nodes_before_scheduler() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = admit_bound(&store, &repo, &rev, "g2-no-finalize-tick");
        let task_id = task["task_id"].as_str().unwrap();
        store
            .compile_and_schedule_product_task(task_id, "tester", &["command".into()])
            .unwrap();
        let finalized = store
            .finalize_product_task_after_execution(task_id, "tester")
            .expect("finalize observe");
        assert_eq!(finalized["phase"], "waiting_for_scheduler");
        let run_id = finalized["run"]["run_id"]
            .as_str()
            .or_else(|| finalized["task"]["run_id"].as_str())
            .unwrap();
        let run = store.get_workflow_run(run_id).unwrap().unwrap();
        assert_ne!(run["status"].as_str(), Some("completed"));
        let ws = finalized["task"]["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap();
        assert!(!std::path::Path::new(ws)
            .join("docs/product_golden_path_fixture.md")
            .exists());
    });
}

#[test]
fn workflow_scheduler_advances_product_run_without_finalize_ticks() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let store = std::sync::Arc::new(store);
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = admit_bound(&store, &repo, &rev, "g2-sched-auto");
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let compiled = store
            .compile_and_schedule_product_task(&task_id, "tester", &["command".into()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap().to_string();
        let ws = compiled["task"]["workspace_binding"]["workspace_path"]
            .as_str()
            .unwrap()
            .to_string();

        let config = engine::scheduler::SchedulerConfig {
            interval_ms: 50,
            executor_type: "command".to_string(),
            supervised_workers_enabled: true,
            worker_count: 1,
            lease_timeout_ms: 120_000,
            ..Default::default()
        };
        let mut scheduler = engine::scheduler::WorkflowScheduler::new(store.clone(), config);
        scheduler.start().expect("scheduler start");

        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut completed = false;
        while std::time::Instant::now() < deadline {
            if let Ok(Some(run)) = store.get_workflow_run(&run_id) {
                if run.get("status").and_then(|v| v.as_str()) == Some("completed") {
                    completed = true;
                    break;
                }
                if matches!(
                    run.get("status").and_then(|v| v.as_str()),
                    Some("failed") | Some("cancelled") | Some("killed")
                ) {
                    let _ = scheduler.stop();
                    panic!("scheduler run failed: {run}");
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = scheduler.stop();
        assert!(
            completed,
            "existing scheduler must complete product run without finalize ticks"
        );
        assert!(std::path::Path::new(&ws)
            .join("docs/product_golden_path_fixture.md")
            .exists());
        let content = std::fs::read_to_string(
            std::path::Path::new(&ws).join("docs/product_golden_path_fixture.md"),
        )
        .unwrap();
        assert_eq!(content, FIXTURE_DETERMINISTIC_NOTE_CONTENT);
    });
}
