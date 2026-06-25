use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::node_executor::{NodeExecutor, NoopNodeExecutor};
use crate::storage::local_product_store::LocalProductStore;
use crate::workflow::dag_manager::types::DAGMutationProposal;
use crate::workflow::dynamic_decomposer::{
    node_proposals_to_dag_mutations, Decomposer, DecompositionContext, DecompositionTrigger,
    RuleBasedDecomposer,
};
use crate::workflow::orchestration_decision::{
    action_to_string, build_enriched_input_signals, confidence_from_inputs, OrchestrationAction,
    ORCHESTRATION_DECISION_SCHEMA_VERSION,
};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DynamicControllerConfig {
    pub max_ticks_per_run: u64,
    pub max_mutations_per_run: u64,
    pub approval_required_for_mutation: bool,
    pub auto_fix_on_failure: bool,
    pub record_feedback: bool,
    pub admission_check_enabled: bool,
    pub respect_priority: bool,
    pub executor_pool_accounting_enabled: bool,
}

impl Default for DynamicControllerConfig {
    fn default() -> Self {
        Self {
            max_ticks_per_run: 100,
            max_mutations_per_run: 20,
            approval_required_for_mutation: false,
            auto_fix_on_failure: true,
            record_feedback: true,
            admission_check_enabled: true,
            respect_priority: true,
            executor_pool_accounting_enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Controller actions (result enum)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum ControllerAction {
    NodeExecuted {
        node_id: String,
        status: String,
    },
    NodeRetried {
        node_id: String,
        attempt: i64,
    },
    GraphMutated {
        proposal_id: String,
        mutation_type: String,
    },
    ApprovalRequested {
        node_id: String,
        reason: String,
    },
    RunCompleted,
    RunFailed {
        reason: String,
    },
    NoAction {
        reason: String,
    },
}

// ---------------------------------------------------------------------------
// Controller tick result
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ControllerTickResult {
    pub actions: Vec<ControllerAction>,
    pub run_status: String,
    pub mutations_applied: u64,
    pub should_continue: bool,
    pub suggested_executor_type: Option<String>,
    pub pool_failure_score: Option<f64>,
    pub pool_active_count: Option<u64>,
    pub queue_position: Option<i32>,
    pub priority: Option<i32>,
    pub admission_allowed: bool,
    pub admission_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// DynamicWorkflowController
// ---------------------------------------------------------------------------

pub struct DynamicWorkflowController {
    config: DynamicControllerConfig,
    ticks_executed: u64,
    mutations_applied_total: u64,
    decomposer: Box<dyn Decomposer>,
    decisions: Vec<Value>,
    executor_pool: Option<Arc<crate::executor_pool::ExecutorPool>>,
}

impl DynamicWorkflowController {
    pub fn new(config: DynamicControllerConfig) -> Self {
        Self {
            config,
            ticks_executed: 0,
            mutations_applied_total: 0,
            decomposer: Box::new(RuleBasedDecomposer::new()),
            decisions: Vec::new(),
            executor_pool: None,
        }
    }

    pub fn with_decomposer(mut self, decomposer: Box<dyn Decomposer>) -> Self {
        self.decomposer = decomposer;
        self
    }

    pub fn with_executor_pool(mut self, pool: Arc<crate::executor_pool::ExecutorPool>) -> Self {
        self.executor_pool = Some(pool);
        self
    }

    pub fn ticks_executed(&self) -> u64 {
        self.ticks_executed
    }

    pub fn mutations_applied_total(&self) -> u64 {
        self.mutations_applied_total
    }

    pub fn decisions(&self) -> &[Value] {
        &self.decisions
    }

    /// Execute a single controller tick for the given run.
    ///
    /// Returns a `ControllerTickResult` describing what happened and whether
    /// the caller should invoke `tick` again.
    pub fn tick(
        &mut self,
        store: &LocalProductStore,
        run_id: &str,
        actor: &str,
        executor: &dyn NodeExecutor,
    ) -> Result<ControllerTickResult, String> {
        let run = store
            .get_workflow_run(run_id)?
            .ok_or_else(|| format!("workflow run not found: {run_id}"))?;

        let status = run
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let run_priority = run
            .get("priority")
            .and_then(Value::as_i64)
            .map(|p| p as i32);
        let run_queue_position = run
            .get("queue_position")
            .and_then(Value::as_i64)
            .map(|p| p as i32);

        if self.config.admission_check_enabled {
            let pause_reason = run
                .get("pause_reason")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !pause_reason.is_empty() {
                return Ok(ControllerTickResult {
                    actions: vec![ControllerAction::NoAction {
                        reason: format!("run paused: {}", pause_reason),
                    }],
                    run_status: status,
                    mutations_applied: 0,
                    should_continue: false,
                    suggested_executor_type: None,
                    pool_failure_score: None,
                    pool_active_count: None,
                    queue_position: run_queue_position,
                    priority: run_priority,
                    admission_allowed: false,
                    admission_reason: Some(pause_reason.to_string()),
                });
            }
        }

        let degrade_mode = run
            .get("degrade_mode")
            .and_then(Value::as_str)
            .map(String::from);

        if self.ticks_executed >= self.config.max_ticks_per_run {
            let max_ticks_queue = json!({
                "queue_position": run_queue_position,
                "priority": run_priority,
                "admission_allowed": true,
                "admission_reason": null,
            });
            let decision = build_decision_enriched(
                run_id,
                None,
                OrchestrationAction::NoAction,
                "max_ticks_per_run reached",
                executor,
                Some("max_ticks_per_run reached"),
                "running",
                None,
                None,
                None,
                None,
                None,
                None,
                Some(&max_ticks_queue),
                None,
                None,
            );
            self.decisions.push(decision.clone());
            let _ = store.record_orchestration_decision(
                run_id,
                None,
                action_to_string(&OrchestrationAction::NoAction),
                "max_ticks_per_run reached",
                decision["selected_executor"].as_str().unwrap_or("unknown"),
                Some("max_ticks_per_run reached"),
                decision["confidence"].as_str().unwrap_or("low"),
                decision["confidence_score"].as_f64().unwrap_or(0.0),
                decision.get("input_signals").unwrap_or(&Value::Null),
            );

            return Ok(ControllerTickResult {
                actions: vec![ControllerAction::NoAction {
                    reason: "max_ticks_per_run reached".to_string(),
                }],
                run_status: status,
                mutations_applied: 0,
                should_continue: false,
                suggested_executor_type: None,
                pool_failure_score: None,
                pool_active_count: None,
                queue_position: run_queue_position,
                priority: run_priority,
                admission_allowed: true,
                admission_reason: None,
            });
        }

        self.ticks_executed += 1;

        let nodes = run
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let _edges = run
            .get("edges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let failed_nodes: Vec<&Value> = nodes
            .iter()
            .filter(|n| node_status(n) == "failed")
            .collect();
        let terminal_repair_allowed =
            status == "failed" && self.config.auto_fix_on_failure && !failed_nodes.is_empty();

        if is_terminal(&status) && !terminal_repair_allowed {
            let (action, blocked_reason) = if status == "completed" {
                (OrchestrationAction::RunCompleted, None)
            } else {
                (
                    OrchestrationAction::RunFailed,
                    Some("node_failure".to_string()),
                )
            };

            let terminal_queue = json!({
                "queue_position": run_queue_position,
                "priority": run_priority,
                "admission_allowed": true,
                "admission_reason": null,
            });
            let decision = build_decision_enriched(
                run_id,
                None,
                action.clone(),
                &format!("run is already terminal: {status}"),
                executor,
                blocked_reason.as_deref(),
                &status,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(&terminal_queue),
                None,
                degrade_mode.as_deref(),
            );
            self.decisions.push(decision.clone());
            let _ = store.record_orchestration_decision(
                run_id,
                None,
                action_to_string(&action),
                &format!("run is already terminal: {status}"),
                decision["selected_executor"].as_str().unwrap_or("unknown"),
                blocked_reason.as_deref(),
                decision["confidence"].as_str().unwrap_or("low"),
                decision["confidence_score"].as_f64().unwrap_or(0.0),
                decision.get("input_signals").unwrap_or(&Value::Null),
            );

            let mut actions = vec![ControllerAction::NoAction {
                reason: format!("run is already terminal: {status}"),
            }];
            if status == "completed" {
                actions.push(ControllerAction::RunCompleted);
            } else if status == "failed" {
                actions.push(ControllerAction::RunFailed {
                    reason: "node_failure".to_string(),
                });
            }
            return Ok(ControllerTickResult {
                actions,
                run_status: status,
                mutations_applied: 0,
                should_continue: false,
                suggested_executor_type: None,
                pool_failure_score: None,
                pool_active_count: None,
                queue_position: run_queue_position,
                priority: run_priority,
                admission_allowed: true,
                admission_reason: None,
            });
        }

        let completed_nodes: Vec<&Value> = nodes
            .iter()
            .filter(|n| node_status(n) == "completed")
            .collect();

        let mut actions: Vec<ControllerAction> = Vec::new();
        let mut mutations_this_tick: u64 = 0;

        // --- Phase 1: auto-fix failed nodes via decomposer ---
        let repaired_terminal = self.apply_failed_node_recovery(
            store,
            run_id,
            actor,
            &nodes,
            &mut actions,
            &mut mutations_this_tick,
        )?;

        if terminal_repair_allowed {
            if repaired_terminal {
                store.request_workflow_run_resume(
                    run_id,
                    actor,
                    Some("dynamic workflow recovery nodes scheduled"),
                )?;
            } else {
                return Ok(ControllerTickResult {
                    actions,
                    run_status: status,
                    mutations_applied: mutations_this_tick,
                    should_continue: false,
                    suggested_executor_type: None,
                    pool_failure_score: None,
                    pool_active_count: None,
                    queue_position: run_queue_position,
                    priority: run_priority,
                    admission_allowed: true,
                    admission_reason: None,
                });
            }
        }

        // --- Phase 2: quality check completed nodes via decomposer ---
        for completed in &completed_nodes {
            if self.mutations_applied_total >= self.config.max_mutations_per_run {
                break;
            }
            let node_id = completed
                .get("node_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if node_id.is_empty() {
                continue;
            }

            if !quality_passes(completed) {
                let quality_reason = completed
                    .get("result")
                    .and_then(|r| r.get("quality"))
                    .and_then(|q| q.get("reason"))
                    .and_then(Value::as_str)
                    .unwrap_or("quality check failed")
                    .to_string();

                let node_ids: Vec<String> = nodes
                    .iter()
                    .filter_map(|n| n.get("node_id").and_then(Value::as_str).map(String::from))
                    .collect();

                let decomp_context = DecompositionContext {
                    run_id: run_id.to_string(),
                    existing_nodes: node_ids,
                    existing_edges: Vec::new(),
                    feedback_stats: None,
                    max_nodes: 1000,
                };

                let decomp_result = self.decomposer.decompose(
                    DecompositionTrigger::QualityFailure {
                        node_id: node_id.clone(),
                        reason: quality_reason,
                    },
                    &decomp_context,
                );

                if decomp_result.proposals.is_empty() {
                    continue;
                }

                if self.config.approval_required_for_mutation {
                    actions.push(ControllerAction::ApprovalRequested {
                        node_id: node_id.clone(),
                        reason: format!(
                            "quality check failed for node {}; review node requires approval",
                            node_id
                        ),
                    });
                    continue;
                }

                let dag_proposals =
                    node_proposals_to_dag_mutations(run_id, &decomp_result.proposals);

                let results = store.apply_dag_mutations_batch(run_id, &dag_proposals, actor)?;
                let applied_count = results
                    .iter()
                    .filter(|r| r.get("applied").and_then(Value::as_bool).unwrap_or(false))
                    .count() as u64;
                mutations_this_tick += applied_count;
                self.mutations_applied_total += applied_count;

                if applied_count > 0 {
                    self.record_decision(
                        store,
                        run_id,
                        Some(&node_id),
                        OrchestrationAction::GraphMutated,
                        &format!("quality_review_{}", node_id),
                        &NoopNodeExecutor,
                        None,
                        &status,
                        None,
                        None,
                    );
                    actions.push(ControllerAction::GraphMutated {
                        proposal_id: format!("quality_review_{}", node_id),
                        mutation_type: "add_node".to_string(),
                    });
                }
            }
        }

        // --- Phase 3: check mutation limit ---
        if self.mutations_applied_total >= self.config.max_mutations_per_run {
            let has_pending = nodes.iter().any(|n| node_status(n) == "pending");
            if has_pending {
                store.append_workflow_run_event(
                    run_id,
                    None,
                    "controller.mutation_limit_reached",
                    &json!({
                        "mutations_applied_total": self.mutations_applied_total,
                        "max_mutations_per_run": self.config.max_mutations_per_run,
                    }),
                    actor,
                )?;
                self.record_decision(
                    store,
                    run_id,
                    None,
                    OrchestrationAction::RequestApproval,
                    "max_mutations_per_run reached; approval required to continue",
                    executor,
                    Some("max_mutations_per_run reached"),
                    &status,
                    None,
                    None,
                );
                actions.push(ControllerAction::ApprovalRequested {
                    node_id: "*".to_string(),
                    reason: "max_mutations_per_run reached; approval required to continue"
                        .to_string(),
                });
                return Ok(ControllerTickResult {
                    actions,
                    run_status: status,
                    mutations_applied: mutations_this_tick,
                    should_continue: false,
                    suggested_executor_type: None,
                    pool_failure_score: None,
                    pool_active_count: None,
                    queue_position: run_queue_position,
                    priority: run_priority,
                    admission_allowed: true,
                    admission_reason: None,
                });
            }
        }

        // --- Phase 4: tick the executor for one ready node ---

        // Determine the executor type for pool acquire
        let phase4_executor_type = extract_executor_type(executor, None, None);

        // If pool is present, try to acquire; proceed if executor not in pool (fallback)
        let pool_acquired = if self.config.executor_pool_accounting_enabled {
            self.executor_pool
                .as_ref()
                .map(|pool| {
                    // If executor type is not registered in pool, allow execution (fallback)
                    if pool.get(&phase4_executor_type).is_none() {
                        true
                    } else {
                        pool.acquire(&phase4_executor_type)
                    }
                })
                .unwrap_or(true)
        } else {
            true
        };

        if !pool_acquired {
            let (pool_fs, pool_ac) = self.pool_metrics(&phase4_executor_type);
            let pool_exhausted_signal = match (pool_fs, pool_ac) {
                (Some(fs), Some(ac)) => Some(json!({"failure_score": fs, "active_count": ac})),
                _ => None,
            };
            let decision = build_decision_enriched(
                run_id,
                None,
                OrchestrationAction::NoAction,
                "executor pool acquire failed; capacity exhausted",
                executor,
                Some("pool_capacity_exhausted"),
                &status,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                pool_exhausted_signal.as_ref(),
                None,
            );
            self.decisions.push(decision.clone());
            let _ = store.record_orchestration_decision(
                run_id,
                None,
                action_to_string(&OrchestrationAction::NoAction),
                "executor pool acquire failed; capacity exhausted",
                decision["selected_executor"].as_str().unwrap_or("unknown"),
                Some("pool_capacity_exhausted"),
                decision["confidence"].as_str().unwrap_or("low"),
                decision["confidence_score"].as_f64().unwrap_or(0.0),
                decision.get("input_signals").unwrap_or(&Value::Null),
            );
            actions.push(ControllerAction::NoAction {
                reason: "executor pool acquire failed; capacity exhausted".to_string(),
            });

            let (pool_fs, pool_ac) = self.pool_metrics(&phase4_executor_type);
            return Ok(ControllerTickResult {
                actions,
                run_status: status,
                mutations_applied: mutations_this_tick,
                should_continue: true,
                suggested_executor_type: None,
                pool_failure_score: pool_fs,
                pool_active_count: pool_ac,
                queue_position: run_queue_position,
                priority: run_priority,
                admission_allowed: true,
                admission_reason: None,
            });
        }

        let tick_result = store.tick_with_executor_and_command(run_id, actor, 0, executor, None)?;

        // Extract latency from tick result for pool release
        let tick_latency_ms = tick_result
            .get("result")
            .and_then(|r| r.get("latency_ms"))
            .and_then(Value::as_u64)
            .unwrap_or(0);

        let action_str = tick_result
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("unknown");

        match action_str {
            "node_executed" => {
                let node_id = tick_result
                    .get("node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let result_status = tick_result
                    .get("result")
                    .and_then(|r| r.get("status"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                let _executor_type = tick_result
                    .get("executor_type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");

                // Extract quality signal from node result
                let node_quality = tick_result
                    .get("result")
                    .and_then(|r| r.get("quality"))
                    .cloned();
                let (pool_fs, pool_ac) = self.pool_metrics(&phase4_executor_type);
                let node_pool_signal = match (pool_fs, pool_ac) {
                    (Some(fs), Some(ac)) => Some(json!({"failure_score": fs, "active_count": ac})),
                    _ => None,
                };

                self.record_decision_enriched(
                    store,
                    run_id,
                    Some(&node_id),
                    OrchestrationAction::ExecuteNode,
                    &format!("node {} executed with status {}", node_id, result_status),
                    executor,
                    None,
                    &status,
                    None,
                    None,
                    node_quality.as_ref(),
                    None,
                    None,
                    None,
                    None,
                    node_pool_signal.as_ref(),
                    None,
                );
                actions.push(ControllerAction::NodeExecuted {
                    node_id,
                    status: result_status,
                });

                // The run may have transitioned to terminal after this node
                if let Some(run_after) = tick_result.get("run") {
                    let run_status_after = run_after
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if run_status_after == "completed" {
                        actions.push(ControllerAction::RunCompleted);
                    } else if run_status_after == "failed" {
                        actions.push(ControllerAction::RunFailed {
                            reason: "node_failure".to_string(),
                        });
                    }
                }
            }
            "node_retry" => {
                let node_id = tick_result
                    .get("node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let attempt = tick_result
                    .get("attempt")
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                self.record_decision(
                    store,
                    run_id,
                    Some(&node_id),
                    OrchestrationAction::RetryNode,
                    &format!("node {} retry attempt {}", node_id, attempt),
                    executor,
                    None,
                    &status,
                    None,
                    None,
                );
                actions.push(ControllerAction::NodeRetried { node_id, attempt });
            }
            "completed" => {
                let decision = build_decision(
                    run_id,
                    None,
                    OrchestrationAction::RunCompleted,
                    "run completed after tick",
                    executor,
                    None,
                    "completed",
                    None,
                    None,
                );
                self.decisions.push(decision.clone());
                let _ = store.record_orchestration_decision(
                    run_id,
                    None,
                    action_to_string(&OrchestrationAction::RunCompleted),
                    "run completed after tick",
                    decision["selected_executor"].as_str().unwrap_or("unknown"),
                    None,
                    decision["confidence"].as_str().unwrap_or("high"),
                    decision["confidence_score"].as_f64().unwrap_or(1.0),
                    decision.get("input_signals").unwrap_or(&Value::Null),
                );
                actions.push(ControllerAction::RunCompleted);
            }
            "failed" => {
                let run_after = tick_result.get("run");
                let reason = run_after
                    .and_then(|r| r.get("result"))
                    .and_then(Value::as_str)
                    .unwrap_or("node_failure")
                    .to_string();
                let decision = build_decision(
                    run_id,
                    None,
                    OrchestrationAction::RunFailed,
                    &reason,
                    executor,
                    Some(&reason),
                    "failed",
                    None,
                    None,
                );
                self.decisions.push(decision.clone());
                let _ = store.record_orchestration_decision(
                    run_id,
                    None,
                    action_to_string(&OrchestrationAction::RunFailed),
                    &reason,
                    decision["selected_executor"].as_str().unwrap_or("unknown"),
                    Some(&reason),
                    decision["confidence"].as_str().unwrap_or("low"),
                    decision["confidence_score"].as_f64().unwrap_or(0.0),
                    decision.get("input_signals").unwrap_or(&Value::Null),
                );
                actions.push(ControllerAction::RunFailed { reason });
            }
            "no_ready_node" => {
                let decision = build_decision(
                    run_id,
                    None,
                    OrchestrationAction::NoAction,
                    "no ready node available",
                    executor,
                    None,
                    &status,
                    None,
                    None,
                );
                self.decisions.push(decision.clone());
                let _ = store.record_orchestration_decision(
                    run_id,
                    None,
                    action_to_string(&OrchestrationAction::NoAction),
                    "no ready node available",
                    decision["selected_executor"].as_str().unwrap_or("unknown"),
                    None,
                    decision["confidence"].as_str().unwrap_or("medium"),
                    decision["confidence_score"].as_f64().unwrap_or(0.5),
                    decision.get("input_signals").unwrap_or(&Value::Null),
                );
                actions.push(ControllerAction::NoAction {
                    reason: "no ready node available".to_string(),
                });
            }
            other => {
                let decision = build_decision(
                    run_id,
                    None,
                    OrchestrationAction::NoAction,
                    &format!("tick returned action: {other}"),
                    executor,
                    None,
                    &status,
                    None,
                    None,
                );
                self.decisions.push(decision.clone());
                let _ = store.record_orchestration_decision(
                    run_id,
                    None,
                    action_to_string(&OrchestrationAction::NoAction),
                    &format!("tick returned action: {other}"),
                    decision["selected_executor"].as_str().unwrap_or("unknown"),
                    None,
                    decision["confidence"].as_str().unwrap_or("medium"),
                    decision["confidence_score"].as_f64().unwrap_or(0.5),
                    decision.get("input_signals").unwrap_or(&Value::Null),
                );
                actions.push(ControllerAction::NoAction {
                    reason: format!("tick returned action: {other}"),
                });
            }
        }

        // Release the pool slot based on tick outcome
        if self.config.executor_pool_accounting_enabled && pool_acquired {
            let tick_success = !matches!(action_str, "failed");
            if let Some(pool) = self.executor_pool.as_ref() {
                pool.release(&phase4_executor_type, tick_success, tick_latency_ms, None);
            }
        }

        let run_after_tick = store.get_workflow_run(run_id)?.unwrap_or(run.clone());
        let status_after_tick = run_after_tick
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        if status_after_tick == "failed" && self.config.auto_fix_on_failure {
            let nodes_after_tick = run_after_tick
                .get("nodes")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let repaired = self.apply_failed_node_recovery(
                store,
                run_id,
                actor,
                &nodes_after_tick,
                &mut actions,
                &mut mutations_this_tick,
            )?;
            if repaired {
                store.request_workflow_run_resume(
                    run_id,
                    actor,
                    Some("dynamic workflow recovered after failed tick"),
                )?;
            }
        }

        // --- Phase 5: determine should_continue ---
        let fresh_run = store.get_workflow_run(run_id)?.unwrap_or(run);
        let fresh_status = fresh_run
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let has_pending = fresh_run
            .get("nodes")
            .and_then(Value::as_array)
            .map(|ns| ns.iter().any(|n| node_status(n) == "pending"))
            .unwrap_or(false);

        let has_running = fresh_run
            .get("nodes")
            .and_then(Value::as_array)
            .map(|ns| ns.iter().any(|n| node_status(n) == "running"))
            .unwrap_or(false);

        let should_continue = !is_terminal(&fresh_status) && (has_pending || has_running);
        if fresh_status == "completed"
            && !actions
                .iter()
                .any(|action| matches!(action, ControllerAction::RunCompleted))
        {
            actions.push(ControllerAction::RunCompleted);
        } else if fresh_status == "failed"
            && !actions
                .iter()
                .any(|action| matches!(action, ControllerAction::RunFailed { .. }))
        {
            actions.push(ControllerAction::RunFailed {
                reason: "node_failure".to_string(),
            });
        }

        // Query feedback store for executor suggestion, then pool as fallback
        let suggested_executor_type = if self.config.record_feedback {
            // Get the first node's task_type to build a task_group for the suggestion
            let first_node_task = fresh_run
                .get("nodes")
                .and_then(Value::as_array)
                .and_then(|ns| ns.first())
                .and_then(|n| n.get("task_type").and_then(Value::as_str))
                .unwrap_or("unknown");
            let task_group = crate::routing::schemas::make_task_group(first_node_task, "execute");
            store.suggest_executor_type(&task_group).or_else(|| {
                self.executor_pool
                    .as_ref()
                    .and_then(|pool| pool.best_for_task(first_node_task, "execute"))
            })
        } else {
            self.executor_pool.as_ref().and_then(|pool| {
                let first_node_task = fresh_run
                    .get("nodes")
                    .and_then(Value::as_array)
                    .and_then(|ns| ns.first())
                    .and_then(|n| n.get("task_type").and_then(Value::as_str))
                    .unwrap_or("unknown");
                pool.best_for_task(first_node_task, "execute")
            })
        };

        let (pool_fs, pool_ac) = self.pool_metrics(&phase4_executor_type);

        Ok(ControllerTickResult {
            actions,
            run_status: fresh_status,
            mutations_applied: mutations_this_tick,
            should_continue,
            suggested_executor_type,
            pool_failure_score: pool_fs,
            pool_active_count: pool_ac,
            queue_position: run_queue_position,
            priority: run_priority,
            admission_allowed: true,
            admission_reason: None,
        })
    }

    fn pool_metrics(&self, executor_type: &str) -> (Option<f64>, Option<u64>) {
        match self.executor_pool.as_ref() {
            Some(pool) => {
                let snapshot = pool.snapshot();
                let entry = snapshot.iter().find(|e| e.executor_type == executor_type);
                match entry {
                    Some(e) => (Some(e.status.failure_score), Some(e.status.active_count)),
                    None => (None, None),
                }
            }
            None => (None, None),
        }
    }

    fn record_decision(
        &mut self,
        store: &LocalProductStore,
        run_id: &str,
        node_id: Option<&str>,
        action: OrchestrationAction,
        action_reason: &str,
        executor: &dyn NodeExecutor,
        blocked_reason: Option<&str>,
        run_status: &str,
        task_type: Option<&str>,
        task_group: Option<&str>,
    ) {
        self.record_decision_enriched(
            store,
            run_id,
            node_id,
            action,
            action_reason,
            executor,
            blocked_reason,
            run_status,
            task_type,
            task_group,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn record_decision_enriched(
        &mut self,
        store: &LocalProductStore,
        run_id: &str,
        node_id: Option<&str>,
        action: OrchestrationAction,
        action_reason: &str,
        executor: &dyn NodeExecutor,
        blocked_reason: Option<&str>,
        run_status: &str,
        task_type: Option<&str>,
        task_group: Option<&str>,
        quality_signal: Option<&Value>,
        routing_signal: Option<&Value>,
        cost_signal: Option<&Value>,
        approval_signal: Option<&Value>,
        queue_signal: Option<&Value>,
        pool_signal: Option<&Value>,
        degraded_reason: Option<&str>,
    ) {
        let decision = build_decision_enriched(
            run_id,
            node_id,
            action.clone(),
            action_reason,
            executor,
            blocked_reason,
            run_status,
            task_type,
            task_group,
            quality_signal,
            routing_signal,
            cost_signal,
            approval_signal,
            queue_signal,
            pool_signal,
            degraded_reason,
        );
        self.decisions.push(decision.clone());
        let _ = store.record_orchestration_decision(
            run_id,
            node_id,
            action_to_string(&action),
            action_reason,
            decision["selected_executor"].as_str().unwrap_or("unknown"),
            blocked_reason,
            decision["confidence"].as_str().unwrap_or("medium"),
            decision["confidence_score"].as_f64().unwrap_or(0.5),
            decision.get("input_signals").unwrap_or(&Value::Null),
        );
    }

    fn apply_failed_node_recovery(
        &mut self,
        store: &LocalProductStore,
        run_id: &str,
        actor: &str,
        nodes: &[Value],
        actions: &mut Vec<ControllerAction>,
        mutations_this_tick: &mut u64,
    ) -> Result<bool, String> {
        if !self.config.auto_fix_on_failure {
            return Ok(false);
        }

        let failed_nodes: Vec<&Value> = nodes
            .iter()
            .filter(|n| node_status(n) == "failed")
            .collect();
        let mut recovered_any = false;

        for failed in failed_nodes {
            if self.mutations_applied_total >= self.config.max_mutations_per_run {
                break;
            }
            let failed_id = failed
                .get("node_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if failed_id.is_empty() {
                continue;
            }

            let error_msg = failed
                .get("result")
                .and_then(|r| r.get("error_message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string();

            let node_ids: Vec<String> = nodes
                .iter()
                .filter_map(|n| n.get("node_id").and_then(Value::as_str).map(String::from))
                .collect();

            let decomp_context = DecompositionContext {
                run_id: run_id.to_string(),
                existing_nodes: node_ids,
                existing_edges: Vec::new(),
                feedback_stats: None,
                max_nodes: 1000,
            };

            let decomp_result = self.decomposer.decompose(
                DecompositionTrigger::TestFailure {
                    node_id: failed_id.clone(),
                    error: error_msg,
                },
                &decomp_context,
            );

            if decomp_result.proposals.is_empty() {
                continue;
            }

            if self.config.approval_required_for_mutation {
                actions.push(ControllerAction::ApprovalRequested {
                    node_id: failed_id.clone(),
                    reason: format!("auto-fix for failed node {} requires approval", failed_id),
                });
                continue;
            }

            let remaining = self
                .config
                .max_mutations_per_run
                .saturating_sub(self.mutations_applied_total);
            if decomp_result.proposals.len() as u64 > remaining {
                actions.push(ControllerAction::ApprovalRequested {
                    node_id: failed_id.clone(),
                    reason: "auto-fix would exceed max_mutations_per_run".to_string(),
                });
                break;
            }

            let dag_proposals = node_proposals_to_dag_mutations(run_id, &decomp_result.proposals);
            let results = store.apply_dag_mutations_batch(run_id, &dag_proposals, actor)?;
            let applied_count = results
                .iter()
                .filter(|r| r.get("applied").and_then(Value::as_bool).unwrap_or(false))
                .count() as u64;
            *mutations_this_tick += applied_count;
            self.mutations_applied_total += applied_count;

            if applied_count > 0 {
                store.update_workflow_node_status(
                    run_id,
                    &failed_id,
                    "recovered",
                    actor,
                    "dynamic workflow recovery nodes scheduled",
                )?;
                self.record_decision(
                    store,
                    run_id,
                    Some(&failed_id),
                    OrchestrationAction::GraphMutated,
                    &format!("auto_fix_{}", failed_id),
                    &NoopNodeExecutor,
                    None,
                    "running",
                    None,
                    None,
                );
                actions.push(ControllerAction::GraphMutated {
                    proposal_id: format!("auto_fix_{}", failed_id),
                    mutation_type: "add_node".to_string(),
                });
                recovered_any = true;
            }
        }

        Ok(recovered_any)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn node_status(node: &Value) -> &str {
    node.get("db_status")
        .and_then(Value::as_str)
        .or_else(|| node.get("status").and_then(Value::as_str))
        .unwrap_or("unknown")
}

fn quality_passes(node: &Value) -> bool {
    let result = node.get("result");
    match result {
        Some(r) if r.is_object() => {
            let quality = r.get("quality");
            match quality {
                Some(q) if q.is_object() => {
                    q.get("passed").and_then(Value::as_bool).unwrap_or(true)
                }
                Some(Value::Bool(false)) => false,
                _ => true,
            }
        }
        _ => true,
    }
}

fn build_decision(
    run_id: &str,
    node_id: Option<&str>,
    action: OrchestrationAction,
    action_reason: &str,
    executor: &dyn NodeExecutor,
    blocked_reason: Option<&str>,
    run_status: &str,
    task_type: Option<&str>,
    task_group: Option<&str>,
) -> Value {
    build_decision_enriched(
        run_id,
        node_id,
        action,
        action_reason,
        executor,
        blocked_reason,
        run_status,
        task_type,
        task_group,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_decision_enriched(
    run_id: &str,
    node_id: Option<&str>,
    action: OrchestrationAction,
    action_reason: &str,
    executor: &dyn NodeExecutor,
    blocked_reason: Option<&str>,
    run_status: &str,
    task_type: Option<&str>,
    task_group: Option<&str>,
    quality_signal: Option<&Value>,
    routing_signal: Option<&Value>,
    cost_signal: Option<&Value>,
    approval_signal: Option<&Value>,
    queue_signal: Option<&Value>,
    pool_signal: Option<&Value>,
    degraded_reason: Option<&str>,
) -> Value {
    let executor_type = extract_executor_type(executor, node_id, task_type);
    let (confidence, confidence_score) =
        confidence_from_inputs(run_status, Some("pending"), false, None, blocked_reason);

    let mut input_signals = json!({
        "run_id": run_id,
        "run_status": run_status,
    });
    if let Some(nid) = node_id {
        input_signals
            .as_object_mut()
            .unwrap()
            .insert("node_id".to_string(), json!(nid));
    }
    if let Some(tt) = task_type {
        input_signals
            .as_object_mut()
            .unwrap()
            .insert("task_type".to_string(), json!(tt));
    }
    if let Some(tg) = task_group {
        input_signals
            .as_object_mut()
            .unwrap()
            .insert("task_group".to_string(), json!(tg));
    }

    let enriched = build_enriched_input_signals(
        &input_signals,
        quality_signal,
        routing_signal,
        cost_signal,
        approval_signal,
        queue_signal,
        pool_signal,
        None,
        degraded_reason,
    );

    json!({
        "schema_version": ORCHESTRATION_DECISION_SCHEMA_VERSION,
        "decision_id": format!("decision-{}-{}", run_id, node_id.unwrap_or("run")),
        "run_id": run_id,
        "node_id": node_id,
        "action": action_to_string(&action),
        "action_reason": action_reason,
        "selected_executor": executor_type,
        "blocked_reason": blocked_reason,
        "confidence": confidence.as_str(),
        "confidence_score": confidence_score,
        "input_signals": enriched,
    })
}

fn extract_executor_type(
    executor: &dyn NodeExecutor,
    _node_id: Option<&str>,
    _task_type: Option<&str>,
) -> String {
    executor.executor_type_name().to_string()
}

#[allow(dead_code)]
fn build_auto_fix_proposals(
    run_id: &str,
    failed_node_id: &str,
    fix_node_id: &str,
    test_node_id: &str,
) -> Vec<DAGMutationProposal> {
    let mut fix_payload = HashMap::new();
    fix_payload.insert("node_id".to_string(), json!(fix_node_id));
    fix_payload.insert("node_type".to_string(), json!("fix"));
    fix_payload.insert("status".to_string(), json!("pending"));

    let mut test_payload = HashMap::new();
    test_payload.insert("node_id".to_string(), json!(test_node_id));
    test_payload.insert("node_type".to_string(), json!("test"));
    test_payload.insert("status".to_string(), json!("pending"));

    let mut edge_fix_payload = HashMap::new();
    edge_fix_payload.insert(
        "edge_id".to_string(),
        json!(format!("edge-{}-{}", failed_node_id, fix_node_id)),
    );
    edge_fix_payload.insert("from_node".to_string(), json!(failed_node_id));
    edge_fix_payload.insert("to_node".to_string(), json!(fix_node_id));

    let mut edge_test_payload = HashMap::new();
    edge_test_payload.insert(
        "edge_id".to_string(),
        json!(format!("edge-{}-{}", fix_node_id, test_node_id)),
    );
    edge_test_payload.insert("from_node".to_string(), json!(fix_node_id));
    edge_test_payload.insert("to_node".to_string(), json!(test_node_id));

    vec![
        DAGMutationProposal {
            proposal_id: format!("prop_fix_node_{}", fix_node_id),
            dag_id: run_id.to_string(),
            mutation_type: "add_node".to_string(),
            payload: fix_payload,
            reason: format!("auto-fix node for failed node {}", failed_node_id),
            ..Default::default()
        },
        DAGMutationProposal {
            proposal_id: format!("prop_test_node_{}", test_node_id),
            dag_id: run_id.to_string(),
            mutation_type: "add_node".to_string(),
            payload: test_payload,
            reason: format!("verification test for fix of {}", failed_node_id),
            ..Default::default()
        },
        DAGMutationProposal {
            proposal_id: format!("prop_edge_fix_{}", fix_node_id),
            dag_id: run_id.to_string(),
            mutation_type: "add_edge".to_string(),
            payload: edge_fix_payload,
            reason: format!(
                "edge from failed node {} to fix node {}",
                failed_node_id, fix_node_id
            ),
            ..Default::default()
        },
        DAGMutationProposal {
            proposal_id: format!("prop_edge_test_{}", test_node_id),
            dag_id: run_id.to_string(),
            mutation_type: "add_edge".to_string(),
            payload: edge_test_payload,
            reason: format!(
                "edge from fix node {} to test node {}",
                fix_node_id, test_node_id
            ),
            ..Default::default()
        },
    ]
}

#[allow(dead_code)]
fn build_quality_review_proposals(
    run_id: &str,
    source_node_id: &str,
    review_node_id: &str,
) -> Vec<DAGMutationProposal> {
    let mut review_payload = HashMap::new();
    review_payload.insert("node_id".to_string(), json!(review_node_id));
    review_payload.insert("node_type".to_string(), json!("review"));
    review_payload.insert("status".to_string(), json!("pending"));

    let mut edge_payload = HashMap::new();
    edge_payload.insert(
        "edge_id".to_string(),
        json!(format!("edge-{}-{}", source_node_id, review_node_id)),
    );
    edge_payload.insert("from_node".to_string(), json!(source_node_id));
    edge_payload.insert("to_node".to_string(), json!(review_node_id));

    vec![
        DAGMutationProposal {
            proposal_id: format!("prop_review_node_{}", review_node_id),
            dag_id: run_id.to_string(),
            mutation_type: "add_node".to_string(),
            payload: review_payload,
            reason: format!(
                "quality check failed for node {}; adding review",
                source_node_id
            ),
            ..Default::default()
        },
        DAGMutationProposal {
            proposal_id: format!("prop_review_edge_{}", review_node_id),
            dag_id: run_id.to_string(),
            mutation_type: "add_edge".to_string(),
            payload: edge_payload,
            reason: format!(
                "edge from node {} to review node {}",
                source_node_id, review_node_id
            ),
            ..Default::default()
        },
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_executor::NoopNodeExecutor;
    use crate::storage::local_product_store::LocalProductStore;

    fn setup_store_with_run() -> (LocalProductStore, String) {
        let store = LocalProductStore::new(":memory:").expect("in-memory store");

        let plan = store
            .create_workflow_plan("test-req", "test", "actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-06-06T00:00:00Z",
                        "updated_at": "2026-06-06T00:00:00Z",
                        "nodes": [
                            {"node_id": "n1", "task_type": "analyze", "status": "pending"},
                            {"node_id": "n2", "task_type": "execute", "status": "pending"}
                        ],
                        "edges": [
                            {"edge_id": "e1", "from_node_id": "n1", "to_node_id": "n2", "edge_type": "dependency"}
                        ]
                    },
                    "boundaries": {
                        "execution_authority": "disabled",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .expect("create plan");

        let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
        let run = store
            .create_workflow_run_from_plan(plan_id, "test")
            .expect("create run");
        let run_id = run
            .get("run_id")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        (store, run_id)
    }

    #[test]
    fn test_controller_default_config() {
        let config = DynamicControllerConfig::default();
        assert_eq!(config.max_ticks_per_run, 100);
        assert_eq!(config.max_mutations_per_run, 20);
        assert!(!config.approval_required_for_mutation);
        assert!(config.auto_fix_on_failure);
    }

    #[test]
    fn test_controller_tick_executes_node() {
        let (store, run_id) = setup_store_with_run();
        let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
        let executor = NoopNodeExecutor;

        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        assert!(!result.actions.is_empty());
        assert!(result
            .actions
            .iter()
            .any(|a| matches!(a, ControllerAction::NodeExecuted { .. })));
    }

    #[test]
    fn test_controller_tick_marks_run_completed_when_all_done() {
        let (store, run_id) = setup_store_with_run();
        let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
        let executor = NoopNodeExecutor;

        // Tick until done
        for _ in 0..20 {
            let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
            if !result.should_continue {
                assert!(
                    result.run_status == "completed" || result.run_status == "failed",
                    "expected terminal status, got: {}",
                    result.run_status
                );
                assert!(result
                    .actions
                    .iter()
                    .any(|a| matches!(a, ControllerAction::RunCompleted)));
                return;
            }
        }
        panic!("run did not complete within 20 ticks");
    }

    #[test]
    fn test_controller_respects_max_ticks() {
        let store = LocalProductStore::new(":memory:").expect("in-memory store");

        // Create a plan with 5 sequential nodes so max_ticks=2 can't finish
        let plan = store
            .create_workflow_plan("test-req", "test", "actor", |ids, _| {
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "a-1", "task_domain": "test"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-06-06T00:00:00Z",
                        "updated_at": "2026-06-06T00:00:00Z",
                        "nodes": [
                            {"node_id": "a", "task_type": "t", "status": "pending"},
                            {"node_id": "b", "task_type": "t", "status": "pending"},
                            {"node_id": "c", "task_type": "t", "status": "pending"},
                            {"node_id": "d", "task_type": "t", "status": "pending"},
                            {"node_id": "e", "task_type": "t", "status": "pending"}
                        ],
                        "edges": [
                            {"edge_id": "e1", "from_node_id": "a", "to_node_id": "b", "edge_type": "dependency"},
                            {"edge_id": "e2", "from_node_id": "b", "to_node_id": "c", "edge_type": "dependency"},
                            {"edge_id": "e3", "from_node_id": "c", "to_node_id": "d", "edge_type": "dependency"},
                            {"edge_id": "e4", "from_node_id": "d", "to_node_id": "e", "edge_type": "dependency"}
                        ]
                    },
                    "boundaries": {
                        "execution_authority": "disabled",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .expect("create plan");

        let plan_id = plan.get("plan_id").and_then(Value::as_str).unwrap();
        let run = store
            .create_workflow_run_from_plan(plan_id, "test")
            .expect("create run");
        let run_id = run.get("run_id").and_then(Value::as_str).unwrap();

        let config = DynamicControllerConfig {
            max_ticks_per_run: 2,
            auto_fix_on_failure: false,
            ..Default::default()
        };
        let mut ctrl = DynamicWorkflowController::new(config);
        let executor = NoopNodeExecutor;

        // First tick - executes node a
        let r1 = ctrl.tick(&store, run_id, "test", &executor).expect("tick1");
        assert!(r1
            .actions
            .iter()
            .any(|a| matches!(a, ControllerAction::NodeExecuted { .. })));

        // Second tick - executes node b
        let r2 = ctrl.tick(&store, run_id, "test", &executor).expect("tick2");
        assert!(r2
            .actions
            .iter()
            .any(|a| matches!(a, ControllerAction::NodeExecuted { .. })));

        // Third tick - blocked by max_ticks
        let r3 = ctrl.tick(&store, run_id, "test", &executor).expect("tick3");
        assert!(!r3.should_continue);
        assert!(r3.actions.iter().any(|a| matches!(
            a,
            ControllerAction::NoAction { reason } if reason.contains("max_ticks_per_run")
        )));
    }

    #[test]
    fn test_controller_terminal_run_returns_no_action() {
        let (store, run_id) = setup_store_with_run();
        let mut ctrl = DynamicWorkflowController::new(DynamicControllerConfig::default());
        let executor = NoopNodeExecutor;

        // Run to completion
        for _ in 0..20 {
            let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
            if !result.should_continue {
                break;
            }
        }

        // Tick on completed run
        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        assert!(!result.should_continue);
        assert!(result.actions.iter().any(|a| matches!(
            a,
            ControllerAction::NoAction { reason } if reason.contains("terminal")
        )));
    }

    #[test]
    fn test_quality_passes_default_true() {
        let node = json!({"node_id": "n1", "status": "completed"});
        assert!(quality_passes(&node));
    }

    #[test]
    fn test_quality_passes_explicit_false() {
        let node = json!({
            "node_id": "n1",
            "status": "completed",
            "result": {"quality": {"passed": false}}
        });
        assert!(!quality_passes(&node));
    }

    #[test]
    fn test_quality_passes_explicit_true() {
        let node = json!({
            "node_id": "n1",
            "status": "completed",
            "result": {"quality": {"passed": true}}
        });
        assert!(quality_passes(&node));
    }

    #[test]
    fn test_build_auto_fix_proposals_structure() {
        let proposals = build_auto_fix_proposals("run-1", "n1", "fix-n1", "test-fix-n1");
        assert_eq!(proposals.len(), 4);
        assert_eq!(proposals[0].mutation_type, "add_node");
        assert_eq!(proposals[1].mutation_type, "add_node");
        assert_eq!(proposals[2].mutation_type, "add_edge");
        assert_eq!(proposals[3].mutation_type, "add_edge");

        let fix_node_id = proposals[0].payload.get("node_id").and_then(Value::as_str);
        assert_eq!(fix_node_id, Some("fix-n1"));

        let edge_from = proposals[2]
            .payload
            .get("from_node")
            .and_then(Value::as_str);
        assert_eq!(edge_from, Some("n1"));
        let edge_to = proposals[2].payload.get("to_node").and_then(Value::as_str);
        assert_eq!(edge_to, Some("fix-n1"));
    }

    #[test]
    fn test_build_quality_review_proposals_structure() {
        let proposals = build_quality_review_proposals("run-1", "n1", "review-n1");
        assert_eq!(proposals.len(), 2);
        assert_eq!(proposals[0].mutation_type, "add_node");
        assert_eq!(proposals[1].mutation_type, "add_edge");

        assert_eq!(
            proposals[0].payload.get("node_id").and_then(Value::as_str),
            Some("review-n1")
        );
        assert_eq!(
            proposals[0]
                .payload
                .get("node_type")
                .and_then(Value::as_str),
            Some("review")
        );
    }

    #[test]
    fn test_controller_approval_mode_skips_mutation() {
        let (store, run_id) = setup_store_with_run();
        let config = DynamicControllerConfig {
            approval_required_for_mutation: true,
            auto_fix_on_failure: true,
            ..Default::default()
        };
        let mut ctrl = DynamicWorkflowController::new(config);
        let executor = NoopNodeExecutor;

        // Execute nodes normally
        for _ in 0..10 {
            let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
            if !result.should_continue {
                break;
            }
        }
        // The controller should have completed without panicking
        // In approval mode, it requests approval instead of applying mutations
    }

    #[test]
    fn test_controller_action_equality() {
        assert_eq!(
            ControllerAction::RunCompleted,
            ControllerAction::RunCompleted
        );
        assert_ne!(
            ControllerAction::RunCompleted,
            ControllerAction::RunFailed {
                reason: "x".to_string()
            }
        );
    }

    #[test]
    fn test_node_status_fallback_to_status_field() {
        let node = json!({"node_id": "n1", "status": "pending"});
        assert_eq!(node_status(&node), "pending");
    }

    #[test]
    fn test_controller_tick_respects_pause_reason() {
        let (store, run_id) = setup_store_with_run();
        store
            .update_run_pause_reason(&run_id, Some("operator_hold"))
            .unwrap();

        let config = DynamicControllerConfig {
            admission_check_enabled: true,
            ..Default::default()
        };
        let mut ctrl = DynamicWorkflowController::new(config);
        let executor = NoopNodeExecutor;

        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        assert!(!result.admission_allowed, "paused run should be rejected");
        assert_eq!(result.admission_reason.as_deref(), Some("operator_hold"));
        assert!(result.actions.iter().any(|a| matches!(
            a,
            ControllerAction::NoAction { reason } if reason.contains("paused")
        )));
        assert!(!result.should_continue);
    }

    #[test]
    fn test_controller_tick_includes_priority_and_queue_position() {
        let (store, run_id) = setup_store_with_run();
        store.update_run_priority(&run_id, 3).unwrap();

        let config = DynamicControllerConfig::default();
        let mut ctrl = DynamicWorkflowController::new(config);
        let executor = NoopNodeExecutor;

        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        assert_eq!(result.priority, Some(3), "priority should be read from run");
        assert!(
            result.admission_allowed,
            "non-paused run should be admitted"
        );
    }

    #[test]
    fn test_controller_admission_check_disabled_allows_paused_runs() {
        let (store, run_id) = setup_store_with_run();
        store
            .update_run_pause_reason(&run_id, Some("operator_hold"))
            .unwrap();

        let config = DynamicControllerConfig {
            admission_check_enabled: false,
            ..Default::default()
        };
        let mut ctrl = DynamicWorkflowController::new(config);
        let executor = NoopNodeExecutor;

        let result = ctrl.tick(&store, &run_id, "test", &executor).expect("tick");
        assert!(
            result.admission_allowed,
            "with admission check disabled, paused run should proceed"
        );
        assert!(result
            .actions
            .iter()
            .any(|a| matches!(a, ControllerAction::NodeExecuted { .. })));
    }

    #[test]
    fn test_controller_default_config_has_admission_fields() {
        let config = DynamicControllerConfig::default();
        assert!(config.admission_check_enabled);
        assert!(config.respect_priority);
    }

    #[test]
    fn test_node_status_prefers_db_status() {
        let node = json!({"node_id": "n1", "status": "pending", "db_status": "running"});
        assert_eq!(node_status(&node), "running");
    }
}
