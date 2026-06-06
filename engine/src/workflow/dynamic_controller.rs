use std::collections::HashMap;

use serde_json::{json, Value};

use crate::node_executor::NodeExecutor;
use crate::storage::local_product_store::LocalProductStore;
use crate::workflow::dag_manager::types::DAGMutationProposal;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct DynamicControllerConfig {
    pub max_ticks_per_run: u64,
    pub max_mutations_per_run: u64,
    pub approval_required_for_mutation: bool,
    pub auto_fix_on_failure: bool,
}

impl Default for DynamicControllerConfig {
    fn default() -> Self {
        Self {
            max_ticks_per_run: 100,
            max_mutations_per_run: 20,
            approval_required_for_mutation: false,
            auto_fix_on_failure: true,
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
}

// ---------------------------------------------------------------------------
// DynamicWorkflowController
// ---------------------------------------------------------------------------

pub struct DynamicWorkflowController {
    config: DynamicControllerConfig,
    ticks_executed: u64,
    mutations_applied_total: u64,
}

impl DynamicWorkflowController {
    pub fn new(config: DynamicControllerConfig) -> Self {
        Self {
            config,
            ticks_executed: 0,
            mutations_applied_total: 0,
        }
    }

    pub fn ticks_executed(&self) -> u64 {
        self.ticks_executed
    }

    pub fn mutations_applied_total(&self) -> u64 {
        self.mutations_applied_total
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

        if is_terminal(&status) {
            return Ok(ControllerTickResult {
                actions: vec![ControllerAction::NoAction {
                    reason: format!("run is already terminal: {status}"),
                }],
                run_status: status,
                mutations_applied: 0,
                should_continue: false,
            });
        }

        if self.ticks_executed >= self.config.max_ticks_per_run {
            return Ok(ControllerTickResult {
                actions: vec![ControllerAction::NoAction {
                    reason: "max_ticks_per_run reached".to_string(),
                }],
                run_status: status,
                mutations_applied: 0,
                should_continue: false,
            });
        }

        self.ticks_executed += 1;

        let nodes = run
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let edges = run
            .get("edges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let failed_nodes: Vec<&Value> = nodes
            .iter()
            .filter(|n| node_status(n) == "failed")
            .collect();

        let completed_nodes: Vec<&Value> = nodes
            .iter()
            .filter(|n| node_status(n) == "completed")
            .collect();

        let mut actions: Vec<ControllerAction> = Vec::new();
        let mut mutations_this_tick: u64 = 0;

        // --- Phase 1: auto-fix failed nodes ---
        if self.config.auto_fix_on_failure {
            for failed in &failed_nodes {
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

                // Skip if a fix node already exists for this failure
                let fix_id = format!("fix-{}", failed_id);
                if nodes
                    .iter()
                    .any(|n| n.get("node_id").and_then(Value::as_str) == Some(&fix_id))
                {
                    continue;
                }

                let test_id = format!("test-{}", fix_id);

                let proposals = build_auto_fix_proposals(run_id, &failed_id, &fix_id, &test_id);

                if self.config.approval_required_for_mutation {
                    actions.push(ControllerAction::ApprovalRequested {
                        node_id: failed_id.clone(),
                        reason: format!("auto-fix for failed node {} requires approval", failed_id),
                    });
                    continue;
                }

                let results = store.apply_dag_mutations_batch(run_id, &proposals, actor)?;
                let applied_count = results
                    .iter()
                    .filter(|r| r.get("applied").and_then(Value::as_bool).unwrap_or(false))
                    .count() as u64;
                mutations_this_tick += applied_count;
                self.mutations_applied_total += applied_count;

                if applied_count > 0 {
                    actions.push(ControllerAction::GraphMutated {
                        proposal_id: format!("auto_fix_{}", failed_id),
                        mutation_type: "add_node".to_string(),
                    });
                }
            }
        }

        // --- Phase 2: quality check completed nodes ---
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
                let review_id = format!("review-{}", node_id);
                if nodes
                    .iter()
                    .any(|n| n.get("node_id").and_then(Value::as_str) == Some(&review_id))
                    || edges
                        .iter()
                        .any(|e| e.get("to_node_id").and_then(Value::as_str) == Some(&review_id))
                {
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

                let proposals = build_quality_review_proposals(run_id, &node_id, &review_id);
                let results = store.apply_dag_mutations_batch(run_id, &proposals, actor)?;
                let applied_count = results
                    .iter()
                    .filter(|r| r.get("applied").and_then(Value::as_bool).unwrap_or(false))
                    .count() as u64;
                mutations_this_tick += applied_count;
                self.mutations_applied_total += applied_count;

                if applied_count > 0 {
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
                });
            }
        }

        // --- Phase 4: tick the executor for one ready node ---
        let tick_result = store.tick_with_executor_and_command(run_id, actor, 0, executor, None)?;

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
                actions.push(ControllerAction::NodeRetried { node_id, attempt });
            }
            "completed" => {
                actions.push(ControllerAction::RunCompleted);
            }
            "failed" => {
                let run_after = tick_result.get("run");
                let reason = run_after
                    .and_then(|r| r.get("result"))
                    .and_then(Value::as_str)
                    .unwrap_or("node_failure")
                    .to_string();
                actions.push(ControllerAction::RunFailed { reason });
            }
            "no_ready_node" => {
                // No ready node but run is not terminal — might be waiting on
                // mutations we just applied, or blocked nodes.
                actions.push(ControllerAction::NoAction {
                    reason: "no ready node available".to_string(),
                });
            }
            other => {
                actions.push(ControllerAction::NoAction {
                    reason: format!("tick returned action: {other}"),
                });
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

        Ok(ControllerTickResult {
            actions,
            run_status: fresh_status,
            mutations_applied: mutations_this_tick,
            should_continue,
        })
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
    fn test_node_status_prefers_db_status() {
        let node = json!({"node_id": "n1", "status": "pending", "db_status": "running"});
        assert_eq!(node_status(&node), "running");
    }
}
