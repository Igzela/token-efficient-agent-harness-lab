use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::node_executor::NoopNodeExecutor;
use crate::storage::local_product_store::LocalProductStore;

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub interval_ms: u64,
    pub max_concurrent: usize,
    pub lease_timeout_ms: u64,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            interval_ms: 2000,
            max_concurrent: 4,
            lease_timeout_ms: 300_000,
        }
    }
}

pub struct WorkflowScheduler {
    store: Arc<LocalProductStore>,
    config: SchedulerConfig,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    started_at: Option<String>,
    tick_count: Arc<std::sync::atomic::AtomicU64>,
    error_count: Arc<std::sync::atomic::AtomicU64>,
    last_tick_at: Arc<std::sync::Mutex<Option<String>>>,
    last_error: Arc<std::sync::Mutex<Option<String>>>,
}

impl WorkflowScheduler {
    pub fn new(store: Arc<LocalProductStore>, config: SchedulerConfig) -> Self {
        Self {
            store,
            config,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
            started_at: None,
            tick_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            error_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            last_tick_at: Arc::new(std::sync::Mutex::new(None)),
            last_error: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn start(&mut self) -> Result<(), String> {
        if self.running.load(Ordering::SeqCst) {
            return Err("scheduler already running".to_string());
        }
        self.running.store(true, Ordering::SeqCst);
        let store = self.store.clone();
        let config = self.config.clone();
        let running = self.running.clone();
        let tick_count = self.tick_count.clone();
        let error_count = self.error_count.clone();
        let last_tick_at = self.last_tick_at.clone();
        let last_error = self.last_error.clone();

        let handle = std::thread::spawn(move || {
            let executor = NoopNodeExecutor;
            while running.load(Ordering::SeqCst) {
                let tick_result = scheduler_tick(&store, &config, &executor);
                match tick_result {
                    Ok(ticks) => {
                        tick_count.fetch_add(ticks, Ordering::SeqCst);
                        if let Ok(mut guard) = last_tick_at.lock() {
                            *guard = Some(
                                chrono::Utc::now()
                                    .format("%Y-%m-%dT%H:%M:%SZ")
                                    .to_string(),
                            );
                        }
                    }
                    Err(e) => {
                        error_count.fetch_add(1, Ordering::SeqCst);
                        if let Ok(mut guard) = last_error.lock() {
                            *guard = Some(e);
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(config.interval_ms));
            }
        });

        self.handle = Some(handle);
        self.started_at = Some(
            chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string(),
        );
        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        if !self.running.load(Ordering::SeqCst) {
            return Err("scheduler not running".to_string());
        }
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| "scheduler thread panicked".to_string())?;
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn status(&self) -> Value {
        json!({
            "schema_version": "scheduler.v1",
            "running": self.is_running(),
            "started_at": self.started_at,
            "config": {
                "interval_ms": self.config.interval_ms,
                "max_concurrent": self.config.max_concurrent,
                "lease_timeout_ms": self.config.lease_timeout_ms,
            },
            "tick_count": self.tick_count.load(Ordering::SeqCst),
            "error_count": self.error_count.load(Ordering::SeqCst),
            "last_tick_at": self.last_tick_at.lock().ok().and_then(|g| g.clone()),
            "last_error": self.last_error.lock().ok().and_then(|g| g.clone()),
        })
    }
}

impl Drop for WorkflowScheduler {
    fn drop(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            self.running.store(false, Ordering::SeqCst);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

fn scheduler_tick(
    store: &LocalProductStore,
    config: &SchedulerConfig,
    executor: &dyn crate::node_executor::NodeExecutor,
) -> Result<u64, String> {
    let _recovered = store.recover_stale_leases(config.lease_timeout_ms)?;

    let active_runs = store.list_active_workflow_run_ids()?;
    let mut ticks = 0u64;
    for run_id in active_runs.iter().take(config.max_concurrent) {
        match store.tick_with_executor(run_id, "scheduler", 0, executor) {
            Ok(_) => {
                ticks += 1;
            }
            Err(_) => {
                // terminal or no-ready-node errors are expected; skip
            }
        }
    }
    Ok(ticks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local_product_store::LocalProductStore;
    use serde_json::json;

    fn test_store() -> Arc<LocalProductStore> {
        Arc::new(LocalProductStore::new(":memory:").unwrap())
    }

    fn make_plan_value(
        ids: &crate::read_only_planner::WorkflowPlanIds,
    ) -> Value {
        json!({
            "schema_version": "read_only_plan.v1",
            "plan_id": ids.plan_id,
            "status": "planned_read_only",
            "workflow_id": ids.workflow_id,
            "dispatch_id": ids.dispatch_id,
            "analysis": {"analysis_id": "a-1", "task_domain": "docs"},
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
                        "task_type": "analysis",
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
            },
            "boundaries": {
                "execution_authority": "disabled",
                "target_repository_writes": "disabled",
                "runtime_workers": "disabled",
            },
        })
    }

    fn create_plan_and_run(store: &LocalProductStore) -> String {
        let plan = store
            .create_workflow_plan("fix auth bug", "test", "actor", |ids, _| {
                Ok(make_plan_value(ids))
            })
            .unwrap();
        let plan_id = plan["plan_id"].as_str().unwrap();
        let run = store.create_workflow_run_from_plan(plan_id, "actor").unwrap();
        run["run_id"].as_str().unwrap().to_string()
    }

    #[test]
    fn scheduler_config_defaults() {
        let config = SchedulerConfig::default();
        assert_eq!(config.interval_ms, 2000);
        assert_eq!(config.max_concurrent, 4);
        assert_eq!(config.lease_timeout_ms, 300_000);
    }

    #[test]
    fn scheduler_start_stop() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
        };
        let mut scheduler = WorkflowScheduler::new(store, config);
        assert!(!scheduler.is_running());

        scheduler.start().unwrap();
        assert!(scheduler.is_running());

        assert!(scheduler.start().is_err());

        scheduler.stop().unwrap();
        assert!(!scheduler.is_running());

        assert!(scheduler.stop().is_err());
    }

    #[test]
    fn scheduler_status_reports_state() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            lease_timeout_ms: 60_000,
        };
        let mut scheduler = WorkflowScheduler::new(store, config);

        let status = scheduler.status();
        assert_eq!(status["running"], false);
        assert_eq!(status["tick_count"], 0);
        assert_eq!(status["config"]["interval_ms"], 50);

        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(120));

        let status = scheduler.status();
        assert_eq!(status["running"], true);
        assert!(status["started_at"].as_str().is_some());

        scheduler.stop().unwrap();
    }

    #[test]
    fn scheduler_ticks_active_runs() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 4,
            lease_timeout_ms: 300_000,
        };
        let mut scheduler = WorkflowScheduler::new(store.clone(), config);
        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        scheduler.stop().unwrap();

        let status = scheduler.status();
        assert!(status["tick_count"].as_u64().unwrap() > 0);

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }

    #[test]
    fn scheduler_lease_recovery_resets_stale_nodes() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        store
            .set_pending_node_to_running_for_test("2020-01-01T00:00:00Z")
            .unwrap();

        let recovered = store.recover_stale_leases(60_000).unwrap();
        assert!(recovered > 0);

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        let nodes = run["nodes"].as_array().unwrap();
        let has_pending = nodes.iter().any(|n| n["db_status"] == "pending");
        assert!(has_pending, "stale lease should reset node to pending");
    }

    #[test]
    fn scheduler_lease_recovery_ignores_fresh_leases() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);
        store.tick_workflow_run(&run_id, "test").unwrap();

        let recovered = store.recover_stale_leases(300_000).unwrap();
        assert_eq!(recovered, 0, "fresh leases should not be recovered");
    }

    #[test]
    fn scheduler_list_active_run_ids() {
        let store = test_store();
        let active = store.list_active_workflow_run_ids().unwrap();
        assert!(active.is_empty());

        create_plan_and_run(&store);
        let active = store.list_active_workflow_run_ids().unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0], "run-0001");
    }

    #[test]
    fn scheduler_stop_on_drop() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
        };
        let mut scheduler = WorkflowScheduler::new(store, config);
        scheduler.start().unwrap();
        assert!(scheduler.is_running());
        drop(scheduler);
    }

    #[test]
    fn scheduler_tick_no_active_runs() {
        let store = test_store();
        let config = SchedulerConfig::default();
        let executor = NoopNodeExecutor;
        let ticks = scheduler_tick(&store, &config, &executor).unwrap();
        assert_eq!(ticks, 0);
    }
}
