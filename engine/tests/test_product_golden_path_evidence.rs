//! Terminal evidence and export_patch path for product golden path.

use engine::node_executor::{
    NodeExecutionInput, NodeExecutionOutput, NodeExecutor, ProcessOutcome,
};
use engine::product_golden_path::{
    validate_intake, ProductExecutorPolicy, ProductTaskIntakeRequest, ProductTaskStatus,
    ProductVerificationCommand, PRODUCT_TASK_GATE,
};
use engine::storage::local_product_store::LocalProductStore;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::process::Command;
use std::sync::{Arc, Barrier, Mutex, OnceLock};

fn sha256_json(value: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(serde_json::to_vec(value).unwrap()))
}

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
            "codex-evidence-fixture.v1".to_string(),
        ),
        ("ACP_CODEX_MODEL", "gpt-5.6-luna".to_string()),
    ])
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
        .args([
            "remote",
            "add",
            "origin",
            "https://example.invalid/evidence-product.git",
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
    intent: &str,
) -> ProductTaskIntakeRequest {
    ProductTaskIntakeRequest {
        objective: "evidence path fixture".to_string(),
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

fn drive_to_awaiting_approval(
    store: &LocalProductStore,
    repo: &std::path::Path,
    rev: &str,
    key: &str,
    intent: &str,
) -> serde_json::Value {
    let validated = validate_intake(&intake(repo, rev, key, intent), "local", "default").unwrap();
    let admitted = store.admit_product_task(&validated, "tester").unwrap();
    let task_id = admitted["task_id"].as_str().unwrap();
    complete_to_approval(store, task_id);
    store.get_product_task(task_id).unwrap().unwrap()
}

struct ReceiptReportingManagedExecutor;

impl NodeExecutor for ReceiptReportingManagedExecutor {
    fn executor_type_name(&self) -> &str {
        "codex_cli"
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let workspace = input
            .node_metadata
            .get("workspace_path")
            .and_then(serde_json::Value::as_str)
            .expect("managed node workspace binding");
        let output_path =
            std::path::Path::new(workspace).join("docs/product_golden_path_fixture.md");
        std::fs::create_dir_all(output_path.parent().unwrap()).unwrap();
        std::fs::write(&output_path, "managed owner receipt fixture\n").unwrap();
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "codex_cli".to_string(),
            output: Some("bounded test executor completed".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens: Some(111),
            output_tokens: Some(23),
            estimated_cost: None,
            latency_ms: Some(1),
            process_outcome: Some(ProcessOutcome::unavailable(
                "in-process test executor has no OS process outcome",
            )),
            resolved_model: None,
        }
    }
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
            .approve_and_output_product_task(task_id, "tester", true)
            .unwrap();
        assert_eq!(
            done["task"]["status"].as_str(),
            Some(ProductTaskStatus::Completed.as_str())
        );
        let evidence = done["terminal_evidence"].as_object().expect("evidence");
        assert_eq!(
            evidence["schema_version"],
            "product_task_terminal_evidence.v2"
        );
        assert!(evidence["evidence_id"].as_str().is_some());
        assert_eq!(evidence["product_task_id"], task_id);
        assert!(evidence["run_id"].as_str().is_some());
        assert!(evidence["workspace_record_id"].as_str().is_some());
        assert_eq!(
            evidence["artifact"]["artifact_id"],
            done["artifact"]["artifact_id"]
        );
        assert_eq!(
            evidence["approval"]["approval_id"],
            done["approval"]["approval_id"]
        );
        assert_eq!(evidence["verification"]["trustworthy"], true);
        assert_eq!(
            evidence["verification"]["receipts"][0]["process_outcome"]["exit_code"],
            0
        );
        assert_eq!(evidence["node"]["executor_type"], "command");
        assert_eq!(evidence["node"]["executor_class"], "fixture_deterministic");
        assert_eq!(evidence["usage"]["status"], "unavailable");
        assert_eq!(evidence["cost"]["status"], "unavailable");
        assert_eq!(evidence["replay"]["status"], "unavailable");
        assert_ne!(evidence["replay"]["status"], "eligible_via_run");
        assert!(evidence["usage"]["reason"].as_str().is_some());
        assert!(evidence["cost"]["reason"].as_str().is_some());
        assert!(evidence["audit_reference"]["audit_id"].as_i64().is_some());
        assert!(evidence.get("workspace_path").is_none());
        let mut hash_input = serde_json::Value::Object(evidence.clone());
        hash_input["content_sha256"] = serde_json::Value::Null;
        assert_eq!(evidence["content_sha256"], sha256_json(&hash_input));
        let receipt = done["output_receipt"].as_object().expect("output receipt");
        assert_eq!(receipt["schema_version"], "product_output_receipt.v1");
        assert_eq!(receipt["artifact_id"], done["artifact"]["artifact_id"]);
        assert_eq!(receipt["approval_id"], done["approval"]["approval_id"]);
        let completed_task = store.get_product_task(task_id).unwrap().unwrap();
        let completed_version = completed_task["version"].as_u64().unwrap();
        assert!(store
            .output_product_task(
                task_id,
                "output-operator",
                completed_version,
                Some("wrong-approval"),
                true,
            )
            .unwrap_err()
            .contains("approval not found"));
        let reused = store
            .output_product_task(
                task_id,
                "output-operator",
                completed_version,
                done["approval"]["approval_id"].as_str(),
                true,
            )
            .unwrap();
        assert_eq!(reused["reused"], true);
        assert_eq!(
            reused["output_receipt"]["receipt_id"],
            receipt["receipt_id"]
        );
        // Idempotent pure reads and duplicate emission do not append audit rows.
        let audit_before_reads = store.audit_events(10_000).unwrap();
        let again = store.get_product_task_terminal_evidence(task_id).unwrap();
        let emitted_again = store
            .emit_product_task_terminal_evidence(task_id, "duplicate-emitter", None)
            .unwrap();
        assert_eq!(again, serde_json::Value::Object(evidence.clone()));
        assert_eq!(emitted_again, again);
        assert_eq!(store.audit_events(10_000).unwrap(), audit_before_reads);
        store
            .rollback_v35_to_v34("rollback-operator", true)
            .unwrap();
        store
            .rollback_v34_to_v33("rollback-operator", true)
            .unwrap();
        store
            .rollback_v33_to_v32("rollback-operator", true)
            .unwrap();
        store
            .rollback_v32_to_v31("rollback-operator", true)
            .expect("empty managed acceptance tables roll back to v31");
        let rollback_error = store
            .rollback_v31_to_v30("rollback-operator", true)
            .unwrap_err();
        assert!(
            rollback_error.contains("authoritative terminal evidence exists"),
            "{rollback_error}"
        );
        assert_eq!(store.schema_version().unwrap(), 31);
        // Target main unchanged
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "hello\n"
        );
    });
}

#[test]
fn terminal_evidence_uses_managed_executor_class_and_owner_reported_usage() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let _runtime = admit_fake_codex_runtime(dir.path());
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let mut request = intake(&repo, &rev, "ev-managed-classification-1", "artifact_only");
        request.executor_policy.allowed_executors = vec!["codex_cli".to_string()];
        request.executor_policy.prefer = Some("codex_cli".to_string());
        let validated = validate_intake(&request, "local", "default").unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "tester", &["codex_cli".into()])
            .unwrap();
        assert_eq!(compiled["executor_class"], "managed_coding");
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        let executor = ReceiptReportingManagedExecutor;
        for _ in 0..8 {
            let tick = store
                .tick_with_executor(run_id, "tester", 1, &executor)
                .unwrap();
            if tick
                .pointer("/run/status")
                .and_then(serde_json::Value::as_str)
                == Some("completed")
            {
                break;
            }
        }
        store
            .finalize_product_task_after_execution(task_id, "tester")
            .unwrap();
        let done = store
            .approve_and_output_product_task(task_id, "tester", true)
            .unwrap();
        let evidence = &done["terminal_evidence"];
        assert_eq!(evidence["node"]["executor_type"], "codex_cli");
        assert_eq!(evidence["node"]["executor_class"], "managed_coding");
        assert_eq!(evidence["usage"]["status"], "linked");
        assert_eq!(evidence["usage"]["input_tokens"], 111);
        assert_eq!(evidence["usage"]["output_tokens"], 23);
        assert_eq!(
            evidence["usage"]["provenance"],
            "node_executor_owner_reported"
        );
        assert_eq!(evidence["cost"]["status"], "unavailable");
    });
}

#[test]
fn terminal_evidence_audit_failure_rolls_back_completion_and_evidence() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = drive_to_awaiting_approval(
            &store,
            &repo,
            &rev,
            "ev-terminal-audit-rollback-1",
            "artifact_only",
        );
        let task_id = task["task_id"].as_str().unwrap();
        let version = task["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", version)
            .unwrap();
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_terminal_evidence_audit
                 BEFORE INSERT ON audit_log
                 WHEN NEW.action = 'product_task.terminal_evidence_committed'
                 BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END;",
            )
            .unwrap();
        let error = store
            .output_product_task(
                task_id,
                "output-operator",
                version,
                approval["approval_id"].as_str(),
                true,
            )
            .unwrap_err();
        assert!(error.contains("audit unavailable"), "{error}");
        let current = store.get_product_task(task_id).unwrap().unwrap();
        assert_eq!(current["status"], "awaiting_approval");
        assert_eq!(current["version"], version);
        assert!(store
            .get_product_task_terminal_evidence(task_id)
            .unwrap_err()
            .contains("not committed"));
        connection
            .execute_batch("DROP TRIGGER fail_terminal_evidence_audit")
            .unwrap();
    });
}

#[test]
fn duplicate_concurrent_output_calls_reuse_one_canonical_terminal_evidence() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = drive_to_awaiting_approval(
            &store,
            &repo,
            &rev,
            "ev-concurrent-terminal-1",
            "artifact_only",
        );
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let version = task["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(&task_id, "independent-operator", version)
            .unwrap();
        let approval_id = approval["approval_id"].as_str().unwrap().to_string();
        drop(store);

        let barrier = Arc::new(Barrier::new(2));
        let mut handles = Vec::new();
        for actor in ["output-a", "output-b"] {
            let store = LocalProductStore::new(dir.path().join("store.db")).unwrap();
            let barrier = Arc::clone(&barrier);
            let task_id = task_id.clone();
            let approval_id = approval_id.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                store
                    .output_product_task(&task_id, actor, version, Some(&approval_id), true)
                    .unwrap()
            }));
        }
        let results = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert!(results
            .iter()
            .all(|result| result["task"]["status"] == "completed"));
        assert_eq!(
            results[0]["terminal_evidence"]["evidence_id"],
            results[1]["terminal_evidence"]["evidence_id"]
        );
        let reopened = LocalProductStore::new(dir.path().join("store.db")).unwrap();
        let terminal_audits = reopened
            .audit_events(10_000)
            .unwrap()
            .into_iter()
            .filter(|event| event["action"] == "product_task.terminal_evidence_committed")
            .count();
        assert_eq!(terminal_audits, 1);
        let output_audits = reopened
            .audit_events(10_000)
            .unwrap()
            .into_iter()
            .filter(|event| event["action"] == "product_task.nonnetwork_output_completed")
            .count();
        assert_eq!(output_audits, 1);
    });
}

/// Deterministic interleaving: winner commits receipt then terminal evidence before the
/// loser rebinds. Loser must reconstruct the same canonical receipt/evidence and must
/// not fail solely because ProductTask version advanced.
#[test]
fn loser_reuses_canonical_output_after_winner_commits_receipt_and_terminal() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = drive_to_awaiting_approval(
            &store,
            &repo,
            &rev,
            "ev-output-authority-interleave-1",
            "artifact_only",
        );
        let task_id = task["task_id"].as_str().unwrap();
        let version = task["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", version)
            .unwrap();
        let approval_id = approval["approval_id"].as_str().unwrap();
        let artifact_id = approval["artifact_id"].as_str().unwrap();

        let winner = store
            .output_product_task(task_id, "output-winner", version, Some(approval_id), true)
            .expect("winner output");
        assert_eq!(winner["task"]["status"], "completed");
        assert_eq!(winner["task"]["version"], version + 1);
        let winner_evidence_id = winner["terminal_evidence"]["evidence_id"]
            .as_str()
            .unwrap()
            .to_string();
        let winner_receipt_id = winner["output_receipt"]["receipt_id"]
            .as_str()
            .unwrap()
            .to_string();

        // Explicit receipt rebind after terminal CAS: version is already advanced.
        let output = serde_json::json!({
            "mode": "artifact_only",
            "status": "artifact_only",
            "product_task_id": task_id,
            "artifact_id": artifact_id,
            "approval_id": approval_id,
            "target_mutation": false,
        });
        let replayed_receipt = store
            .record_product_nonnetwork_output_receipt(
                artifact_id,
                task_id,
                approval_id,
                "artifact_only",
                version,
                &output,
                "output-loser-receipt",
            )
            .expect("loser receipt after winner terminal");
        assert_eq!(replayed_receipt["receipt_id"], winner_receipt_id);

        let loser = store
            .output_product_task(task_id, "output-loser", version, Some(approval_id), true)
            .expect("loser full output after winner terminal");
        assert_eq!(loser["task"]["status"], "completed");
        assert_eq!(loser["reused"], true);
        assert_eq!(
            loser["terminal_evidence"]["evidence_id"].as_str().unwrap(),
            winner_evidence_id
        );
        assert_eq!(
            loser["output_receipt"]["receipt_id"].as_str().unwrap(),
            winner_receipt_id
        );

        // Process restart must reproduce the same canonical outcome.
        drop(store);
        let reopened = LocalProductStore::new(dir.path().join("store.db")).unwrap();
        let restarted = reopened
            .output_product_task(task_id, "output-restart", version, Some(approval_id), true)
            .expect("restart replay");
        assert_eq!(restarted["reused"], true);
        assert_eq!(
            restarted["terminal_evidence"]["evidence_id"]
                .as_str()
                .unwrap(),
            winner_evidence_id
        );
        let terminal_audits = reopened
            .audit_events(10_000)
            .unwrap()
            .into_iter()
            .filter(|event| event["action"] == "product_task.terminal_evidence_committed")
            .count();
        assert_eq!(terminal_audits, 1);
        let output_audits = reopened
            .audit_events(10_000)
            .unwrap()
            .into_iter()
            .filter(|event| event["action"] == "product_task.nonnetwork_output_completed")
            .count();
        assert_eq!(output_audits, 1);
    });
}

#[test]
fn concurrent_conflicting_output_identities_fail_closed() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = drive_to_awaiting_approval(
            &store,
            &repo,
            &rev,
            "ev-output-conflict-identity-1",
            "artifact_only",
        );
        let task_id = task["task_id"].as_str().unwrap();
        let version = task["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", version)
            .unwrap();
        let approval_id = approval["approval_id"].as_str().unwrap();
        let artifact_id = approval["artifact_id"].as_str().unwrap();

        let winner = store
            .output_product_task(task_id, "output-winner", version, Some(approval_id), true)
            .expect("winner");
        assert_eq!(winner["task"]["status"], "completed");

        let conflicting = serde_json::json!({
            "mode": "artifact_only",
            "status": "artifact_only",
            "product_task_id": task_id,
            "artifact_id": artifact_id,
            "approval_id": "approval-not-the-winner",
            "target_mutation": false,
        });
        let error = store
            .record_product_nonnetwork_output_receipt(
                artifact_id,
                task_id,
                "approval-not-the-winner",
                "artifact_only",
                version,
                &conflicting,
                "output-conflict",
            )
            .unwrap_err();
        assert!(
            error.contains("authority")
                || error.contains("binding")
                || error.contains("approval")
                || error.contains("stale"),
            "conflicting identity must fail closed: {error}"
        );

        let stale_approval_error = store
            .output_product_task(
                task_id,
                "output-stale-approval",
                version,
                Some("approval-missing-or-replaced"),
                true,
            )
            .unwrap_err();
        assert!(
            stale_approval_error.contains("approval")
                || stale_approval_error.contains("not found")
                || stale_approval_error.contains("stale"),
            "stale approval must fail closed: {stale_approval_error}"
        );

        let durable = store.get_product_task(task_id).unwrap().unwrap();
        assert_eq!(durable["status"], "completed");
        assert_eq!(
            durable["version"], winner["task"]["version"],
            "failed conflict must not create a second terminal version"
        );
    });
}

#[test]
fn duplicate_concurrent_output_stress_retains_single_canonical_terminal() {
    with_gates(|| {
        for iteration in 0..12 {
            let (dir, store) = temp_store();
            let repo = dir.path().join("repo");
            let rev = init_git_repo(&repo);
            let task = drive_to_awaiting_approval(
                &store,
                &repo,
                &rev,
                &format!("ev-concurrent-stress-{iteration}"),
                "artifact_only",
            );
            let task_id = task["task_id"].as_str().unwrap().to_string();
            let version = task["version"].as_u64().unwrap();
            let approval = store
                .approve_product_task(&task_id, "independent-operator", version)
                .unwrap();
            let approval_id = approval["approval_id"].as_str().unwrap().to_string();
            drop(store);

            let barrier = Arc::new(Barrier::new(4));
            let mut handles = Vec::new();
            for actor_idx in 0..4 {
                let store = LocalProductStore::new(dir.path().join("store.db")).unwrap();
                let barrier = Arc::clone(&barrier);
                let task_id = task_id.clone();
                let approval_id = approval_id.clone();
                handles.push(std::thread::spawn(move || {
                    barrier.wait();
                    store.output_product_task(
                        &task_id,
                        &format!("stress-{actor_idx}"),
                        version,
                        Some(&approval_id),
                        true,
                    )
                }));
            }
            let results = handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>();
            let errors: Vec<_> = results
                .iter()
                .filter_map(|result| result.as_ref().err().cloned())
                .collect();
            assert!(
                errors.is_empty(),
                "iteration {iteration} concurrent output races must not fail: {errors:?}"
            );
            let successes: Vec<_> = results.into_iter().map(Result::unwrap).collect();
            assert!(successes
                .iter()
                .all(|result| result["task"]["status"] == "completed"));
            let evidence_ids: std::collections::BTreeSet<_> = successes
                .iter()
                .map(|result| {
                    result["terminal_evidence"]["evidence_id"]
                        .as_str()
                        .unwrap()
                        .to_string()
                })
                .collect();
            assert_eq!(evidence_ids.len(), 1, "iteration {iteration}");
            let reopened = LocalProductStore::new(dir.path().join("store.db")).unwrap();
            let terminal_audits = reopened
                .audit_events(10_000)
                .unwrap()
                .into_iter()
                .filter(|event| event["action"] == "product_task.terminal_evidence_committed")
                .count();
            assert_eq!(terminal_audits, 1, "iteration {iteration}");
            let output_audits = reopened
                .audit_events(10_000)
                .unwrap()
                .into_iter()
                .filter(|event| event["action"] == "product_task.nonnetwork_output_completed")
                .count();
            assert_eq!(output_audits, 1, "iteration {iteration}");
        }
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
        let receipt = done["output_receipt"].as_object().expect("output receipt");
        assert_eq!(receipt["state"], "completed");
        assert_eq!(receipt["patch_hash"], done["artifact"]["patch_hash"]);
        let durable_artifact = store
            .get_supervised_patch_artifact(done["artifact"]["artifact_id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            durable_artifact["product_output_receipt"]["receipt_id"],
            receipt["receipt_id"]
        );
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
fn export_patch_refuses_default_branch_drift_after_workspace_binding() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let validated = validate_intake(
            &intake(
                &repo,
                &rev,
                "ev-export-default-branch-drift",
                "export_patch",
            ),
            "local",
            "default",
        )
        .unwrap();
        let task = store.admit_product_task(&validated, "tester").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        complete_to_approval(&store, task_id);
        std::fs::write(repo.join("README.md"), "advanced externally\n").unwrap();
        for args in [
            &["add", "README.md"][..],
            &["commit", "-m", "external advance"][..],
        ] {
            let output = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(output.status.success());
        }
        let awaiting = store.get_product_task(task_id).unwrap().unwrap();
        let version = awaiting["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", version)
            .unwrap();
        let error = store
            .output_product_task(
                task_id,
                "output-operator",
                version,
                approval["approval_id"].as_str(),
                true,
            )
            .unwrap_err();
        assert_eq!(error, "git source identity changed before output");
        assert_eq!(
            store.get_product_task(task_id).unwrap().unwrap()["status"],
            "awaiting_approval"
        );
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
        // Network unavailability is not successful Draft PR completion.
        assert_eq!(
            done["task"]["status"].as_str(),
            Some(ProductTaskStatus::OutputPending.as_str())
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "hello\n"
        );
    });
}

#[test]
fn approval_and_output_are_separate_and_missing_confirmation_has_zero_side_effects() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = drive_to_awaiting_approval(
            &store,
            &repo,
            &rev,
            "ev-separate-authority-1",
            "export_patch",
        );
        let task_id = task["task_id"].as_str().unwrap();
        let version = task["version"].as_u64().unwrap();

        let before_approvals = store
            .workflow_run_approvals(task["run_id"].as_str().unwrap(), 20)
            .unwrap();
        let before_audit = store.audit_events(1_000).unwrap();

        let error = store
            .output_product_task(task_id, "missing-confirmation", version, None, false)
            .unwrap_err();
        assert!(error.contains("confirm_output=true"));

        let unchanged = store.get_product_task(task_id).unwrap().unwrap();
        assert_eq!(unchanged["status"], "awaiting_approval");
        assert_eq!(unchanged["version"], version);
        assert_eq!(
            store
                .workflow_run_approvals(task["run_id"].as_str().unwrap(), 20)
                .unwrap(),
            before_approvals
        );
        assert_eq!(store.audit_events(1_000).unwrap(), before_audit);

        let approval = store
            .approve_product_task(task_id, "independent-operator", version)
            .expect("independent approval");
        assert_eq!(approval["approval_kind"], "product_output");
        assert_eq!(approval["product_task_id"], task_id);
        assert!(approval["artifact_id"].as_str().is_some());
        assert_eq!(approval["run_id"], task["run_id"]);
        assert_eq!(approval["workspace_record_id"], task["workspace_record_id"]);
        assert_eq!(approval["output_intent"], "export_patch");
        assert_eq!(approval["approved_by"], "independent-operator");

        let run_id = approval["run_id"].as_str().unwrap();
        let node_id = approval["node_id"].as_str().unwrap();
        let audit_after_valid = store.audit_events(1_000).unwrap();
        for (field, replacement, expected_error) in [
            (
                "expected_task_version",
                serde_json::json!(version + 1),
                "stale product task version or state",
            ),
            (
                "artifact_id",
                serde_json::json!("patch-artifact-mismatch"),
                "artifact missing",
            ),
            (
                "verification_sha256",
                serde_json::json!("0".repeat(64)),
                "verification binding changed",
            ),
            (
                "output_intent",
                serde_json::json!("draft_pr"),
                "task binding changed",
            ),
        ] {
            let mut tampered = approval.clone();
            tampered[field] = replacement;
            let error = store
                .record_product_output_approval(run_id, node_id, "unauthorized-binding", &tampered)
                .unwrap_err();
            assert!(
                error.contains(expected_error),
                "unexpected {field} rejection: {error}"
            );
        }
        assert_eq!(store.audit_events(1_000).unwrap(), audit_after_valid);
    });
}

#[test]
fn draft_pr_network_gate_disabled_remains_output_pending() {
    with_gates(|| {
        std::env::remove_var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT");
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task =
            drive_to_awaiting_approval(&store, &repo, &rev, "ev-draft-disabled-2", "draft_pr");
        let task_id = task["task_id"].as_str().unwrap();
        let version = task["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", version)
            .expect("approval");

        let result = store
            .output_product_task(
                task_id,
                "output-operator",
                version,
                approval["approval_id"].as_str(),
                true,
            )
            .expect("accurate non-terminal result");
        assert_eq!(result["output"]["status"], "network_output_unavailable");
        assert_eq!(result["task"]["status"], "output_pending");
        assert_ne!(result["task"]["status"], "completed");
    });
}

#[test]
fn draft_pr_rejects_non_github_remote_before_branch_push() {
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
            .args(["remote", "set-url", "origin"])
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
        assert_eq!(done["output"]["status"], "blocked");
        assert_eq!(done["task"]["status"], "output_pending");
        let refs = Command::new("git")
            .args(["show-ref"])
            .current_dir(&bare)
            .output()
            .unwrap();
        let refs_txt = String::from_utf8_lossy(&refs.stdout);
        assert!(
            !refs_txt.contains("acp/"),
            "inadmissible remote must not receive acp branch: {refs_txt}"
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

#[test]
fn progressive_output_operation_survives_restart_and_retries_only_pr_phase() {
    with_gates(|| {
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = drive_to_awaiting_approval(
            &store,
            &repo,
            &rev,
            "ev-progressive-operation-1",
            "draft_pr",
        );
        let task_id = task["task_id"].as_str().unwrap();
        let task_version = task["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(
                task_id,
                "independent-operator",
                task["version"].as_u64().unwrap(),
            )
            .unwrap();
        let artifact = store
            .supervised_patch_artifacts(100)
            .unwrap()
            .into_iter()
            .find(|artifact| artifact["run_id"] == task["run_id"])
            .unwrap();
        let artifact_id = artifact["artifact_id"].as_str().unwrap();
        let request = serde_json::json!({
            "schema_version": "product_draft_pr_output_request.v1",
            "product_task_id": task_id,
            "artifact_id": artifact_id,
            "approval_id": approval["approval_id"],
            "output_intent": "draft_pr",
            "expected_task_version": task_version,
            "workspace_id": artifact["workspace_id"],
            "run_id": artifact["run_id"],
            "target_id": artifact["target_id"],
            "patch_hash": artifact["patch_hash"],
            "source_revision": artifact["source_revision"],
            "target_repository": "disposable/acceptance",
            "repository_host": "github.com",
            "base_branch": "main",
            "head_branch": format!("acp/product-{task_id}"),
            "remote": "origin",
            "commit_message": "bounded test",
            "pr_title": "Draft: bounded test",
            "pr_body": "Do not merge automatically.",
        });
        let request_sha256 = sha256_json(&request);
        let mut mismatched_request = request.clone();
        mismatched_request["output_intent"] = serde_json::json!("export_patch");
        let mismatched_sha256 = sha256_json(&mismatched_request);
        let mismatch = store
            .claim_product_output_operation(
                artifact_id,
                &mismatched_request,
                &mismatched_sha256,
                task_version,
                "output-operator",
            )
            .unwrap_err();
        assert!(
            mismatch.contains("task state or intent authority changed"),
            "unexpected output-intent rejection: {mismatch}"
        );
        assert!(
            store
                .get_supervised_patch_artifact(artifact_id)
                .unwrap()
                .unwrap()
                .get("product_output_operation")
                .is_none(),
            "mismatched output intent must have zero durable operation effect"
        );
        let stale = store
            .claim_product_output_operation(
                artifact_id,
                &request,
                &request_sha256,
                task_version.saturating_add(1),
                "stale-output-operator",
            )
            .unwrap_err();
        assert!(
            stale.contains("stale product task version"),
            "unexpected stale-version rejection: {stale}"
        );
        assert!(
            store
                .get_supervised_patch_artifact(artifact_id)
                .unwrap()
                .unwrap()
                .get("product_output_operation")
                .is_none(),
            "stale output caller must have zero durable operation effect"
        );
        let claimed = store
            .claim_product_output_operation(
                artifact_id,
                &request,
                &request_sha256,
                task_version,
                "output-operator",
            )
            .unwrap();
        assert_eq!(claimed["claim_action"], "push_branch");
        let operation_id = claimed["operation_id"].as_str().unwrap().to_string();
        let commit_sha = "a".repeat(40);
        let branch = store
            .record_product_output_branch_pushed(
                artifact_id,
                &operation_id,
                claimed["current_version"].as_u64().unwrap(),
                &commit_sha,
                "output-operator",
            )
            .unwrap();
        assert_eq!(branch["branch_push"]["status"], "completed");
        drop(store);

        let store_a = LocalProductStore::new(dir.path().join("store.db")).unwrap();
        let store_b = LocalProductStore::new(dir.path().join("store.db")).unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let artifact_a = artifact_id.to_string();
        let artifact_b = artifact_id.to_string();
        let request_a = request.clone();
        let request_b = request.clone();
        let request_sha_a = request_sha256.clone();
        let request_sha_b = request_sha256.clone();
        let barrier_a = Arc::clone(&barrier);
        let barrier_b = Arc::clone(&barrier);
        let handle_a = std::thread::spawn(move || {
            barrier_a.wait();
            store_a
                .claim_product_output_operation(
                    &artifact_a,
                    &request_a,
                    &request_sha_a,
                    task_version,
                    "output-operator-a",
                )
                .unwrap()
        });
        let handle_b = std::thread::spawn(move || {
            barrier_b.wait();
            store_b
                .claim_product_output_operation(
                    &artifact_b,
                    &request_b,
                    &request_sha_b,
                    task_version,
                    "output-operator-b",
                )
                .unwrap()
        });
        let claims = [handle_a.join().unwrap(), handle_b.join().unwrap()];
        let pr_claim = claims
            .iter()
            .find(|claim| claim["claim_action"] == "create_or_reconcile_pr")
            .unwrap()
            .clone();
        let concurrent = claims
            .iter()
            .find(|claim| claim["claim_action"] == "operation_in_progress")
            .unwrap();
        assert_eq!(pr_claim["claim_action"], "create_or_reconcile_pr");
        assert_eq!(pr_claim["branch_push"]["commit_sha"], commit_sha);
        assert_eq!(concurrent["claim_action"], "operation_in_progress");
        assert_eq!(concurrent["current_version"], pr_claim["current_version"]);
        let reopened = LocalProductStore::new(dir.path().join("store.db")).unwrap();
        reopened
            .mark_product_output_pr_failed_known(
                artifact_id,
                &operation_id,
                pr_claim["current_version"].as_u64().unwrap(),
                "output-operator",
                "github_pr_create_failed_known: status 422",
            )
            .unwrap();
        let retry = reopened
            .claim_product_output_operation(
                artifact_id,
                &request,
                &request_sha256,
                task_version,
                "output-operator",
            )
            .unwrap();
        assert_eq!(retry["claim_action"], "create_or_reconcile_pr");
        assert_eq!(retry["branch_push"]["commit_sha"], commit_sha);
        assert!(retry["attempt"].as_u64().unwrap() > pr_claim["attempt"].as_u64().unwrap());

        let pull_request = serde_json::json!({
            "number": 17,
            "url": "https://github.com/disposable/acceptance/pull/17",
            "state": "open",
            "draft": true,
            "reused": false,
            "repository": "disposable/acceptance",
            "base_branch": "main",
            "head_branch": format!("acp/product-{task_id}"),
            "head_sha": commit_sha,
        });
        let mut wrong_repository = pull_request.clone();
        wrong_repository["url"] = serde_json::json!("https://github.com/disposable/other/pull/17");
        assert!(reopened
            .complete_product_output_draft_pr(
                artifact_id,
                &operation_id,
                retry["current_version"].as_u64().unwrap(),
                &wrong_repository,
                "output-operator",
            )
            .unwrap_err()
            .contains("admitted repository"));
        let completed = reopened
            .complete_product_output_draft_pr(
                artifact_id,
                &operation_id,
                retry["current_version"].as_u64().unwrap(),
                &pull_request,
                "output-operator",
            )
            .unwrap();
        assert_eq!(completed["state"], "completed");
        assert_eq!(completed["pr_create"]["number"], 17);
        assert_eq!(
            completed["pr_create"]["repository"],
            "disposable/acceptance"
        );
        assert_eq!(completed["pr_create"]["head_sha"], "a".repeat(40));
        let late_failure = reopened
            .mark_product_output_pr_failed_known(
                artifact_id,
                &operation_id,
                retry["current_version"].as_u64().unwrap(),
                "late-output-operator",
                "late failure must not replace completion",
            )
            .unwrap_err();
        assert!(
            late_failure.contains("version mismatch")
                || late_failure.contains("stale product output operation version")
                || late_failure.contains("phase is not current"),
            "unexpected late-write rejection: {late_failure}"
        );
        let after_late_failure = reopened
            .claim_product_output_operation(
                artifact_id,
                &request,
                &request_sha256,
                task_version,
                "output-operator",
            )
            .unwrap();
        assert_eq!(after_late_failure["claim_action"], "reused");
        assert_eq!(after_late_failure["state"], "completed");
        let reused = reopened
            .claim_product_output_operation(
                artifact_id,
                &request,
                &request_sha256,
                task_version,
                "output-operator",
            )
            .unwrap();
        assert_eq!(reused["claim_action"], "reused");
        assert_eq!(reused["operation_id"], operation_id);
    });
}

#[test]
fn terminal_completion_revalidates_workspace_verification_atomically() {
    with_gates(|| {
        std::env::set_var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT", "0");
        let (dir, store) = temp_store();
        let repo = dir.path().join("repo");
        let rev = init_git_repo(&repo);
        let task = drive_to_awaiting_approval(
            &store,
            &repo,
            &rev,
            "ev-terminal-authority-race-1",
            "draft_pr",
        );
        let task_id = task["task_id"].as_str().unwrap();
        let approval = store
            .approve_product_task(
                task_id,
                "independent-operator",
                task["version"].as_u64().unwrap(),
            )
            .unwrap();
        let pending = store
            .output_product_task(
                task_id,
                "output-operator",
                task["version"].as_u64().unwrap(),
                approval["approval_id"].as_str(),
                true,
            )
            .unwrap();
        let pending_version = pending["task"]["version"].as_u64().unwrap();
        assert_eq!(pending["task"]["status"], "output_pending");
        let artifact = store
            .get_supervised_patch_artifact(approval["artifact_id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        let artifact_id = artifact["artifact_id"].as_str().unwrap();
        let request = serde_json::json!({
            "schema_version": "product_draft_pr_output_request.v1",
            "product_task_id": task_id,
            "artifact_id": artifact_id,
            "approval_id": approval["approval_id"],
            "output_intent": "draft_pr",
            "expected_task_version": pending_version,
            "workspace_id": artifact["workspace_id"],
            "run_id": artifact["run_id"],
            "target_id": artifact["target_id"],
            "patch_hash": artifact["patch_hash"],
            "source_revision": artifact["source_revision"],
            "target_repository": "disposable/acceptance",
            "repository_host": "github.com",
            "base_branch": "main",
            "head_branch": format!("acp/product-{task_id}"),
            "remote": "origin",
            "commit_message": "bounded terminal authority test",
            "pr_title": "Draft: terminal authority test",
            "pr_body": "Do not merge automatically.",
        });
        let request_sha256 = sha256_json(&request);
        let branch_claim = store
            .claim_product_output_operation(
                artifact_id,
                &request,
                &request_sha256,
                pending_version,
                "output-operator",
            )
            .unwrap();
        let operation_id = branch_claim["operation_id"].as_str().unwrap();
        let commit_sha = "b".repeat(40);
        store
            .record_product_output_branch_pushed(
                artifact_id,
                operation_id,
                branch_claim["current_version"].as_u64().unwrap(),
                &commit_sha,
                "output-operator",
            )
            .unwrap();
        let pr_claim = store
            .claim_product_output_operation(
                artifact_id,
                &request,
                &request_sha256,
                pending_version,
                "output-operator",
            )
            .unwrap();
        let pull_request = serde_json::json!({
            "number": 29,
            "url": "https://github.com/disposable/acceptance/pull/29",
            "state": "open",
            "draft": true,
            "reused": false,
            "repository": "disposable/acceptance",
            "base_branch": "main",
            "head_branch": format!("acp/product-{task_id}"),
            "head_sha": commit_sha,
        });
        store
            .complete_product_output_draft_pr(
                artifact_id,
                operation_id,
                pr_claim["current_version"].as_u64().unwrap(),
                &pull_request,
                "output-operator",
            )
            .unwrap();

        let workspace_id = artifact["workspace_id"].as_str().unwrap();
        let workspace = store
            .get_supervised_patch_workspace(workspace_id)
            .unwrap()
            .unwrap();
        let original_verification = workspace["verification"].clone();
        let mut replaced_verification = original_verification.clone();
        replaced_verification["authority_race"] = serde_json::json!(true);
        store
            .record_workspace_verification(
                workspace_id,
                &replaced_verification,
                "concurrent-verifier",
            )
            .unwrap();
        let stale = store
            .complete_product_task_draft_pr_output(
                task_id,
                artifact_id,
                operation_id,
                pr_claim["current_version"].as_u64().unwrap(),
                pending_version,
                &pull_request,
                "output-operator",
            )
            .unwrap_err();
        assert!(
            stale.contains("verification authority changed")
                || stale.contains("verification approval binding changed"),
            "{stale}"
        );
        assert_eq!(
            store.get_product_task(task_id).unwrap().unwrap()["status"],
            "output_pending"
        );

        store
            .record_workspace_verification(
                workspace_id,
                &original_verification,
                "concurrent-verifier-rollback",
            )
            .unwrap();
        let completed = store
            .complete_product_task_draft_pr_output(
                task_id,
                artifact_id,
                operation_id,
                pr_claim["current_version"].as_u64().unwrap(),
                pending_version,
                &pull_request,
                "output-operator",
            )
            .unwrap();
        assert_eq!(completed["task"]["status"], "completed");
        assert_eq!(completed["operation"]["pr_create"]["number"], 29);
        let replayed = store
            .complete_product_task_draft_pr_output(
                task_id,
                artifact_id,
                operation_id,
                pr_claim["current_version"].as_u64().unwrap(),
                pending_version,
                &pull_request,
                "duplicate-output-operator",
            )
            .unwrap();
        assert_eq!(replayed["reused"], true);
        assert_eq!(
            replayed["terminal_evidence"]["evidence_id"],
            completed["terminal_evidence"]["evidence_id"]
        );
        std::env::remove_var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT");
    });
}
