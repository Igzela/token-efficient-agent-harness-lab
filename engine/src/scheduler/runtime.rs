use std::sync::Arc;

use crate::executor_pool::ExecutorPool;
use crate::node_executor::{NodeExecutor, NoopNodeExecutor};
use crate::orchestration::{
    ConflictResolver, HumanApprovalGate, ResultAggregator, TaskDecomposer, WorkQueue,
    WorkflowEngine,
};
use crate::routing::{
    AutoDowngradePolicy, AutoUpgradePolicy, FeedbackIntegrator, RoutingHistoryStore,
    RoutingObservationStore,
};
use crate::storage::local_product_store::LocalProductStore;
use crate::workflow::backpressure::{Backpressure, BackpressureConfig};
use crate::workflow::dynamic_controller::{
    ControllerAction, DynamicControllerConfig, DynamicWorkflowController,
};
use crate::workflow::orchestration_decision::{
    action_to_string, build_enriched_input_signals, confidence_from_inputs, OrchestrationAction,
};
use serde_json::{json, Value};

use super::{env_flag_enabled, SchedulerConfig, TickResult};

pub(super) struct SchedulerRuntime<'a> {
    store: &'a LocalProductStore,
    config: &'a SchedulerConfig,
    executor: Arc<dyn NodeExecutor>,
    pool: &'a Arc<ExecutorPool>,
}

impl<'a> SchedulerRuntime<'a> {
    pub(super) fn new(
        store: &'a LocalProductStore,
        config: &'a SchedulerConfig,
        executor: Arc<dyn NodeExecutor>,
        pool: &'a Arc<ExecutorPool>,
    ) -> Self {
        Self {
            store,
            config,
            executor,
            pool,
        }
    }

    pub(super) fn tick_with_limit(&self, tick_limit: usize) -> Result<TickResult, String> {
        scheduler_tick_with_limit(
            self.store,
            self.config,
            self.executor.clone(),
            self.pool,
            tick_limit,
        )
    }
}

pub(super) struct SchedulerModules {
    pub(super) queue: WorkQueue,
    pub(super) aggregator: ResultAggregator,
    pub(super) feedback: FeedbackIntegrator,
    pub(super) observation_store: RoutingObservationStore,
    pub(super) history_store: RoutingHistoryStore,
    pub(super) conflict_resolver: ConflictResolver,
    pub(super) approval_gate: HumanApprovalGate,
    pub(super) workflow_engine: WorkflowEngine,
}

impl SchedulerModules {
    pub(super) fn new() -> Self {
        Self {
            queue: WorkQueue::new(),
            aggregator: ResultAggregator::new(),
            feedback: FeedbackIntegrator::new(
                Some(AutoDowngradePolicy::new("scheduler_downgrade")),
                Some(AutoUpgradePolicy::new("scheduler_upgrade")),
            ),
            observation_store: RoutingObservationStore::new(),
            history_store: RoutingHistoryStore::new(None),
            conflict_resolver: ConflictResolver::new(),
            approval_gate: HumanApprovalGate::new(0.7),
            workflow_engine: WorkflowEngine::new(TaskDecomposer::new(None)),
        }
    }
}

#[allow(dead_code)]
pub(super) fn scheduler_tick(
    store: &LocalProductStore,
    config: &SchedulerConfig,
    executor_arc: Arc<dyn NodeExecutor>,
    pool: &Arc<ExecutorPool>,
) -> Result<TickResult, String> {
    scheduler_tick_with_limit(store, config, executor_arc, pool, config.max_concurrent)
}

fn scheduler_tick_with_limit(
    store: &LocalProductStore,
    config: &SchedulerConfig,
    executor_arc: Arc<dyn NodeExecutor>,
    pool: &Arc<ExecutorPool>,
    tick_limit: usize,
) -> Result<TickResult, String> {
    if dynamic_workflow_enabled(config) {
        return dynamic_scheduler_tick(store, config, executor_arc, pool, tick_limit);
    }

    let _recovered = store.recover_stale_leases(config.lease_timeout_ms)?;

    let mut paused_runs = Vec::new();
    let mut degraded_runs = Vec::new();
    let mut backpressure_active = false;
    let mut queue_depth = 0usize;

    let active_runs: Vec<String> = if config.queue_enabled {
        let prioritized = store.list_active_workflow_runs_prioritized()?;
        queue_depth = prioritized.len();
        let mut selected = Vec::new();
        for run in &prioritized {
            let run_id = run
                .get("run_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let pause = run.get("pause_reason").and_then(Value::as_str);
            let degrade = run.get("degrade_mode").and_then(Value::as_str);
            if pause.is_some() {
                paused_runs.push(run_id);
            } else if degrade.is_some() {
                degraded_runs.push(run_id.clone());
                selected.push(run_id);
            } else {
                selected.push(run_id);
            }
        }
        selected
    } else {
        store.list_active_workflow_run_ids()?
    };

    if config.queue_enabled && config.backpressure_enabled {
        let total_active = active_runs.len();
        let total_capacity = config.max_concurrent.max(1);
        let utilization = total_active as f64 / total_capacity as f64;

        let mut bp = Backpressure::new(BackpressureConfig {
            activation_threshold: config.backpressure_activation,
            ..Default::default()
        });
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let overdue_ids: Vec<String> = active_runs
            .iter()
            .filter(|id| {
                store
                    .get_workflow_run(id)
                    .ok()
                    .flatten()
                    .map_or(false, |r| {
                        r.get("status").and_then(|v| v.as_str()) == Some("running")
                            && r.get("started_at")
                                .and_then(|v| v.as_str())
                                .map_or(false, |s| {
                                    chrono::DateTime::parse_from_rfc3339(s).ok().map_or(
                                        false,
                                        |t| {
                                            (chrono::Utc::now() - t.with_timezone(&chrono::Utc))
                                                .num_seconds()
                                                > 300
                                        },
                                    )
                                })
                    })
            })
            .cloned()
            .collect();
        let overdue_count = overdue_ids.len();
        let decision = bp.evaluate(
            utilization,
            queue_depth,
            config.max_queued,
            overdue_count,
            now_ms,
            Some(&overdue_ids),
        );
        backpressure_active = decision.active;

        for run_id in &decision.runs_to_pause {
            store.update_run_pause_reason(run_id, Some("backpressure"))?;
            store.update_run_degrade_mode(run_id, decision.degrade_mode.as_deref())?;
            paused_runs.push(run_id.clone());
        }

        if backpressure_active {
            let (bp_conf, bp_score) =
                confidence_from_inputs("running", None, false, None, Some("backpressure"));
            let bp_base = serde_json::json!({
                "source": "scheduler_backpressure",
                "active": true,
                "paused_count": decision.runs_to_pause.len(),
                "utilization": utilization,
            });
            let bp_queue_signal = json!({
                "backpressure_active": true,
                "degrade_mode": decision.degrade_mode,
                "effective_concurrency": decision.effective_concurrency,
            });
            let bp_pool_signal = json!({"utilization": utilization});
            let bp_enriched = build_enriched_input_signals(
                &bp_base,
                None,
                None,
                None,
                None,
                Some(&bp_queue_signal),
                Some(&bp_pool_signal),
                None,
                None,
            );
            let _ = store.record_orchestration_decision(
                "scheduler",
                None,
                "backpressure_pause",
                &decision.reason,
                "backpressure",
                None,
                bp_conf.as_str(),
                bp_score,
                &bp_enriched,
            );
        }
    }

    let mut ticks = 0u64;
    let mut retries = 0u64;
    let mut aggregations = 0u64;
    let mut recommendations = Vec::new();
    let mut modules = SchedulerModules::new();

    for run_id in active_runs.iter().take(tick_limit) {
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

        let (acquired_type, pool_executor_arc) =
            select_scheduler_executor(config, pool, &executor_arc);

        match store.tick_with_executor(run_id, "scheduler", 0, &*pool_executor_arc) {
            Ok(result) => {
                ticks += 1;
                let action = result.get("action").and_then(|v| v.as_str());
                if action == Some("node_retry") {
                    retries += 1;
                }

                if let Some(ref et) = acquired_type {
                    pool.release(et, true, 0, None);
                }

                let tick_node_id = result.get("node_id").and_then(|v| v.as_str());
                let tick_action = match action {
                    Some("node_completed") => OrchestrationAction::RunCompleted,
                    Some("node_failed") => OrchestrationAction::RunFailed,
                    Some("node_retry") => OrchestrationAction::RetryNode,
                    _ => OrchestrationAction::ExecuteNode,
                };
                let (tick_confidence, tick_score) = confidence_from_inputs(
                    "running",
                    tick_node_id.or(Some("pending")),
                    true,
                    None,
                    None,
                );

                let tick_quality = result.get("result").and_then(|r| r.get("quality")).cloned();
                let exec_type = pool_executor_arc.executor_type_name();
                let tick_pool_signal = pool
                    .snapshot()
                    .iter()
                    .find(|e| e.executor_type == exec_type)
                    .map(|e| {
                        json!({
                            "failure_score": e.status.failure_score,
                            "active_count": e.status.active_count,
                        })
                    });

                let tick_base = serde_json::json!({"source": "scheduler_tick", "action": action});
                let tick_enriched = build_enriched_input_signals(
                    &tick_base,
                    tick_quality.as_ref(),
                    None,
                    None,
                    None,
                    None,
                    tick_pool_signal.as_ref(),
                    None,
                    None,
                );

                let _ = store.record_orchestration_decision(
                    run_id,
                    tick_node_id,
                    action_to_string(&tick_action),
                    "scheduler tick result",
                    pool_executor_arc.executor_type_name(),
                    None,
                    tick_confidence.as_str(),
                    tick_score,
                    &tick_enriched,
                );

                if action == Some("node_completed") || action == Some("node_failed") {
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

                    let task_group = crate::routing::make_task_group("scheduler", "auto");
                    let (should_adapt, reason) = modules.feedback.should_adapt(
                        &modules.observation_store,
                        &mut modules.history_store,
                        &task_group,
                        "noop",
                    );
                    if should_adapt {
                        recommendations.push(super::AdaptationRecommendation {
                            task_group,
                            should_adapt,
                            reason,
                        });
                    }
                }

                if let Some(run_data) = store.get_workflow_run(run_id).ok().flatten() {
                    let status = run_data
                        .get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    if matches!(status, "completed" | "failed" | "cancelled") {
                        let graph = build_graph_from_run(&run_data, run_id);
                        let has_edges = !graph.edges.is_empty();
                        let engine_graph = if has_edges {
                            modules.workflow_engine.tick(&graph)
                        } else {
                            graph.clone()
                        };

                        let conflicts = modules.conflict_resolver.detect_conflicts(&engine_graph);
                        for conflict in &conflicts {
                            let resolved = modules.conflict_resolver.resolve(conflict);
                            if resolved.resolution_result.as_deref() == Some("workflow_cancelled") {
                                continue;
                            }
                        }

                        for node in &engine_graph.nodes {
                            if matches!(node.status.as_str(), "completed" | "failed")
                                && modules.approval_gate.requires_approval(&engine_graph, node)
                            {
                                let _needs_approval = true;
                            }
                        }

                        if modules.aggregator.is_complete(&engine_graph) {
                            let _result_map = modules.aggregator.aggregate(&engine_graph);
                            aggregations += 1;
                        }
                    }
                }
            }
            Err(_) => {
                if let Some(ref et) = acquired_type {
                    pool.release(et, false, 0, None);
                }
            }
        }
    }
    Ok(TickResult {
        ticks,
        retries,
        aggregations,
        adaptation_recommendations: recommendations,
        paused_runs,
        degraded_runs,
        backpressure_active,
        queue_depth,
    })
}

fn dynamic_scheduler_tick(
    store: &LocalProductStore,
    config: &SchedulerConfig,
    executor_arc: Arc<dyn NodeExecutor>,
    pool: &Arc<ExecutorPool>,
    tick_limit: usize,
) -> Result<TickResult, String> {
    let _recovered = store.recover_stale_leases(config.lease_timeout_ms)?;

    let mut paused_runs = Vec::new();
    let mut degraded_runs = Vec::new();
    let mut backpressure_active = false;
    let mut queue_depth = 0usize;

    let mut active_runs: Vec<String> = if config.queue_enabled {
        let prioritized = store.list_active_workflow_runs_prioritized()?;
        queue_depth = prioritized.len();
        let mut selected = Vec::new();
        for run in &prioritized {
            let run_id = run
                .get("run_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let pause = run.get("pause_reason").and_then(Value::as_str);
            let degrade = run.get("degrade_mode").and_then(Value::as_str);
            if pause.is_some() {
                paused_runs.push(run_id);
            } else if degrade.is_some() {
                degraded_runs.push(run_id.clone());
                selected.push(run_id);
            } else {
                selected.push(run_id);
            }
        }
        selected
    } else {
        store.list_active_workflow_run_ids()?
    };

    if config.queue_enabled && config.backpressure_enabled {
        let total_active = active_runs.len();
        let total_capacity = config.max_concurrent.max(1);
        let utilization = total_active as f64 / total_capacity as f64;

        let mut bp = Backpressure::new(BackpressureConfig {
            activation_threshold: config.backpressure_activation,
            ..Default::default()
        });
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let decision = bp.evaluate(utilization, queue_depth, config.max_queued, 0, now_ms, None);
        backpressure_active = decision.active;

        for run_id in &decision.runs_to_pause {
            store.update_run_pause_reason(run_id, Some("backpressure"))?;
            store.update_run_degrade_mode(run_id, decision.degrade_mode.as_deref())?;
            paused_runs.push(run_id.clone());
        }
    }

    for run in store.list_workflow_runs_with_offset(500, 0)? {
        let Some(run_id) = run.get("run_id").and_then(Value::as_str) else {
            continue;
        };
        if run.get("status").and_then(Value::as_str) == Some("failed")
            && !active_runs.iter().any(|id| id == run_id)
        {
            active_runs.push(run_id.to_string());
        }
    }
    let mut ticks = 0u64;
    let mut retries = 0u64;
    let mut aggregations = 0u64;

    for run_id in active_runs.iter().take(tick_limit) {
        let (acquired_type, pool_executor_arc) =
            select_scheduler_executor(config, pool, &executor_arc);

        let mut controller = DynamicWorkflowController::new(DynamicControllerConfig {
            executor_pool_accounting_enabled: false,
            ..DynamicControllerConfig::default()
        })
        .with_executor_pool(Arc::clone(pool));
        match controller.tick(store, run_id, "scheduler", &*pool_executor_arc) {
            Ok(result) => {
                let did_work = result
                    .actions
                    .iter()
                    .any(|action| !matches!(action, ControllerAction::NoAction { .. }));
                if did_work {
                    ticks += 1;
                }
                retries += result
                    .actions
                    .iter()
                    .filter(|action| matches!(action, ControllerAction::NodeRetried { .. }))
                    .count() as u64;
                if result
                    .actions
                    .iter()
                    .any(|action| matches!(action, ControllerAction::RunCompleted))
                {
                    aggregations += 1;
                }

                if let Some(ref et) = acquired_type {
                    pool.release(et, true, 0, None);
                }
            }
            Err(_) => {
                if let Some(ref et) = acquired_type {
                    pool.release(et, false, 0, None);
                }
            }
        }
    }

    Ok(TickResult {
        ticks,
        retries,
        aggregations,
        adaptation_recommendations: Vec::new(),
        paused_runs,
        degraded_runs,
        backpressure_active,
        queue_depth,
    })
}

pub(super) fn dynamic_workflow_enabled(config: &SchedulerConfig) -> bool {
    env_flag_enabled("ACP_ENABLE_DYNAMIC_WORKFLOW")
        || std::env::var("ACP_SCHEDULER_MODE")
            .map(|mode| mode.eq_ignore_ascii_case("dynamic"))
            .unwrap_or(false)
        || matches!(
            config.executor_type.as_str(),
            "dynamic" | "dynamic_noop" | "dynamic_workflow"
        )
}

fn select_scheduler_executor(
    config: &SchedulerConfig,
    pool: &Arc<ExecutorPool>,
    configured: &Arc<dyn NodeExecutor>,
) -> (Option<String>, Arc<dyn NodeExecutor>) {
    if config.executor_type == "adaptive_provider" {
        return (None, Arc::clone(configured));
    }
    let best_executor = pool.best_for_task("scheduler", "auto");
    match best_executor {
        Some(ref executor_type) if pool.acquire(executor_type) => {
            let executor = pool
                .get(executor_type)
                .unwrap_or_else(|| Arc::new(NoopNodeExecutor));
            (Some(executor_type.clone()), executor)
        }
        _ => (None, Arc::clone(configured)),
    }
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
