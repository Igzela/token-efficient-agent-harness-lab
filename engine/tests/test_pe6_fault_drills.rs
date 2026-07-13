//! PE-6 owner-backed drills.
//!
//! Every test provisions only a tempfile database/workspace, a fake provider,
//! or a short-lived controlled child process.  The Python PE-6 harness invokes
//! these tests through fixed registry entries; this file does not add runtime
//! fault authority.

use engine::node_executor::{CommandNodeExecutor, FailNodeExecutor};
use engine::node_executor::{NodeExecutionInput, NodeExecutor};
use engine::provider::audit::ProviderAuditRecorder;
use engine::provider::cost_gate::{check_cost_gates, CostGateBlock, CostGateConfig};
use engine::provider::executor::ProviderNodeExecutor;
use engine::provider::fake::FakeProvider;
use engine::provider::{
    DisabledProvider, Provider, ProviderError, ProviderRequest, ProviderResponse, ProviderResult,
};
use engine::storage::backup_manager::BackupManager;
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

fn pe6_check(name: &str, category: &str, outcome: &str, observation: &str) -> Value {
    json!({
        "name": name,
        "category": category,
        "outcome": outcome,
        "observation": observation,
    })
}

fn emit_pe6_owner_evidence(
    observed_state_before_fault: &str,
    observed_fault: &str,
    observed_recovery_or_refusal: &str,
    checks: Vec<Value>,
    cleanup_observation: &str,
) {
    let scenario_path = std::env::var("ACP_PE6_SCENARIO_PATH").ok();
    let evidence_path = std::env::var("ACP_PE6_EVIDENCE_PATH").ok();
    if scenario_path.is_none() && evidence_path.is_none() {
        return;
    }
    let scenario_path = PathBuf::from(scenario_path.expect("scenario evidence path"));
    let evidence_path = PathBuf::from(evidence_path.expect("owner evidence output path"));
    let scenario: Value =
        serde_json::from_slice(&fs::read(&scenario_path).expect("read harness scenario"))
            .expect("parse harness scenario");
    let resource_ids = scenario["resources"]
        .as_array()
        .expect("scenario resources")
        .iter()
        .map(|resource| resource["resource_id"].clone())
        .collect::<Vec<_>>();
    use sha2::{Digest, Sha256};
    let mut scenario_bytes = serde_json::to_vec(&scenario).expect("canonical scenario");
    scenario_bytes.push(b'\n');
    let scenario_sha256 = hex::encode(Sha256::digest(&scenario_bytes));
    let evidence = json!({
        "schema_version": "fault_owner_evidence.v2",
        "scenario_id": scenario["scenario_id"],
        "scenario_version": scenario["scenario_version"],
        "scenario_sha256": scenario_sha256,
        "source_head": scenario["source_head"],
        "fault": {
            "fault_id": scenario["fault"]["fault_id"],
            "injection_point": scenario["fault"]["injection_point"],
        },
        "owner": {
            "identity": scenario["owner"],
            "resource_ids": resource_ids,
        },
        "observed_state_before_fault": observed_state_before_fault,
        "observed_fault": observed_fault,
        "observed_recovery_or_refusal": observed_recovery_or_refusal,
        "checks": checks,
        "cleanup": {"outcome": "passed", "observation": cleanup_observation},
    });
    let mut encoded = serde_json::to_vec(&evidence).expect("encode owner evidence");
    encoded.push(b'\n');
    fs::write(evidence_path, encoded).expect("write owner evidence");
}

fn single_node_plan(
    ids: &engine::storage::local_product_store::WorkflowPlanIds,
    command: Option<&str>,
) -> Value {
    let mut node = json!({
        "schema_version": "workflow_node.v1",
        "node_id": format!("node-{}", ids.workflow_id),
        "workflow_id": ids.workflow_id,
        "task_type": "command",
        "assigned_agent_id": null,
        "status": "pending",
        "input_refs": [],
        "output_ref": null,
        "budget": 0.1,
        "cost_incurred": 0.0,
        "error": null,
        "created_at": "2026-07-13T00:00:00Z",
        "started_at": null,
        "completed_at": null
    });
    if let Some(command) = command {
        node["command"] = json!(command);
    }
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-pe6", "task_domain": "pe6"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-07-13T00:00:00Z",
            "updated_at": "2026-07-13T00:00:00Z",
            "nodes": [node],
            "edges": [],
            "started_at": null,
            "completed_at": null,
            "result": null
        },
        "boundaries": {
            "execution": "disabled",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled"
        }
    })
}

fn new_run(store: &LocalProductStore, command: Option<&str>) -> String {
    let plan = store
        .create_workflow_plan("pe6 drill", "pe6", "pe6-test", |ids, _| {
            Ok(single_node_plan(ids, command))
        })
        .expect("create PE-6 plan");
    store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "pe6-test")
        .expect("create PE-6 run")["run_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn pe6_sqlite_atomicity_and_integrity() {
    let directory = tempdir().expect("disposable SQLite directory");
    let directory_path = directory.path().to_path_buf();
    let db_path = directory.path().join("pe6.sqlite");
    let store = LocalProductStore::new(&db_path).expect("SQLite store");

    let run_id = new_run(&store, None);
    store
        .append_audit(
            "pe6-test",
            "pe6.drill.started",
            &run_id,
            &json!({"run_id": run_id, "scenario_id": "pe6.storage.sqlite.atomicity.v2"}),
        )
        .expect("audit append");

    let executor = FailNodeExecutor::default();
    let first = store
        .tick_with_executor(&run_id, "pe6-test", 1, &executor)
        .expect("first bounded retry");
    assert_eq!(first["action"], "node_retry");
    let second = store
        .tick_with_executor(&run_id, "pe6-test", 1, &executor)
        .expect("retry exhaustion");
    assert_eq!(second["result"]["status"], "failed");

    // Replaying a terminal run cannot create a second authority transition.
    let replay = store.tick_with_executor(&run_id, "pe6-test", 1, &executor);
    assert!(
        replay.is_err(),
        "terminal replay must be refused, not re-executed"
    );
    let run = store.get_workflow_run(&run_id).expect("read run").unwrap();
    assert_eq!(run["status"], "failed");
    assert_eq!(store.check_integrity().expect("integrity").status, "ok");
    assert!(!store.audit_events(100).expect("audit events").is_empty());

    drop(store);
    let reopened = LocalProductStore::new(&db_path).expect("restart SQLite store");
    assert_eq!(
        reopened
            .check_integrity()
            .expect("reopened integrity")
            .status,
        "ok"
    );
    assert_eq!(
        reopened
            .get_workflow_run(&run_id)
            .expect("reopened run")
            .unwrap()["status"],
        "failed"
    );
    drop(reopened);
    directory
        .close()
        .expect("remove disposable SQLite directory");
    assert!(!directory_path.exists());
    emit_pe6_owner_evidence(
        "one pending SQLite workflow run existed before bounded executor failures",
        "a duplicate replay attempted a second terminal authority transition",
        "the storage owner refused terminal replay and retained one failed run",
        vec![
            pe6_check(
                "pe6.sqlite.duplicate_replay_refused",
                "recovery",
                "passed",
                "terminal replay returned an error",
            ),
            pe6_check(
                "pe6.sqlite.no_partial_authority",
                "integrity",
                "passed",
                "integrity remained ok before and after reopen",
            ),
            pe6_check(
                "pe6.sqlite.failure_audit_present",
                "audit",
                "passed",
                "bounded audit events were present",
            ),
            pe6_check(
                "pe6.sqlite.reopen_terminal_state",
                "restart",
                "passed",
                "reopen retained the single failed state",
            ),
            pe6_check(
                "pe6.sqlite.rollback_not_exercised",
                "rollback",
                "unsupported",
                "this duplicate-refusal drill did not activate a rollback target",
            ),
            pe6_check(
                "pe6.sqlite.tempdir_removed",
                "cleanup",
                "passed",
                "the disposable SQLite directory no longer existed",
            ),
        ],
        "the disposable SQLite directory was removed and observed absent",
    );
}

#[test]
fn pe6_sqlite_backup_restore_and_cleanup() {
    let directory = tempdir().expect("disposable backup directory");
    let directory_path = directory.path().to_path_buf();
    let db_path = directory.path().join("source.db");
    let store = LocalProductStore::new(&db_path).expect("SQLite store");
    store
        .append_audit("pe6-test", "pe6.backup", "pe6", &json!({"bounded": true}))
        .expect("seed audit");
    store.checkpoint_wal().expect("checkpoint WAL");
    drop(store);

    let manager = BackupManager::new(&directory.path().join("backups")).expect("backup manager");
    let record = manager
        .create_backup(
            &db_path,
            "PE-6 backup",
            "pe6-backup",
            "2026-07-13T00:00:00Z",
        )
        .expect("create backup");
    manager
        .save_metadata(&[record])
        .expect("save backup metadata");
    let verified = manager.verify_backup("pe6-backup").expect("verify backup");
    assert!(verified.success && verified.checksum_ok && verified.integrity_ok);

    let restored = directory.path().join("restored.db");
    let restore_result = manager
        .restore_backup_with_verify("pe6-backup", &restored, 1.0)
        .expect("restore backup");
    assert!(restore_result.success && restore_result.records_restored > 0);

    // A tampered backup is refused before restore; the clean target remains.
    std::fs::OpenOptions::new()
        .append(true)
        .open(directory.path().join("backups/pe6-backup.db"))
        .expect("open disposable backup")
        .write_all(b"tamper")
        .expect("tamper disposable backup");
    let tampered = manager
        .verify_backup("pe6-backup")
        .expect("verify tampered backup");
    assert!(!tampered.success && !tampered.checksum_ok);
    assert!(restored.exists());
    drop(manager);
    directory
        .close()
        .expect("remove disposable backup directory");
    assert!(!directory_path.exists());
    emit_pe6_owner_evidence(
        "a verified backup and verified restored database existed before tampering",
        "bytes were appended to the disposable backup after its checksum was recorded",
        "backup verification refused the tampered source and left the verified restore intact",
        vec![
            pe6_check(
                "pe6.backup.tamper_refused",
                "recovery",
                "passed",
                "verification reported checksum failure",
            ),
            pe6_check(
                "pe6.backup.clean_restore_retained",
                "rollback",
                "passed",
                "the earlier verified restore remained present",
            ),
            pe6_check(
                "pe6.backup.checksum_bound",
                "integrity",
                "passed",
                "tampered bytes changed checksum validity",
            ),
            pe6_check(
                "pe6.backup.audit_not_exercised",
                "audit",
                "unsupported",
                "backup metadata rather than the runtime audit owner was tested",
            ),
            pe6_check(
                "pe6.backup.restart_not_exercised",
                "restart",
                "unsupported",
                "process restart was outside this storage drill",
            ),
            pe6_check(
                "pe6.backup.tempdir_removed",
                "cleanup",
                "passed",
                "the disposable backup directory no longer existed",
            ),
        ],
        "the disposable backup and restore directory was removed and observed absent",
    );
}

#[test]
fn pe6_workflow_timeout_retry_concurrency_and_restart() {
    let directory = tempdir().expect("disposable workflow directory");
    let directory_path = directory.path().to_path_buf();
    let store =
        Arc::new(LocalProductStore::new(directory.path().join("workflow.db")).expect("store"));
    let timeout_run = new_run(&store, Some("sleep 1"));
    let timeout_executor = CommandNodeExecutor {
        timeout_ms: 20,
        allowed_commands: vec!["sleep".to_string()],
        allowed_binaries: vec!["sleep".to_string()],
        env_vars: Vec::new(),
    };
    let timeout_result = store
        .tick_with_executor(&timeout_run, "pe6-test", 0, &timeout_executor)
        .expect("bounded timeout");
    assert_eq!(timeout_result["result"]["error_domain"], "command_timeout");
    assert_eq!(
        store.get_workflow_run(&timeout_run).unwrap().unwrap()["status"],
        "failed"
    );

    let run_id = new_run(&store, None);
    let executor = Arc::new(FailNodeExecutor::default());
    let barrier = Arc::new(Barrier::new(2));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let store = Arc::clone(&store);
        let executor = Arc::clone(&executor);
        let barrier = Arc::clone(&barrier);
        let run_id = run_id.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            store.tick_with_executor(&run_id, "pe6-test", 0, &*executor)
        }));
    }
    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("tick thread").expect("tick result"))
        .collect::<Vec<_>>();
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(
                result["action"].as_str(),
                Some("node_retry") | Some("node_executed")
            ))
            .count(),
        1,
        "concurrent ticks must have one authority winner"
    );

    let _lease_run = new_run(&store, None);
    let leased = store
        .set_pending_node_to_running_for_test("2000-01-01T00:00:00Z")
        .expect("test lease injection");
    assert_eq!(leased, 1);
    assert_eq!(
        store.recover_stale_leases(1).expect("stale lease recovery"),
        1
    );
    drop(store);
    let reopened = LocalProductStore::new(directory.path().join("workflow.db"))
        .expect("restart workflow store");
    assert_eq!(
        reopened
            .check_integrity()
            .expect("restart integrity")
            .status,
        "ok"
    );
    drop(reopened);
    directory
        .close()
        .expect("remove disposable workflow directory");
    assert!(!directory_path.exists());
    emit_pe6_owner_evidence(
        "separate pending workflow runs existed for timeout, concurrency, and stale lease checks",
        "a command timed out, two ticks raced, and one stale running lease was injected",
        "timeout failed closed, one concurrent tick won, the lease recovered, and reopen stayed integral",
        vec![
            pe6_check("pe6.workflow.timeout_failed_closed", "recovery", "passed", "the timed command ended with command_timeout"),
            pe6_check("pe6.workflow.single_concurrent_winner", "integrity", "passed", "exactly one racing tick obtained authority"),
            pe6_check("pe6.workflow.stale_lease_recovered", "recovery", "passed", "exactly one stale lease was recovered"),
            pe6_check("pe6.workflow.reopen_integrity", "restart", "passed", "reopened workflow storage reported ok"),
            pe6_check("pe6.workflow.rollback_not_exercised", "rollback", "unsupported", "no deployment rollback target participated"),
            pe6_check("pe6.workflow.audit_not_asserted", "audit", "unsupported", "this drill did not assert an audit row"),
            pe6_check("pe6.workflow.tempdir_removed", "cleanup", "passed", "the disposable workflow directory no longer existed"),
        ],
        "the disposable workflow database directory was removed and observed absent",
    );
}

#[derive(Clone, Copy)]
enum ControlledProviderMode {
    Success,
    TimeoutError,
    Secret,
    Slow,
}

struct ControlledProvider {
    calls: AtomicUsize,
    mode: ControlledProviderMode,
}

#[async_trait::async_trait]
impl Provider for ControlledProvider {
    fn provider_id(&self) -> &str {
        "pe6-counting-provider"
    }

    fn is_enabled(&self) -> bool {
        true
    }

    async fn invoke(&self, _request: &ProviderRequest) -> ProviderResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            ControlledProviderMode::TimeoutError => Err(ProviderError {
                schema_version: "provider_error.v1".to_string(),
                provider_id: self.provider_id().to_string(),
                error_domain: "provider_timeout".to_string(),
                message: "bounded fake timeout".to_string(),
                retryable: true,
            }),
            ControlledProviderMode::Slow => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(Self::response(self, "slow but bounded"))
            }
            ControlledProviderMode::Secret => {
                Ok(Self::response(self, "api_key=sk-test-only-fixture"))
            }
            ControlledProviderMode::Success => {
                Ok(Self::response(self, "safe deterministic output"))
            }
        }
    }
}

impl ControlledProvider {
    fn response(&self, output: &str) -> ProviderResponse {
        ProviderResponse {
            schema_version: "provider_response.v1".to_string(),
            provider_id: self.provider_id().to_string(),
            model: "fake-model".to_string(),
            output: output.to_string(),
            input_tokens: Some(1),
            output_tokens: Some(1),
            estimated_cost: Some(0.0),
            provider_request_id: None,
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn pe6_provider_timeout_retry_budget_audit_and_redaction() {
    let fake = FakeProvider::new("pe6-fake");
    let request = ProviderRequest::local_stub("pe6-fake", "fake-model", "bounded input");
    let first = fake.invoke(&request).await.expect("fake provider");
    let second = fake.invoke(&request).await.expect("repeat fake provider");
    assert_eq!(first, second, "fake provider output must be deterministic");
    assert_eq!(first.estimated_cost, Some(0.0));

    let disabled = DisabledProvider::new("pe6-kill-switch");
    assert_eq!(
        disabled.invoke(&request).await.unwrap_err().error_domain,
        "provider_disabled"
    );

    let gate = check_cost_gates(&CostGateConfig::new(Some(0.01), None), 1.0, 0.0);
    assert!(matches!(
        gate,
        Err(CostGateBlock::PerDispatchExceeded { .. })
    ));
    let counted = Arc::new(ControlledProvider {
        calls: AtomicUsize::new(0),
        mode: ControlledProviderMode::Success,
    });
    let gated_executor = ProviderNodeExecutor::new(counted.clone())
        .with_cost_gate(CostGateConfig::new(Some(0.01), None), 0.0)
        .with_max_retries(1);
    let input = NodeExecutionInput {
        node_id: "pe6-node".to_string(),
        task_type: "provider".to_string(),
        run_id: "pe6-run".to_string(),
        workflow_id: "pe6-workflow".to_string(),
        node_metadata: json!({"prompt": "bounded", "reserved_cost_usd": 1.0}),
    };
    let blocked = gated_executor.execute_node(&input);
    assert_eq!(
        blocked.error_domain.as_deref(),
        Some("provider_cost_gate_blocked")
    );
    assert_eq!(
        counted.calls.load(Ordering::SeqCst),
        0,
        "cost block occurs before provider call"
    );

    let failing = Arc::new(ControlledProvider {
        calls: AtomicUsize::new(0),
        mode: ControlledProviderMode::TimeoutError,
    });
    let failed = ProviderNodeExecutor::new(failing.clone())
        .with_max_retries(1)
        .execute_node(&NodeExecutionInput {
            node_metadata: json!({"prompt": "bounded", "reserved_cost_usd": 0.0}),
            ..input.clone()
        });
    assert_eq!(failed.error_domain.as_deref(), Some("provider_timeout"));
    assert_eq!(
        failing.calls.load(Ordering::SeqCst),
        2,
        "retry count is bounded"
    );

    let secret = Arc::new(ControlledProvider {
        calls: AtomicUsize::new(0),
        mode: ControlledProviderMode::Secret,
    });
    let redacted = ProviderNodeExecutor::new(secret).execute_node(&NodeExecutionInput {
        node_metadata: json!({"prompt": "bounded", "reserved_cost_usd": 0.0}),
        ..input.clone()
    });
    assert!(!redacted.output.unwrap_or_default().contains("api_key=sk-"));

    let slow = ControlledProvider {
        calls: AtomicUsize::new(0),
        mode: ControlledProviderMode::Slow,
    };
    let timed = tokio::time::timeout(Duration::from_millis(5), slow.invoke(&request)).await;
    assert!(
        timed.is_err(),
        "the fake timeout must be bounded and cancellable"
    );

    let recorder = ProviderAuditRecorder::new();
    let event = recorder.create_and_record(
        "pe6-dispatch",
        "pe6-fake",
        "error",
        Some(&json!({"error_domain": "provider_timeout", "redaction_status": "redacted"})),
    );
    let encoded = serde_json::to_string(&event).unwrap();
    assert!(!encoded.contains("bounded input"));
    assert!(!encoded.contains("api_key"));
    assert_eq!(event.redaction_status, "redacted");
    emit_pe6_owner_evidence(
        "a deterministic fake provider, budget gate, and bounded retry executor were configured",
        "the provider returned retryable timeout errors and a slow invocation exceeded its timeout",
        "retries stopped at two calls, cost blocked before invoke, and sensitive output was redacted",
        vec![
            pe6_check("pe6.provider.timeout_cancellable", "recovery", "passed", "the slow fake invocation was cancelled by timeout"),
            pe6_check("pe6.provider.retry_budget_bounded", "recovery", "passed", "the timeout provider was called exactly twice"),
            pe6_check("pe6.provider.cost_gate_preinvoke", "integrity", "passed", "the cost gate prevented any provider call"),
            pe6_check("pe6.provider.output_redacted", "integrity", "passed", "the returned output omitted the test key marker"),
            pe6_check("pe6.provider.audit_redacted", "audit", "passed", "the audit event contained bounded redaction metadata only"),
            pe6_check("pe6.provider.rollback_not_exercised", "rollback", "unsupported", "provider execution has no release rollback target"),
            pe6_check("pe6.provider.restart_not_exercised", "restart", "unsupported", "no provider process restart was claimed"),
            pe6_check("pe6.provider.stack_resources_dropped", "cleanup", "passed", "all fake provider resources were process-local values"),
        ],
        "fake provider and audit recorder values were bounded to the test process",
    );
}

#[cfg(feature = "pg-tests")]
#[test]
fn pe6_postgres_atomicity_when_service_is_available() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        eprintln!("ACP_TEST_DATABASE_URL not set; PE-6 PostgreSQL drill is unsupported");
        return;
    };
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || url != "postgres://testuser:testpass@localhost:5432/testdb"
    {
        eprintln!(
            "PostgreSQL URL is not the disposable GitHub Actions service; PE-6 drill is unsupported"
        );
        return;
    }
    let store = LocalProductStore::new_postgres(&url, || "2026-07-13T00:00:00Z".to_string())
        .expect("ephemeral PostgreSQL store");
    let key = format!("pe6-drill-{}", uuid::Uuid::new_v4());
    let value = json!({"scenario": "pe6.storage.postgres.atomicity.v2"});
    let interrupted = store.inject_pg_config_transaction_failure_for_test(&key, &value);
    assert!(
        interrupted.is_err(),
        "the in-transaction interruption must fire"
    );
    assert!(store
        .config_snapshot()
        .expect("PG read after fault")
        .get(&key)
        .is_none());
    assert_eq!(
        store
            .audit_events(500)
            .expect("PG audit after fault")
            .iter()
            .filter(|event| event["resource"] == key)
            .count(),
        0,
        "the interrupted transaction must not append audit authority",
    );
    store
        .set_config_value(&key, value, "pe6-test")
        .expect("safe PG retry");
    assert!(store
        .config_snapshot()
        .expect("PG read after retry")
        .get(&key)
        .is_some());
    assert_eq!(store.check_integrity().expect("PG integrity").status, "ok");
    assert_eq!(
        store
            .audit_events(500)
            .expect("PG audit after retry")
            .iter()
            .filter(|event| event["resource"] == key && event["action"] == "config.update")
            .count(),
        1,
        "retry must create exactly one config authority audit",
    );
    store
        .cleanup_pg_fault_drill_for_test(&key)
        .expect("PG drill cleanup");
    assert!(store
        .config_snapshot()
        .expect("PG read after cleanup")
        .get(&key)
        .is_none());
    assert_eq!(
        store
            .audit_events(500)
            .expect("PG audit after cleanup")
            .iter()
            .filter(|event| event["resource"] == key)
            .count(),
        0,
    );
    emit_pe6_owner_evidence(
        "the disposable PostgreSQL key had no config or audit row before injection",
        "the test seam returned an error after config write but before audit and commit",
        "PostgreSQL rolled back both partial writes; one retry committed exactly one authority",
        vec![
            pe6_check(
                "pe6.postgres.partial_config_rolled_back",
                "recovery",
                "passed",
                "the key was absent after transaction interruption",
            ),
            pe6_check(
                "pe6.postgres.retry_single_authority",
                "integrity",
                "passed",
                "one retry produced one config row and one audit row",
            ),
            pe6_check(
                "pe6.postgres.interrupted_audit_absent",
                "audit",
                "passed",
                "the failed transaction produced no audit row",
            ),
            pe6_check(
                "pe6.postgres.rollback_is_transactional",
                "rollback",
                "passed",
                "the transaction drop restored the pre-fault empty state",
            ),
            pe6_check(
                "pe6.postgres.retry_safe_without_restart",
                "restart",
                "passed",
                "the same store safely retried after interruption",
            ),
            pe6_check(
                "pe6.postgres.rows_removed",
                "cleanup",
                "passed",
                "the disposable config and audit rows were removed",
            ),
        ],
        "the disposable PostgreSQL config and audit rows were deleted and observed absent",
    );
}
