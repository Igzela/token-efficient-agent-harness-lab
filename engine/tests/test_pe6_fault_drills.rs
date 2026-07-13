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
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

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
    let db_path = directory.path().join("pe6.sqlite");
    let store = LocalProductStore::new(&db_path).expect("SQLite store");

    let run_id = new_run(&store, None);
    store
        .append_audit(
            "pe6-test",
            "pe6.drill.started",
            &run_id,
            &json!({"run_id": run_id, "scenario_id": "pe6.storage.sqlite.atomicity.v1"}),
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
}

#[test]
fn pe6_sqlite_backup_restore_and_cleanup() {
    let directory = tempdir().expect("disposable backup directory");
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
}

#[test]
fn pe6_workflow_timeout_retry_concurrency_and_restart() {
    let directory = tempdir().expect("disposable workflow directory");
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
async fn pe6_provider_timeout_kill_budget_audit_and_redaction() {
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
    store
        .set_config_value(
            &key,
            json!({"scenario": "pe6.storage.postgres.atomicity.v1"}),
            "pe6-test",
        )
        .expect("PG transaction");
    assert!(store
        .config_snapshot()
        .expect("PG read")
        .get(&key)
        .is_some());
    assert_eq!(store.check_integrity().expect("PG integrity").status, "ok");
    store
        .append_audit("pe6-test", "pe6.pg.drill", &key, &json!({"bounded": true}))
        .expect("PG audit");
}
