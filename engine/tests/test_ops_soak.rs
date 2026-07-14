use std::sync::Arc;
use std::thread;

use engine::executor_pool::{register_default_executors, ExecutorPool};
use engine::node_executor::{FailNodeExecutor, NoopNodeExecutor};
use engine::storage::backup_manager::BackupManager;
use engine::storage::local_product_store::LocalProductStore;
use engine::workflow::backpressure::{Backpressure, BackpressureConfig};
use engine::workflow::dynamic_controller::{DynamicControllerConfig, DynamicWorkflowController};
use serde_json::{json, Value};
use tempfile::tempdir;

fn make_single_node_plan(ids: &engine::storage::local_product_store::WorkflowPlanIds) -> Value {
    json!({
        "schema_version": "read_only_plan.v1",
        "plan_id": ids.plan_id,
        "status": "planned_read_only",
        "workflow_id": ids.workflow_id,
        "dispatch_id": ids.dispatch_id,
        "analysis": {"analysis_id": "analysis-0001", "task_domain": "test"},
        "graph": {
            "schema_version": "workflow_graph.v1",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "status": "decomposed",
            "created_at": "2026-06-05T00:00:00Z",
            "updated_at": "2026-06-05T00:00:00Z",
            "nodes": [
                {
                    "schema_version": "workflow_node.v1",
                    "node_id": "node-a",
                    "workflow_id": ids.workflow_id,
                    "task_type": "command",
                    "assigned_agent_id": null,
                    "status": "pending",
                    "input_refs": [],
                    "output_ref": null,
                    "budget": 0.1,
                    "cost_incurred": 0.0,
                    "error": null,
                    "created_at": "2026-06-05T00:00:00Z",
                    "started_at": null,
                    "completed_at": null
                }
            ],
            "edges": [],
            "started_at": null,
            "completed_at": null,
            "result": null
        },
        "boundaries": {
            "execution": "disabled",
            "target_repository_writes": "disabled",
            "runtime_workers": "disabled",
        },
    })
}

fn make_plan_and_run(store: &LocalProductStore) -> (String, String) {
    let plan = store
        .create_workflow_plan("test run", "api", "actor", |ids, _| {
            Ok(make_single_node_plan(ids))
        })
        .unwrap();
    let plan_id = plan["plan_id"].as_str().unwrap().to_string();
    let run = store
        .create_workflow_run_from_plan(&plan_id, "actor")
        .unwrap();
    let run_id = run["run_id"].as_str().unwrap().to_string();
    (plan_id, run_id)
}

// ---------------------------------------------------------------------------
// 1. Multi-run soak: 3 runs with noop executor, all complete
// ---------------------------------------------------------------------------

#[test]
fn test_soak_multi_run_noop_executor() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let executor = NoopNodeExecutor;

    let mut run_ids = Vec::new();
    for i in 0..3 {
        let plan = store
            .create_workflow_plan(&format!("soak run {i}"), "api", "actor", |ids, _| {
                Ok(make_single_node_plan(ids))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
        run_ids.push(run["run_id"].as_str().unwrap().to_string());
    }

    for run_id in &run_ids {
        let result = store
            .tick_with_executor(run_id, "actor", 0, &executor)
            .unwrap();
        assert_eq!(result["action"], "node_executed");
        assert_eq!(result["result"]["status"], "completed");
    }

    for run_id in &run_ids {
        let run = store.get_workflow_run(run_id).unwrap().unwrap();
        assert_eq!(
            run["status"], "completed",
            "run {run_id} should be completed"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Mixed executor soak: noop and fail in one batch
// ---------------------------------------------------------------------------

#[test]
fn test_soak_multi_run_mixed_executors() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let (_, run_ok) = make_plan_and_run(&store);
    let (_, run_fail) = make_plan_and_run(&store);

    let r_ok = store
        .tick_with_executor(&run_ok, "actor", 0, &NoopNodeExecutor)
        .unwrap();
    assert_eq!(r_ok["action"], "node_executed");
    assert_eq!(r_ok["result"]["status"], "completed");

    let r_fail = store
        .tick_with_executor(&run_fail, "actor", 0, &FailNodeExecutor::default())
        .unwrap();
    assert_eq!(r_fail["action"], "node_executed");
    assert_eq!(r_fail["result"]["status"], "failed");

    assert_eq!(
        store.get_workflow_run(&run_ok).unwrap().unwrap()["status"],
        "completed"
    );
    assert_eq!(
        store.get_workflow_run(&run_fail).unwrap().unwrap()["status"],
        "failed"
    );
}

// ---------------------------------------------------------------------------
// 3. Sequential scheduler-style tick: 3 runs, tick rounds until terminal
// ---------------------------------------------------------------------------

#[test]
fn test_soak_scheduler_tick_sequential() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let executor = NoopNodeExecutor;

    let mut run_ids = Vec::new();
    for i in 0..3 {
        let plan = store
            .create_workflow_plan(&format!("sequential run {i}"), "api", "actor", |ids, _| {
                Ok(make_single_node_plan(ids))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
        run_ids.push(run["run_id"].as_str().unwrap().to_string());
    }

    for _round in 0..10 {
        for run_id in &run_ids {
            let run = store.get_workflow_run(run_id).unwrap().unwrap();
            if run["status"] == "completed" || run["status"] == "failed" {
                continue;
            }
            let _ = store.tick_with_executor(run_id, "actor", 0, &executor);
        }
    }

    for run_id in &run_ids {
        let run = store.get_workflow_run(run_id).unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }
}

// ---------------------------------------------------------------------------
// 4. Priority ordering across 5 runs
// ---------------------------------------------------------------------------

#[test]
fn test_soak_queue_priority_ordering() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    for i in 0..5 {
        let plan = store
            .create_workflow_plan(&format!("priority run {i}"), "api", "actor", |ids, _| {
                Ok(make_single_node_plan(ids))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
        let run_id = run["run_id"].as_str().unwrap();
        store.update_run_priority(run_id, (i + 1) as i64).unwrap();
    }

    let prioritized = store.list_active_workflow_runs_prioritized().unwrap();
    assert_eq!(prioritized.len(), 5);

    let priorities: Vec<i64> = prioritized
        .iter()
        .map(|r| r["priority"].as_i64().unwrap())
        .collect();
    for w in priorities.windows(2) {
        assert!(
            w[0] <= w[1],
            "priorities should be ascending: {:?}",
            priorities
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Backpressure lifecycle: activate at high util, deactivate at low util
// ---------------------------------------------------------------------------

#[test]
fn test_soak_backpressure_lifecycle() {
    let config = BackpressureConfig {
        enabled: true,
        activation_threshold: 0.5,
        deactivation_threshold: 0.2,
        max_paused_runs: 10,
        degrade_concurrency_factor: 0.5,
        cooldown_after_pause_ms: 1000,
    };
    let mut bp = Backpressure::new(config);

    let d1 = bp.evaluate(0.9, 0, 100, 0, 1000, None);
    assert!(d1.active, "should activate at 0.9 utilization");

    let d2 = bp.evaluate(0.3, 0, 100, 0, 2000, None);
    assert!(
        d2.active,
        "should remain active above deactivation threshold"
    );

    let d3 = bp.evaluate(0.05, 0, 100, 0, 3000, None);
    assert!(!d3.active, "should deactivate below threshold");
}

// ---------------------------------------------------------------------------
// 6. Executor pool: failure score tracking with cooldown recovery
// ---------------------------------------------------------------------------

#[test]
fn test_soak_executor_pool_failure_tracking() {
    let pool = ExecutorPool::new();
    register_default_executors(
        &pool,
        false,
        Arc::new(LocalProductStore::new(":memory:").unwrap()),
    );

    // Record failures via release(success=false)
    for _ in 0..5 {
        assert!(pool.acquire("noop"), "should acquire noop");
        pool.release("noop", false, 5000, None);
    }

    let snapshot = pool.snapshot();
    let entry = snapshot.iter().find(|e| e.executor_type == "noop").unwrap();
    assert!(
        entry.status.failure_score > 0.0,
        "failure score should accumulate after failures"
    );
    assert!(
        entry.metrics.failed_executions > 0,
        "should have recorded failed executions"
    );

    // Verify cooldown was triggered
    assert!(
        entry.status.cooldown_until.is_some(),
        "should be on cooldown after repeated failures"
    );
    assert!(
        !entry.status.available,
        "should be unavailable during cooldown"
    );

    // Verify total execution count matches
    assert_eq!(
        entry.metrics.total_executions, 5,
        "should have 5 total executions"
    );
}

// ---------------------------------------------------------------------------
// 7. Backup during active writes: checkpoints ensure different checksums
// ---------------------------------------------------------------------------

#[test]
fn test_soak_backup_during_active_store() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();
    let backup_dir = dir.path().join("backups");
    let bm = BackupManager::new(&backup_dir).unwrap();

    store
        .record_dispatch(
            "req-1",
            "api",
            &json!({
                "record": {"dispatch_id": "d1", "final_status": "noop_completed"},
                "decision": {"selected_tier": "noop"},
                "analysis": {"risk_level": "low"},
            }),
            "actor",
        )
        .unwrap();
    store.checkpoint_wal().unwrap();

    let backup1 = bm
        .create_backup(
            &dir.path().join("test.db"),
            "after-d1",
            "b1",
            "2026-06-07T00:00:00Z",
        )
        .unwrap();
    bm.save_metadata(std::slice::from_ref(&backup1)).unwrap();
    assert_eq!(backup1.backup_id, "b1");
    assert!(backup1.size_bytes > 0);

    store
        .record_dispatch(
            "req-2",
            "api",
            &json!({
                "record": {"dispatch_id": "d2", "final_status": "noop_completed"},
                "decision": {"selected_tier": "noop"},
                "analysis": {"risk_level": "low"},
            }),
            "actor",
        )
        .unwrap();
    store.checkpoint_wal().unwrap();

    let backup2 = bm
        .create_backup(
            &dir.path().join("test.db"),
            "after-d2",
            "b2",
            "2026-06-07T00:01:00Z",
        )
        .unwrap();
    bm.save_metadata(std::slice::from_ref(&backup2)).unwrap();

    assert_ne!(
        backup1.checksum, backup2.checksum,
        "backups should differ after additional data"
    );
    assert_eq!(backup2.backup_id, "b2");
}

// ---------------------------------------------------------------------------
// 8. Backup restore dry-run: verify integrity without side effects
// ---------------------------------------------------------------------------

#[test]
fn test_soak_backup_restore_dry_run() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    store
        .record_dispatch(
            "req-1",
            "api",
            &json!({
                "record": {"dispatch_id": "d1", "final_status": "noop_completed"},
                "decision": {"selected_tier": "noop"},
                "analysis": {"risk_level": "low"},
            }),
            "actor",
        )
        .unwrap();
    store.checkpoint_wal().unwrap();

    let backup_dir = dir.path().join("backups");
    let bm = BackupManager::new(&backup_dir).unwrap();
    let backup = bm
        .create_backup(
            &dir.path().join("test.db"),
            "test",
            "b1",
            "2026-06-07T00:00:00Z",
        )
        .unwrap();
    bm.save_metadata(std::slice::from_ref(&backup)).unwrap();
    assert_eq!(backup.backup_id, "b1");

    let dry_run = bm
        .restore_dry_run(&backup.backup_id, &dir.path().join("test.db"))
        .unwrap();
    assert!(dry_run.dry_run, "should be a dry run");
    assert!(dry_run.success, "dry run should succeed");
    assert!(dry_run.checksum_ok, "checksum should match");
}

// ---------------------------------------------------------------------------
// 9. DynamicWorkflowController: decision trace accumulates across ticks
// ---------------------------------------------------------------------------

#[test]
fn test_soak_decision_trace_accumulation() {
    let store = LocalProductStore::new(":memory:").unwrap();
    let (_, run_id) = make_plan_and_run(&store);

    let config = DynamicControllerConfig {
        max_ticks_per_run: 10,
        auto_fix_on_failure: false,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);

    for _ in 0..5 {
        let _ = ctrl.tick(&store, &run_id, "actor", &NoopNodeExecutor);
    }

    assert_eq!(ctrl.ticks_executed(), 5, "should have executed 5 ticks");
    assert!(
        !ctrl.decisions().is_empty(),
        "should have at least one decision"
    );
}

// ---------------------------------------------------------------------------
// 10. DynamicWorkflowController multi-tick: run reaches terminal
// ---------------------------------------------------------------------------

#[test]
fn test_soak_dynamic_controller_multi_tick() {
    let store = LocalProductStore::new(":memory:").unwrap();
    let (_, run_id) = make_plan_and_run(&store);

    let config = DynamicControllerConfig {
        max_ticks_per_run: 20,
        auto_fix_on_failure: false,
        ..Default::default()
    };
    let mut ctrl = DynamicWorkflowController::new(config);

    for _ in 0..20 {
        let result = ctrl
            .tick(&store, &run_id, "actor", &NoopNodeExecutor)
            .unwrap();
        if !result.should_continue {
            break;
        }
    }

    let run = store.get_workflow_run(&run_id).unwrap().unwrap();
    let final_status = run["status"].as_str().unwrap();
    assert!(
        final_status == "completed" || final_status == "failed",
        "run should be terminal, got {final_status}"
    );
}

// ---------------------------------------------------------------------------
// 11. Concurrent store writes: 4 threads each insert a dispatch
// ---------------------------------------------------------------------------

#[test]
fn test_soak_concurrent_store_writes() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());

    let mut handles = Vec::new();
    for i in 0..4 {
        let store_clone = Arc::clone(&store);
        handles.push(thread::spawn(move || {
            store_clone
                .record_dispatch(
                    &format!("req-{i}"),
                    "api",
                    &json!({
                        "record": {"dispatch_id": format!("d{i}"), "final_status": "noop_completed"},
                        "decision": {"selected_tier": "noop"},
                        "analysis": {"risk_level": "low"},
                    }),
                    "actor",
                )
                .unwrap();
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let dispatches = store.list_dispatches(10).unwrap();
    assert_eq!(
        dispatches.len(),
        4,
        "should have 4 dispatches after concurrent writes"
    );
}

// ---------------------------------------------------------------------------
// 12. Audit events accumulate over time
// ---------------------------------------------------------------------------

#[test]
fn test_soak_audit_event_accumulation() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    for i in 0..10 {
        store
            .append_audit(
                "actor",
                "config.change",
                &format!("key-{i}"),
                &json!({"index": i}),
            )
            .unwrap();
    }

    let events = store.audit_events(20).unwrap();
    assert_eq!(events.len(), 10, "should have 10 audit events");

    let indices: Vec<i64> = events
        .iter()
        .filter_map(|e| e["details"]["index"].as_i64())
        .collect();
    assert_eq!(indices.len(), 10);
}

// ---------------------------------------------------------------------------
// 13. Cost tracking accumulates across dispatches
// ---------------------------------------------------------------------------

#[test]
fn test_soak_cost_tracking_accumulation() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    for i in 0..5 {
        let cost = (i as f64 + 1.0) * 0.01;
        store
            .record_dispatch(
                &format!("req-{i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("d{i}"), "final_status": "noop_completed"},
                    "decision": {"selected_tier": "noop"},
                    "analysis": {"risk_level": "low"},
                    "execution_result": {
                        "estimated_cost": cost,
                        "input_tokens": 100 * (i as i64 + 1),
                        "output_tokens": 50 * (i as i64 + 1),
                        "executor_type": "noop",
                        "latency_ms": 10,
                    },
                }),
                "actor",
            )
            .unwrap();
    }

    let summary = store.cost_summary().unwrap();
    let total = summary["total_estimated_cost_usd"].as_f64().unwrap();
    assert!(
        total > 0.0,
        "total cost should be positive after dispatches with costs"
    );
}

// ---------------------------------------------------------------------------
// 14. Integrity check passes after loading diverse data
// ---------------------------------------------------------------------------

#[test]
fn test_soak_integrity_check_after_load() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    for i in 0..3 {
        store
            .record_dispatch(
                &format!("req-{i}"),
                "api",
                &json!({
                    "record": {"dispatch_id": format!("d{i}"), "final_status": "noop_completed"},
                    "decision": {"selected_tier": "noop"},
                    "analysis": {"risk_level": "low"},
                }),
                "actor",
            )
            .unwrap();
    }

    for i in 0..3 {
        let plan = store
            .create_workflow_plan(&format!("integrity run {i}"), "api", "actor", |ids, _| {
                Ok(make_single_node_plan(ids))
            })
            .unwrap();
        store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
    }

    for i in 0..3 {
        store
            .append_audit("actor", "test.event", &format!("res-{i}"), &json!({}))
            .unwrap();
    }

    let report = store.check_integrity().unwrap();
    assert_eq!(
        report.status, "ok",
        "integrity should pass after loading diverse data"
    );

    let table_names: Vec<&str> = report.tables.iter().map(|t| t.name.as_str()).collect();
    assert!(table_names.contains(&"dispatch_history"));
    assert!(table_names.contains(&"workflow_runs"));
    assert!(table_names.contains(&"audit_log"));

    let dispatch_table = report
        .tables
        .iter()
        .find(|t| t.name == "dispatch_history")
        .unwrap();
    assert!(dispatch_table.row_count >= 3);
}

// ---------------------------------------------------------------------------
// 15. Export/import roundtrip preserves data
// ---------------------------------------------------------------------------

#[test]
fn test_soak_export_import_roundtrip() {
    let dir = tempdir().unwrap();
    let store1 = LocalProductStore::new(dir.path().join("source.db")).unwrap();

    store1
        .record_dispatch(
            "req-1",
            "api",
            &json!({
                "record": {"dispatch_id": "d1", "final_status": "noop_completed"},
                "decision": {"selected_tier": "noop"},
                "analysis": {"risk_level": "low"},
            }),
            "actor",
        )
        .unwrap();

    store1
        .set_config_value("my_key", json!("my_value"), "actor")
        .unwrap();

    let snapshot = store1.export_snapshot("noop", false).unwrap();
    assert!(snapshot.is_object());
    assert!(
        !snapshot["dispatches"].as_array().unwrap().is_empty(),
        "export should contain dispatches"
    );

    let store2 = LocalProductStore::new(dir.path().join("target.db")).unwrap();
    let result = store2.import_snapshot(&snapshot).unwrap();
    assert!(
        result.errors.is_empty(),
        "import should have no errors: {:?}",
        result.errors
    );

    let dispatches = store2.list_dispatches(10).unwrap();
    assert_eq!(dispatches.len(), 1, "imported store should have 1 dispatch");
    assert_eq!(dispatches[0]["dispatch_id"], "d1");
}

// ---------------------------------------------------------------------------
// 16. Concurrent tick on different runs: no cross-run interference
// ---------------------------------------------------------------------------

#[test]
fn test_soak_concurrent_tick_different_runs() {
    let dir = tempdir().unwrap();
    let store = Arc::new(LocalProductStore::new(dir.path().join("test.db")).unwrap());
    let thread_count = 6;

    let mut run_ids = Vec::new();
    for i in 0..thread_count {
        let plan = store
            .create_workflow_plan(&format!("concurrent run {i}"), "api", "actor", |ids, _| {
                Ok(make_single_node_plan(ids))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
        run_ids.push(run["run_id"].as_str().unwrap().to_string());
    }
    let run_ids = Arc::new(run_ids);

    let mut handles = Vec::new();
    for t in 0..thread_count {
        let store = Arc::clone(&store);
        let run_ids = Arc::clone(&run_ids);
        handles.push(thread::spawn(move || {
            let executor = NoopNodeExecutor;
            let run_id = &run_ids[t];
            store.tick_with_executor(run_id, "thread-{t}", 0, &executor)
        }));
    }

    let mut success_count = 0;
    for h in handles {
        if let Ok(Ok(result)) = h.join() {
            if result["action"] == "node_executed" {
                success_count += 1;
            }
        }
    }

    assert_eq!(
        success_count, thread_count,
        "all threads should execute their run"
    );

    for run_id in run_ids.iter() {
        let run = store.get_workflow_run(run_id).unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }
}

// ---------------------------------------------------------------------------
// 17. Pause/resume cycle: pause blocks tick, resume allows it
// ---------------------------------------------------------------------------

#[test]
fn test_soak_pause_resume_cycle() {
    let store = LocalProductStore::new(":memory:").unwrap();
    let (_, run_id) = make_plan_and_run(&store);

    store
        .update_run_pause_reason(&run_id, Some("backpressure"))
        .unwrap();

    let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig {
        admission_check_enabled: true,
        ..Default::default()
    });
    let result = ctrl
        .tick(&store, &run_id, "actor", &NoopNodeExecutor)
        .unwrap();
    assert!(!result.should_continue, "paused run should not continue");
    assert!(
        !result.admission_allowed,
        "paused run should not be admitted"
    );

    store.update_run_pause_reason(&run_id, None).unwrap();

    let result = ctrl
        .tick(&store, &run_id, "actor", &NoopNodeExecutor)
        .unwrap();
    assert!(result.admission_allowed, "unpaused run should be admitted");
}

// ---------------------------------------------------------------------------
// 18. Tenant queue breakdown
// ---------------------------------------------------------------------------

#[test]
fn test_soak_tenant_queue_breakdown() {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("test.db")).unwrap();

    let plan = store
        .create_workflow_plan("tenant test", "api", "actor", |ids, _| {
            Ok(make_single_node_plan(ids))
        })
        .unwrap();
    let plan_id = plan["plan_id"].as_str().unwrap().to_string();

    for _ in 0..3 {
        store
            .create_workflow_run_with_queue_metadata(
                &plan_id,
                "actor",
                5,
                None,
                None,
                Some("alpha"),
            )
            .unwrap();
    }
    for _ in 0..2 {
        store
            .create_workflow_run_with_queue_metadata(&plan_id, "actor", 3, None, None, Some("beta"))
            .unwrap();
    }

    let tenants = store.list_tenants_with_quota().unwrap();
    assert!(tenants.len() >= 2, "should have at least 2 tenants");

    let tenant_ids: Vec<&str> = tenants
        .iter()
        .map(|t| t["tenant_id"].as_str().unwrap())
        .collect();
    assert!(tenant_ids.contains(&"alpha"));
    assert!(tenant_ids.contains(&"beta"));

    let status = store.get_queue_status().unwrap();
    assert!(status["total_queued"].as_i64().unwrap_or(0) >= 5);
}

// ---------------------------------------------------------------------------
// 19. Lease recovery: stale lease gets recovered
// ---------------------------------------------------------------------------

#[test]
fn test_soak_stale_lease_recovery() {
    let store = LocalProductStore::new(":memory:").unwrap();
    let (_, _run_id) = make_plan_and_run(&store);

    let leased = store
        .set_pending_node_to_running_for_test("2020-01-01T00:00:00Z")
        .unwrap();
    assert!(leased > 0, "should have leased at least one node");

    let recovered = store.recover_stale_leases(60_000).unwrap();
    assert!(recovered > 0, "should have recovered stale lease");
}

// ---------------------------------------------------------------------------
// 20. End-to-end ops soak drill: create -> execute -> integrity -> backup -> export
// ---------------------------------------------------------------------------

#[test]
fn test_e2e_ops_soak_drill() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("e2e-soak.db");
    let store = Arc::new(LocalProductStore::new(&db_path).unwrap());
    let pool = ExecutorPool::new();
    register_default_executors(&pool, false, store.clone());

    let run_count = 10;
    let mut run_ids = Vec::new();

    // Phase 1: Create and execute runs
    for i in 0..run_count {
        let plan = store
            .create_workflow_plan(&format!("e2e run {i}"), "api", "actor", |ids, _| {
                Ok(make_single_node_plan(ids))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "actor")
            .unwrap();
        run_ids.push(run["run_id"].as_str().unwrap().to_string());
    }

    for run_id in &run_ids {
        store
            .tick_with_executor(run_id, "e2e-actor", 0, &NoopNodeExecutor)
            .unwrap();
    }

    // Phase 2: Record pool activity
    for _ in 0..3 {
        pool.acquire("noop");
        pool.release("noop", true, 50, Some(0.001));
    }
    let snapshot = pool.snapshot();
    assert!(!snapshot.is_empty(), "pool should have entries");

    // Phase 3: Integrity check
    let integrity = store.check_integrity().unwrap();
    assert_eq!(
        integrity.status, "ok",
        "integrity should pass after e2e soak"
    );

    // Phase 4: Export
    let export = store.export_snapshot("noop", false).unwrap();
    assert!(export.is_object(), "export should succeed");

    // Phase 5: Backup
    let backup_dir = dir.path().join("backups");
    store.checkpoint_wal().unwrap();
    let bm = BackupManager::new(&backup_dir).unwrap();
    let record = bm
        .create_backup(&db_path, "e2e-soak", "backup-e2e", "2026-06-07T00:00:00Z")
        .unwrap();
    bm.save_metadata(std::slice::from_ref(&record)).unwrap();
    assert!(record.size_bytes > 0);

    let verify = bm.verify_backup("backup-e2e").unwrap();
    assert!(verify.checksum_ok);
    assert!(verify.integrity_ok);

    // Phase 6: Queue status
    let queue_status = store.get_queue_status().unwrap();
    assert!(queue_status.get("total_queued").is_some());

    // Phase 7: Verify all runs are terminal
    let mut terminal = 0;
    for run_id in &run_ids {
        let run = store.get_workflow_run(run_id).unwrap().unwrap();
        if run["status"] == "completed" || run["status"] == "failed" {
            terminal += 1;
        }
    }
    assert_eq!(
        terminal, run_count,
        "all {run_count} runs should be terminal"
    );
}
