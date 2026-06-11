use rusqlite::{params, Row};
use serde_json::{json, Value};

use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};
use crate::workflow::context_pack::{
    assemble_context_injection, ContextAssemblyConfig, ContextSource,
};
use crate::workflow::dag_manager::{types::DAGMutationProposal, DAGManager};

pub const WORKFLOW_RUN_SCHEMA_VERSION: &str = "workflow_run.v1";

enum LeaseResult {
    Terminal {
        action: String,
        run: Value,
    },
    NoReadyNode {
        run: Value,
    },
    Leased {
        node_id: String,
        task_type: String,
        workflow_id: String,
        attempt: i64,
        node_metadata: Value,
    },
}

impl LocalProductStore {
    pub fn create_workflow_run_from_plan(
        &self,
        plan_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        let plan = self
            .get_workflow_plan(plan_id)?
            .ok_or_else(|| format!("plan not found: {plan_id}"))?;
        let graph = required_object(&plan, "graph")?;
        let workflow_id = plan
            .get("workflow_id")
            .and_then(Value::as_str)
            .or_else(|| graph.get("workflow_id").and_then(Value::as_str))
            .ok_or_else(|| format!("plan {plan_id} missing workflow_id"))?;
        let dispatch_id = plan
            .get("dispatch_id")
            .and_then(Value::as_str)
            .or_else(|| graph.get("dispatch_id").and_then(Value::as_str))
            .unwrap_or("");
        let nodes = graph
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("plan {plan_id} graph missing nodes"))?;
        let edges = graph
            .get("edges")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("plan {plan_id} graph missing edges"))?;

        let run_id = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_sequence(conn, "workflow_runs", "run_sequence")?;
                let run_id = format!("run-{sequence:04}");
                let created_at = self.now();
                let boundaries = workflow_run_boundaries();
                let run = json!({
                    "schema_version": WORKFLOW_RUN_SCHEMA_VERSION,
                    "run_sequence": sequence,
                    "run_id": run_id,
                    "plan_id": plan_id,
                    "workflow_id": workflow_id,
                    "dispatch_id": dispatch_id,
                    "status": "created",
                    "created_at": created_at,
                    "updated_at": created_at,
                    "started_at": null,
                    "completed_at": null,
                    "result": null,
                    "graph": graph,
                    "boundaries": boundaries,
                });
                conn.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status, workflow_id,
                      dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json,
                      priority, deadline_at, sla_ms, tenant_id, queue_position, pause_reason, degrade_mode)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, NULL, ?9, ?10,
                             5, NULL, NULL, NULL, NULL, NULL, NULL)",
                    params![
                        sequence,
                        run_id,
                        plan_id,
                        created_at,
                        created_at,
                        "created",
                        workflow_id,
                        null_if_empty(dispatch_id),
                        boundaries.to_string(),
                        run.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;

                for node in nodes {
                    insert_workflow_run_node_locked(conn, &run_id, node)?;
                }
                for edge in edges {
                    insert_workflow_run_edge_locked(conn, &run_id, edge)?;
                }
                insert_workflow_run_event_locked(
                    conn,
                    &run_id,
                    None,
                    "workflow_run.created",
                    actor,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "metadata_only": true,
                    }),
                    &created_at,
                )?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "workflow_run.create",
                    &run_id,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "metadata_only": true,
                    }),
                )?;
                Ok(run_id)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let sequence: i64 = pg_next_sequence(&mut tx, "workflow_runs", "run_sequence")?;
                let run_id = format!("run-{sequence:04}");
                let created_at = self.now();
                let boundaries = workflow_run_boundaries();
                let run = json!({
                    "schema_version": WORKFLOW_RUN_SCHEMA_VERSION,
                    "run_sequence": sequence,
                    "run_id": run_id,
                    "plan_id": plan_id,
                    "workflow_id": workflow_id,
                    "dispatch_id": dispatch_id,
                    "status": "created",
                    "created_at": created_at,
                    "updated_at": created_at,
                    "started_at": null,
                    "completed_at": null,
                    "result": null,
                    "graph": graph,
                    "boundaries": boundaries,
                });
                let boundaries_json = boundaries.to_string();
                let run_json = run.to_string();
                let dispatch_opt = null_if_empty(dispatch_id);
                let status_str = "created";
                let plan_ref: &str = plan_id;
                let wf_ref: &str = workflow_id;
                let run_ref: &str = &run_id;
                let cat_ref: &str = &created_at;
                let params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![
                    &sequence,
                    &run_id as &(dyn postgres::types::ToSql + Sync),
                    &plan_ref,
                    &cat_ref,
                    &cat_ref,
                    &status_str,
                    &wf_ref,
                    &dispatch_opt,
                    &boundaries_json,
                    &run_json,
                ];
                tx.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status, workflow_id,
                      dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json,
                      priority, deadline_at, sla_ms, tenant_id, queue_position, pause_reason, degrade_mode)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, NULL, NULL, $9, $10,
                             5, NULL, NULL, NULL, NULL, NULL, NULL)",
                    &params,
                )
                .map_err(|e| e.to_string())?;

                for node in nodes {
                    pg_insert_workflow_run_node(&mut tx, &run_id, node)?;
                }
                for edge in edges {
                    pg_insert_workflow_run_edge(&mut tx, &run_id, edge)?;
                }
                pg_insert_workflow_run_event(
                    &mut tx,
                    &run_id,
                    None,
                    "workflow_run.created",
                    actor,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "metadata_only": true,
                    }),
                    &created_at,
                )?;
                pg_append_audit(
                    &mut tx,
                    &created_at,
                    actor,
                    "workflow_run.create",
                    &run_id,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "metadata_only": true,
                    }),
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(run_id)
            }),
        }?;

        self.get_workflow_run(&run_id)?
            .ok_or_else(|| format!("workflow run not found after create: {run_id}"))
    }

    pub fn search_workflow_runs(
        &self,
        limit: i64,
        offset: i64,
        search: Option<&str>,
    ) -> Result<Vec<Value>, String> {
        let Some(search) = search.map(str::trim).filter(|value| !value.is_empty()) else {
            return self.list_workflow_runs_with_offset(limit, offset);
        };
        let pattern = format!("%{}%", escape_like(&search.to_lowercase()));
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                                workflow_id, dispatch_id, started_at, completed_at, result_json,
                                last_heartbeat_at, boundaries_json, run_json,
                                priority, deadline_at, sla_ms, tenant_id, queue_position,
                                pause_reason, degrade_mode
                         FROM workflow_runs
                         WHERE lower(run_id) LIKE ?1 ESCAPE '\\'
                            OR lower(COALESCE(plan_id, '')) LIKE ?1 ESCAPE '\\'
                            OR lower(status) LIKE ?1 ESCAPE '\\'
                            OR lower(workflow_id) LIKE ?1 ESCAPE '\\'
                            OR lower(COALESCE(dispatch_id, '')) LIKE ?1 ESCAPE '\\'
                         ORDER BY run_sequence DESC
                         LIMIT ?2 OFFSET ?3",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![pattern, limit, offset], workflow_run_summary_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                                workflow_id, dispatch_id, started_at, completed_at, result_json,
                                last_heartbeat_at, boundaries_json, run_json,
                                priority, deadline_at, sla_ms, tenant_id, queue_position,
                                pause_reason, degrade_mode
                         FROM workflow_runs
                         WHERE lower(run_id) LIKE $1 ESCAPE '\\'
                            OR lower(COALESCE(plan_id, '')) LIKE $1 ESCAPE '\\'
                            OR lower(status) LIKE $1 ESCAPE '\\'
                            OR lower(workflow_id) LIKE $1 ESCAPE '\\'
                            OR lower(COALESCE(dispatch_id, '')) LIKE $1 ESCAPE '\\'
                         ORDER BY run_sequence DESC
                         LIMIT $2 OFFSET $3",
                        &[&pattern, &limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_collect_workflow_run_summaries(rows)
            }),
        }
    }

    pub fn list_workflow_runs_with_offset(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                                workflow_id, dispatch_id, started_at, completed_at, result_json,
                                last_heartbeat_at, boundaries_json, run_json,
                                priority, deadline_at, sla_ms, tenant_id, queue_position,
                                pause_reason, degrade_mode
                         FROM workflow_runs
                         ORDER BY run_sequence DESC
                         LIMIT ?1 OFFSET ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], workflow_run_summary_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                                workflow_id, dispatch_id, started_at, completed_at, result_json,
                                last_heartbeat_at, boundaries_json, run_json,
                                priority, deadline_at, sla_ms, tenant_id, queue_position,
                                pause_reason, degrade_mode
                         FROM workflow_runs
                         ORDER BY run_sequence DESC
                         LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                    .map_err(|e| e.to_string())?;
                pg_collect_workflow_run_summaries(rows)
            }),
        }
    }

    pub fn export_workflow_runs(&self, limit: i64) -> Result<Vec<Value>, String> {
        let summaries = self.list_workflow_runs_with_offset(limit, 0)?;
        let mut runs = Vec::new();
        for summary in summaries {
            if let Some(run_id) = summary.get("run_id").and_then(Value::as_str) {
                if let Some(run) = self.get_workflow_run(run_id)? {
                    runs.push(run);
                }
            }
        }
        Ok(runs)
    }

    pub fn get_workflow_run(&self, run_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                                workflow_id, dispatch_id, started_at, completed_at, result_json,
                                last_heartbeat_at, boundaries_json, run_json,
                                priority, deadline_at, sla_ms, tenant_id, queue_position,
                                pause_reason, degrade_mode
                         FROM workflow_runs
                         WHERE run_id = ?1
                         LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![run_id], workflow_run_summary_row)
                    .map_err(|e| e.to_string())?;
                let Some(row) = rows.next() else {
                    return Ok(None);
                };
                let run = row.map_err(|e| e.to_string())?;
                Ok(Some(workflow_run_with_children(conn, run)?))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                                workflow_id, dispatch_id, started_at, completed_at, result_json,
                                last_heartbeat_at, boundaries_json, run_json,
                                priority, deadline_at, sla_ms, tenant_id, queue_position,
                                pause_reason, degrade_mode
                         FROM workflow_runs
                         WHERE run_id = $1
                         LIMIT 1",
                        &[&run_id],
                    )
                    .map_err(|e| e.to_string())?;
                let Some(row) = rows.into_iter().next() else {
                    return Ok(None);
                };
                let run = pg_workflow_run_summary_row(&row);
                Ok(Some(pg_workflow_run_with_children(client, run)?))
            }),
        }
    }

    pub fn append_workflow_run_event(
        &self,
        run_id: &str,
        node_id: Option<&str>,
        event_type: &str,
        details: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let created_at = self.now();
                let event = insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    node_id,
                    event_type,
                    actor,
                    details,
                    &created_at,
                )?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "workflow_run.event",
                    run_id,
                    &json!({"event_type": event_type, "node_id": node_id, "metadata_only": true}),
                )?;
                Ok(event)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let created_at = self.now();
                let event = pg_insert_workflow_run_event(
                    client,
                    run_id,
                    node_id,
                    event_type,
                    actor,
                    details,
                    &created_at,
                )?;
                pg_append_audit(
                    client,
                    &created_at,
                    actor,
                    "workflow_run.event",
                    run_id,
                    &json!({"event_type": event_type, "node_id": node_id, "metadata_only": true}),
                )?;
                Ok(event)
            }),
        }
    }

    pub fn workflow_run_events(&self, run_id: &str, limit: i64) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                workflow_run_events_locked(conn, run_id, limit)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                pg_workflow_run_events(client, run_id, limit)
            }),
        }
    }

    pub fn record_workflow_run_approval(
        &self,
        run_id: &str,
        node_id: &str,
        decision: &str,
        actor: &str,
        reason: Option<&str>,
        bound_patch_hash: Option<&str>,
        bound_source_revision: Option<&str>,
        bound_changed_files: Option<&[String]>,
        expires_at: Option<&str>,
    ) -> Result<Value, String> {
        if !matches!(decision, "requested" | "approved" | "rejected") {
            return Err(format!("invalid workflow approval decision: {decision}"));
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let sequence = next_sequence(conn, "workflow_run_approvals", "approval_sequence")?;
                let approval_id = format!("workflow-approval-{sequence:04}");
                let created_at = self.now();
                let mut approval = json!({
                    "approval_sequence": sequence,
                    "approval_id": approval_id,
                    "run_id": run_id,
                    "node_id": node_id,
                    "decision": decision,
                    "actor": actor,
                    "reason": reason,
                    "created_at": created_at,
                    "metadata_only": true,
                    "execution_authority": "disabled",
                });
                if let Some(obj) = approval.as_object_mut() {
                    if let Some(hash) = bound_patch_hash {
                        obj.insert("bound_patch_hash".to_string(), json!(hash));
                    }
                    if let Some(source) = bound_source_revision {
                        obj.insert("bound_source_revision".to_string(), json!(source));
                    }
                    if let Some(files) = bound_changed_files {
                        obj.insert("bound_changed_files".to_string(), json!(files));
                    }
                    if let Some(exp) = expires_at {
                        obj.insert("expires_at".to_string(), json!(exp));
                    }
                }
                conn.execute(
                    "INSERT INTO workflow_run_approvals
                     (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
                      created_at, approval_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        sequence,
                        approval_id,
                        run_id,
                        node_id,
                        decision,
                        actor,
                        reason,
                        created_at,
                        approval.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "workflow_run.approval_record",
                    run_id,
                    &json!({
                        "node_id": node_id,
                        "decision": decision,
                        "metadata_only": true,
                        "execution_authority": "disabled",
                    }),
                )?;
                Ok(approval)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let sequence =
                    pg_next_sequence(client, "workflow_run_approvals", "approval_sequence")?;
                let approval_id = format!("workflow-approval-{sequence:04}");
                let created_at = self.now();
                let mut approval = json!({
                    "approval_sequence": sequence,
                    "approval_id": approval_id,
                    "run_id": run_id,
                    "node_id": node_id,
                    "decision": decision,
                    "actor": actor,
                    "reason": reason,
                    "created_at": created_at,
                    "metadata_only": true,
                    "execution_authority": "disabled",
                });
                if let Some(obj) = approval.as_object_mut() {
                    if let Some(hash) = bound_patch_hash {
                        obj.insert("bound_patch_hash".to_string(), json!(hash));
                    }
                    if let Some(source) = bound_source_revision {
                        obj.insert("bound_source_revision".to_string(), json!(source));
                    }
                    if let Some(files) = bound_changed_files {
                        obj.insert("bound_changed_files".to_string(), json!(files));
                    }
                    if let Some(exp) = expires_at {
                        obj.insert("expires_at".to_string(), json!(exp));
                    }
                }
                client
                    .execute(
                        "INSERT INTO workflow_run_approvals
                         (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
                          created_at, approval_json)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                        &[
                            &sequence,
                            &approval_id,
                            &run_id,
                            &node_id,
                            &decision,
                            &actor,
                            &reason,
                            &created_at,
                            &approval.to_string(),
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                pg_append_audit(
                    client,
                    &created_at,
                    actor,
                    "workflow_run.approval_record",
                    run_id,
                    &json!({
                        "node_id": node_id,
                        "decision": decision,
                        "metadata_only": true,
                        "execution_authority": "disabled",
                    }),
                )?;
                Ok(approval)
            }),
        }
    }

    pub fn workflow_run_approvals(&self, run_id: &str, limit: i64) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                workflow_run_approvals_locked(conn, run_id, limit)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                pg_workflow_run_approvals(client, run_id, limit)
            }),
        }
    }

    pub fn request_workflow_run_resume(
        &self,
        run_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Value, String> {
        self.update_workflow_run_status_with_event(
            run_id,
            "running",
            "workflow_resume_requested",
            actor,
            reason,
        )
    }

    pub fn request_workflow_run_cancel(
        &self,
        run_id: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Value, String> {
        self.update_workflow_run_status_with_event(
            run_id,
            "cancelled",
            "workflow_cancel_requested",
            actor,
            reason,
        )
    }

    pub fn tick_workflow_run(&self, run_id: &str, actor: &str) -> Result<Value, String> {
        use crate::node_executor::NoopNodeExecutor;
        self.tick_with_executor(run_id, actor, 0, &NoopNodeExecutor)
    }

    pub fn tick_with_retry(
        &self,
        run_id: &str,
        actor: &str,
        max_retries: i64,
    ) -> Result<Value, String> {
        use crate::node_executor::NoopNodeExecutor;
        self.tick_with_executor(run_id, actor, max_retries, &NoopNodeExecutor)
    }

    pub fn tick_with_executor(
        &self,
        run_id: &str,
        actor: &str,
        max_retries: i64,
        executor: &dyn crate::node_executor::NodeExecutor,
    ) -> Result<Value, String> {
        self.tick_with_executor_and_command(run_id, actor, max_retries, executor, None)
    }

    pub fn tick_with_executor_and_command(
        &self,
        run_id: &str,
        actor: &str,
        max_retries: i64,
        executor: &dyn crate::node_executor::NodeExecutor,
        command_override: Option<&str>,
    ) -> Result<Value, String> {
        // Phase 1: Lease a ready node (inside lock)
        let leased = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;

                let run_status: String = conn
                    .query_row(
                        "SELECT status FROM workflow_runs WHERE run_id = ?1",
                        params![run_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;

                if is_run_terminal(&run_status) {
                    return Err(format!("workflow run {run_id} is terminal: {run_status}"));
                }

                let now = self.now();
                if run_status == "created" {
                    update_workflow_run_status_locked(conn, run_id, "running", &now)?;
                    insert_workflow_run_event_locked(
                        conn,
                        run_id,
                        None,
                        "workflow_run.tick_started",
                        actor,
                        &json!({"trigger": "tick", "max_retries": max_retries}),
                        &now,
                    )?;
                }

                let Some(node_id) = find_ready_node_locked(conn, run_id)? else {
                    let (all_done, has_failure) = check_run_completion_locked(conn, run_id)?;
                    if all_done {
                        let terminal_status = if has_failure { "failed" } else { "completed" };
                        let now = self.now();
                        update_workflow_run_status_locked(conn, run_id, terminal_status, &now)?;
                        insert_workflow_run_event_locked(
                            conn,
                            run_id,
                            None,
                            &format!("workflow_run.{terminal_status}"),
                            actor,
                            &json!({"reason": if has_failure { "node_failure" } else { "all_nodes_completed" }}),
                            &now,
                        )?;
                        append_audit_locked(
                            conn,
                            &now,
                            actor,
                            &format!("workflow_run.{terminal_status}"),
                            run_id,
                            &json!({"metadata_only": true}),
                        )?;
                        let run = get_run_row(conn, run_id)?;
                        return Ok(LeaseResult::Terminal { action: terminal_status.to_string(), run });
                    }
                    let run = get_run_row(conn, run_id)?;
                    return Ok(LeaseResult::NoReadyNode { run });
                };

                let now = self.now();
                conn.execute(
                    "UPDATE workflow_run_nodes SET status = 'running', started_at = ?1, leased_at = ?1, attempt_count = attempt_count + 1
                     WHERE run_id = ?2 AND node_id = ?3 AND status = 'pending'",
                    params![now, run_id, node_id],
                ).map_err(|e| e.to_string())?;

                let attempt: i64 = conn
                    .query_row(
                        "SELECT attempt_count FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(1);

                let task_type: String = conn
                    .query_row(
                        "SELECT task_type FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .unwrap_or_else(|_| "unknown".to_string());

                let workflow_id: String = conn
                    .query_row(
                        "SELECT workflow_id FROM workflow_runs WHERE run_id = ?1",
                        params![run_id],
                        |row| row.get(0),
                    )
                    .unwrap_or_default();

                let node_metadata = conn
                    .query_row(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                        params![run_id, node_id],
                        |row| {
                            let text: String = row.get(0)?;
                            Ok::<Value, rusqlite::Error>(serde_json::from_str(&text).unwrap_or(Value::Null))
                        },
                    )
                    .unwrap_or(Value::Null);

                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    Some(&node_id),
                    "node.leased",
                    actor,
                    &json!({"node_id": node_id, "status": "running", "attempt": attempt}),
                    &now,
                )?;

                Ok(LeaseResult::Leased {
                    node_id,
                    task_type,
                    workflow_id,
                    attempt,
                    node_metadata,
                })
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;

                pg_ensure_run_exists(&mut tx, run_id)?;

                let run_status: String = tx
                    .query_one(
                        "SELECT status FROM workflow_runs WHERE run_id = $1",
                        &[&run_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);

                if is_run_terminal(&run_status) {
                    return Err(format!("workflow run {run_id} is terminal: {run_status}"));
                }

                let now = self.now();
                if run_status == "created" {
                    pg_update_workflow_run_status(&mut tx, run_id, "running", &now)?;
                    pg_insert_workflow_run_event(
                        &mut tx,
                        run_id,
                        None,
                        "workflow_run.tick_started",
                        actor,
                        &json!({"trigger": "tick", "max_retries": max_retries}),
                        &now,
                    )?;
                }

                let Some(node_id) = pg_find_ready_node(&mut tx, run_id)? else {
                    let (all_done, has_failure) = pg_check_run_completion(&mut tx, run_id)?;
                    if all_done {
                        let terminal_status = if has_failure { "failed" } else { "completed" };
                        let now = self.now();
                        pg_update_workflow_run_status(&mut tx, run_id, terminal_status, &now)?;
                        pg_insert_workflow_run_event(
                            &mut tx,
                            run_id,
                            None,
                            &format!("workflow_run.{terminal_status}"),
                            actor,
                            &json!({"reason": if has_failure { "node_failure" } else { "all_nodes_completed" }}),
                            &now,
                        )?;
                        pg_append_audit(
                            &mut tx,
                            &now,
                            actor,
                            &format!("workflow_run.{terminal_status}"),
                            run_id,
                            &json!({"metadata_only": true}),
                        )?;
                        let run = pg_get_run_row(&mut tx, run_id)?;
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(LeaseResult::Terminal { action: terminal_status.to_string(), run });
                    }
                    let run = pg_get_run_row(&mut tx, run_id)?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(LeaseResult::NoReadyNode { run });
                };

                let now = self.now();
                tx.execute(
                    "UPDATE workflow_run_nodes SET status = 'running', started_at = $1, leased_at = $1, attempt_count = attempt_count + 1
                     WHERE run_id = $2 AND node_id = $3 AND status = 'pending'",
                    &[&now, &run_id, &node_id],
                ).map_err(|e| e.to_string())?;

                let attempt: i64 = tx
                    .query_one(
                        "SELECT attempt_count FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map(|r| r.get(0))
                    .unwrap_or(1);

                let task_type: String = tx
                    .query_one(
                        "SELECT task_type FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map(|r| r.get(0))
                    .unwrap_or_else(|_| "unknown".to_string());

                let workflow_id: String = tx
                    .query_one(
                        "SELECT workflow_id FROM workflow_runs WHERE run_id = $1",
                        &[&run_id],
                    )
                    .map(|r| r.get(0))
                    .unwrap_or_default();

                let node_metadata = tx
                    .query_one(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map(|r| {
                        let text: String = r.get(0);
                        serde_json::from_str(&text).unwrap_or(Value::Null)
                    })
                    .unwrap_or(Value::Null);

                pg_insert_workflow_run_event(
                    &mut tx,
                    run_id,
                    Some(&node_id),
                    "node.leased",
                    actor,
                    &json!({"node_id": node_id, "status": "running", "attempt": attempt}),
                    &now,
                )?;

                tx.commit().map_err(|e| e.to_string())?;
                Ok(LeaseResult::Leased {
                    node_id,
                    task_type,
                    workflow_id,
                    attempt,
                    node_metadata,
                })
            }),
        }?;

        // Phase 2: Execute (outside SQLite lock)
        match leased {
            LeaseResult::Terminal { action, run } => Ok(json!({ "action": action, "run": run })),
            LeaseResult::NoReadyNode { run } => {
                Ok(json!({ "action": "no_ready_node", "run": run }))
            }
            LeaseResult::Leased {
                node_id,
                task_type,
                workflow_id,
                attempt,
                mut node_metadata,
            } => {
                // Inject workspace_path from supervised_patch_workspaces if available
                if let Ok(Some(workspace)) = self.get_supervised_patch_workspace_for_run(run_id) {
                    if let Some(ws_path) = workspace.get("workspace_path").and_then(|v| v.as_str())
                    {
                        if let Some(obj) = node_metadata.as_object_mut() {
                            obj.insert("workspace_path".to_string(), json!(ws_path));
                        }
                    }
                }
                if let Some(context_injection) =
                    self.context_injection_for_node(run_id, &node_id)?
                {
                    if let Some(obj) = node_metadata.as_object_mut() {
                        obj.insert("context_injection".to_string(), context_injection);
                    } else {
                        node_metadata = json!({"context_injection": context_injection});
                    }
                }
                // Inject command override if provided
                if let Some(cmd) = command_override {
                    if let Some(obj) = node_metadata.as_object_mut() {
                        obj.insert("command".to_string(), json!(cmd));
                    }
                }
                let input = crate::node_executor::NodeExecutionInput {
                    node_id: node_id.clone(),
                    task_type,
                    run_id: run_id.to_string(),
                    workflow_id,
                    node_metadata,
                };
                let output = executor.execute_node(&input);

                // Phase 3: Record result (inside lock)
                let tick_result = match &self.db {
                    DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                        let now = self.now();
                        let final_status = &output.status;
                        let should_retry = final_status == "failed"
                            && attempt <= max_retries;

                        let actual_status = if should_retry { "pending" } else { final_status };

                        conn.execute(
                            "UPDATE workflow_run_nodes SET status = ?1, completed_at = ?2 WHERE run_id = ?3 AND node_id = ?4",
                            params![actual_status, now, run_id, node_id],
                        ).map_err(|e| e.to_string())?;

                        let node_json_text: String = conn
                            .query_row(
                                "SELECT node_json FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                                params![run_id, node_id],
                                |row| row.get(0),
                            )
                            .map_err(|e| e.to_string())?;
                        let mut node_json: Value =
                            serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                        let result_json = output.to_value();
                        if let Some(obj) = node_json.as_object_mut() {
                            obj.insert("status".to_string(), json!(actual_status));
                            obj.insert("result".to_string(), result_json.clone());
                            if actual_status == "completed" {
                                obj.insert("completed_at".to_string(), json!(now));
                            }
                        }
                        conn.execute(
                            "UPDATE workflow_run_nodes SET node_json = ?1 WHERE run_id = ?2 AND node_id = ?3",
                            params![node_json.to_string(), run_id, node_id],
                        )
                        .map_err(|e| e.to_string())?;

                        if should_retry {
                            conn.execute(
                                "UPDATE workflow_run_nodes SET blocked_reason = ?1 WHERE run_id = ?2 AND node_id = ?3",
                                params![format!("retry after attempt {attempt}: {}", output.error_message.as_deref().unwrap_or("")), run_id, node_id],
                            ).map_err(|e| e.to_string())?;
                            insert_workflow_run_event_locked(
                                conn,
                                run_id,
                                Some(&node_id),
                                "node.retry_scheduled",
                                actor,
                                &json!({"node_id": node_id, "attempt": attempt, "error_domain": output.error_domain, "error_message": output.error_message}),
                                &now,
                            )?;
                            append_audit_locked(
                                conn,
                                &now,
                                actor,
                                "workflow_run.node_tick",
                                run_id,
                                &json!({
                                    "node_id": node_id,
                                    "executor_type": output.executor_type,
                                    "status": "retry_scheduled",
                                    "attempt": attempt,
                                    "latency_ms": output.latency_ms,
                                }),
                            )?;
                        } else {
                            let event_type = if final_status == "completed" { "node.completed" } else { "node.failed" };
                            insert_workflow_run_event_locked(
                                conn,
                                run_id,
                                Some(&node_id),
                                event_type,
                                actor,
                                &json!({"node_id": node_id, "executor_type": output.executor_type, "attempt": attempt, "result": result_json}),
                                &now,
                            )?;
                            append_audit_locked(
                                conn,
                                &now,
                                actor,
                                "workflow_run.node_tick",
                                run_id,
                                &json!({
                                    "node_id": node_id,
                                    "executor_type": output.executor_type,
                                    "status": final_status,
                                    "attempt": attempt,
                                    "latency_ms": output.latency_ms,
                                    "error_domain": output.error_domain,
                                }),
                            )?;
                        }

                        let (all_done, has_failure) = check_run_completion_locked(conn, run_id)?;
                        if all_done {
                            let terminal_status = if has_failure { "failed" } else { "completed" };
                            let now = self.now();
                            update_workflow_run_status_locked(conn, run_id, terminal_status, &now)?;
                            insert_workflow_run_event_locked(
                                conn,
                                run_id,
                                None,
                                &format!("workflow_run.{terminal_status}"),
                                actor,
                                &json!({"reason": if has_failure { "node_failure" } else { "all_nodes_completed" }}),
                                &now,
                            )?;
                            append_audit_locked(
                                conn,
                                &now,
                                actor,
                                &format!("workflow_run.{terminal_status}"),
                                run_id,
                                &json!({"metadata_only": true}),
                            )?;
                        }

                        let run = get_run_row(conn, run_id)?;
                        Ok(json!({
                            "action": if should_retry { "node_retry" } else { "node_executed" },
                            "node_id": node_id,
                            "executor_type": output.executor_type,
                            "attempt": attempt,
                            "result": output.to_value(),
                            "run": run,
                        }))
                    }),
                    #[cfg(feature = "pg")]
                    DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                        let mut tx = client.transaction().map_err(|e| e.to_string())?;

                        let now = self.now();
                        let final_status = &output.status;
                        let should_retry = final_status == "failed"
                            && attempt <= max_retries;

                        let actual_status = if should_retry { "pending" } else { final_status };

                        tx.execute(
                            "UPDATE workflow_run_nodes SET status = $1, completed_at = $2 WHERE run_id = $3 AND node_id = $4",
                            &[&actual_status, &now, &run_id, &node_id],
                        ).map_err(|e| e.to_string())?;

                        let node_json_text: String = tx
                            .query_one(
                                "SELECT node_json FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                                &[&run_id, &node_id],
                            )
                            .map_err(|e| e.to_string())?
                            .get(0);
                        let mut node_json: Value =
                            serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                        let result_json = output.to_value();
                        if let Some(obj) = node_json.as_object_mut() {
                            obj.insert("status".to_string(), json!(actual_status));
                            obj.insert("result".to_string(), result_json.clone());
                            if actual_status == "completed" {
                                obj.insert("completed_at".to_string(), json!(now));
                            }
                        }
                        tx.execute(
                            "UPDATE workflow_run_nodes SET node_json = $1 WHERE run_id = $2 AND node_id = $3",
                            &[&node_json.to_string(), &run_id, &node_id],
                        )
                        .map_err(|e| e.to_string())?;

                        if should_retry {
                            tx.execute(
                                "UPDATE workflow_run_nodes SET blocked_reason = $1 WHERE run_id = $2 AND node_id = $3",
                                &[&format!("retry after attempt {attempt}: {}", output.error_message.as_deref().unwrap_or("")), &run_id, &node_id],
                            ).map_err(|e| e.to_string())?;
                            pg_insert_workflow_run_event(
                                &mut tx,
                                run_id,
                                Some(&node_id),
                                "node.retry_scheduled",
                                actor,
                                &json!({"node_id": node_id, "attempt": attempt, "error_domain": output.error_domain, "error_message": output.error_message}),
                                &now,
                            )?;
                            pg_append_audit(
                                &mut tx,
                                &now,
                                actor,
                                "workflow_run.node_tick",
                                run_id,
                                &json!({
                                    "node_id": node_id,
                                    "executor_type": output.executor_type,
                                    "status": "retry_scheduled",
                                    "attempt": attempt,
                                    "latency_ms": output.latency_ms,
                                }),
                            )?;
                        } else {
                            let event_type = if final_status == "completed" { "node.completed" } else { "node.failed" };
                            pg_insert_workflow_run_event(
                                &mut tx,
                                run_id,
                                Some(&node_id),
                                event_type,
                                actor,
                                &json!({"node_id": node_id, "executor_type": output.executor_type, "attempt": attempt, "result": result_json}),
                                &now,
                            )?;
                            pg_append_audit(
                                &mut tx,
                                &now,
                                actor,
                                "workflow_run.node_tick",
                                run_id,
                                &json!({
                                    "node_id": node_id,
                                    "executor_type": output.executor_type,
                                    "status": final_status,
                                    "attempt": attempt,
                                    "latency_ms": output.latency_ms,
                                    "error_domain": output.error_domain,
                                }),
                            )?;
                        }

                        let (all_done, has_failure) = pg_check_run_completion(&mut tx, run_id)?;
                        if all_done {
                            let terminal_status = if has_failure { "failed" } else { "completed" };
                            let now = self.now();
                            pg_update_workflow_run_status(&mut tx, run_id, terminal_status, &now)?;
                            pg_insert_workflow_run_event(
                                &mut tx,
                                run_id,
                                None,
                                &format!("workflow_run.{terminal_status}"),
                                actor,
                                &json!({"reason": if has_failure { "node_failure" } else { "all_nodes_completed" }}),
                                &now,
                            )?;
                            pg_append_audit(
                                &mut tx,
                                &now,
                                actor,
                                &format!("workflow_run.{terminal_status}"),
                                run_id,
                                &json!({"metadata_only": true}),
                            )?;
                        }

                        let run = pg_get_run_row(&mut tx, run_id)?;
                        tx.commit().map_err(|e| e.to_string())?;
                        Ok(json!({
                            "action": if should_retry { "node_retry" } else { "node_executed" },
                            "node_id": node_id,
                            "executor_type": output.executor_type,
                            "attempt": attempt,
                            "result": output.to_value(),
                            "run": run,
                        }))
                    }),
                }?;

                // Record scheduler feedback for feedback-driven routing
                let task_group =
                    crate::routing::schemas::make_task_group(&input.task_type, "execute");
                let quality_score = output
                    .estimated_cost
                    .map(|c| if c > 0.0 { 1.0 / c } else { 1.0 })
                    .unwrap_or(1.0);
                let _ = self.insert_scheduler_feedback(
                    run_id,
                    Some(&node_id),
                    &output.executor_type,
                    &task_group,
                    output.status == "completed",
                    output.latency_ms.unwrap_or(0),
                    attempt,
                    quality_score,
                    output.estimated_cost.unwrap_or(0.0),
                    output.error_domain.as_deref(),
                );

                Ok(tick_result)
            }
        }
    }

    fn context_injection_for_node(
        &self,
        run_id: &str,
        node_id: &str,
    ) -> Result<Option<Value>, String> {
        let config = ContextAssemblyConfig::from_env();
        if !config.enabled {
            return Ok(None);
        }

        let sources = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT e.edge_id, e.from_node_id, n.node_json
                         FROM workflow_run_edges e
                         JOIN workflow_run_nodes n
                           ON n.run_id = e.run_id AND n.node_id = e.from_node_id
                         WHERE e.run_id = ?1
                           AND e.to_node_id = ?2
                           AND n.status = 'completed'
                         ORDER BY e.edge_id ASC",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![run_id, node_id], |row| {
                        let edge_id: String = row.get(0)?;
                        let from_node_id: String = row.get(1)?;
                        let node_json_text: String = row.get(2)?;
                        let node_json: Value =
                            serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                        Ok((edge_id, from_node_id, node_json))
                    })
                    .map_err(|e| e.to_string())?;
                let mut sources = Vec::new();
                for row in rows {
                    let (edge_id, from_node_id, node_json) = row.map_err(|e| e.to_string())?;
                    if let Some(output) = completed_node_output(&node_json) {
                        sources.push(ContextSource {
                            edge_id,
                            from_node_id,
                            output,
                        });
                    }
                }
                Ok(sources)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT e.edge_id, e.from_node_id, n.node_json
                         FROM workflow_run_edges e
                         JOIN workflow_run_nodes n
                           ON n.run_id = e.run_id AND n.node_id = e.from_node_id
                         WHERE e.run_id = $1
                           AND e.to_node_id = $2
                           AND n.status = 'completed'
                         ORDER BY e.edge_id ASC",
                        &[&run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?;
                let mut sources = Vec::new();
                for row in rows {
                    let node_json_text: String = row.get(2);
                    let node_json: Value =
                        serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                    if let Some(output) = completed_node_output(&node_json) {
                        sources.push(ContextSource {
                            edge_id: row.get(0),
                            from_node_id: row.get(1),
                            output,
                        });
                    }
                }
                Ok(sources)
            })?,
        };

        Ok(assemble_context_injection(node_id, &sources, &config))
    }

    pub fn validate_approval_binding(
        &self,
        run_id: &str,
        artifact_id: &str,
    ) -> Result<Value, String> {
        let approvals = self.workflow_run_approvals(run_id, 1000)?;
        let artifact = self
            .get_supervised_patch_artifact(artifact_id)?
            .ok_or_else(|| format!("artifact not found: {artifact_id}"))?;

        let artifact_hash = artifact
            .get("patch_hash")
            .and_then(Value::as_str)
            .unwrap_or("");
        let artifact_source = artifact
            .get("source_revision")
            .and_then(Value::as_str)
            .unwrap_or("");
        let artifact_files = artifact
            .get("changed_files")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut binding_checks = Vec::new();
        let mut approved_approval = None;

        for approval in &approvals {
            let decision = approval
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or("");
            if decision != "approved" {
                continue;
            }
            let bound_hash = approval
                .get("bound_patch_hash")
                .and_then(Value::as_str)
                .unwrap_or("");
            let bound_source = approval
                .get("bound_source_revision")
                .and_then(Value::as_str)
                .unwrap_or("");
            let bound_files = approval
                .get("bound_changed_files")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let expires_at = approval
                .get("expires_at")
                .and_then(Value::as_str)
                .unwrap_or("");

            let hash_match = !bound_hash.is_empty() && bound_hash == artifact_hash;
            let source_match = !bound_source.is_empty() && bound_source == artifact_source;
            let files_match = !bound_files.is_empty() && bound_files == artifact_files;
            let now_str = self.now();
            let not_expired = expires_at.is_empty() || expires_at > now_str.as_str();

            binding_checks.push(json!({
                "approval_id": approval.get("approval_id"),
                "hash_match": hash_match,
                "source_match": source_match,
                "files_match": files_match,
                "not_expired": not_expired,
            }));

            if hash_match && source_match && files_match && not_expired {
                approved_approval = Some(approval.clone());
                break;
            }
        }

        let eligible = approved_approval.is_some();
        Ok(json!({
            "run_id": run_id,
            "artifact_id": artifact_id,
            "export_eligible": eligible,
            "binding_checks": binding_checks,
            "approving_approval": approved_approval,
        }))
    }

    pub fn import_workflow_run(&self, run: &Value) -> Result<bool, String> {
        let run_id = run
            .get("run_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workflow run missing run_id".to_string())?;
        if self.get_workflow_run(run_id)?.is_some() {
            return Ok(false);
        }
        let workflow_id = run
            .get("workflow_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("workflow run {run_id} missing workflow_id"))?;
        let status = run
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("created");
        let boundaries = run
            .get("boundaries")
            .cloned()
            .unwrap_or_else(workflow_run_boundaries);
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
        let events = run
            .get("events")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let approvals = run
            .get("approvals")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_sequence(conn, "workflow_runs", "run_sequence")?;
                let created_at = run
                    .get("created_at")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| self.now());
                let updated_at = run
                    .get("updated_at")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| created_at.clone());
                conn.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status, workflow_id,
                      dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        sequence,
                        run_id,
                        run.get("plan_id").and_then(Value::as_str),
                        created_at,
                        updated_at,
                        status,
                        workflow_id,
                        run.get("dispatch_id").and_then(Value::as_str),
                        run.get("started_at").and_then(Value::as_str),
                        run.get("completed_at").and_then(Value::as_str),
                        optional_json_text(run.get("result")),
                        boundaries.to_string(),
                        run.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                for node in nodes {
                    insert_workflow_run_node_locked(conn, run_id, &node)?;
                }
                for edge in edges {
                    insert_workflow_run_edge_locked(conn, run_id, &edge)?;
                }
                for event in events {
                    import_workflow_run_event_locked(conn, run_id, &event)?;
                }
                for approval in approvals {
                    import_workflow_run_approval_locked(conn, run_id, &approval)?;
                }
                append_audit_locked(
                    conn,
                    &self.now(),
                    "import",
                    "workflow_run.import",
                    run_id,
                    &json!({"workflow_id": workflow_id, "metadata_only": true}),
                )?;
                Ok(true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let sequence = pg_next_sequence(&mut tx, "workflow_runs", "run_sequence")?;
                let created_at = run
                    .get("created_at")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| self.now());
                let updated_at = run
                    .get("updated_at")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| created_at.clone());
                tx.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status, workflow_id,
                      dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                    &[
                        &sequence,
                        &run_id,
                        &run.get("plan_id").and_then(Value::as_str),
                        &created_at,
                        &updated_at,
                        &status,
                        &workflow_id,
                        &run.get("dispatch_id").and_then(Value::as_str),
                        &run.get("started_at").and_then(Value::as_str),
                        &run.get("completed_at").and_then(Value::as_str),
                        &optional_json_text(run.get("result")),
                        &boundaries.to_string(),
                        &run.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                for node in nodes {
                    pg_insert_workflow_run_node(&mut tx, run_id, &node)?;
                }
                for edge in edges {
                    pg_insert_workflow_run_edge(&mut tx, run_id, &edge)?;
                }
                for event in events {
                    pg_import_workflow_run_event(&mut tx, run_id, &event)?;
                }
                for approval in approvals {
                    pg_import_workflow_run_approval(&mut tx, run_id, &approval)?;
                }
                pg_append_audit(
                    &mut tx,
                    &self.now(),
                    "import",
                    "workflow_run.import",
                    run_id,
                    &json!({"workflow_id": workflow_id, "metadata_only": true}),
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(true)
            }),
        }
    }

    pub fn list_active_workflow_run_ids(&self) -> Result<Vec<String>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare("SELECT run_id FROM workflow_runs WHERE status IN ('running', 'created') ORDER BY run_sequence")
                    .map_err(|e| e.to_string())?;
                let ids = stmt
                    .query_map([], |row| row.get(0))
                    .map_err(|e| e.to_string())?
                    .filter_map(|r| r.ok())
                    .collect();
                Ok(ids)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT run_id FROM workflow_runs WHERE status IN ('running', 'created') ORDER BY run_sequence",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                let ids: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
                Ok(ids)
            }),
        }
    }

    pub fn list_active_workflow_runs_prioritized(&self) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT run_id, run_sequence, workflow_id, status, priority, deadline_at,
                                sla_ms, tenant_id, queue_position, pause_reason, degrade_mode,
                                created_at, started_at
                         FROM workflow_runs
                         WHERE status IN ('running', 'created')
                         ORDER BY CASE WHEN pause_reason IS NOT NULL THEN 1 ELSE 0 END,
                                  priority ASC, created_at ASC",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok(json!({
                            "run_id": row.get::<_, String>(0)?,
                            "run_sequence": row.get::<_, i64>(1)?,
                            "workflow_id": row.get::<_, String>(2)?,
                            "status": row.get::<_, String>(3)?,
                            "priority": row.get::<_, i64>(4)?,
                            "deadline_at": row.get::<_, Option<String>>(5)?,
                            "sla_ms": row.get::<_, Option<i64>>(6)?,
                            "tenant_id": row.get::<_, Option<String>>(7)?,
                            "queue_position": row.get::<_, Option<i64>>(8)?,
                            "pause_reason": row.get::<_, Option<String>>(9)?,
                            "degrade_mode": row.get::<_, Option<String>>(10)?,
                            "created_at": row.get::<_, String>(11)?,
                            "started_at": row.get::<_, Option<String>>(12)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT run_id, run_sequence, workflow_id, status, priority, deadline_at,
                                sla_ms, tenant_id, queue_position, pause_reason, degrade_mode,
                                created_at, started_at
                         FROM workflow_runs
                         WHERE status IN ('running', 'created')
                         ORDER BY CASE WHEN pause_reason IS NOT NULL THEN 1 ELSE 0 END,
                                  priority ASC, created_at ASC",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                let values: Vec<Value> = rows
                    .iter()
                    .map(|row| {
                        json!({
                            "run_id": row.get::<_, String>(0),
                            "run_sequence": row.get::<_, i64>(1),
                            "workflow_id": row.get::<_, String>(2),
                            "status": row.get::<_, String>(3),
                            "priority": row.get::<_, i64>(4),
                            "deadline_at": row.get::<_, Option<String>>(5),
                            "sla_ms": row.get::<_, Option<i64>>(6),
                            "tenant_id": row.get::<_, Option<String>>(7),
                            "queue_position": row.get::<_, Option<i64>>(8),
                            "pause_reason": row.get::<_, Option<String>>(9),
                            "degrade_mode": row.get::<_, Option<String>>(10),
                            "created_at": row.get::<_, String>(11),
                            "started_at": row.get::<_, Option<String>>(12),
                        })
                    })
                    .collect();
                Ok(values)
            }),
        }
    }

    pub fn update_run_priority(&self, run_id: &str, priority: i64) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let updated = self.now();
                let rows = conn
                    .execute(
                        "UPDATE workflow_runs SET priority = ?1, updated_at = ?2 WHERE run_id = ?3",
                        params![priority, updated, run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let updated = self.now();
                let rows = client
                    .execute(
                        "UPDATE workflow_runs SET priority = $1, updated_at = $2 WHERE run_id = $3",
                        &[&priority, &updated, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                Ok(())
            }),
        }
    }

    pub fn update_run_pause_reason(
        &self,
        run_id: &str,
        pause_reason: Option<&str>,
    ) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let updated = self.now();
                let rows = conn
                    .execute(
                        "UPDATE workflow_runs SET pause_reason = ?1, updated_at = ?2 WHERE run_id = ?3",
                        params![pause_reason, updated, run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let updated = self.now();
                let rows = client
                    .execute(
                        "UPDATE workflow_runs SET pause_reason = $1, updated_at = $2 WHERE run_id = $3",
                        &[&pause_reason, &updated, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                Ok(())
            }),
        }
    }

    pub fn update_run_degrade_mode(
        &self,
        run_id: &str,
        degrade_mode: Option<&str>,
    ) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let updated = self.now();
                let rows = conn
                    .execute(
                        "UPDATE workflow_runs SET degrade_mode = ?1, updated_at = ?2 WHERE run_id = ?3",
                        params![degrade_mode, updated, run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let updated = self.now();
                let rows = client
                    .execute(
                        "UPDATE workflow_runs SET degrade_mode = $1, updated_at = $2 WHERE run_id = $3",
                        &[&degrade_mode, &updated, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                Ok(())
            }),
        }
    }

    pub fn set_run_queue_position(
        &self,
        run_id: &str,
        position: Option<i32>,
    ) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let updated = self.now();
                let rows = conn.execute(
                    "UPDATE workflow_runs SET queue_position = ?1, updated_at = ?2 WHERE run_id = ?3",
                    params![position, updated, run_id],
                )
                .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let updated = self.now();
                let rows = client
                    .execute(
                        "UPDATE workflow_runs SET queue_position = $1, updated_at = $2 WHERE run_id = $3",
                        &[&position, &updated, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                Ok(())
            }),
        }
    }

    pub fn get_queue_status(&self) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let total_queued: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_runs WHERE status IN ('created') AND pause_reason IS NULL",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let total_running: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_runs WHERE status = 'running'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let total_paused: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_runs WHERE pause_reason IS NOT NULL",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let total_completed: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_runs WHERE status = 'completed'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let total_failed: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_runs WHERE status = 'failed'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let avg_priority: Value = conn
                    .query_row(
                        "SELECT COALESCE(AVG(priority), 5.0) FROM workflow_runs WHERE status IN ('created', 'running')",
                        [],
                        |row| {
                            let avg: f64 = row.get(0)?;
                            Ok(json!(avg))
                        },
                    )
                    .unwrap_or(json!(5.0));
                let overdue_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_runs
                         WHERE deadline_at IS NOT NULL AND deadline_at < datetime('now')
                           AND status IN ('created', 'running')",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                Ok(json!({
                    "total_queued": total_queued,
                    "total_running": total_running,
                    "total_paused": total_paused,
                    "total_completed": total_completed,
                    "total_failed": total_failed,
                    "avg_priority": avg_priority,
                    "overdue_count": overdue_count,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let total_queued: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_runs WHERE status IN ('created') AND pause_reason IS NULL",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let total_running: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_runs WHERE status = 'running'",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let total_paused: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_runs WHERE pause_reason IS NOT NULL",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let total_completed: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_runs WHERE status = 'completed'",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let total_failed: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_runs WHERE status = 'failed'",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let avg_priority: Value = client
                    .query_one(
                        "SELECT COALESCE(AVG(priority), 5.0) FROM workflow_runs WHERE status IN ('created', 'running')",
                        &[],
                    )
                    .map(|row| {
                        let avg: f64 = row.get(0);
                        json!(avg)
                    })
                    .unwrap_or(json!(5.0));
                let overdue_count: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_runs
                         WHERE deadline_at IS NOT NULL AND deadline_at < now()
                           AND status IN ('created', 'running')",
                        &[],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                Ok(json!({
                    "total_queued": total_queued,
                    "total_running": total_running,
                    "total_paused": total_paused,
                    "total_completed": total_completed,
                    "total_failed": total_failed,
                    "avg_priority": avg_priority,
                    "overdue_count": overdue_count,
                }))
            }),
        }
    }

    pub fn list_tenants_with_quota(&self) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT tenant_id, COUNT(*) as run_count, AVG(priority) as avg_priority
                         FROM workflow_runs
                         WHERE status IN ('created', 'running') AND tenant_id IS NOT NULL
                         GROUP BY tenant_id",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok(json!({
                            "tenant_id": row.get::<_, String>(0)?,
                            "run_count": row.get::<_, i64>(1)?,
                            "avg_priority": row.get::<_, f64>(2)?,
                        }))
                    })
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT tenant_id, COUNT(*) as run_count, AVG(priority) as avg_priority
                         FROM workflow_runs
                         WHERE status IN ('created', 'running') AND tenant_id IS NOT NULL
                         GROUP BY tenant_id",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                let values: Vec<Value> = rows
                    .iter()
                    .map(|row| {
                        json!({
                            "tenant_id": row.get::<_, String>(0),
                            "run_count": row.get::<_, i64>(1),
                            "avg_priority": row.get::<_, f64>(2),
                        })
                    })
                    .collect();
                Ok(values)
            }),
        }
    }

    pub fn create_workflow_run_with_queue_metadata(
        &self,
        plan_id: &str,
        actor: &str,
        priority: i64,
        deadline_at: Option<&str>,
        sla_ms: Option<i64>,
        tenant_id: Option<&str>,
    ) -> Result<Value, String> {
        let plan = self
            .get_workflow_plan(plan_id)?
            .ok_or_else(|| format!("plan not found: {plan_id}"))?;
        let graph = required_object(&plan, "graph")?;
        let workflow_id = plan
            .get("workflow_id")
            .and_then(Value::as_str)
            .or_else(|| graph.get("workflow_id").and_then(Value::as_str))
            .ok_or_else(|| format!("plan {plan_id} missing workflow_id"))?;
        let dispatch_id = plan
            .get("dispatch_id")
            .and_then(Value::as_str)
            .or_else(|| graph.get("dispatch_id").and_then(Value::as_str))
            .unwrap_or("");
        let nodes = graph
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("plan {plan_id} graph missing nodes"))?;
        let edges = graph
            .get("edges")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("plan {plan_id} graph missing edges"))?;

        let run_id = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_sequence(conn, "workflow_runs", "run_sequence")?;
                let run_id = format!("run-{sequence:04}");
                let created_at = self.now();
                let boundaries = workflow_run_boundaries();
                let run = json!({
                    "schema_version": WORKFLOW_RUN_SCHEMA_VERSION,
                    "run_sequence": sequence,
                    "run_id": run_id,
                    "plan_id": plan_id,
                    "workflow_id": workflow_id,
                    "dispatch_id": dispatch_id,
                    "status": "created",
                    "created_at": created_at,
                    "updated_at": created_at,
                    "started_at": null,
                    "completed_at": null,
                    "result": null,
                    "graph": graph,
                    "boundaries": boundaries,
                    "priority": priority,
                    "deadline_at": deadline_at,
                    "sla_ms": sla_ms,
                    "tenant_id": tenant_id,
                    "queue_position": null,
                    "pause_reason": null,
                    "degrade_mode": null,
                });
                conn.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status, workflow_id,
                      dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json,
                      priority, deadline_at, sla_ms, tenant_id, queue_position, pause_reason, degrade_mode)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, NULL, ?9, ?10,
                             ?11, ?12, ?13, ?14, NULL, NULL, NULL)",
                    params![
                        sequence,
                        run_id,
                        plan_id,
                        created_at,
                        created_at,
                        "created",
                        workflow_id,
                        null_if_empty(dispatch_id),
                        boundaries.to_string(),
                        run.to_string(),
                        priority,
                        deadline_at,
                        sla_ms,
                        tenant_id,
                    ],
                )
                .map_err(|e| e.to_string())?;

                for node in nodes {
                    insert_workflow_run_node_locked(conn, &run_id, node)?;
                }
                for edge in edges {
                    insert_workflow_run_edge_locked(conn, &run_id, edge)?;
                }
                insert_workflow_run_event_locked(
                    conn,
                    &run_id,
                    None,
                    "workflow_run.created",
                    actor,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "priority": priority,
                        "tenant_id": tenant_id,
                        "metadata_only": true,
                    }),
                    &created_at,
                )?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "workflow_run.create",
                    &run_id,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "priority": priority,
                        "tenant_id": tenant_id,
                        "metadata_only": true,
                    }),
                )?;
                Ok(run_id)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let sequence = pg_next_sequence(&mut tx, "workflow_runs", "run_sequence")?;
                let run_id = format!("run-{sequence:04}");
                let created_at = self.now();
                let boundaries = workflow_run_boundaries();
                let run = json!({
                    "schema_version": WORKFLOW_RUN_SCHEMA_VERSION,
                    "run_sequence": sequence,
                    "run_id": run_id,
                    "plan_id": plan_id,
                    "workflow_id": workflow_id,
                    "dispatch_id": dispatch_id,
                    "status": "created",
                    "created_at": created_at,
                    "updated_at": created_at,
                    "started_at": null,
                    "completed_at": null,
                    "result": null,
                    "graph": graph,
                    "boundaries": boundaries,
                    "priority": priority,
                    "deadline_at": deadline_at,
                    "sla_ms": sla_ms,
                    "tenant_id": tenant_id,
                    "queue_position": null,
                    "pause_reason": null,
                    "degrade_mode": null,
                });
                tx.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status, workflow_id,
                      dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json,
                      priority, deadline_at, sla_ms, tenant_id, queue_position, pause_reason, degrade_mode)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, NULL, NULL, $9, $10,
                             $11, $12, $13, $14, NULL, NULL, NULL)",
                    &[
                        &sequence,
                        &run_id,
                        &plan_id,
                        &created_at,
                        &created_at,
                        &"created",
                        &workflow_id,
                        &null_if_empty(dispatch_id),
                        &boundaries.to_string(),
                        &run.to_string(),
                        &priority,
                        &deadline_at,
                        &sla_ms,
                        &tenant_id,
                    ],
                )
                .map_err(|e| e.to_string())?;

                for node in nodes {
                    pg_insert_workflow_run_node(&mut tx, &run_id, node)?;
                }
                for edge in edges {
                    pg_insert_workflow_run_edge(&mut tx, &run_id, edge)?;
                }
                pg_insert_workflow_run_event(
                    &mut tx,
                    &run_id,
                    None,
                    "workflow_run.created",
                    actor,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "priority": priority,
                        "tenant_id": tenant_id,
                        "metadata_only": true,
                    }),
                    &created_at,
                )?;
                pg_append_audit(
                    &mut tx,
                    &created_at,
                    actor,
                    "workflow_run.create",
                    &run_id,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "priority": priority,
                        "tenant_id": tenant_id,
                        "metadata_only": true,
                    }),
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(run_id)
            }),
        }?;

        self.get_workflow_run(&run_id)?
            .ok_or_else(|| format!("workflow run not found after create: {run_id}"))
    }

    pub fn set_pending_node_to_running_for_test(&self, leased_at: &str) -> Result<i64, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let node_id: Option<String> = conn
                    .query_row(
                        "SELECT node_id FROM workflow_run_nodes WHERE status = 'pending' LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .ok();
                let Some(node_id) = node_id else {
                    return Ok(0);
                };
                let count = conn
                    .execute(
                        "UPDATE workflow_run_nodes SET status = 'running', leased_at = ?1 WHERE node_id = ?2",
                        rusqlite::params![leased_at, node_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(count as i64)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT node_id FROM workflow_run_nodes WHERE status = 'pending' LIMIT 1",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                let Some(row) = rows.into_iter().next() else {
                    return Ok(0);
                };
                let node_id: String = row.get(0);
                let count = client
                    .execute(
                        "UPDATE workflow_run_nodes SET status = 'running', leased_at = $1 WHERE node_id = $2",
                        &[&leased_at, &node_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(count as i64)
            }),
        }
    }

    pub fn recover_stale_leases(&self, lease_timeout_ms: u64) -> Result<i64, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let now = self.now();
                let mut stmt = conn
                    .prepare("SELECT run_id, node_id, leased_at FROM workflow_run_nodes WHERE status = 'running' AND leased_at IS NOT NULL")
                    .map_err(|e| e.to_string())?;
                let stale_nodes: Vec<(String, String, String)> = stmt
                    .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))
                    .map_err(|e| e.to_string())?
                    .filter_map(|r| r.ok())
                    .filter(|(_, _, leased_at)| {
                        if let (Ok(lease_time), Ok(now_time)) = (
                            chrono::NaiveDateTime::parse_from_str(leased_at, "%Y-%m-%dT%H:%M:%SZ"),
                            chrono::NaiveDateTime::parse_from_str(&now, "%Y-%m-%dT%H:%M:%SZ"),
                        ) {
                            (now_time - lease_time).num_milliseconds() as u64 > lease_timeout_ms
                        } else {
                            false
                        }
                    })
                    .collect();
                let count = stale_nodes.len() as i64;
                for (run_id, node_id, _) in &stale_nodes {
                    conn.execute(
                        "UPDATE workflow_run_nodes SET status = 'pending', leased_at = NULL WHERE run_id = ?1 AND node_id = ?2 AND status = 'running'",
                        params![run_id, node_id],
                    ).map_err(|e| e.to_string())?;
                }
                Ok(count)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let now = self.now();
                let rows = client
                    .query(
                        "SELECT run_id, node_id, leased_at FROM workflow_run_nodes WHERE status = 'running' AND leased_at IS NOT NULL",
                        &[],
                    )
                    .map_err(|e| e.to_string())?;
                let stale_nodes: Vec<(String, String, String)> = rows
                    .iter()
                    .map(|row| (row.get(0), row.get(1), row.get(2)))
                    .filter(|(_, _, leased_at): &(String, String, String)| {
                        if let (Ok(lease_time), Ok(now_time)) = (
                            chrono::NaiveDateTime::parse_from_str(leased_at, "%Y-%m-%dT%H:%M:%SZ"),
                            chrono::NaiveDateTime::parse_from_str(&now, "%Y-%m-%dT%H:%M:%SZ"),
                        ) {
                            (now_time - lease_time).num_milliseconds() as u64 > lease_timeout_ms
                        } else {
                            false
                        }
                    })
                    .collect();
                let count = stale_nodes.len() as i64;
                for (run_id, node_id, _) in &stale_nodes {
                    client.execute(
                        "UPDATE workflow_run_nodes SET status = 'pending', leased_at = NULL WHERE run_id = $1 AND node_id = $2 AND status = 'running'",
                        &[run_id, node_id],
                    ).map_err(|e| e.to_string())?;
                }
                Ok(count)
            }),
        }
    }

    fn update_workflow_run_status_with_event(
        &self,
        run_id: &str,
        status: &str,
        event_type: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let updated_at = self.now();
                update_workflow_run_status_locked(conn, run_id, status, &updated_at)?;
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    None,
                    event_type,
                    actor,
                    &json!({"reason": reason, "metadata_only": true, "execution_authority": "disabled"}),
                    &updated_at,
                )?;
                append_audit_locked(
                    conn,
                    &updated_at,
                    actor,
                    event_type,
                    run_id,
                    &json!({"reason": reason, "metadata_only": true, "execution_authority": "disabled"}),
                )?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let updated_at = self.now();
                pg_update_workflow_run_status(client, run_id, status, &updated_at)?;
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    None,
                    event_type,
                    actor,
                    &json!({"reason": reason, "metadata_only": true, "execution_authority": "disabled"}),
                    &updated_at,
                )?;
                pg_append_audit(
                    client,
                    &updated_at,
                    actor,
                    event_type,
                    run_id,
                    &json!({"reason": reason, "metadata_only": true, "execution_authority": "disabled"}),
                )?;
                Ok(())
            }),
        }?;
        self.get_workflow_run(run_id)?
            .ok_or_else(|| format!("workflow run not found after update: {run_id}"))
    }

    pub fn insert_workflow_node(
        &self,
        run_id: &str,
        node: &Value,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        let node_id = node
            .get("node_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workflow node missing node_id".to_string())?
            .to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let existing: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if existing > 0 {
                    return Err(format!("duplicate node_id: {node_id}"));
                }
                insert_workflow_run_node_locked(conn, run_id, node)?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    Some(&node_id),
                    "dag.mutation.node_added",
                    actor,
                    &json!({"node_id": node_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let existing: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                if existing > 0 {
                    return Err(format!("duplicate node_id: {node_id}"));
                }
                pg_insert_workflow_run_node(client, run_id, node)?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    Some(&node_id),
                    "dag.mutation.node_added",
                    actor,
                    &json!({"node_id": node_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(())
            }),
        }?;
        Ok(json!({
            "action": "node_inserted",
            "node_id": node_id,
            "run_id": run_id,
            "metadata_only": true,
        }))
    }

    pub fn remove_workflow_node(
        &self,
        run_id: &str,
        node_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let connected_edges: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_run_edges WHERE run_id = ?1 AND (from_node_id = ?2 OR to_node_id = ?2)",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                conn.execute(
                    "DELETE FROM workflow_run_edges WHERE run_id = ?1 AND (from_node_id = ?2 OR to_node_id = ?2)",
                    params![run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                conn.execute(
                    "DELETE FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                    params![run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    Some(node_id),
                    "dag.mutation.node_removed",
                    actor,
                    &json!({"node_id": node_id, "removed_edges": connected_edges, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "node_removed",
                    "node_id": node_id,
                    "run_id": run_id,
                    "removed_edges": connected_edges,
                    "metadata_only": true,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let connected_edges: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_run_edges WHERE run_id = $1 AND (from_node_id = $2 OR to_node_id = $2)",
                        &[&run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                client.execute(
                    "DELETE FROM workflow_run_edges WHERE run_id = $1 AND (from_node_id = $2 OR to_node_id = $2)",
                    &[&run_id, &node_id],
                )
                .map_err(|e| e.to_string())?;
                client.execute(
                    "DELETE FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                    &[&run_id, &node_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    Some(node_id),
                    "dag.mutation.node_removed",
                    actor,
                    &json!({"node_id": node_id, "removed_edges": connected_edges, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "node_removed",
                    "node_id": node_id,
                    "run_id": run_id,
                    "removed_edges": connected_edges,
                    "metadata_only": true,
                }))
            }),
        }
    }

    pub fn update_workflow_node_status(
        &self,
        run_id: &str,
        node_id: &str,
        new_status: &str,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        let valid_statuses = [
            "pending",
            "running",
            "completed",
            "failed",
            "cancelled",
            "blocked",
            "waiting_human",
            "recovered",
        ];
        if !valid_statuses.contains(&new_status) {
            return Err(format!("invalid node status: {new_status}"));
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let now = self.now();
                conn.execute(
                    "UPDATE workflow_run_nodes SET status = ?1 WHERE run_id = ?2 AND node_id = ?3",
                    params![new_status, run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                let node_json_text: String = conn
                    .query_row(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let mut node_json: Value =
                    serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                if let Some(obj) = node_json.as_object_mut() {
                    obj.insert("status".to_string(), json!(new_status));
                }
                conn.execute(
                    "UPDATE workflow_run_nodes SET node_json = ?1 WHERE run_id = ?2 AND node_id = ?3",
                    params![node_json.to_string(), run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    Some(node_id),
                    "dag.mutation.node_status_updated",
                    actor,
                    &json!({"node_id": node_id, "new_status": new_status, "reason": reason, "metadata_only": true}),
                    &now,
                )?;
                Ok(json!({
                    "action": "node_status_updated",
                    "node_id": node_id,
                    "run_id": run_id,
                    "new_status": new_status,
                    "metadata_only": true,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let now = self.now();
                client.execute(
                    "UPDATE workflow_run_nodes SET status = $1 WHERE run_id = $2 AND node_id = $3",
                    &[&new_status, &run_id, &node_id],
                )
                .map_err(|e| e.to_string())?;
                let node_json_text: String = client
                    .query_one(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let mut node_json: Value =
                    serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                if let Some(obj) = node_json.as_object_mut() {
                    obj.insert("status".to_string(), json!(new_status));
                }
                client.execute(
                    "UPDATE workflow_run_nodes SET node_json = $1 WHERE run_id = $2 AND node_id = $3",
                    &[&node_json.to_string(), &run_id, &node_id],
                )
                .map_err(|e| e.to_string())?;
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    Some(node_id),
                    "dag.mutation.node_status_updated",
                    actor,
                    &json!({"node_id": node_id, "new_status": new_status, "reason": reason, "metadata_only": true}),
                    &now,
                )?;
                Ok(json!({
                    "action": "node_status_updated",
                    "node_id": node_id,
                    "run_id": run_id,
                    "new_status": new_status,
                    "metadata_only": true,
                }))
            }),
        }
    }

    pub fn insert_workflow_edge(
        &self,
        run_id: &str,
        edge: &Value,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        let edge_id = edge
            .get("edge_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workflow edge missing edge_id".to_string())?
            .to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let existing: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_run_edges WHERE run_id = ?1 AND edge_id = ?2",
                        params![run_id, edge_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if existing > 0 {
                    return Err(format!("duplicate edge_id: {edge_id}"));
                }
                insert_workflow_run_edge_locked(conn, run_id, edge)?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    None,
                    "dag.mutation.edge_added",
                    actor,
                    &json!({"edge_id": edge_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let existing: i64 = client
                    .query_one(
                        "SELECT COUNT(*) FROM workflow_run_edges WHERE run_id = $1 AND edge_id = $2",
                        &[&run_id, &edge_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                if existing > 0 {
                    return Err(format!("duplicate edge_id: {edge_id}"));
                }
                pg_insert_workflow_run_edge(client, run_id, edge)?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    None,
                    "dag.mutation.edge_added",
                    actor,
                    &json!({"edge_id": edge_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(())
            }),
        }?;
        Ok(json!({
            "action": "edge_inserted",
            "edge_id": edge_id,
            "run_id": run_id,
            "metadata_only": true,
        }))
    }

    pub fn remove_workflow_edge(
        &self,
        run_id: &str,
        edge_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                conn.execute(
                    "DELETE FROM workflow_run_edges WHERE run_id = ?1 AND edge_id = ?2",
                    params![run_id, edge_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    None,
                    "dag.mutation.edge_removed",
                    actor,
                    &json!({"edge_id": edge_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "edge_removed",
                    "edge_id": edge_id,
                    "run_id": run_id,
                    "metadata_only": true,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                client
                    .execute(
                        "DELETE FROM workflow_run_edges WHERE run_id = $1 AND edge_id = $2",
                        &[&run_id, &edge_id],
                    )
                    .map_err(|e| e.to_string())?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    None,
                    "dag.mutation.edge_removed",
                    actor,
                    &json!({"edge_id": edge_id, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "edge_removed",
                    "edge_id": edge_id,
                    "run_id": run_id,
                    "metadata_only": true,
                }))
            }),
        }
    }

    pub fn rewire_workflow_edge(
        &self,
        run_id: &str,
        edge_id: &str,
        new_from: Option<&str>,
        new_to: Option<&str>,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let edge_json_text: String = conn
                    .query_row(
                        "SELECT edge_json FROM workflow_run_edges WHERE run_id = ?1 AND edge_id = ?2",
                        params![run_id, edge_id],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                let mut edge_json: Value =
                    serde_json::from_str(&edge_json_text).unwrap_or(Value::Null);
                let current_from = edge_json
                    .get("from_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let current_to = edge_json
                    .get("to_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let final_from = new_from.unwrap_or(&current_from);
                let final_to = new_to.unwrap_or(&current_to);
                if let Some(obj) = edge_json.as_object_mut() {
                    obj.insert("from_node_id".to_string(), json!(final_from));
                    obj.insert("to_node_id".to_string(), json!(final_to));
                }
                conn.execute(
                    "UPDATE workflow_run_edges SET from_node_id = ?1, to_node_id = ?2, edge_json = ?3 WHERE run_id = ?4 AND edge_id = ?5",
                    params![final_from, final_to, edge_json.to_string(), run_id, edge_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    None,
                    "dag.mutation.edge_rewired",
                    actor,
                    &json!({"edge_id": edge_id, "old_from": current_from, "old_to": current_to, "new_from": final_from, "new_to": final_to, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "edge_rewired",
                    "edge_id": edge_id,
                    "run_id": run_id,
                    "old_from": current_from,
                    "old_to": current_to,
                    "new_from": final_from,
                    "new_to": final_to,
                    "metadata_only": true,
                }))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let edge_json_text: String = client
                    .query_one(
                        "SELECT edge_json FROM workflow_run_edges WHERE run_id = $1 AND edge_id = $2",
                        &[&run_id, &edge_id],
                    )
                    .map_err(|e| e.to_string())?
                    .get(0);
                let mut edge_json: Value =
                    serde_json::from_str(&edge_json_text).unwrap_or(Value::Null);
                let current_from = edge_json
                    .get("from_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let current_to = edge_json
                    .get("to_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let final_from = new_from.unwrap_or(&current_from);
                let final_to = new_to.unwrap_or(&current_to);
                if let Some(obj) = edge_json.as_object_mut() {
                    obj.insert("from_node_id".to_string(), json!(final_from));
                    obj.insert("to_node_id".to_string(), json!(final_to));
                }
                client.execute(
                    "UPDATE workflow_run_edges SET from_node_id = $1, to_node_id = $2, edge_json = $3 WHERE run_id = $4 AND edge_id = $5",
                    &[&final_from, &final_to, &edge_json.to_string(), &run_id, &edge_id],
                )
                .map_err(|e| e.to_string())?;
                let created_at = self.now();
                pg_insert_workflow_run_event(
                    client,
                    run_id,
                    None,
                    "dag.mutation.edge_rewired",
                    actor,
                    &json!({"edge_id": edge_id, "old_from": current_from, "old_to": current_to, "new_from": final_from, "new_to": final_to, "reason": reason, "metadata_only": true}),
                    &created_at,
                )?;
                Ok(json!({
                    "action": "edge_rewired",
                    "edge_id": edge_id,
                    "run_id": run_id,
                    "old_from": current_from,
                    "old_to": current_to,
                    "new_from": final_from,
                    "new_to": final_to,
                    "metadata_only": true,
                }))
            }),
        }
    }

    pub fn apply_dag_mutations_batch(
        &self,
        run_id: &str,
        proposals: &[DAGMutationProposal],
        actor: &str,
    ) -> Result<Vec<Value>, String> {
        let (base_nodes, base_edges) = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                let nodes = workflow_run_nodes_locked(conn, run_id)?;
                let edges = workflow_run_edges_locked(conn, run_id)?;
                Ok((nodes, edges))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                let nodes = pg_workflow_run_nodes(client, run_id)?;
                let edges = pg_workflow_run_edges(client, run_id)?;
                Ok((nodes, edges))
            }),
        }?;

        let dag_nodes: Vec<crate::workflow::dag_manager::DAGNode> = base_nodes
            .iter()
            .map(|n| crate::workflow::dag_manager::DAGNode {
                node_id: n
                    .get("node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                task_id: n.get("task_type").and_then(Value::as_str).map(String::from),
                node_type: n
                    .get("task_type")
                    .and_then(Value::as_str)
                    .unwrap_or("task")
                    .to_string(),
                status: n
                    .get("db_status")
                    .and_then(Value::as_str)
                    .or_else(|| n.get("status").and_then(Value::as_str))
                    .unwrap_or("pending")
                    .to_string(),
                tier: n
                    .get("tier")
                    .and_then(Value::as_str)
                    .unwrap_or("cheap_executor")
                    .to_string(),
                metadata: Default::default(),
            })
            .collect();

        let dag_edges: Vec<crate::workflow::dag_manager::DAGEdge> = base_edges
            .iter()
            .map(|e| crate::workflow::dag_manager::DAGEdge {
                edge_id: e
                    .get("edge_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                from_node: e
                    .get("from_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                to_node: e
                    .get("to_node_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                dependency_type: e
                    .get("edge_type")
                    .and_then(Value::as_str)
                    .unwrap_or("dependency")
                    .to_string(),
                status: "pending".to_string(),
            })
            .collect();

        let mut mgr = DAGManager::new(run_id, &self.now());
        {
            let state = mgr.current_state();
            let _ = state;
        }
        // Load base state into manager
        for n in &dag_nodes {
            let mut payload = std::collections::HashMap::new();
            payload.insert("node_id".to_string(), json!(n.node_id));
            payload.insert("node_type".to_string(), json!(n.node_type));
            payload.insert("status".to_string(), json!(n.status));
            payload.insert("tier".to_string(), json!(n.tier));
            let proposal = DAGMutationProposal {
                proposal_id: format!("load_node_{}", n.node_id),
                dag_id: run_id.to_string(),
                mutation_type: "add_node".to_string(),
                payload,
                ..Default::default()
            };
            mgr.apply_mutation(&proposal);
        }
        for e in &dag_edges {
            let mut payload = std::collections::HashMap::new();
            payload.insert("edge_id".to_string(), json!(e.edge_id));
            payload.insert("from_node".to_string(), json!(e.from_node));
            payload.insert("to_node".to_string(), json!(e.to_node));
            let proposal = DAGMutationProposal {
                proposal_id: format!("load_edge_{}", e.edge_id),
                dag_id: run_id.to_string(),
                mutation_type: "add_edge".to_string(),
                payload,
                ..Default::default()
            };
            mgr.apply_mutation(&proposal);
        }

        let mut results = Vec::new();
        for proposal in proposals {
            let result = mgr.apply_mutation(proposal);
            if !result.applied {
                results.push(json!({
                    "proposal_id": proposal.proposal_id,
                    "applied": false,
                    "errors": result.errors,
                }));
                continue;
            }
            let persist_result = match proposal.mutation_type.as_str() {
                "add_node" => {
                    let node_value = proposal
                        .payload
                        .get("node_id")
                        .cloned()
                        .map(|nid| {
                            let mut n = json!({
                                "node_id": nid,
                                "task_type": proposal.payload.get("task_type").or_else(|| proposal.payload.get("node_type")).and_then(Value::as_str).unwrap_or("task"),
                                "status": proposal.payload.get("status").and_then(Value::as_str).unwrap_or("pending"),
                            });
                            if let Some(pid) = proposal.payload.get("profile_id") {
                                if let Some(obj) = n.as_object_mut() {
                                    obj.insert("profile_id".to_string(), pid.clone());
                                }
                            }
                            n
                        })
                        .unwrap_or(Value::Null);
                    self.insert_workflow_node(run_id, &node_value, actor, &proposal.reason)
                        .map(Some)
                }
                "remove_node" => {
                    let nid = proposal.target_node_id.as_deref().unwrap_or("");
                    self.remove_workflow_node(run_id, nid, actor, &proposal.reason)
                        .map(Some)
                }
                "add_edge" => {
                    let edge_value = json!({
                        "edge_id": proposal.payload.get("edge_id").and_then(Value::as_str).unwrap_or(""),
                        "from_node_id": proposal.payload.get("from_node").and_then(Value::as_str).unwrap_or(""),
                        "to_node_id": proposal.payload.get("to_node").and_then(Value::as_str).unwrap_or(""),
                    });
                    self.insert_workflow_edge(run_id, &edge_value, actor, &proposal.reason)
                        .map(Some)
                }
                "remove_edge" => {
                    let eid = proposal.target_edge_id.as_deref().unwrap_or("");
                    self.remove_workflow_edge(run_id, eid, actor, &proposal.reason)
                        .map(Some)
                }
                "rewire_edge" => {
                    let eid = proposal.target_edge_id.as_deref().unwrap_or("");
                    let new_from = proposal.payload.get("from_node").and_then(Value::as_str);
                    let new_to = proposal.payload.get("to_node").and_then(Value::as_str);
                    self.rewire_workflow_edge(
                        run_id,
                        eid,
                        new_from,
                        new_to,
                        actor,
                        &proposal.reason,
                    )
                    .map(Some)
                }
                "update_node" => {
                    let nid = proposal.target_node_id.as_deref().unwrap_or("");
                    let status = proposal
                        .payload
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("pending");
                    self.update_workflow_node_status(run_id, nid, status, actor, &proposal.reason)
                        .map(Some)
                }
                _ => Err(format!(
                    "unsupported mutation type: {}",
                    proposal.mutation_type
                )),
            };
            match persist_result {
                Ok(_) => {
                    match &self.db {
                        DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                            let created_at = self.now();
                            insert_workflow_run_event_locked(
                                conn,
                                run_id,
                                None,
                                "dag.mutation.applied",
                                actor,
                                &json!({
                                    "proposal_id": proposal.proposal_id,
                                    "mutation_type": proposal.mutation_type,
                                    "metadata_only": true,
                                }),
                                &created_at,
                            )?;
                            Ok(())
                        }),
                        #[cfg(feature = "pg")]
                        DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                            let created_at = self.now();
                            pg_insert_workflow_run_event(
                                client,
                                run_id,
                                None,
                                "dag.mutation.applied",
                                actor,
                                &json!({
                                    "proposal_id": proposal.proposal_id,
                                    "mutation_type": proposal.mutation_type,
                                    "metadata_only": true,
                                }),
                                &created_at,
                            )?;
                            Ok(())
                        }),
                    }?;
                    results.push(json!({
                        "proposal_id": proposal.proposal_id,
                        "applied": true,
                        "new_dag_version": result.new_dag_version,
                    }));
                }
                Err(e) => {
                    results.push(json!({
                        "proposal_id": proposal.proposal_id,
                        "applied": false,
                        "errors": [e],
                    }));
                }
            }
        }
        Ok(results)
    }

    pub fn replay_mutation_events(&self, run_id: &str) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                Ok(())
            }),
        }?;

        let run = self
            .get_workflow_run(run_id)?
            .ok_or_else(|| format!("workflow run not found: {run_id}"))?;

        let base_nodes = run
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let base_edges = run
            .get("edges")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let all_events = match &self.db {
            DatabaseConnection::Sqlite(_) => {
                self.with_conn(|conn| workflow_run_events_locked(conn, run_id, 100_000))?
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| pg_workflow_run_events(client, run_id, 100_000))?
            }
        };

        let mutation_events: Vec<&Value> = all_events
            .iter()
            .filter(|e| {
                e.get("event_type")
                    .and_then(Value::as_str)
                    .map(|t| t.starts_with("dag.mutation."))
                    .unwrap_or(false)
            })
            .collect();

        let mut nodes: Vec<Value> = base_nodes.clone();
        let mut edges: Vec<Value> = base_edges.clone();
        let mut mutations_replayed = 0u64;

        for event in &mutation_events {
            let event_type = event
                .get("event_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            let details = event.get("details").cloned().unwrap_or(Value::Null);

            match event_type {
                "dag.mutation.node_added" => {
                    if let Some(node_id) = details.get("node_id").and_then(Value::as_str) {
                        if !nodes
                            .iter()
                            .any(|n| n.get("node_id").and_then(Value::as_str) == Some(node_id))
                        {
                            nodes.push(json!({
                                "node_id": node_id,
                                "status": "pending",
                                "task_type": "unknown",
                            }));
                        }
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.node_removed" => {
                    if let Some(node_id) = details.get("node_id").and_then(Value::as_str) {
                        nodes.retain(|n| n.get("node_id").and_then(Value::as_str) != Some(node_id));
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.node_status_updated" => {
                    if let Some(node_id) = details.get("node_id").and_then(Value::as_str) {
                        if let Some(new_status) = details.get("new_status").and_then(Value::as_str)
                        {
                            if let Some(node) = nodes
                                .iter_mut()
                                .find(|n| n.get("node_id").and_then(Value::as_str) == Some(node_id))
                            {
                                if let Some(obj) = node.as_object_mut() {
                                    obj.insert("status".to_string(), json!(new_status));
                                }
                            }
                        }
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.edge_added" => {
                    if let Some(edge_id) = details.get("edge_id").and_then(Value::as_str) {
                        if !edges
                            .iter()
                            .any(|e| e.get("edge_id").and_then(Value::as_str) == Some(edge_id))
                        {
                            edges.push(json!({
                                "edge_id": edge_id,
                            }));
                        }
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.edge_removed" => {
                    if let Some(edge_id) = details.get("edge_id").and_then(Value::as_str) {
                        edges.retain(|e| e.get("edge_id").and_then(Value::as_str) != Some(edge_id));
                    }
                    mutations_replayed += 1;
                }
                "dag.mutation.edge_rewired" => {
                    if let Some(edge_id) = details.get("edge_id").and_then(Value::as_str) {
                        if let Some(edge) = edges
                            .iter_mut()
                            .find(|e| e.get("edge_id").and_then(Value::as_str) == Some(edge_id))
                        {
                            if let Some(obj) = edge.as_object_mut() {
                                if let Some(new_from) =
                                    details.get("new_from").and_then(Value::as_str)
                                {
                                    obj.insert("from_node_id".to_string(), json!(new_from));
                                }
                                if let Some(new_to) = details.get("new_to").and_then(Value::as_str)
                                {
                                    obj.insert("to_node_id".to_string(), json!(new_to));
                                }
                            }
                        }
                    }
                    mutations_replayed += 1;
                }
                _ => {}
            }
        }

        let protected_completed_nodes: Vec<String> = base_nodes
            .iter()
            .filter(|n| {
                let status = n
                    .get("db_status")
                    .and_then(Value::as_str)
                    .or_else(|| n.get("status").and_then(Value::as_str))
                    .unwrap_or("");
                matches!(status, "completed" | "failed")
            })
            .filter_map(|n| n.get("node_id").and_then(Value::as_str).map(String::from))
            .collect();

        Ok(json!({
            "run_id": run_id,
            "mutations_replayed": mutations_replayed,
            "nodes": nodes,
            "edges": edges,
            "protected_completed_nodes": protected_completed_nodes,
            "metadata_only": true,
        }))
    }
}

fn insert_workflow_run_node_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    node: &Value,
) -> Result<(), String> {
    let node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow node missing node_id".to_string())?;
    let task_type = node
        .get("task_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = node
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let profile_id = node.get("profile_id").and_then(Value::as_str);
    conn.execute(
        "INSERT OR REPLACE INTO workflow_run_nodes
         (run_id, node_id, task_type, status, node_json,
          started_at, completed_at, attempt_count, timeout_ms, blocked_reason, leased_at, profile_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            run_id,
            node_id,
            task_type,
            status,
            node.to_string(),
            node.get("started_at").and_then(Value::as_str),
            node.get("completed_at").and_then(Value::as_str),
            node.get("attempt_count")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            node.get("timeout_ms").and_then(Value::as_i64),
            node.get("blocked_reason").and_then(Value::as_str),
            node.get("leased_at").and_then(Value::as_str),
            profile_id,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn insert_workflow_run_edge_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    edge: &Value,
) -> Result<(), String> {
    let edge_id = edge
        .get("edge_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow edge missing edge_id".to_string())?;
    let from_node_id = edge
        .get("from_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing from_node_id"))?;
    let to_node_id = edge
        .get("to_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing to_node_id"))?;
    let edge_type = edge
        .get("edge_type")
        .and_then(Value::as_str)
        .unwrap_or("dependency");
    conn.execute(
        "INSERT OR REPLACE INTO workflow_run_edges
         (run_id, edge_id, from_node_id, to_node_id, edge_type, edge_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            run_id,
            edge_id,
            from_node_id,
            to_node_id,
            edge_type,
            edge.to_string(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn insert_workflow_run_event_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    node_id: Option<&str>,
    event_type: &str,
    actor: &str,
    details: &Value,
    created_at: &str,
) -> Result<Value, String> {
    let sequence = next_sequence(conn, "workflow_run_events", "event_sequence")?;
    let event_id = format!("workflow-event-{sequence:04}");
    conn.execute(
        "INSERT INTO workflow_run_events
         (event_sequence, event_id, run_id, node_id, event_type, actor, created_at, details_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            sequence,
            event_id,
            run_id,
            node_id,
            event_type,
            actor,
            created_at,
            details.to_string(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({
        "event_sequence": sequence,
        "event_id": event_id,
        "run_id": run_id,
        "node_id": node_id,
        "event_type": event_type,
        "actor": actor,
        "created_at": created_at,
        "details": details,
        "metadata_only": true,
    }))
}

fn import_workflow_run_event_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    event: &Value,
) -> Result<(), String> {
    let sequence = next_sequence(conn, "workflow_run_events", "event_sequence")?;
    let event_id = event
        .get("event_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("workflow-event-{sequence:04}"));
    let event_type = event
        .get("event_type")
        .and_then(Value::as_str)
        .unwrap_or("workflow_run.imported");
    let actor = event
        .get("actor")
        .and_then(Value::as_str)
        .unwrap_or("import");
    let created_at = event
        .get("created_at")
        .and_then(Value::as_str)
        .unwrap_or("import");
    let details = event.get("details").cloned().unwrap_or(Value::Null);
    conn.execute(
        "INSERT INTO workflow_run_events
         (event_sequence, event_id, run_id, node_id, event_type, actor, created_at, details_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            sequence,
            event_id,
            run_id,
            event.get("node_id").and_then(Value::as_str),
            event_type,
            actor,
            created_at,
            details.to_string(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn import_workflow_run_approval_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    approval: &Value,
) -> Result<(), String> {
    let sequence = next_sequence(conn, "workflow_run_approvals", "approval_sequence")?;
    let approval_id = approval
        .get("approval_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("workflow-approval-{sequence:04}"));
    let node_id = approval
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow approval {approval_id} missing node_id"))?;
    let decision = approval
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or("requested");
    let actor = approval
        .get("actor")
        .and_then(Value::as_str)
        .unwrap_or("import");
    let created_at = approval
        .get("created_at")
        .and_then(Value::as_str)
        .unwrap_or("import");
    conn.execute(
        "INSERT INTO workflow_run_approvals
         (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
          created_at, approval_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            sequence,
            approval_id,
            run_id,
            node_id,
            decision,
            actor,
            approval.get("reason").and_then(Value::as_str),
            created_at,
            approval.to_string(),
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn update_workflow_run_status_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    status: &str,
    updated_at: &str,
) -> Result<(), String> {
    let started_at_sql = if status == "running" {
        ", started_at = COALESCE(started_at, ?3)"
    } else {
        ""
    };
    let completed_at_sql = if matches!(status, "completed" | "failed" | "cancelled") {
        ", completed_at = ?3"
    } else if status == "running" {
        ", completed_at = NULL"
    } else {
        ""
    };
    let sql = format!(
        "UPDATE workflow_runs SET status = ?1, updated_at = ?2{started_at_sql}{completed_at_sql} WHERE run_id = ?4"
    );
    conn.execute(&sql, params![status, updated_at, updated_at, run_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn workflow_run_summary_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let run_text: String = row.get(13)?;
    let run: Value = serde_json::from_str(&run_text).unwrap_or(Value::Null);
    let boundaries_text: String = row.get(12)?;
    let boundaries: Value =
        serde_json::from_str(&boundaries_text).unwrap_or_else(|_| workflow_run_boundaries());
    let mut value = workflow_run_value(
        row.get::<_, i64>(0)?,
        &row.get::<_, String>(1)?,
        row.get::<_, Option<String>>(2)?.as_deref(),
        &row.get::<_, String>(3)?,
        &row.get::<_, String>(4)?,
        &row.get::<_, String>(5)?,
        &row.get::<_, String>(6)?,
        row.get::<_, Option<String>>(7)?.as_deref(),
        row.get::<_, Option<String>>(8)?.as_deref(),
        row.get::<_, Option<String>>(9)?.as_deref(),
        row.get::<_, Option<String>>(10)?.as_deref(),
        row.get::<_, Option<String>>(11)?.as_deref(),
        &boundaries,
        &run,
    );
    // Overlay queue/priority columns (indices 14-20) if present
    if let Some(obj) = value.as_object_mut() {
        if let Ok(priority) = row.get::<_, i64>(14) {
            obj.insert("priority".to_string(), json!(priority));
        }
        if let Ok(deadline_at) = row.get::<_, Option<String>>(15) {
            obj.insert("deadline_at".to_string(), json!(deadline_at));
        }
        if let Ok(sla_ms) = row.get::<_, Option<i64>>(16) {
            obj.insert("sla_ms".to_string(), json!(sla_ms));
        }
        if let Ok(tenant_id) = row.get::<_, Option<String>>(17) {
            obj.insert("tenant_id".to_string(), json!(tenant_id));
        }
        if let Ok(queue_position) = row.get::<_, Option<i64>>(18) {
            obj.insert("queue_position".to_string(), json!(queue_position));
        }
        if let Ok(pause_reason) = row.get::<_, Option<String>>(19) {
            obj.insert("pause_reason".to_string(), json!(pause_reason));
        }
        if let Ok(degrade_mode) = row.get::<_, Option<String>>(20) {
            obj.insert("degrade_mode".to_string(), json!(degrade_mode));
        }
    }
    Ok(value)
}

fn workflow_run_value(
    sequence: i64,
    run_id: &str,
    plan_id: Option<&str>,
    created_at: &str,
    updated_at: &str,
    status: &str,
    workflow_id: &str,
    dispatch_id: Option<&str>,
    started_at: Option<&str>,
    completed_at: Option<&str>,
    result_json: Option<&str>,
    last_heartbeat_at: Option<&str>,
    boundaries: &Value,
    run: &Value,
) -> Value {
    let mut value = run.clone();
    if !value.is_object() {
        value = json!({});
    }
    let result = result_json.and_then(|raw| serde_json::from_str::<Value>(raw).ok());
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "schema_version".to_string(),
            json!(WORKFLOW_RUN_SCHEMA_VERSION),
        );
        obj.insert("run_sequence".to_string(), json!(sequence));
        obj.insert("run_id".to_string(), json!(run_id));
        obj.insert("plan_id".to_string(), json!(plan_id));
        obj.insert("created_at".to_string(), json!(created_at));
        obj.insert("updated_at".to_string(), json!(updated_at));
        obj.insert("status".to_string(), json!(status));
        obj.insert("workflow_id".to_string(), json!(workflow_id));
        obj.insert("dispatch_id".to_string(), json!(dispatch_id));
        obj.insert("started_at".to_string(), json!(started_at));
        obj.insert("completed_at".to_string(), json!(completed_at));
        obj.insert("last_heartbeat_at".to_string(), json!(last_heartbeat_at));
        obj.insert("result".to_string(), result.unwrap_or(Value::Null));
        obj.insert("boundaries".to_string(), boundaries.clone());
    }
    value
}

fn workflow_run_with_children(
    conn: &rusqlite::Connection,
    mut run: Value,
) -> Result<Value, String> {
    let run_id = run
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow run missing run_id".to_string())?
        .to_string();
    if let Some(obj) = run.as_object_mut() {
        obj.insert(
            "nodes".to_string(),
            Value::Array(workflow_run_nodes_locked(conn, &run_id)?),
        );
        obj.insert(
            "edges".to_string(),
            Value::Array(workflow_run_edges_locked(conn, &run_id)?),
        );
        obj.insert(
            "events".to_string(),
            Value::Array(workflow_run_events_locked(conn, &run_id, 10_000)?),
        );
        obj.insert(
            "approvals".to_string(),
            Value::Array(workflow_run_approvals_locked(conn, &run_id, 10_000)?),
        );
    }
    Ok(run)
}

fn workflow_run_nodes_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT node_json, status, started_at, completed_at, attempt_count,
                    timeout_ms, blocked_reason, leased_at, profile_id
             FROM workflow_run_nodes WHERE run_id = ?1 ORDER BY rowid ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![run_id], |row| {
            let text: String = row.get(0)?;
            let mut node: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if let Some(obj) = node.as_object_mut() {
                obj.insert("db_status".to_string(), json!(row.get::<_, String>(1)?));
                if let Ok(Some(v)) = row.get::<_, Option<String>>(2) {
                    obj.insert("started_at".to_string(), json!(v));
                }
                if let Ok(Some(v)) = row.get::<_, Option<String>>(3) {
                    obj.insert("completed_at".to_string(), json!(v));
                }
                obj.insert("attempt_count".to_string(), json!(row.get::<_, i64>(4)?));
                if let Ok(Some(v)) = row.get::<_, Option<i64>>(5) {
                    obj.insert("timeout_ms".to_string(), json!(v));
                }
                if let Ok(Some(v)) = row.get::<_, Option<String>>(6) {
                    obj.insert("blocked_reason".to_string(), json!(v));
                }
                if let Ok(Some(v)) = row.get::<_, Option<String>>(7) {
                    obj.insert("leased_at".to_string(), json!(v));
                }
                if let Ok(Some(v)) = row.get::<_, Option<String>>(8) {
                    obj.insert("profile_id".to_string(), json!(v));
                }
            }
            Ok(node)
        })
        .map_err(|e| e.to_string())?;
    collect_values(rows)
}

fn workflow_run_edges_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare("SELECT edge_json FROM workflow_run_edges WHERE run_id = ?1 ORDER BY rowid ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![run_id], |row| {
            let text: String = row.get(0)?;
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
        })
        .map_err(|e| e.to_string())?;
    collect_values(rows)
}

fn workflow_run_events_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT event_sequence, event_id, run_id, node_id, event_type, actor, created_at,
                    details_json
             FROM workflow_run_events
             WHERE run_id = ?1
             ORDER BY event_sequence ASC
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![run_id, limit], workflow_run_event_row)
        .map_err(|e| e.to_string())?;
    collect_values(rows)
}

fn workflow_run_event_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let details_text: String = row.get(7)?;
    let details: Value = serde_json::from_str(&details_text).unwrap_or(Value::Null);
    Ok(json!({
        "event_sequence": row.get::<_, i64>(0)?,
        "event_id": row.get::<_, String>(1)?,
        "run_id": row.get::<_, String>(2)?,
        "node_id": row.get::<_, Option<String>>(3)?,
        "event_type": row.get::<_, String>(4)?,
        "actor": row.get::<_, String>(5)?,
        "created_at": row.get::<_, String>(6)?,
        "details": details,
        "metadata_only": true,
    }))
}

fn completed_node_output(node_json: &Value) -> Option<Value> {
    if let Some(output) = node_json.pointer("/result/output") {
        if !output.is_null() {
            return Some(output.clone());
        }
    }
    if let Some(result) = node_json.get("result") {
        if !result.is_null() {
            return Some(result.clone());
        }
    }
    node_json
        .get("output_ref")
        .filter(|value| !value.is_null())
        .cloned()
}

fn workflow_run_approvals_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT approval_json FROM workflow_run_approvals
             WHERE run_id = ?1
             ORDER BY approval_sequence ASC
             LIMIT ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![run_id, limit], |row| {
            let text: String = row.get(0)?;
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
        })
        .map_err(|e| e.to_string())?;
    collect_values(rows)
}

fn ensure_run_exists_locked(conn: &rusqlite::Connection, run_id: &str) -> Result<(), String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM workflow_runs WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if count == 0 {
        Err(format!("workflow run not found: {run_id}"))
    } else {
        Ok(())
    }
}

fn next_sequence(conn: &rusqlite::Connection, table: &str, column: &str) -> Result<i64, String> {
    let sql = format!("SELECT COALESCE(MAX({column}), 0) + 1 FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

fn required_object<'a>(value: &'a Value, field: &str) -> Result<&'a Value, String> {
    let value = value.get(field).ok_or_else(|| format!("missing {field}"))?;
    if value.is_object() {
        Ok(value)
    } else {
        Err(format!("{field} must be an object"))
    }
}

fn null_if_empty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn optional_json_text(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::Null) | None => None,
        Some(value) => Some(value.to_string()),
    }
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn find_ready_node_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<Option<String>, String> {
    let mut stmt = conn
        .prepare("SELECT node_id FROM workflow_run_nodes WHERE run_id = ?1 AND status = 'pending' ORDER BY node_id")
        .map_err(|e| e.to_string())?;
    let pending_nodes: Vec<String> = stmt
        .query_map(params![run_id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    for node_id in pending_nodes {
        // Check if all predecessor nodes are completed. Recovery/fix nodes are
        // allowed to depend on failed or recovered nodes; ordinary downstream
        // work still requires completed predecessors.
        let target_task_type: String = conn
            .query_row(
                "SELECT task_type FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                params![run_id, node_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut edge_stmt = conn
            .prepare(
                "SELECT wrn.status FROM workflow_run_edges wre
                 JOIN workflow_run_nodes wrn ON wrn.run_id = wre.run_id AND wrn.node_id = wre.from_node_id
                 WHERE wre.run_id = ?1 AND wre.to_node_id = ?2",
            )
            .map_err(|e| e.to_string())?;
        let predecessor_statuses: Vec<String> = edge_stmt
            .query_map(params![run_id, node_id], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        if predecessor_statuses.iter().all(|s| {
            s == "completed"
                || (target_task_type == "fix" && matches!(s.as_str(), "failed" | "recovered"))
        }) {
            return Ok(Some(node_id));
        }
    }
    Ok(None)
}

fn check_run_completion_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<(bool, bool), String> {
    let mut stmt = conn
        .prepare("SELECT status FROM workflow_run_nodes WHERE run_id = ?1")
        .map_err(|e| e.to_string())?;
    let statuses: Vec<String> = stmt
        .query_map(params![run_id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

    if statuses.is_empty() {
        return Ok((true, false));
    }
    let all_done = statuses.iter().all(|s| {
        matches!(
            s.as_str(),
            "completed" | "failed" | "cancelled" | "recovered"
        )
    });
    let has_failure = statuses.iter().any(|s| s == "failed");
    Ok((all_done, has_failure))
}

fn is_run_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled")
}

fn get_run_row(conn: &rusqlite::Connection, run_id: &str) -> Result<Value, String> {
    let mut stmt = conn
        .prepare(
            "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                    workflow_id, dispatch_id, started_at, completed_at, result_json,
                    last_heartbeat_at, boundaries_json, run_json,
                    priority, deadline_at, sla_ms, tenant_id, queue_position,
                    pause_reason, degrade_mode
             FROM workflow_runs WHERE run_id = ?1 LIMIT 1",
        )
        .map_err(|e| e.to_string())?;
    let mut rows = stmt
        .query_map(params![run_id], workflow_run_summary_row)
        .map_err(|e| e.to_string())?;
    let Some(row) = rows.next() else {
        return Err(format!("workflow run not found: {run_id}"));
    };
    row.map_err(|e| e.to_string())
}

pub fn workflow_run_boundaries() -> Value {
    json!({
        "execution_authority": "disabled",
        "target_repository_writes": "disabled",
        "runtime_workers": "disabled",
        "sandbox_process_execution": "not_implemented",
        "provider_calls": "not_invoked",
        "approval_execution_authority": "disabled",
        "resume_execution_authority": "disabled",
        "cancel_execution_authority": "disabled",
        "deploy_merge_controls": "not_available",
    })
}

// ---------------------------------------------------------------------------
// PostgreSQL helper functions
// ---------------------------------------------------------------------------

#[cfg(feature = "pg")]
fn pg_next_sequence(
    client: &mut impl postgres::GenericClient,
    table: &str,
    column: &str,
) -> Result<i64, String> {
    let sql = format!("SELECT COALESCE(MAX({column}), 0) + 1 FROM {table}");
    let val: i64 = client
        .query_one(&sql, &[])
        .map_err(|e| e.to_string())?
        .get(0);
    Ok(val)
}

#[cfg(feature = "pg")]
fn pg_ensure_run_exists(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<(), String> {
    let count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM workflow_runs WHERE run_id = $1",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?
        .get(0);
    if count == 0 {
        Err(format!("workflow run not found: {run_id}"))
    } else {
        Ok(())
    }
}

#[cfg(feature = "pg")]
fn pg_append_audit(
    client: &mut impl postgres::GenericClient,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    let details_json = details.to_string();
    let params: Vec<&(dyn postgres::types::ToSql + Sync)> =
        vec![&now, &actor, &action, &resource, &details_json];
    client
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
             VALUES ($1, $2, $3, $4, $5)",
            &params,
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_insert_workflow_run_node(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node: &Value,
) -> Result<(), String> {
    let node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow node missing node_id".to_string())?;
    let task_type = node
        .get("task_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let status = node
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending");
    let profile_id = node.get("profile_id").and_then(Value::as_str);
    let node_json = node.to_string();
    let started_at = node.get("started_at").and_then(Value::as_str);
    let completed_at = node.get("completed_at").and_then(Value::as_str);
    let attempt_count: i32 = node
        .get("attempt_count")
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let timeout_ms: Option<i32> = node
        .get("timeout_ms")
        .and_then(Value::as_i64)
        .map(|v| v as i32);
    let blocked_reason = node.get("blocked_reason").and_then(Value::as_str);
    let leased_at = node.get("leased_at").and_then(Value::as_str);
    let params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![
        &run_id,
        &node_id,
        &task_type,
        &status,
        &node_json,
        &started_at,
        &completed_at,
        &attempt_count,
        &timeout_ms,
        &blocked_reason,
        &leased_at,
        &profile_id,
    ];
    client
        .execute(
            "INSERT INTO workflow_run_nodes
             (run_id, node_id, task_type, status, node_json,
              started_at, completed_at, attempt_count, timeout_ms, blocked_reason, leased_at, profile_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             ON CONFLICT (run_id, node_id) DO UPDATE SET
              task_type = EXCLUDED.task_type, status = EXCLUDED.status, node_json = EXCLUDED.node_json,
              started_at = EXCLUDED.started_at, completed_at = EXCLUDED.completed_at,
              attempt_count = EXCLUDED.attempt_count, timeout_ms = EXCLUDED.timeout_ms,
              blocked_reason = EXCLUDED.blocked_reason, leased_at = EXCLUDED.leased_at,
              profile_id = EXCLUDED.profile_id",
            &params,
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_insert_workflow_run_edge(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    edge: &Value,
) -> Result<(), String> {
    let edge_id = edge
        .get("edge_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow edge missing edge_id".to_string())?;
    let from_node_id = edge
        .get("from_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing from_node_id"))?;
    let to_node_id = edge
        .get("to_node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow edge {edge_id} missing to_node_id"))?;
    let edge_type = edge
        .get("edge_type")
        .and_then(Value::as_str)
        .unwrap_or("dependency");
    let edge_json = edge.to_string();
    let params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![
        &run_id,
        &edge_id,
        &from_node_id,
        &to_node_id,
        &edge_type,
        &edge_json,
    ];
    client
        .execute(
            "INSERT INTO workflow_run_edges
             (run_id, edge_id, from_node_id, to_node_id, edge_type, edge_json)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (run_id, edge_id) DO UPDATE SET
              from_node_id = EXCLUDED.from_node_id, to_node_id = EXCLUDED.to_node_id,
              edge_type = EXCLUDED.edge_type, edge_json = EXCLUDED.edge_json",
            &params,
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_next_event_sequence(client: &mut impl postgres::GenericClient) -> Result<i64, String> {
    pg_next_sequence(client, "workflow_run_events", "event_sequence")
}

#[cfg(feature = "pg")]
fn pg_insert_workflow_run_event(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node_id: Option<&str>,
    event_type: &str,
    actor: &str,
    details: &Value,
    created_at: &str,
) -> Result<Value, String> {
    let sequence = pg_next_event_sequence(client)?;
    let event_id = format!("workflow-event-{sequence:04}");
    let details_json = details.to_string();
    let params: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![
        &sequence,
        &event_id,
        &run_id,
        &node_id,
        &event_type,
        &actor,
        &created_at,
        &details_json,
    ];
    client
        .execute(
            "INSERT INTO workflow_run_events
             (event_sequence, event_id, run_id, node_id, event_type, actor, created_at, details_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &params,
        )
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "event_sequence": sequence,
        "event_id": event_id,
        "run_id": run_id,
        "node_id": node_id,
        "event_type": event_type,
        "actor": actor,
        "created_at": created_at,
        "details": details,
        "metadata_only": true,
    }))
}

#[cfg(feature = "pg")]
fn pg_import_workflow_run_event(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    event: &Value,
) -> Result<(), String> {
    let sequence = pg_next_event_sequence(client)?;
    let event_id = event
        .get("event_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("workflow-event-{sequence:04}"));
    let event_type = event
        .get("event_type")
        .and_then(Value::as_str)
        .unwrap_or("workflow_run.imported");
    let actor = event
        .get("actor")
        .and_then(Value::as_str)
        .unwrap_or("import");
    let created_at = event
        .get("created_at")
        .and_then(Value::as_str)
        .unwrap_or("import");
    let details = event.get("details").cloned().unwrap_or(Value::Null);
    client
        .execute(
            "INSERT INTO workflow_run_events
             (event_sequence, event_id, run_id, node_id, event_type, actor, created_at, details_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            &[
                &sequence,
                &event_id,
                &run_id,
                &event.get("node_id").and_then(Value::as_str),
                &event_type,
                &actor,
                &created_at,
                &details.to_string(),
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_import_workflow_run_approval(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    approval: &Value,
) -> Result<(), String> {
    let sequence = pg_next_sequence(client, "workflow_run_approvals", "approval_sequence")?;
    let approval_id = approval
        .get("approval_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("workflow-approval-{sequence:04}"));
    let node_id = approval
        .get("node_id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("workflow approval {approval_id} missing node_id"))?;
    let decision = approval
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or("requested");
    let actor = approval
        .get("actor")
        .and_then(Value::as_str)
        .unwrap_or("import");
    let created_at = approval
        .get("created_at")
        .and_then(Value::as_str)
        .unwrap_or("import");
    client
        .execute(
            "INSERT INTO workflow_run_approvals
             (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
              created_at, approval_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            &[
                &sequence,
                &approval_id,
                &run_id,
                &node_id,
                &decision,
                &actor,
                &approval.get("reason").and_then(Value::as_str),
                &created_at,
                &approval.to_string(),
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_update_workflow_run_status(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    status: &str,
    updated_at: &str,
) -> Result<(), String> {
    let started_at_sql = if status == "running" {
        ", started_at = COALESCE(started_at, $3)"
    } else {
        ""
    };
    let completed_at_sql = if matches!(status, "completed" | "failed" | "cancelled") {
        ", completed_at = $3"
    } else if status == "running" {
        ", completed_at = NULL"
    } else {
        ""
    };
    let sql = format!(
        "UPDATE workflow_runs SET status = $1, updated_at = $2{started_at_sql}{completed_at_sql} WHERE run_id = $4"
    );
    client
        .execute(&sql, &[&status, &updated_at, &updated_at, &run_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_find_ready_node(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<Option<String>, String> {
    let rows = client
        .query(
            "SELECT node_id FROM workflow_run_nodes WHERE run_id = $1 AND status = 'pending' ORDER BY node_id",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?;
    let pending_nodes: Vec<String> = rows.iter().map(|r| r.get(0)).collect();

    for node_id in pending_nodes {
        let target_task_type: String = client
            .query_one(
                "SELECT task_type FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                &[&run_id, &node_id],
            )
            .map_err(|e| e.to_string())?
            .get(0);
        let edge_rows = client
            .query(
                "SELECT wrn.status FROM workflow_run_edges wre
                 JOIN workflow_run_nodes wrn ON wrn.run_id = wre.run_id AND wrn.node_id = wre.from_node_id
                 WHERE wre.run_id = $1 AND wre.to_node_id = $2",
                &[&run_id, &node_id],
            )
            .map_err(|e| e.to_string())?;
        let predecessor_statuses: Vec<String> = edge_rows.iter().map(|r| r.get(0)).collect();

        if predecessor_statuses.iter().all(|s| {
            s == "completed"
                || (target_task_type == "fix" && matches!(s.as_str(), "failed" | "recovered"))
        }) {
            return Ok(Some(node_id));
        }
    }
    Ok(None)
}

#[cfg(feature = "pg")]
fn pg_check_run_completion(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<(bool, bool), String> {
    let rows = client
        .query(
            "SELECT status FROM workflow_run_nodes WHERE run_id = $1",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?;
    let statuses: Vec<String> = rows.iter().map(|r| r.get(0)).collect();

    if statuses.is_empty() {
        return Ok((true, false));
    }
    let all_done = statuses.iter().all(|s| {
        matches!(
            s.as_str(),
            "completed" | "failed" | "cancelled" | "recovered"
        )
    });
    let has_failure = statuses.iter().any(|s| s == "failed");
    Ok((all_done, has_failure))
}

#[cfg(feature = "pg")]
fn pg_workflow_run_summary_row(row: &postgres::Row) -> Value {
    let run_text: String = row.get(13);
    let run: Value = serde_json::from_str(&run_text).unwrap_or(Value::Null);
    let boundaries_text: String = row.get(12);
    let boundaries: Value =
        serde_json::from_str(&boundaries_text).unwrap_or_else(|_| workflow_run_boundaries());
    let run_seq: i64 = row.get(0);
    let priority: i32 = row.get(14);
    let sla_ms: Option<i32> = row.get(16);
    let queue_pos: Option<i32> = row.get(18);
    let mut value = workflow_run_value(
        run_seq,
        &row.get::<_, String>(1),
        row.get::<_, Option<String>>(2).as_deref(),
        &row.get::<_, String>(3),
        &row.get::<_, String>(4),
        &row.get::<_, String>(5),
        &row.get::<_, String>(6),
        row.get::<_, Option<String>>(7).as_deref(),
        row.get::<_, Option<String>>(8).as_deref(),
        row.get::<_, Option<String>>(9).as_deref(),
        row.get::<_, Option<String>>(10).as_deref(),
        row.get::<_, Option<String>>(11).as_deref(),
        &boundaries,
        &run,
    );
    if let Some(obj) = value.as_object_mut() {
        obj.insert("priority".to_string(), json!(priority as i64));
        obj.insert(
            "deadline_at".to_string(),
            json!(row.get::<_, Option<String>>(15)),
        );
        obj.insert("sla_ms".to_string(), json!(sla_ms.map(|v| v as i64)));
        obj.insert(
            "tenant_id".to_string(),
            json!(row.get::<_, Option<String>>(17)),
        );
        obj.insert(
            "queue_position".to_string(),
            json!(queue_pos.map(|v| v as i64)),
        );
        obj.insert(
            "pause_reason".to_string(),
            json!(row.get::<_, Option<String>>(19)),
        );
        obj.insert(
            "degrade_mode".to_string(),
            json!(row.get::<_, Option<String>>(20)),
        );
    }
    value
}

#[cfg(feature = "pg")]
fn pg_collect_workflow_run_summaries(rows: Vec<postgres::Row>) -> Result<Vec<Value>, String> {
    Ok(rows.iter().map(pg_workflow_run_summary_row).collect())
}

#[cfg(feature = "pg")]
fn pg_workflow_run_with_children(
    client: &mut postgres::Client,
    mut run: Value,
) -> Result<Value, String> {
    let run_id = run
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workflow run missing run_id".to_string())?
        .to_string();
    if let Some(obj) = run.as_object_mut() {
        obj.insert(
            "nodes".to_string(),
            Value::Array(pg_workflow_run_nodes(client, &run_id)?),
        );
        obj.insert(
            "edges".to_string(),
            Value::Array(pg_workflow_run_edges(client, &run_id)?),
        );
        obj.insert(
            "events".to_string(),
            Value::Array(pg_workflow_run_events(client, &run_id, 10_000)?),
        );
        obj.insert(
            "approvals".to_string(),
            Value::Array(pg_workflow_run_approvals(client, &run_id, 10_000)?),
        );
    }
    Ok(run)
}

#[cfg(feature = "pg")]
fn pg_workflow_run_nodes(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<Vec<Value>, String> {
    let rows = client
        .query(
            "SELECT node_json, status, started_at, completed_at, attempt_count,
                    timeout_ms, blocked_reason, leased_at, profile_id
             FROM workflow_run_nodes WHERE run_id = $1 ORDER BY ctid ASC",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?;
    let values: Vec<Value> = rows
        .iter()
        .map(|row| {
            let text: String = row.get(0);
            let mut node: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            if let Some(obj) = node.as_object_mut() {
                obj.insert("db_status".to_string(), json!(row.get::<_, String>(1)));
                if let Some(v) = row.get::<_, Option<String>>(2) {
                    obj.insert("started_at".to_string(), json!(v));
                }
                if let Some(v) = row.get::<_, Option<String>>(3) {
                    obj.insert("completed_at".to_string(), json!(v));
                }
                let attempt: i32 = row.get(4);
                obj.insert("attempt_count".to_string(), json!(attempt as i64));
                let timeout: Option<i32> = row.get(5);
                if let Some(v) = timeout {
                    obj.insert("timeout_ms".to_string(), json!(v as i64));
                }
                if let Some(v) = row.get::<_, Option<String>>(6) {
                    obj.insert("blocked_reason".to_string(), json!(v));
                }
                if let Some(v) = row.get::<_, Option<String>>(7) {
                    obj.insert("leased_at".to_string(), json!(v));
                }
                if let Some(v) = row.get::<_, Option<String>>(8) {
                    obj.insert("profile_id".to_string(), json!(v));
                }
            }
            node
        })
        .collect();
    Ok(values)
}

#[cfg(feature = "pg")]
fn pg_workflow_run_edges(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<Vec<Value>, String> {
    let rows = client
        .query(
            "SELECT edge_json FROM workflow_run_edges WHERE run_id = $1 ORDER BY ctid ASC",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?;
    let values: Vec<Value> = rows
        .iter()
        .map(|row| {
            let text: String = row.get(0);
            serde_json::from_str(&text).unwrap_or(Value::Null)
        })
        .collect();
    Ok(values)
}

#[cfg(feature = "pg")]
fn pg_workflow_run_events(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let rows = client
        .query(
            "SELECT event_sequence, event_id, run_id, node_id, event_type, actor, created_at,
                    details_json
             FROM workflow_run_events
             WHERE run_id = $1
             ORDER BY event_sequence ASC
             LIMIT $2",
            &[&run_id, &limit],
        )
        .map_err(|e| e.to_string())?;
    let values: Vec<Value> = rows
        .iter()
        .map(|row| {
            let details_text: String = row.get(7);
            let details: Value = serde_json::from_str(&details_text).unwrap_or(Value::Null);
            json!({
                "event_sequence": row.get::<_, i64>(0),
                "event_id": row.get::<_, String>(1),
                "run_id": row.get::<_, String>(2),
                "node_id": row.get::<_, Option<String>>(3),
                "event_type": row.get::<_, String>(4),
                "actor": row.get::<_, String>(5),
                "created_at": row.get::<_, String>(6),
                "details": details,
                "metadata_only": true,
            })
        })
        .collect();
    Ok(values)
}

#[cfg(feature = "pg")]
fn pg_workflow_run_approvals(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    limit: i64,
) -> Result<Vec<Value>, String> {
    let rows = client
        .query(
            "SELECT approval_json FROM workflow_run_approvals
             WHERE run_id = $1
             ORDER BY approval_sequence ASC
             LIMIT $2",
            &[&run_id, &limit],
        )
        .map_err(|e| e.to_string())?;
    let values: Vec<Value> = rows
        .iter()
        .map(|row| {
            let text: String = row.get(0);
            serde_json::from_str(&text).unwrap_or(Value::Null)
        })
        .collect();
    Ok(values)
}

#[cfg(feature = "pg")]
fn pg_get_run_row(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<Value, String> {
    let rows = client
        .query(
            "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                    workflow_id, dispatch_id, started_at, completed_at, result_json,
                    last_heartbeat_at, boundaries_json, run_json,
                    priority, deadline_at, sla_ms, tenant_id, queue_position,
                    pause_reason, degrade_mode
             FROM workflow_runs WHERE run_id = $1 LIMIT 1",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?;
    let Some(row) = rows.into_iter().next() else {
        return Err(format!("workflow run not found: {run_id}"));
    };
    Ok(pg_workflow_run_summary_row(&row))
}
