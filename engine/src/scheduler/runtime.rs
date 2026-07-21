use std::sync::Arc;

use crate::executor_pool::ExecutorPool;
#[cfg(test)]
use crate::node_executor::NoopNodeExecutor;
use crate::node_executor::{FailNodeExecutor, NodeExecutor};
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
    // Production evidence recovery shares the scheduler tick; it does not
    // introduce another queue or authority owner. Terminal runs are retried
    // idempotently after process restart, including ticks with no active work.
    store.recover_budget_intelligence_for_terminal_runs(32, "scheduler")?;
    store.recover_registered_offline_replays(32, "scheduler")?;
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

        let route = scheduler_route_for_run(store, run_id, config);
        let Some((acquired_type, pool_executor_arc)) =
            select_scheduler_executor(config, pool, &executor_arc, route.as_ref())
        else {
            continue;
        };

        match store.tick_with_executor_with_agent_caps(
            run_id,
            "scheduler",
            config.max_retries,
            &*pool_executor_arc,
            config.agent_max_concurrent_global,
            config.agent_max_concurrent_per_run,
        ) {
            Ok(result) => {
                ticks += 1;
                let action = result.get("action").and_then(|v| v.as_str());
                if action == Some("node_retry") {
                    retries += 1;
                }

                let execution = result.get("result");
                let executed = matches!(action, Some("node_executed" | "node_retry"));
                let execution_succeeded = execution
                    .and_then(|value| value.get("status"))
                    .and_then(Value::as_str)
                    == Some("completed");
                if let Some(ref executor_type) = acquired_type {
                    if executed {
                        let latency_ms = execution
                            .and_then(|value| value.get("latency_ms"))
                            .and_then(Value::as_i64)
                            .unwrap_or(0)
                            .max(0) as u64;
                        let cost = execution
                            .and_then(|value| value.get("estimated_cost"))
                            .and_then(Value::as_f64)
                            .filter(|value| value.is_finite() && *value >= 0.0);
                        pool.release(executor_type, execution_succeeded, latency_ms, cost);
                    } else {
                        pool.release_without_recording(executor_type);
                    }
                }

                let tick_node_id = result.get("node_id").and_then(|v| v.as_str());
                let tick_action = match (action, execution_succeeded) {
                    (Some("node_executed"), true) => OrchestrationAction::RunCompleted,
                    (Some("node_executed"), false) => OrchestrationAction::RunFailed,
                    (Some("node_retry"), _) => OrchestrationAction::RetryNode,
                    _ => OrchestrationAction::ExecuteNode,
                };
                let (tick_confidence, tick_score) = confidence_from_inputs(
                    "running",
                    tick_node_id.or(Some("pending")),
                    execution_succeeded,
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

                if executed {
                    let success = execution_succeeded;
                    let quality = if success { 0.8 } else { 0.2 };
                    let executor_type = pool_executor_arc.executor_type_name();
                    modules.feedback.record_outcome(
                        &mut modules.observation_store,
                        run_id,
                        "scheduler",
                        "auto",
                        executor_type,
                        executor_type,
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
                        executor_type,
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
                    pool.release_without_recording(et);
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
        let route = scheduler_route_for_run(store, run_id, config);
        let Some((acquired_type, pool_executor_arc)) =
            select_scheduler_executor(config, pool, &executor_arc, route.as_ref())
        else {
            continue;
        };

        let mut controller = DynamicWorkflowController::new(DynamicControllerConfig {
            executor_pool_accounting_enabled: false,
            max_retries: config.max_retries,
            agent_concurrency_caps: Some((
                config.agent_max_concurrent_global,
                config.agent_max_concurrent_per_run,
            )),
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

                if let Some(ref executor_type) = acquired_type {
                    let execution_statuses: Vec<&str> = result
                        .actions
                        .iter()
                        .filter_map(|action| match action {
                            ControllerAction::NodeExecuted { status, .. } => Some(status.as_str()),
                            ControllerAction::NodeRetried { .. } => Some("failed"),
                            _ => None,
                        })
                        .collect();
                    if execution_statuses.is_empty() {
                        pool.release_without_recording(executor_type);
                    } else {
                        let success = execution_statuses
                            .iter()
                            .all(|status| *status == "completed");
                        let (latency_ms, estimated_cost) =
                            dynamic_execution_metrics(store, run_id, &result.actions);
                        pool.release(executor_type, success, latency_ms, estimated_cost);
                    }
                }
            }
            Err(error) => {
                if let Some(ref et) = acquired_type {
                    pool.release_without_recording(et);
                }
                return Err(format!(
                    "dynamic scheduler tick failed for run {run_id}: {error}"
                ));
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

fn dynamic_execution_metrics(
    store: &LocalProductStore,
    run_id: &str,
    actions: &[ControllerAction],
) -> (u64, Option<f64>) {
    let node_id = actions.iter().find_map(|action| match action {
        ControllerAction::NodeExecuted { node_id, .. }
        | ControllerAction::NodeRetried { node_id, .. } => Some(node_id.as_str()),
        _ => None,
    });
    let Some(node_id) = node_id else {
        return (0, None);
    };
    let Some(run) = store.get_workflow_run(run_id).ok().flatten() else {
        return (0, None);
    };
    let Some(result) = run
        .get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node.get("node_id").and_then(Value::as_str) == Some(node_id))
        })
        .and_then(|node| node.get("result"))
    else {
        return (0, None);
    };
    let latency_ms = result
        .get("latency_ms")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 0)
        .unwrap_or(0) as u64;
    let estimated_cost = result
        .get("estimated_cost")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0);
    (latency_ms, estimated_cost)
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
    route: Option<&SchedulerExecutorRoute>,
) -> Option<(Option<String>, Arc<dyn NodeExecutor>)> {
    if route.and_then(|route| route.suggested_executor.as_deref()) == Some("agent_step") {
        return pool.get("agent_step").and_then(|executor| {
            pool.acquire("agent_step")
                .then_some((Some("agent_step".to_string()), executor))
        });
    }
    if config.executor_type == "adaptive_provider" {
        return Some((None, Arc::clone(configured)));
    }
    let legacy_dynamic = matches!(
        config.executor_type.as_str(),
        "dynamic" | "dynamic_noop" | "dynamic_workflow"
    );
    let pool_routed = matches!(
        config.executor_type.as_str(),
        "dynamic" | "dynamic_noop" | "dynamic_workflow" | "auto" | "pool"
    );
    if !pool_routed {
        return match pool.get(&config.executor_type) {
            Some(executor) if pool.acquire(&config.executor_type) => {
                Some((Some(config.executor_type.clone()), executor))
            }
            Some(_) => None,
            None => Some((None, Arc::clone(configured))),
        };
    }
    let unavailable = |message: String| {
        Some((
            None,
            Arc::new(FailNodeExecutor {
                error_domain: "scheduler_executor_unavailable".to_string(),
                error_message: message,
            }) as Arc<dyn NodeExecutor>,
        ))
    };
    let Some(route) = route else {
        if legacy_dynamic {
            let best_executor = pool.best_for_task("scheduler", "auto");
            return match best_executor {
                Some(ref executor_type) if pool.acquire(executor_type) => {
                    let Some(executor) = pool.get(executor_type) else {
                        pool.release_without_recording(executor_type);
                        return None;
                    };
                    Some((Some(executor_type.clone()), executor))
                }
                _ if pool.snapshot().is_empty() => Some((None, Arc::clone(configured))),
                _ => None,
            };
        }
        return unavailable("no ready workflow node has a compatible executor".to_string());
    };
    let allow_cli_domain_route = route.cli_workspace_bound
        && matches!(route.task_domain.as_str(), "code" | "architecture")
        && !matches!(
            route.task_type.as_str(),
            "agent_step"
                | "adaptive_provider"
                | "command"
                | "claude_code_cli"
                | "codex_cli"
                | crate::external_runtime::LANGGRAPH_TASK_TYPE
                | crate::opencode_runtime::OPENCODE_TASK_TYPE
        );
    if let Some(executor_type) = route.suggested_executor.as_deref() {
        let fixture_executor_mismatch =
            matches!(executor_type, "noop" | "stub" | "fail") && route.task_type != executor_type;
        if matches!(executor_type, "claude_code_cli" | "codex_cli") && !route.cli_workspace_bound {
            return unavailable(
                "CLI scheduler route requires an app-owned workspace bound to the run".to_string(),
            );
        }
        if !fixture_executor_mismatch
            && pool.supports_task(
                executor_type,
                &route.task_type,
                &route.task_domain,
                allow_cli_domain_route,
            )
        {
            if let Some(executor) = pool.get(executor_type) {
                if pool.acquire(executor_type) {
                    return Some((Some(executor_type.to_string()), executor));
                }
                return None;
            }
        }
        if route.required {
            return unavailable(format!(
                "required scheduler executor is unavailable or incompatible: {executor_type}"
            ));
        }
    }
    let best_executor = if allow_cli_domain_route {
        pool.best_cli_for_domain(&route.task_domain)
    } else if legacy_dynamic
        || matches!(
            route.task_type.as_str(),
            "command" | "local_runner_validation"
        )
    {
        pool.best_for_task(&route.task_type, &route.task_domain)
    } else {
        None
    };
    match best_executor {
        Some(ref executor_type) if pool.acquire(executor_type) => {
            let Some(executor) = pool.get(executor_type) else {
                pool.release_without_recording(executor_type);
                return unavailable(format!(
                    "selected scheduler executor disappeared before dispatch: {executor_type}"
                ));
            };
            Some((Some(executor_type.clone()), executor))
        }
        _ if allow_cli_domain_route && pool.has_cli_for_domain(&route.task_domain) => None,
        _ if legacy_dynamic && pool.snapshot().is_empty() => Some((None, Arc::clone(configured))),
        _ if legacy_dynamic => None,
        _ => unavailable(format!(
            "no compatible scheduler executor for task_type={} task_domain={}",
            route.task_type, route.task_domain
        )),
    }
}

#[derive(Debug, Clone)]
struct SchedulerExecutorRoute {
    task_type: String,
    task_domain: String,
    suggested_executor: Option<String>,
    required: bool,
    cli_workspace_bound: bool,
}

fn scheduler_route_for_run(
    store: &LocalProductStore,
    run_id: &str,
    config: &SchedulerConfig,
) -> Option<SchedulerExecutorRoute> {
    let task_type = store
        .next_ready_workflow_node_task_type_with_agent_caps(
            run_id,
            Some((
                config.agent_max_concurrent_global,
                config.agent_max_concurrent_per_run,
            )),
        )
        .ok()
        .flatten()?;
    let run = store.get_workflow_run(run_id).ok().flatten();
    let task_domain = run
        .as_ref()
        .and_then(|run| run.get("plan_id"))
        .and_then(Value::as_str)
        .and_then(|plan_id| store.get_workflow_plan(plan_id).ok().flatten())
        .and_then(|plan| {
            plan.pointer("/analysis/task_domain")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| task_type.clone());
    let cli_workspace_bound = run
        .as_ref()
        .and_then(|run| ready_node_for_task_type(run, &task_type))
        .and_then(|node| node.get("workspace_path"))
        .and_then(Value::as_str)
        .zip(
            store
                .get_supervised_patch_workspace_for_run(run_id)
                .ok()
                .flatten()
                .and_then(|workspace| {
                    workspace
                        .get("workspace_path")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }),
        )
        .is_some_and(|(requested, bound)| requested == bound);
    let mut required = false;
    let suggested_executor = if task_type == "agent_step" {
        required = true;
        Some("agent_step".to_string())
    } else if task_type == crate::external_runtime::LANGGRAPH_TASK_TYPE {
        required = true;
        Some(crate::external_runtime::LANGGRAPH_EXECUTOR_TYPE.to_string())
    } else if task_type == crate::opencode_runtime::OPENCODE_TASK_TYPE {
        // Reserved exact-capability: never fall back to noop/stub/CLI when absent.
        required = true;
        Some(crate::opencode_runtime::OPENCODE_EXECUTOR_TYPE.to_string())
    } else if matches!(
        task_type.as_str(),
        "adaptive_provider" | "claude_code_cli" | "codex_cli"
    ) {
        required = true;
        Some(task_type.clone())
    } else if let Some(node) = run
        .as_ref()
        .and_then(|run| ready_node_for_task_type(run, &task_type))
    {
        if task_type == "command" {
            if let Some(executor_type @ ("claude_code_cli" | "codex_cli")) =
                node.get("executor").and_then(Value::as_str)
            {
                required = true;
                Some(executor_type.to_string())
            } else {
                let task_group = crate::routing::schemas::make_task_group(&task_type, "execute");
                store.suggest_executor_type(&task_group)
            }
        } else {
            let task_group = crate::routing::schemas::make_task_group(&task_type, "execute");
            store.suggest_executor_type(&task_group)
        }
    } else {
        let task_group = crate::routing::schemas::make_task_group(&task_type, "execute");
        store.suggest_executor_type(&task_group)
    };
    Some(SchedulerExecutorRoute {
        task_type,
        task_domain,
        suggested_executor,
        required,
        cli_workspace_bound,
    })
}

#[cfg(test)]
fn suggested_executor_for_run(
    store: &LocalProductStore,
    run_id: &str,
    config: &SchedulerConfig,
) -> Option<String> {
    scheduler_route_for_run(store, run_id, config)?.suggested_executor
}

fn ready_node_for_task_type<'a>(run: &'a Value, task_type: &str) -> Option<&'a Value> {
    let nodes = run.get("nodes")?.as_array()?;
    let edges = run.get("edges").and_then(Value::as_array);
    let mut candidates = nodes
        .iter()
        .filter(|node| {
            node.get("db_status").and_then(Value::as_str) == Some("pending")
                && node.get("task_type").and_then(Value::as_str) == Some(task_type)
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.get("node_id")
            .and_then(Value::as_str)
            .cmp(&right.get("node_id").and_then(Value::as_str))
    });
    candidates.into_iter().find(|candidate| {
        let Some(node_id) = candidate.get("node_id").and_then(Value::as_str) else {
            return false;
        };
        edges.is_none_or(|edges| {
            edges
                .iter()
                .filter(|edge| edge.get("to_node_id").and_then(Value::as_str) == Some(node_id))
                .all(|edge| {
                    let predecessor = edge.get("from_node_id").and_then(Value::as_str);
                    let predecessor_status = nodes
                        .iter()
                        .find(|node| node.get("node_id").and_then(Value::as_str) == predecessor)
                        .and_then(|node| {
                            node.get("db_status")
                                .or_else(|| node.get("status"))
                                .and_then(Value::as_str)
                        });
                    predecessor_status == Some("completed")
                        || (task_type == "fix"
                            && matches!(predecessor_status, Some("failed" | "recovered")))
                })
        })
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
    use crate::executor_pool::{
        CostProfile, ExecutorCapabilities, ExecutorEntry, ExecutorMetrics, ExecutorStatus,
    };
    use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn route(
        task_type: &str,
        task_domain: &str,
        suggested_executor: Option<&str>,
        required: bool,
    ) -> SchedulerExecutorRoute {
        SchedulerExecutorRoute {
            task_type: task_type.to_string(),
            task_domain: task_domain.to_string(),
            suggested_executor: suggested_executor.map(str::to_string),
            required,
            cli_workspace_bound: matches!(task_domain, "code" | "architecture")
                || matches!(suggested_executor, Some("claude_code_cli" | "codex_cli")),
        }
    }

    #[test]
    fn dynamic_routing_prefers_available_feedback_suggestion() {
        let pool = Arc::new(ExecutorPool::new());
        for executor_type in ["fallback", "preferred"] {
            pool.register(ExecutorEntry {
                executor_type: executor_type.to_string(),
                executor: Arc::new(NoopNodeExecutor),
                capabilities: ExecutorCapabilities::default(),
                status: ExecutorStatus::default(),
                cost_profile: CostProfile::default(),
                metrics: ExecutorMetrics::default(),
            });
        }
        let config = SchedulerConfig {
            executor_type: "dynamic".to_string(),
            ..Default::default()
        };

        let route = route("analysis", "test", Some("preferred"), false);
        let selected = select_scheduler_executor(
            &config,
            &pool,
            &(Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>),
            Some(&route),
        )
        .expect("suggested executor should be selected");

        assert_eq!(selected.0.as_deref(), Some("preferred"));
        pool.release_without_recording("preferred");
    }

    #[test]
    fn reserved_agent_step_suggestion_overrides_wildcard_or_configured_noop() {
        let pool = Arc::new(ExecutorPool::new());
        crate::executor_pool::register_default_executors(
            &pool,
            false,
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        crate::executor_pool::register_agent_step_executor(&pool, Arc::new(NoopNodeExecutor), 1);
        let config = SchedulerConfig {
            executor_type: "noop".to_string(),
            ..Default::default()
        };

        let route = route("agent_step", "agent_runtime", Some("agent_step"), true);
        let selected = select_scheduler_executor(
            &config,
            &pool,
            &(Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>),
            Some(&route),
        )
        .expect("reserved agent executor should be selected");

        assert_eq!(selected.0.as_deref(), Some("agent_step"));
        pool.release_without_recording("agent_step");
    }

    #[test]
    fn reserved_agent_step_suggestion_has_no_wildcard_fallback() {
        let pool = Arc::new(ExecutorPool::new());
        crate::executor_pool::register_default_executors(
            &pool,
            false,
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        let config = SchedulerConfig {
            executor_type: "noop".to_string(),
            ..Default::default()
        };

        let route = route("agent_step", "agent_runtime", Some("agent_step"), true);
        assert!(select_scheduler_executor(
            &config,
            &pool,
            &(Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>),
            Some(&route),
        )
        .is_none());
    }

    #[test]
    fn auto_pool_selects_explicit_provider_and_policy_wrapped_cli_deterministically() {
        let pool = Arc::new(ExecutorPool::new());
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        crate::executor_pool::register_adaptive_provider_executor(
            &pool,
            Arc::new(NoopNodeExecutor),
            2,
        );
        crate::executor_pool::register_cli_executors(
            &pool,
            &crate::cli::CliConfig {
                enabled: true,
                claude_code_bin: None,
                claude_code_enabled: false,
                codex_bin: Some("/definitely/not/executed".to_string()),
                codex_enabled: true,
                timeout_ms: 1_000,
            },
            store,
        );
        let config = SchedulerConfig {
            executor_type: "auto".to_string(),
            ..Default::default()
        };
        let configured = Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>;

        for expected in ["adaptive_provider", "codex_cli"] {
            let route = route(
                expected,
                if expected == "adaptive_provider" {
                    "adaptive"
                } else {
                    "code"
                },
                Some(expected),
                true,
            );
            let selected = select_scheduler_executor(&config, &pool, &configured, Some(&route))
                .expect("explicit executor should be schedulable");
            assert_eq!(selected.0.as_deref(), Some(expected));
            pool.release_without_recording(expected);
        }
    }

    #[test]
    fn required_provider_or_cli_route_never_falls_back_when_unavailable() {
        let pool = Arc::new(ExecutorPool::new());
        crate::executor_pool::register_default_executors(
            &pool,
            false,
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        let config = SchedulerConfig {
            executor_type: "pool".to_string(),
            ..Default::default()
        };
        let configured = Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>;

        let provider_route = route(
            "adaptive_provider",
            "adaptive",
            Some("adaptive_provider"),
            true,
        );
        let provider =
            select_scheduler_executor(&config, &pool, &configured, Some(&provider_route))
                .expect("unavailable provider should produce explicit failure");
        assert_eq!(provider.1.executor_type_name(), "fail");
        let cli_route = route("codex_cli", "code", Some("codex_cli"), true);
        let cli = select_scheduler_executor(&config, &pool, &configured, Some(&cli_route))
            .expect("unavailable CLI should produce explicit failure");
        assert_eq!(cli.1.executor_type_name(), "fail");
    }

    #[test]
    fn auto_routes_ordinary_code_work_to_available_cli_not_noop() {
        let pool = Arc::new(ExecutorPool::new());
        crate::executor_pool::register_default_executors(
            &pool,
            false,
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        crate::executor_pool::register_cli_executors(
            &pool,
            &crate::cli::CliConfig {
                enabled: true,
                claude_code_bin: Some("/definitely/not/executed-claude".to_string()),
                claude_code_enabled: true,
                codex_bin: Some("/definitely/not/executed-codex".to_string()),
                codex_enabled: true,
                timeout_ms: 1_000,
            },
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        let config = SchedulerConfig {
            executor_type: "auto".to_string(),
            ..Default::default()
        };
        let route = route("implementation", "code", None, false);
        let selected = select_scheduler_executor(
            &config,
            &pool,
            &(Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>),
            Some(&route),
        )
        .expect("code work should select a CLI");

        assert_eq!(selected.0.as_deref(), Some("codex_cli"));
        pool.release_without_recording("codex_cli");
    }

    #[test]
    fn auto_no_compatible_executor_returns_explicit_failure_not_noop() {
        let pool = Arc::new(ExecutorPool::new());
        crate::executor_pool::register_default_executors(
            &pool,
            false,
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        let config = SchedulerConfig {
            executor_type: "auto".to_string(),
            ..Default::default()
        };
        let route = route("summarize", "docs", None, false);
        let selected = select_scheduler_executor(
            &config,
            &pool,
            &(Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>),
            Some(&route),
        )
        .expect("unsupported work should produce bounded failure evidence");

        assert!(selected.0.is_none());
        let output = selected.1.execute_node(&NodeExecutionInput {
            node_id: "unsupported-node".to_string(),
            task_type: "summarize".to_string(),
            run_id: "unsupported-run".to_string(),
            workflow_id: "unsupported-workflow".to_string(),
            node_metadata: json!({}),
        });
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("scheduler_executor_unavailable")
        );
    }

    #[test]
    fn auto_ignores_stale_feedback_that_suggests_fixture_noop() {
        let pool = Arc::new(ExecutorPool::new());
        crate::executor_pool::register_default_executors(
            &pool,
            false,
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        let config = SchedulerConfig {
            executor_type: "auto".to_string(),
            ..Default::default()
        };
        let route = route("summarize", "docs", Some("noop"), false);
        let selected = select_scheduler_executor(
            &config,
            &pool,
            &(Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>),
            Some(&route),
        )
        .expect("stale fixture feedback should produce bounded failure evidence");

        assert!(selected.0.is_none());
        assert_eq!(selected.1.executor_type_name(), "fail");
    }

    #[test]
    fn auto_never_routes_code_to_cli_without_an_app_owned_workspace_binding() {
        let pool = Arc::new(ExecutorPool::new());
        crate::executor_pool::register_cli_executors(
            &pool,
            &crate::cli::CliConfig {
                enabled: true,
                claude_code_bin: Some("/definitely/not/executed-claude".to_string()),
                claude_code_enabled: true,
                codex_bin: Some("/definitely/not/executed-codex".to_string()),
                codex_enabled: true,
                timeout_ms: 1_000,
            },
            Arc::new(LocalProductStore::new(":memory:").unwrap()),
        );
        let config = SchedulerConfig {
            executor_type: "auto".to_string(),
            ..Default::default()
        };
        let mut route = route("implementation", "code", None, false);
        route.cli_workspace_bound = false;
        let selected = select_scheduler_executor(
            &config,
            &pool,
            &(Arc::new(NoopNodeExecutor) as Arc<dyn NodeExecutor>),
            Some(&route),
        )
        .expect("unbound code work should produce bounded failure evidence");

        assert!(selected.0.is_none());
        assert_eq!(selected.1.executor_type_name(), "fail");
    }

    #[test]
    fn arbitrary_implementation_node_is_not_reinterpreted_as_provider_work() {
        let store = LocalProductStore::new(":memory:").unwrap();
        let plan = store
            .create_workflow_plan("ordinary implementation", "test", "test", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [{
                            "node_id": "implementation-node",
                            "task_type": "implementation",
                            "status": "pending"
                        }],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
            .unwrap();

        assert_ne!(
            suggested_executor_for_run(
                &store,
                run["run_id"].as_str().unwrap(),
                &SchedulerConfig::default()
            )
            .as_deref(),
            Some("adaptive_provider")
        );
    }

    #[test]
    fn legacy_command_metadata_selects_exact_cli_under_scheduler_authority() {
        let store = LocalProductStore::new(":memory:").unwrap();
        let plan = store
            .create_workflow_plan("CLI node", "test", "test", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [{
                            "node_id": "cli-node",
                            "task_type": "command",
                            "status": "pending",
                            "executor": "codex_cli",
                            "prompt": "bounded fixture"
                        }],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
            .unwrap();

        assert_eq!(
            suggested_executor_for_run(
                &store,
                run["run_id"].as_str().unwrap(),
                &SchedulerConfig::default()
            )
            .as_deref(),
            Some("codex_cli")
        );
    }

    #[test]
    fn scheduler_suggestion_uses_ready_dependency_order_for_mixed_agent_run() {
        let store = LocalProductStore::new(":memory:").unwrap();
        let plan = store
            .create_workflow_plan("mixed agent routing", "test", "test", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [
                            {
                                "node_id": "a-agent-blocked",
                                "task_type": "agent_step",
                                "status": "pending",
                                "agent_id": "agent-mixed",
                                "agent_role": "reviewer",
                                "profile_id": "bounded",
                                "agent_objective": "review after command",
                                "capability_profile": ["review"],
                                "model": "fixture"
                            },
                            {
                                "node_id": "z-command-ready",
                                "task_type": "command",
                                "status": "pending",
                                "command": "echo ready"
                            }
                        ],
                        "edges": [{
                            "edge_id": "edge-command-agent",
                            "from_node_id": "z-command-ready",
                            "to_node_id": "a-agent-blocked"
                        }]
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
            .unwrap();
        let run_id = run["run_id"].as_str().unwrap();

        assert_eq!(
            store
                .next_ready_workflow_node_task_type_with_agent_caps(run_id, None)
                .unwrap(),
            Some("command".to_string())
        );
        assert_ne!(
            suggested_executor_for_run(&store, run_id, &SchedulerConfig::default()).as_deref(),
            Some("agent_step")
        );

        let command_tick = store
            .tick_with_executor(run_id, "test", 0, &crate::node_executor::NoopNodeExecutor)
            .unwrap();
        assert_eq!(command_tick["node_id"], "z-command-ready");
        assert_eq!(
            store
                .next_ready_workflow_node_task_type_with_agent_caps(run_id, None)
                .unwrap(),
            Some("agent_step".to_string())
        );
        let wrong_executor_tick = store
            .tick_with_executor(run_id, "test", 0, &crate::node_executor::NoopNodeExecutor)
            .unwrap();
        assert_eq!(wrong_executor_tick["action"], "node_executed");
        assert_eq!(wrong_executor_tick["node_id"], "a-agent-blocked");
        assert_eq!(wrong_executor_tick["result"]["status"], "failed");
        assert_eq!(
            wrong_executor_tick["result"]["error_domain"],
            "reserved_executor_mismatch"
        );
        let current = store.get_workflow_run(run_id).unwrap().unwrap();
        assert_eq!(current["nodes"][0]["db_status"], "failed");
    }

    #[test]
    fn scheduler_suggestion_skips_capped_agent_when_normal_node_is_ready() {
        let store = LocalProductStore::new(":memory:").unwrap();
        let plan = store
            .create_workflow_plan("cap-aware mixed routing", "test", "test", |ids, _| {
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [
                            {
                                "node_id": "a-agent-capped",
                                "task_type": "agent_step",
                                "status": "pending",
                                "agent_id": "agent-capped",
                                "assigned_agent_id": "agent-capped",
                                "agent_role": "reviewer",
                                "profile_id": "bounded",
                                "agent_objective": "bounded review",
                                "capability_profile": ["review"],
                                "decision_source": "fixture",
                                "max_actions": 1
                            },
                            {
                                "node_id": "z-command-ready",
                                "task_type": "command",
                                "status": "pending",
                                "command": "echo ready"
                            }
                        ],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
            .unwrap();
        let run_id = run["run_id"].as_str().unwrap();
        let config = SchedulerConfig {
            agent_max_concurrent_global: 0,
            agent_max_concurrent_per_run: 0,
            ..Default::default()
        };

        assert_eq!(
            store
                .next_ready_workflow_node_task_type_with_agent_caps(run_id, None)
                .unwrap(),
            Some("agent_step".to_string())
        );
        assert_eq!(
            store
                .next_ready_workflow_node_task_type_with_agent_caps(run_id, Some((0, 0)))
                .unwrap(),
            Some("command".to_string())
        );
        assert_ne!(
            suggested_executor_for_run(&store, run_id, &config).as_deref(),
            Some("agent_step")
        );
    }

    struct DynamicBlockingAgentExecutor {
        started: std::sync::mpsc::Sender<()>,
        release: Arc<std::sync::Barrier>,
    }

    impl NodeExecutor for DynamicBlockingAgentExecutor {
        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.started.send(()).unwrap();
            self.release.wait();
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("first agent step completed".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: Some(3),
                output_tokens: Some(2),
                estimated_cost: Some(0.01),
                latency_ms: Some(5),
                process_outcome: None,
            }
        }
    }

    struct DynamicCountingAgentExecutor {
        calls: Arc<AtomicUsize>,
    }

    impl NodeExecutor for DynamicCountingAgentExecutor {
        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            self.calls.fetch_add(1, Ordering::SeqCst);
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("unexpected second agent step".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(0),
                process_outcome: None,
            }
        }
    }

    #[test]
    fn dynamic_scheduler_respects_active_agent_per_run_cap() {
        let store = Arc::new(LocalProductStore::new(":memory:").unwrap());
        let plan = store
            .create_workflow_plan("dynamic agent cap", "test", "test", |ids, _| {
                let agent_node = |node_id: &str| {
                    json!({
                        "node_id": node_id,
                        "task_type": "agent_step",
                        "status": "pending",
                        "agent_id": format!("agent-{node_id}"),
                        "assigned_agent_id": format!("agent-{node_id}"),
                        "agent_role": "fixture-agent",
                        "profile_id": "bounded",
                        "agent_objective": "verify dynamic scheduler concurrency",
                        "capability_profile": ["fixture"],
                        "decision_source": "fixture",
                        "max_actions": 1
                    })
                };
                Ok(json!({
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "graph": {
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "nodes": [agent_node("agent-a"), agent_node("agent-b")],
                        "edges": []
                    },
                    "analysis": {},
                    "boundaries": {"execution_authority": "bounded_trusted_local"}
                }))
            })
            .unwrap();
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().unwrap(), "test")
            .unwrap();
        let run_id = run["run_id"].as_str().unwrap().to_string();

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let release = Arc::new(std::sync::Barrier::new(2));
        let blocking = DynamicBlockingAgentExecutor {
            started: started_tx,
            release: Arc::clone(&release),
        };
        let blocking_store = Arc::clone(&store);
        let blocking_run_id = run_id.clone();
        let blocking_thread = std::thread::spawn(move || {
            blocking_store.tick_with_executor_with_agent_caps(
                &blocking_run_id,
                "test",
                0,
                &blocking,
                2,
                1,
            )
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("first agent lease should reach its executor");

        let calls = Arc::new(AtomicUsize::new(0));
        let counting: Arc<dyn NodeExecutor> = Arc::new(DynamicCountingAgentExecutor {
            calls: Arc::clone(&calls),
        });
        let pool = Arc::new(ExecutorPool::new());
        crate::executor_pool::register_agent_step_executor(&pool, Arc::clone(&counting), 2);
        let config = SchedulerConfig {
            executor_type: "dynamic".to_string(),
            agent_max_concurrent_global: 2,
            agent_max_concurrent_per_run: 1,
            ..Default::default()
        };

        let tick = dynamic_scheduler_tick(&store, &config, counting, &pool, 1).unwrap();
        assert_eq!(tick.ticks, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let during = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(during["nodes"][0]["db_status"], "running");
        assert_eq!(during["nodes"][1]["db_status"], "pending");

        release.wait();
        assert_eq!(
            blocking_thread.join().unwrap().unwrap()["action"],
            "node_executed"
        );
    }
}
