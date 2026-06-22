use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use engine::provider::adaptive_execution::{
    AdaptiveExecutionExecutor, AdaptiveExecutionGate, AdaptiveExecutionKillSwitch,
    PersistingAdaptiveProviderNodeExecutor,
};
use engine::provider::{Provider, ProviderAuditRecorder};
use engine::scheduler::{SchedulerConfig, WorkflowScheduler};
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use tempfile::tempdir;

fn adaptive_plan(ids: &engine::read_only_planner::WorkflowPlanIds, candidate_id: &str) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "a-1", "task_domain": "coding"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-06-22T00:00:00Z",
            "updated_at": "2026-06-22T00:00:00Z",
            "nodes": [{
                "node_id": "node-a",
                "task_type": "implementation",
                "status": "pending",
                "adaptive_execution": {
                    "observation_context": {
                        "request_id": "request-trusted-worker",
                        "task_class": "coding",
                        "objective": "quality",
                        "risk_level": "low",
                        "candidate_id": candidate_id,
                        "policy_hash": null
                    },
                    "plan": {
                        "mode": "single",
                        "endpoint": {
                            "endpoint_id": "worker-stub",
                            "model": "worker-model",
                            "reserved_cost_usd": 0.02
                        }
                    },
                    "limits": {
                        "max_calls": 1,
                        "max_cost_usd": 0.02,
                        "max_elapsed_ms": 1000,
                        "max_concurrency": 1,
                        "max_total_tokens": 4096
                    }
                }
            }],
            "edges": []
        },
        "boundaries": {
            "execution_authority": "trusted_local_bounded",
            "target_repository_writes": "disabled",
            "runtime_workers": "bounded"
        }
    })
}

#[test]
fn trusted_worker_advances_explicit_adaptive_plan_and_persists_safe_observation() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("trusted-worker.db")).unwrap());
    let provider = Arc::new(
        engine::provider::stub::StubProvider::new("worker-stub").with_default_model("worker-model"),
    ) as Arc<dyn Provider>;
    let executor = Arc::new(AdaptiveExecutionExecutor::new(
        BTreeMap::from([("worker-stub".to_string(), provider)]),
        Arc::new(ProviderAuditRecorder::with_store(store.clone())),
        AdaptiveExecutionKillSwitch::default(),
    ));
    let worker = Arc::new(PersistingAdaptiveProviderNodeExecutor::new(
        executor,
        AdaptiveExecutionGate::from_flags(true, true, true),
        store.clone(),
        "scheduler",
    ));
    let plan = store
        .create_workflow_plan("bounded adaptive task", "test", "actor", |ids, _| {
            Ok(adaptive_plan(ids, "single-trusted-worker"))
        })
        .unwrap();
    let run = store
        .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap().to_string();
    let config = SchedulerConfig {
        interval_ms: 20,
        max_concurrent: 1,
        lease_timeout_ms: 60_000,
        executor_type: "adaptive_provider".to_string(),
        worker_count: 1,
        supervised_workers_enabled: true,
        ..Default::default()
    };
    let mut scheduler = WorkflowScheduler::new(store.clone(), config).with_worker_executor(worker);

    scheduler.start().unwrap();
    std::thread::sleep(Duration::from_millis(120));
    scheduler.stop().unwrap();

    let completed = store.get_workflow_run(&run_id).unwrap().unwrap();
    assert_eq!(completed["status"], "completed");
    assert_eq!(
        completed["nodes"][0]["result"]["executor_type"],
        "adaptive_provider"
    );
    let observations = store.adaptive_observations().unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(observations[0].run_id, run_id);
    assert_eq!(observations[0].candidate_id, "single-trusted-worker");
    assert_eq!(observations[0].candidate_kind, "single");
    assert!(observations[0].success);
}
