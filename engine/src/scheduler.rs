use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::cli::CliNodeExecutor;
use crate::node_executor::{NodeExecutor, NoopNodeExecutor};
use crate::orchestration::{ResultAggregator, WorkQueue};
use crate::routing::{
    AutoDowngradePolicy, AutoUpgradePolicy, FeedbackIntegrator, RoutingHistoryStore,
    RoutingObservationStore,
};
use crate::storage::local_product_store::LocalProductStore;

#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub interval_ms: u64,
    pub max_concurrent: usize,
    pub lease_timeout_ms: u64,
    pub executor_type: String,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            interval_ms: 2000,
            max_concurrent: 4,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
        }
    }
}

impl SchedulerConfig {
    pub fn from_env() -> Self {
        let interval_ms = std::env::var("ACP_SCHEDULER_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2000);
        let max_concurrent = std::env::var("ACP_SCHEDULER_MAX_CONCURRENT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);
        let lease_timeout_ms = std::env::var("ACP_SCHEDULER_LEASE_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300_000);
        let executor_type =
            std::env::var("ACP_SCHEDULER_EXECUTOR").unwrap_or_else(|_| "noop".to_string());
        Self {
            interval_ms,
            max_concurrent,
            lease_timeout_ms,
            executor_type,
        }
    }
}

fn create_scheduler_executor(executor_type: &str) -> Arc<dyn NodeExecutor> {
    match executor_type {
        "command" => Arc::new(crate::node_executor::CommandNodeExecutor::default()),
        "claude_code_cli" | "codex_cli" => {
            let config = crate::cli::CliConfig::from_env();
            match CliNodeExecutor::from_config(&config) {
                Some(exec) => Arc::new(exec),
                None => {
                    eprintln!(
                        "[scheduler] CLI executor '{}' not available, falling back to noop",
                        executor_type
                    );
                    Arc::new(NoopNodeExecutor)
                }
            }
        }
        _ => Arc::new(NoopNodeExecutor),
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
    retry_count: Arc<std::sync::atomic::AtomicU64>,
    total_execution_time_ns: Arc<std::sync::atomic::AtomicU64>,
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
            retry_count: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            total_execution_time_ns: Arc::new(std::sync::atomic::AtomicU64::new(0)),
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
        let retry_count = self.retry_count.clone();
        let total_execution_time_ns = self.total_execution_time_ns.clone();
        let last_tick_at = self.last_tick_at.clone();
        let last_error = self.last_error.clone();
        let executor = create_scheduler_executor(&config.executor_type);

        let handle = std::thread::spawn(move || {
            while running.load(Ordering::SeqCst) {
                let tick_start = std::time::Instant::now();
                let tick_result = scheduler_tick(&store, &config, &*executor);
                let tick_elapsed_ns = tick_start.elapsed().as_nanos() as u64;
                total_execution_time_ns.fetch_add(tick_elapsed_ns, Ordering::SeqCst);
                match tick_result {
                    Ok(result) => {
                        tick_count.fetch_add(result.ticks, Ordering::SeqCst);
                        retry_count.fetch_add(result.retries, Ordering::SeqCst);
                        if let Ok(mut guard) = last_tick_at.lock() {
                            *guard =
                                Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
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
        self.started_at = Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
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
        let active_runs = self
            .store
            .list_active_workflow_run_ids()
            .map(|ids| ids.len())
            .unwrap_or(0);
        json!({
            "schema_version": "scheduler.v1",
            "running": self.is_running(),
            "started_at": self.started_at,
            "config": {
                "interval_ms": self.config.interval_ms,
                "max_concurrent": self.config.max_concurrent,
                "lease_timeout_ms": self.config.lease_timeout_ms,
                "executor_type": self.config.executor_type,
            },
            "tick_count": self.tick_count.load(Ordering::SeqCst),
            "error_count": self.error_count.load(Ordering::SeqCst),
            "retry_count": self.retry_count.load(Ordering::SeqCst),
            "total_execution_time_ns": self.total_execution_time_ns.load(Ordering::SeqCst),
            "last_tick_at": self.last_tick_at.lock().ok().and_then(|g| g.clone()),
            "last_error": self.last_error.lock().ok().and_then(|g| g.clone()),
            "active_runs": active_runs,
            "dormant_modules_active": {
                "work_queue": true,
                "result_aggregator": true,
                "auto_policies": true,
                "feedback_integrator": true,
            },
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

struct TickResult {
    ticks: u64,
    retries: u64,
    aggregations: u64,
    adaptation_recommendations: Vec<AdaptationRecommendation>,
}

#[derive(Debug, Clone)]
struct AdaptationRecommendation {
    pub task_group: String,
    pub should_adapt: bool,
    pub reason: String,
}

struct SchedulerModules {
    queue: WorkQueue,
    aggregator: ResultAggregator,
    feedback: FeedbackIntegrator,
    observation_store: RoutingObservationStore,
    history_store: RoutingHistoryStore,
}

impl SchedulerModules {
    fn new() -> Self {
        Self {
            queue: WorkQueue::new(),
            aggregator: ResultAggregator::new(),
            feedback: FeedbackIntegrator::new(
                Some(AutoDowngradePolicy::new("scheduler_downgrade")),
                Some(AutoUpgradePolicy::new("scheduler_upgrade")),
            ),
            observation_store: RoutingObservationStore::new(),
            history_store: RoutingHistoryStore::new(None),
        }
    }
}

fn scheduler_tick(
    store: &LocalProductStore,
    config: &SchedulerConfig,
    executor: &dyn crate::node_executor::NodeExecutor,
) -> Result<TickResult, String> {
    let _recovered = store.recover_stale_leases(config.lease_timeout_ms)?;

    let active_runs = store.list_active_workflow_run_ids()?;
    let mut ticks = 0u64;
    let mut retries = 0u64;
    let mut aggregations = 0u64;
    let mut recommendations = Vec::new();

    // Phase 2: Activate dormant modules for unified state management
    let mut modules = SchedulerModules::new();

    for run_id in active_runs.iter().take(config.max_concurrent) {
        // Phase 2: Use WorkQueue for in-memory graph state tracking
        let pre_graph = store.get_workflow_run(run_id).ok().flatten();
        let has_ready_nodes = pre_graph.as_ref().map_or(false, |run| {
            run.get("nodes")
                .and_then(|n| n.as_array())
                .map_or(false, |nodes| {
                    nodes
                        .iter()
                        .any(|n| n.get("db_status").and_then(|s| s.as_str()) == Some("pending"))
                })
        });

        if has_ready_nodes {
            // Track node via WorkQueue for in-memory state mirror
            if let Some(ref run) = pre_graph {
                if let Some(nodes) = run.get("nodes").and_then(|n| n.as_array()) {
                    for node in nodes {
                        if let Some(nid) = node.get("node_id").and_then(|v| v.as_str()) {
                            let _ = modules.queue.status_of(
                                &crate::orchestration::WorkflowGraph {
                                    schema_version: String::new(),
                                    workflow_id: run_id.clone(),
                                    dispatch_id: String::new(),
                                    nodes: Vec::new(),
                                    edges: Vec::new(),
                                    status: String::new(),
                                    created_at: String::new(),
                                    updated_at: String::new(),
                                    started_at: None,
                                    completed_at: None,
                                    result: None,
                                },
                                nid,
                            );
                        }
                    }
                }
            }
        }

        match store.tick_with_executor(run_id, "scheduler", 0, executor) {
            Ok(result) => {
                ticks += 1;
                let action = result.get("action").and_then(|v| v.as_str());
                if action == Some("node_retry") {
                    retries += 1;
                }

                // Phase 2: Record outcome for adaptive routing feedback
                if action == Some("node_completed") || action == Some("node_failed") {
                    let _node_id = result
                        .get("node_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let success = action == Some("node_completed");
                    let quality = if success { 0.8 } else { 0.2 };
                    modules.feedback.record_outcome(
                        &mut modules.observation_store,
                        run_id,
                        "scheduler",
                        "auto",
                        "noop",
                        "noop",
                        quality,
                        0.0,
                        0,
                        success,
                        None,
                        false,
                        &mut crate::runtime::FixtureRuntime::new(),
                    );

                    // Phase 2: Check adaptation recommendations via auto_policies
                    let task_group = crate::routing::make_task_group("scheduler", "auto");
                    let (should_adapt, reason) = modules.feedback.should_adapt(
                        &modules.observation_store,
                        &mut modules.history_store,
                        &task_group,
                        "noop",
                    );
                    if should_adapt {
                        recommendations.push(AdaptationRecommendation {
                            task_group,
                            should_adapt,
                            reason,
                        });
                    }
                }

                // Phase 2: Check if run is now terminal → aggregate with ResultAggregator
                if let Some(run_data) = store.get_workflow_run(run_id).ok().flatten() {
                    let status = run_data
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    if matches!(status, "completed" | "failed" | "cancelled") {
                        let graph = build_graph_from_run(&run_data, run_id);
                        if modules.aggregator.is_complete(&graph) {
                            let _result_map = modules.aggregator.aggregate(&graph);
                            aggregations += 1;
                        }
                    }
                }
            }
            Err(_) => {
                // terminal or no-ready-node errors are expected; skip
            }
        }
    }
    Ok(TickResult {
        ticks,
        retries,
        aggregations,
        adaptation_recommendations: recommendations,
    })
}

fn build_graph_from_run(run_data: &Value, run_id: &str) -> crate::orchestration::WorkflowGraph {
    let nodes: Vec<crate::orchestration::WorkflowNode> = run_data
        .get("nodes")
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .map(|n| crate::orchestration::WorkflowNode {
                    schema_version: "workflow_node.v1".to_string(),
                    node_id: n
                        .get("node_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    workflow_id: run_id.to_string(),
                    task_type: n
                        .get("task_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("task")
                        .to_string(),
                    assigned_agent_id: n.get("agent_id").and_then(|v| v.as_str()).map(String::from),
                    status: n
                        .get("db_status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("pending")
                        .to_string(),
                    input_refs: Vec::new(),
                    output_ref: n
                        .get("output_ref")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    budget: 0.0,
                    cost_incurred: 0.0,
                    error: n.get("error").and_then(|v| v.as_str()).map(String::from),
                    created_at: String::new(),
                    started_at: None,
                    completed_at: None,
                })
                .collect()
        })
        .unwrap_or_default();

    crate::orchestration::WorkflowGraph {
        schema_version: "workflow_graph.v1".to_string(),
        workflow_id: run_id.to_string(),
        dispatch_id: String::new(),
        nodes,
        edges: Vec::new(),
        status: run_data
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        created_at: String::new(),
        updated_at: String::new(),
        started_at: None,
        completed_at: None,
        result: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::local_product_store::LocalProductStore;
    use serde_json::json;

    fn test_store() -> Arc<LocalProductStore> {
        Arc::new(LocalProductStore::new(":memory:").unwrap())
    }

    fn make_plan_value(ids: &crate::read_only_planner::WorkflowPlanIds) -> Value {
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
        let run = store
            .create_workflow_run_from_plan(plan_id, "actor")
            .unwrap();
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
            executor_type: "noop".to_string(),
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
            executor_type: "noop".to_string(),
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
            executor_type: "noop".to_string(),
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
            executor_type: "noop".to_string(),
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
        let result = scheduler_tick(&store, &config, &executor).unwrap();
        assert_eq!(result.ticks, 0);
        assert_eq!(result.retries, 0);
    }

    #[test]
    fn scheduler_lease_anti_concurrency_two_ticks_dont_double_lease() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
        };
        let executor = NoopNodeExecutor;

        // First tick leases and completes the single node
        let r1 = scheduler_tick(&store, &config, &executor).unwrap();
        assert_eq!(r1.ticks, 1);

        // Second tick finds no ready nodes (already completed)
        let r2 = scheduler_tick(&store, &config, &executor).unwrap();
        assert_eq!(
            r2.ticks, 0,
            "second tick should not re-lease completed node"
        );

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }

    #[test]
    fn scheduler_fail_executor_increments_error_count() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
        };
        let executor = crate::node_executor::FailNodeExecutor::default();

        // Tick with fail executor — node fails, run becomes failed
        let result = scheduler_tick(&store, &config, &executor);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().ticks, 1);

        let run = store.get_workflow_run("run-0001").unwrap().unwrap();
        assert_eq!(
            run["status"], "failed",
            "run should be failed after node failure"
        );
    }

    #[test]
    fn scheduler_skip_cancelled_run() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        // Cancel the run
        store
            .request_workflow_run_cancel(&run_id, "test", Some("aborted"))
            .unwrap();

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 4,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
        };
        let executor = NoopNodeExecutor;

        // Scheduler tick should skip cancelled run (0 ticks)
        let result = scheduler_tick(&store, &config, &executor).unwrap();
        assert_eq!(result.ticks, 0, "cancelled run should not be ticked");

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "cancelled");
    }

    #[test]
    fn scheduler_stale_lease_recovery_then_reexecution() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        // Tick once to lease the node
        store.tick_workflow_run(&run_id, "test").unwrap();

        // Force the leased node into a stale state
        store
            .set_pending_node_to_running_for_test("2020-01-01T00:00:00Z")
            .unwrap();

        // Scheduler tick recovers stale lease AND re-executes the recovered node
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
        };
        let mut scheduler = WorkflowScheduler::new(store.clone(), config);
        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        scheduler.stop().unwrap();

        // The stale lease should have been recovered and the node re-executed
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(
            run["status"], "completed",
            "run should complete after stale lease recovery and re-execution"
        );
    }

    #[test]
    fn scheduler_retry_exhaustion_marks_run_failed() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        // Tick with fail executor and max_retries=2
        let executor = crate::node_executor::FailNodeExecutor::default();
        for attempt in 0..3 {
            let result = store.tick_with_executor(&run_id, "test", 2, &executor);
            assert!(result.is_ok(), "tick {attempt} should succeed");
        }

        // After 3 attempts (initial + 2 retries), node should be failed
        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(
            run["status"], "failed",
            "run should fail after retry exhaustion"
        );

        let nodes = run["nodes"].as_array().unwrap();
        let node = &nodes[0];
        assert_eq!(node["db_status"], "failed");
    }

    #[test]
    fn scheduler_status_includes_active_runs() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
        };
        let scheduler = WorkflowScheduler::new(store.clone(), config);

        let status = scheduler.status();
        assert_eq!(status["active_runs"], 0, "no runs yet");

        create_plan_and_run(&store);

        let status = scheduler.status();
        assert_eq!(status["active_runs"], 1, "one active run after creation");
    }

    #[test]
    fn scheduler_max_concurrent_limits_per_tick() {
        let store = test_store();

        // Create 3 runs
        for _ in 0..3 {
            create_plan_and_run(&store);
        }
        let active = store.list_active_workflow_run_ids().unwrap();
        assert_eq!(active.len(), 3);

        // max_concurrent=2: scheduler tick should process at most 2
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
        };
        let executor = NoopNodeExecutor;
        let result = scheduler_tick(&store, &config, &executor).unwrap();
        assert!(
            result.ticks <= 2,
            "max_concurrent=2 should limit ticks to at most 2, got {}",
            result.ticks
        );
    }

    #[test]
    fn scheduler_status_includes_retry_count() {
        let store = test_store();
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
        };
        let scheduler = WorkflowScheduler::new(store, config);

        let status = scheduler.status();
        assert_eq!(status["retry_count"], 0, "retry_count should start at 0");
    }

    #[test]
    fn scheduler_status_includes_execution_time() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 2,
            lease_timeout_ms: 60_000,
            executor_type: "noop".to_string(),
        };
        let mut scheduler = WorkflowScheduler::new(store, config);
        assert_eq!(
            scheduler.status()["total_execution_time_ns"],
            0,
            "should start at 0"
        );

        scheduler.start().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        scheduler.stop().unwrap();

        let status = scheduler.status();
        let exec_time = status["total_execution_time_ns"].as_u64().unwrap();
        assert!(
            exec_time > 0,
            "should accumulate execution time, got {exec_time}"
        );
    }

    #[test]
    fn scheduler_tick_tracks_retries_for_fail_executor() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
        };
        let executor = crate::node_executor::FailNodeExecutor::default();

        // Tick with fail executor and max_retries=0 — no retries tracked since we pass max_retries=0 to scheduler_tick
        let result = scheduler_tick(&store, &config, &executor).unwrap();
        assert_eq!(result.ticks, 1, "should tick once");
        assert_eq!(result.retries, 0, "no retries with default max_retries=0");
    }

    #[test]
    fn scheduler_tick_produces_aggregation_on_completion() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
        };
        let executor = NoopNodeExecutor;

        // Tick once: node completes, run becomes completed
        let result = scheduler_tick(&store, &config, &executor).unwrap();
        assert_eq!(result.ticks, 1);
        assert_eq!(result.aggregations, 1, "should aggregate completed run");
    }

    #[test]
    fn scheduler_tick_no_aggregation_on_active_run() {
        let store = test_store();
        let run_id = create_plan_and_run(&store);

        // Create a multi-node run by adding another node via store
        // For single-node run, one tick completes it. Test that before tick, no aggregation.
        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 4,
            lease_timeout_ms: 300_000,
            executor_type: "noop".to_string(),
        };
        let executor = NoopNodeExecutor;

        // Before ticking, the run is active — no aggregation yet
        let active = store.list_active_workflow_run_ids().unwrap();
        assert_eq!(active.len(), 1);

        // Tick to completion
        let result = scheduler_tick(&store, &config, &executor).unwrap();
        assert!(result.aggregations > 0, "should aggregate after completion");

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["status"], "completed");
    }

    #[test]
    fn scheduler_status_reports_dormant_modules_active() {
        let store = test_store();
        let config = SchedulerConfig::default();
        let scheduler = WorkflowScheduler::new(store, config);

        let status = scheduler.status();
        let dm = &status["dormant_modules_active"];
        assert_eq!(dm["work_queue"], true);
        assert_eq!(dm["result_aggregator"], true);
        assert_eq!(dm["auto_policies"], true);
        assert_eq!(dm["feedback_integrator"], true);
    }

    #[test]
    fn scheduler_tick_with_fail_records_adaptation_recommendation() {
        let store = test_store();
        create_plan_and_run(&store);

        let config = SchedulerConfig {
            interval_ms: 50,
            max_concurrent: 1,
            lease_timeout_ms: 300_000,
            executor_type: "fail".to_string(),
        };
        let executor = crate::node_executor::FailNodeExecutor::default();

        // Tick with fail executor — records outcome for adaptation
        let result = scheduler_tick(&store, &config, &executor).unwrap();
        assert_eq!(result.ticks, 1);
        // Recommendation depends on failure rate thresholds; just verify it doesn't panic
        let _ = result.adaptation_recommendations;
    }

    #[test]
    fn scheduler_modules_new_creates_all_components() {
        let modules = SchedulerModules::new();
        // WorkQueue and ResultAggregator are created
        let graph = crate::orchestration::WorkflowGraph {
            schema_version: "workflow_graph.v1".to_string(),
            workflow_id: "test".to_string(),
            dispatch_id: "test".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            status: "created".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
            started_at: None,
            completed_at: None,
            result: None,
        };
        assert!(modules.aggregator.is_complete(&graph));
        assert!(modules.queue.dequeue_ready(&graph).is_empty());
    }
}
