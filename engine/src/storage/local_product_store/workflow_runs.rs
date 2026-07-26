use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};
use sha2::Digest;

use super::{
    append_audit_locked, collect_values, DatabaseConnection, LocalProductStore,
    MemoryRetrievalRequest, MemoryScope,
};
use crate::agent_memory::build_memory_context_for_node;
use crate::provider::redaction::contains_sensitive_patterns;
use crate::recursive_execution::{
    RecursiveFailureReason, MAX_RECURSIVE_LEASES, MAX_RECURSIVE_RETRIES,
};
use crate::workflow::context_pack::{
    assemble_context_injection_with_bridge, ContextAssemblyConfig, ContextSource,
};
pub(crate) mod dag_mutations;
mod operator_approvals;
mod queue_lease;

use dag_mutations::{insert_workflow_run_edge_locked, insert_workflow_run_node_locked};
#[cfg(feature = "pg")]
use dag_mutations::{pg_insert_workflow_run_edge, pg_insert_workflow_run_node};

pub const WORKFLOW_RUN_SCHEMA_VERSION: &str = "workflow_run.v1";
pub(crate) const API_OWNED_SUPERVISED_PATCH: &str = "api_owned_supervised_patch";
const EXECUTION_OWNER_CONFLICT_PREFIX: &str = "workflow run execution owner conflict";
const MAX_AGENT_OBJECTIVE_BYTES: usize = 4096;

pub(crate) fn require_run_execution_owner(
    run_id: &str,
    current_owner: Option<&str>,
    expected_owner: Option<&str>,
) -> Result<(), String> {
    match (current_owner, expected_owner) {
        (Some(API_OWNED_SUPERVISED_PATCH), Some(API_OWNED_SUPERVISED_PATCH)) => Ok(()),
        (Some(API_OWNED_SUPERVISED_PATCH), _) => Err(format!(
            "{EXECUTION_OWNER_CONFLICT_PREFIX}: {run_id} is owned by the supervised-patch API"
        )),
        (_, Some(API_OWNED_SUPERVISED_PATCH)) => Err(format!(
            "{EXECUTION_OWNER_CONFLICT_PREFIX}: {run_id} is no longer owned by the supervised-patch API"
        )),
        _ => Ok(()),
    }
}

pub(crate) fn is_execution_owner_conflict(error: &str) -> bool {
    error.starts_with(EXECUTION_OWNER_CONFLICT_PREFIX)
}

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
        self.create_workflow_run_from_plan_scoped(plan_id, actor, "local", "local")
    }

    pub fn create_workflow_run_from_plan_scoped(
        &self,
        plan_id: &str,
        actor: &str,
        tenant_id: &str,
        workspace_id: &str,
    ) -> Result<Value, String> {
        if !valid_scope_identifier(tenant_id) || !valid_scope_identifier(workspace_id) {
            return Err("workflow run tenant/workspace scope is invalid".to_string());
        }
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
        let executable = workflow_plan_has_execution_authority(&plan);
        let mut run_boundaries = workflow_run_boundaries_for_plan(&plan)?;
        let boundaries_object = run_boundaries
            .as_object_mut()
            .ok_or_else(|| "workflow run boundaries must be an object".to_string())?;
        boundaries_object.insert("tenant_id".to_string(), json!(tenant_id));
        boundaries_object.insert("workspace_id".to_string(), json!(workspace_id));
        let execution_authority = run_boundaries
            .get("execution_authority")
            .and_then(Value::as_str)
            .unwrap_or("disabled")
            .to_string();

        let run_id = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let sequence = next_sequence(&tx, "workflow_runs", "run_sequence")?;
                let run_id = format!("run-{sequence:04}");
                let created_at = self.now();
                let boundaries = run_boundaries.clone();
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
                    "tenant_id": tenant_id,
                    "workspace_id": workspace_id,
                });
                tx.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status, workflow_id,
                      dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json,
                      priority, deadline_at, sla_ms, tenant_id, queue_position, pause_reason, degrade_mode)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, NULL, ?9, ?10,
                             5, NULL, NULL, ?11, NULL, NULL, NULL)",
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
                        tenant_id,
                    ],
                )
                .map_err(|e| e.to_string())?;

                for node in nodes {
                    insert_workflow_run_node_locked(&tx, &run_id, node)?;
                    insert_agent_state_for_node_sqlite(&tx, &run_id, node, &created_at, actor)?;
                }
                for edge in edges {
                    insert_workflow_run_edge_locked(&tx, &run_id, edge)?;
                }
                insert_workflow_run_event_locked(
                    &tx,
                    &run_id,
                    None,
                    "workflow_run.created",
                    actor,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "metadata_only": !executable,
                        "execution_authority": execution_authority,
                        "tenant_id": tenant_id,
                        "workspace_id": workspace_id,
                    }),
                    &created_at,
                )?;
                append_audit_locked(
                    &tx,
                    &created_at,
                    actor,
                    "workflow_run.create",
                    &run_id,
                    &json!({
                        "plan_id": plan_id,
                        "workflow_id": workflow_id,
                        "dispatch_id": dispatch_id,
                        "metadata_only": !executable,
                        "execution_authority": execution_authority,
                        "tenant_id": tenant_id,
                        "workspace_id": workspace_id,
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(run_id)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                let sequence: i64 = pg_next_sequence(&mut tx, "workflow_runs", "run_sequence")?;
                let run_id = format!("run-{sequence:04}");
                let created_at = self.now();
                let boundaries = run_boundaries.clone();
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
                    "tenant_id": tenant_id,
                    "workspace_id": workspace_id,
                });
                let boundaries_json = boundaries.to_string();
                let run_json = run.to_string();
                let dispatch_opt = null_if_empty(dispatch_id);
                let status_str = "created";
                let plan_ref: &str = plan_id;
                let wf_ref: &str = workflow_id;
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
                    &tenant_id,
                ];
                tx.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status, workflow_id,
                      dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json,
                      priority, deadline_at, sla_ms, tenant_id, queue_position, pause_reason, degrade_mode)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, NULL, NULL, $9, $10,
                             5, NULL, NULL, $11, NULL, NULL, NULL)",
                    &params,
                )
                .map_err(|e| e.to_string())?;

                for node in nodes {
                    pg_insert_workflow_run_node(&mut tx, &run_id, node)?;
                    insert_agent_state_for_node_pg(
                        &mut tx,
                        &run_id,
                        node,
                        &created_at,
                        actor,
                    )?;
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
                        "metadata_only": !executable,
                        "execution_authority": execution_authority,
                        "tenant_id": tenant_id,
                        "workspace_id": workspace_id,
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
                        "metadata_only": !executable,
                        "execution_authority": execution_authority,
                        "tenant_id": tenant_id,
                        "workspace_id": workspace_id,
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

    /// Persist an independently-authorized product-output approval under the
    /// existing workflow approval owner. This grants output authority only;
    /// it never grants workflow execution authority.
    pub fn record_product_output_approval(
        &self,
        run_id: &str,
        node_id: &str,
        actor: &str,
        binding: &Value,
    ) -> Result<Value, String> {
        if binding.get("schema_version").and_then(Value::as_str)
            != Some("product_output_approval.v1")
        {
            return Err("invalid product output approval binding schema".to_string());
        }
        let created_at = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                ensure_run_exists_locked(&tx, run_id)?;
                validate_product_output_approval_binding_locked(&tx, run_id, node_id, binding)?;
                let sequence = next_sequence(&tx, "workflow_run_approvals", "approval_sequence")?;
                let approval_id = format!("workflow-approval-{sequence:04}");
                let mut approval = binding.clone();
                let object = approval.as_object_mut().ok_or_else(|| {
                    "product output approval binding must be an object".to_string()
                })?;
                object.insert("approval_sequence".to_string(), json!(sequence));
                object.insert("approval_id".to_string(), json!(approval_id));
                object.insert("run_id".to_string(), json!(run_id));
                object.insert("node_id".to_string(), json!(node_id));
                object.insert("decision".to_string(), json!("approved"));
                object.insert("actor".to_string(), json!(actor));
                object.insert("approved_by".to_string(), json!(actor));
                object.insert("created_at".to_string(), json!(created_at));
                object.insert("approval_kind".to_string(), json!("product_output"));
                object.insert("metadata_only".to_string(), json!(false));
                object.insert("output_authority".to_string(), json!("product_output"));
                object.insert("execution_authority".to_string(), json!("disabled"));
                tx.execute(
                    "INSERT INTO workflow_run_approvals
                     (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
                      created_at, approval_json)
                     VALUES (?1, ?2, ?3, ?4, 'approved', ?5, ?6, ?7, ?8)",
                    params![
                        sequence,
                        approval_id,
                        run_id,
                        node_id,
                        actor,
                        "independent product output approval",
                        created_at,
                        approval.to_string(),
                    ],
                )
                .map_err(|error| error.to_string())?;
                append_audit_locked(
                    &tx,
                    &created_at,
                    actor,
                    "product_task.output_approval_recorded",
                    binding
                        .get("product_task_id")
                        .and_then(Value::as_str)
                        .unwrap_or(run_id),
                    &json!({
                        "approval_id": approval_id,
                        "run_id": run_id,
                        "node_id": node_id,
                        "artifact_id": binding.get("artifact_id"),
                        "verification_sha256": binding.get("verification_sha256"),
                        "output_intent": binding.get("output_intent"),
                        "output_authority": "product_output",
                        "execution_authority": "disabled",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(approval)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                pg_ensure_run_exists(&mut tx, run_id)?;
                pg_validate_product_output_approval_binding(&mut tx, run_id, node_id, binding)?;
                let sequence =
                    pg_next_sequence(&mut tx, "workflow_run_approvals", "approval_sequence")?;
                let approval_id = format!("workflow-approval-{sequence:04}");
                let mut approval = binding.clone();
                let object = approval.as_object_mut().ok_or_else(|| {
                    "product output approval binding must be an object".to_string()
                })?;
                object.insert("approval_sequence".to_string(), json!(sequence));
                object.insert("approval_id".to_string(), json!(approval_id));
                object.insert("run_id".to_string(), json!(run_id));
                object.insert("node_id".to_string(), json!(node_id));
                object.insert("decision".to_string(), json!("approved"));
                object.insert("actor".to_string(), json!(actor));
                object.insert("approved_by".to_string(), json!(actor));
                object.insert("created_at".to_string(), json!(created_at));
                object.insert("approval_kind".to_string(), json!("product_output"));
                object.insert("metadata_only".to_string(), json!(false));
                object.insert("output_authority".to_string(), json!("product_output"));
                object.insert("execution_authority".to_string(), json!("disabled"));
                tx.execute(
                    "INSERT INTO workflow_run_approvals
                     (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
                      created_at, approval_json)
                     VALUES ($1, $2, $3, $4, 'approved', $5, $6, $7, $8)",
                    &[
                        &sequence,
                        &approval_id,
                        &run_id,
                        &node_id,
                        &actor,
                        &Some("independent product output approval"),
                        &created_at,
                        &approval.to_string(),
                    ],
                )
                .map_err(|error| error.to_string())?;
                pg_append_audit(
                    &mut tx,
                    &created_at,
                    actor,
                    "product_task.output_approval_recorded",
                    binding
                        .get("product_task_id")
                        .and_then(Value::as_str)
                        .unwrap_or(run_id),
                    &json!({
                        "approval_id": approval_id,
                        "run_id": run_id,
                        "node_id": node_id,
                        "artifact_id": binding.get("artifact_id"),
                        "verification_sha256": binding.get("verification_sha256"),
                        "output_intent": binding.get("output_intent"),
                        "output_authority": "product_output",
                        "execution_authority": "disabled",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
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
        self.tick_with_executor_and_command_inner(run_id, actor, max_retries, executor, None, None)
    }

    pub(crate) fn tick_managed_supervised_patch_with_executor(
        &self,
        run_id: &str,
        actor: &str,
        executor: &dyn crate::node_executor::NodeExecutor,
    ) -> Result<Value, String> {
        self.tick_with_executor_and_command_inner_for_owner(
            run_id,
            actor,
            0,
            executor,
            None,
            None,
            Some(API_OWNED_SUPERVISED_PATCH),
        )
    }

    /// Like tick_with_executor, but enforces agent concurrency caps inside the lease lock.
    pub fn tick_with_executor_with_agent_caps(
        &self,
        run_id: &str,
        actor: &str,
        max_retries: i64,
        executor: &dyn crate::node_executor::NodeExecutor,
        agent_global_cap: usize,
        agent_per_run_cap: usize,
    ) -> Result<Value, String> {
        self.tick_with_executor_and_command_inner(
            run_id,
            actor,
            max_retries,
            executor,
            None,
            Some((agent_global_cap, agent_per_run_cap)),
        )
    }

    pub fn tick_with_executor_and_command(
        &self,
        run_id: &str,
        actor: &str,
        max_retries: i64,
        executor: &dyn crate::node_executor::NodeExecutor,
        command_override: Option<&str>,
    ) -> Result<Value, String> {
        self.tick_with_executor_and_command_inner(
            run_id,
            actor,
            max_retries,
            executor,
            command_override,
            None,
        )
    }

    /// Internal version with optional agent concurrency caps.
    /// When caps are Some((global_cap, per_run_cap)), agent_step nodes will not be
    /// leased if the number of running agent_step nodes meets or exceeds either cap.
    /// The check is race-condition-free (inside the SQLite/transaction lock).
    pub fn tick_with_executor_and_command_inner(
        &self,
        run_id: &str,
        actor: &str,
        max_retries: i64,
        executor: &dyn crate::node_executor::NodeExecutor,
        command_override: Option<&str>,
        agent_concurrency_caps: Option<(usize, usize)>,
    ) -> Result<Value, String> {
        self.tick_with_executor_and_command_inner_for_owner(
            run_id,
            actor,
            max_retries,
            executor,
            command_override,
            agent_concurrency_caps,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn tick_with_executor_and_command_inner_for_owner(
        &self,
        run_id: &str,
        actor: &str,
        max_retries: i64,
        executor: &dyn crate::node_executor::NodeExecutor,
        command_override: Option<&str>,
        agent_concurrency_caps: Option<(usize, usize)>,
        expected_execution_owner: Option<&str>,
    ) -> Result<Value, String> {
        let agent_executor = executor.executor_type_name() == "agent_step";
        let recursive_usage_mode = executor.recursive_usage_mode();
        // Phase 1: Lease a ready node. SQLite needs an immediate transaction here:
        // the in-process mutex only protects one LocalProductStore connection, while
        // production restarts and concurrent API owners can open the same database
        // through independent connections.
        let leased = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
                    .map_err(|error| error.to_string())?;
                let lease_result = (|| {
                    ensure_run_exists_locked(conn, run_id)?;

                let (run_status, execution_owner): (String, Option<String>) = conn
                    .query_row(
                        "SELECT status, pause_reason FROM workflow_runs WHERE run_id = ?1",
                        params![run_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .map_err(|e| e.to_string())?;

                require_run_execution_owner(
                    run_id,
                    execution_owner.as_deref(),
                    expected_execution_owner,
                )?;

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

                // Find a ready node, skipping agent_step nodes that are at concurrency cap
                let mut capped_skip: Vec<String> = Vec::new();
                let mut found_is_agent_step = false;
                let node_id = loop {
                    let capped_skip_refs = capped_skip
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>();
                    let matching_node = find_ready_node_locked(
                        conn,
                        run_id,
                        &capped_skip_refs,
                        Some(agent_executor),
                    )?;
                    let candidate = if matching_node.is_none() && capped_skip.is_empty() {
                        find_ready_node_locked(conn, run_id, &capped_skip_refs, None)?
                    } else {
                        matching_node
                    };
                    let Some(nid) = candidate else {
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
                            let terminal_audit_details =
                                workflow_run_terminal_audit_details_locked(conn, run_id)?;
                            append_audit_locked(
                                conn,
                                &now,
                                actor,
                                &format!("workflow_run.{terminal_status}"),
                                run_id,
                                &terminal_audit_details,
                            )?;
                            let run = get_run_row(conn, run_id)?;
                            return Ok(LeaseResult::Terminal { action: terminal_status.to_string(), run });
                        }
                        let run = get_run_row(conn, run_id)?;
                        return Ok(LeaseResult::NoReadyNode { run });
                    };

                    if agent_concurrency_caps.is_none() {
                        let node_task_type: String = conn
                            .query_row(
                                "SELECT task_type FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                                params![run_id, nid],
                                |row| row.get(0),
                            )
                            .unwrap_or_default();
                        if crate::recursive_execution::recursive_enabled()
                            && node_task_type == "agent_step"
                            && workflow_node_is_recursive_locked(conn, run_id, &nid)?
                        {
                            let recursive_running = count_running_recursive_steps_locked(conn)?;
                            if recursive_running >= MAX_RECURSIVE_LEASES as i64 {
                                append_audit_locked(
                                    conn,
                                    &now,
                                    "scheduler",
                                    "recursive.claim_conflict",
                                    &nid,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": nid,
                                        "reason": "scheduler_capacity_exhausted",
                                        "running": recursive_running,
                                        "cap": MAX_RECURSIVE_LEASES,
                                    }),
                                )?;
                                capped_skip.push(nid);
                                continue;
                            }
                        }
                    }

                    // Check agent concurrency caps; skip capped agent_step nodes
                    if let Some((global_cap, per_run_cap)) = agent_concurrency_caps {
                        let node_task_type: String = conn
                            .query_row(
                                "SELECT task_type FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                                params![run_id, nid],
                                |row| row.get(0),
                            )
                            .unwrap_or_default();
                        if node_task_type == "agent_step" {
                            let is_recursive = crate::recursive_execution::recursive_enabled()
                                && workflow_node_is_recursive_locked(conn, run_id, &nid)?;
                            if is_recursive {
                                let recursive_running = count_running_recursive_steps_locked(conn)?;
                                if recursive_running >= MAX_RECURSIVE_LEASES as i64 {
                                    append_audit_locked(
                                        conn,
                                        &now,
                                        "scheduler",
                                        "recursive.claim_conflict",
                                        &nid,
                                        &json!({
                                            "run_id": run_id,
                                            "node_id": nid,
                                            "reason": "scheduler_capacity_exhausted",
                                            "running": recursive_running,
                                            "cap": MAX_RECURSIVE_LEASES,
                                        }),
                                    )?;
                                    capped_skip.push(nid);
                                    continue;
                                }
                            }
                            let global_running = count_running_agent_steps_locked(conn)?;
                            let per_run_running = count_running_agent_steps_for_run_locked(conn, run_id)?;
                            if global_running >= global_cap as i64 {
                                append_audit_locked(
                                    conn,
                                    &now,
                                    "scheduler",
                                    "agent_step.claim_conflict",
                                    &nid,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": nid,
                                        "reason": "global_cap_exceeded",
                                        "running": global_running,
                                        "cap": global_cap,
                                        "per_run_running": per_run_running,
                                    }),
                                )?;
                                capped_skip.push(nid);
                                continue;
                            }
                            if per_run_running >= per_run_cap as i64 {
                                append_audit_locked(
                                    conn,
                                    &now,
                                    "scheduler",
                                    "agent_step.claim_conflict",
                                    &nid,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": nid,
                                        "reason": "per_run_cap_exceeded",
                                        "running": per_run_running,
                                        "cap": per_run_cap,
                                        "global_running": global_running,
                                    }),
                                )?;
                                capped_skip.push(nid);
                                continue;
                            }
                            found_is_agent_step = true;
                            // Claim attempt audit
                            append_audit_locked(
                                conn,
                                &now,
                                "scheduler",
                                "agent_step.claim_attempt",
                                &nid,
                                &json!({
                                    "run_id": run_id,
                                    "node_id": nid,
                                    "global_running": global_running,
                                    "per_run_running": per_run_running,
                                }),
                            )?;
                        }
                    }
                    break nid;
                };

                let now = self.now();
                let recursive_claim = agent_concurrency_caps.is_none()
                    && crate::recursive_execution::recursive_enabled()
                    && conn
                        .query_row(
                            "SELECT task_type FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                            params![run_id, node_id],
                            |row| row.get::<_, String>(0),
                        )
                        .map(|task_type| task_type == "agent_step")
                        .unwrap_or(false)
                    && workflow_node_is_recursive_locked(conn, run_id, &node_id)?;
                let updated = if found_is_agent_step {
                    let (global_cap, per_run_cap) = agent_concurrency_caps
                        .ok_or_else(|| "agent concurrency caps are required".to_string())?;
                    conn.execute(
                        "UPDATE workflow_run_nodes
                         SET status = 'running', started_at = ?1, leased_at = ?1,
                             attempt_count = attempt_count + 1
                         WHERE run_id = ?2 AND node_id = ?3 AND status = 'pending'
                           AND (SELECT COUNT(*) FROM workflow_run_nodes
                                WHERE task_type='agent_step' AND status='running') < ?4
                           AND (SELECT COUNT(*) FROM workflow_run_nodes
                                WHERE run_id=?2 AND task_type='agent_step' AND status='running') < ?5",
                        params![now, run_id, node_id, global_cap as i64, per_run_cap as i64],
                    )
                    .map_err(|e| e.to_string())?
                } else if recursive_claim {
                    conn.execute(
                        "UPDATE workflow_run_nodes
                         SET status = 'running', started_at = ?1, leased_at = ?1,
                             attempt_count = attempt_count + 1
                         WHERE run_id = ?2 AND node_id = ?3 AND status = 'pending'
                           AND (SELECT COUNT(*) FROM workflow_run_nodes
                                WHERE task_type='agent_step' AND status='running'
                                  AND (EXISTS (
                                        SELECT 1 FROM recursive_execution_nodes rn
                                        WHERE rn.node_id=workflow_run_nodes.node_id
                                          AND rn.root_run_id=workflow_run_nodes.run_id)
                                    OR json_extract(node_json, '$.recursive_node_id') IS NOT NULL
                                    OR json_extract(node_json, '$.recursive_root_node_id') IS NOT NULL)) < ?4",
                        params![now, run_id, node_id, MAX_RECURSIVE_LEASES as i64],
                    )
                    .map_err(|e| e.to_string())?
                } else {
                    conn.execute(
                        "UPDATE workflow_run_nodes SET status = 'running', started_at = ?1, leased_at = ?1, attempt_count = attempt_count + 1
                         WHERE run_id = ?2 AND node_id = ?3 AND status = 'pending'",
                        params![now, run_id, node_id],
                    )
                    .map_err(|e| e.to_string())?
                };
                if updated == 0 {
                    if found_is_agent_step {
                        append_audit_locked(
                            conn,
                            &now,
                            "scheduler",
                            "agent_step.claim_conflict",
                            &node_id,
                            &json!({
                                "run_id": run_id,
                                "node_id": node_id,
                                "reason": "lease_lost",
                                "metadata_only": true,
                            }),
                        )?;
                    }
                    return Ok(LeaseResult::NoReadyNode {
                        run: get_run_row(conn, run_id)?,
                    });
                }

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

                let node_metadata_text: String = conn
                    .query_row(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                        params![run_id, node_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let node_metadata: Value = serde_json::from_str(&node_metadata_text)
                    .map_err(|_| "recursive_node_identity_malformed".to_string())?;
                if !node_metadata.is_object() {
                    return Err("recursive_node_identity_malformed".to_string());
                }

                let recursive_node_id = if let Some(recursive_marker) = node_metadata
                    .get("recursive_node_id")
                    .and_then(Value::as_str)
                {
                    if recursive_marker != node_id {
                        return Err("recursive_node_identity_malformed".to_string());
                    }
                    Some(recursive_marker.to_string())
                } else if let Some(root_id) = node_metadata
                    .get("recursive_root_node_id")
                    .and_then(Value::as_str)
                {
                    if crate::recursive_execution::recursive_enabled() {
                        if root_id != node_id {
                            return Err("recursive_node_identity_malformed".to_string());
                        }
                        super::recursive_execution::ensure_recursive_root_tree_sqlite(
                            conn,
                            run_id,
                            &workflow_id,
                            &node_id,
                            &node_metadata,
                            &now,
                        )?;
                        Some(root_id.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(recursive_node_id) = recursive_node_id.as_deref() {
                    super::recursive_execution::sync_recursive_lease_sqlite(
                        conn,
                        run_id,
                        recursive_node_id,
                        &format!("workflow:{run_id}:{node_id}:{attempt}"),
                        &now,
                    )?;
                }

                insert_workflow_run_event_locked(
                    conn,
                    run_id,
                    Some(&node_id),
                    "node.leased",
                    actor,
                    &json!({"node_id": node_id, "status": "running", "attempt": attempt}),
                    &now,
                )?;

                if task_type == "agent_step" {
                    append_audit_locked(
                        conn,
                        &now,
                        "scheduler",
                        "agent_step.claim_success",
                        &node_id,
                        &json!({
                            "run_id": run_id,
                            "node_id": node_id,
                            "attempt": attempt,
                        }),
                    )?;
                }

                    Ok(LeaseResult::Leased {
                        node_id,
                        task_type,
                        workflow_id,
                        attempt,
                        node_metadata,
                    })
                })();
                match lease_result {
                    Ok(leased) => {
                        conn.execute_batch("COMMIT")
                            .map_err(|error| error.to_string())?;
                        Ok(leased)
                    }
                    Err(error) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Err(error)
                    }
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;

                pg_ensure_run_exists(&mut tx, run_id)?;

                let run_row = tx
                    .query_one(
                        "SELECT status, pause_reason FROM workflow_runs WHERE run_id = $1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|e| e.to_string())?;
                let run_status: String = run_row.get(0);
                let execution_owner: Option<String> = run_row.get(1);

                require_run_execution_owner(
                    run_id,
                    execution_owner.as_deref(),
                    expected_execution_owner,
                )?;

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

                // Find a ready node, skipping agent_step nodes that are at concurrency cap (PG branch)
                let mut capped_skip: Vec<String> = Vec::new();
                let mut found_is_agent_step = false;
                let node_id = loop {
                    let capped_skip_refs = capped_skip
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>();
                    let matching_node = pg_find_ready_node(
                        &mut tx,
                        run_id,
                        &capped_skip_refs,
                        Some(agent_executor),
                    )?;
                    let candidate = if matching_node.is_none() && capped_skip.is_empty() {
                        pg_find_ready_node(&mut tx, run_id, &capped_skip_refs, None)?
                    } else {
                        matching_node
                    };
                    let Some(nid) = candidate else {
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
                            let terminal_audit_details =
                                pg_workflow_run_terminal_audit_details(&mut tx, run_id)?;
                            pg_append_audit(
                                &mut tx,
                                &now,
                                actor,
                                &format!("workflow_run.{terminal_status}"),
                                run_id,
                                &terminal_audit_details,
                            )?;
                            let run = pg_get_run_row(&mut tx, run_id)?;
                            tx.commit().map_err(|e| e.to_string())?;
                            return Ok(LeaseResult::Terminal { action: terminal_status.to_string(), run });
                        }
                        let run = pg_get_run_row(&mut tx, run_id)?;
                        tx.commit().map_err(|e| e.to_string())?;
                        return Ok(LeaseResult::NoReadyNode { run });
                    };

                    if agent_concurrency_caps.is_none() {
                        let node_task_type: String = tx
                            .query_one(
                                "SELECT task_type FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                                &[&run_id, &nid],
                            )
                            .map(|r| r.get(0))
                            .unwrap_or_default();
                        if crate::recursive_execution::recursive_enabled()
                            && node_task_type == "agent_step"
                            && pg_workflow_node_is_recursive(&mut tx, run_id, &nid)?
                        {
                            tx.batch_execute("SELECT pg_advisory_xact_lock(734775128237)")
                                .map_err(|error| error.to_string())?;
                            let recursive_running = pg_count_running_recursive_steps(&mut tx)?;
                            if recursive_running >= MAX_RECURSIVE_LEASES as i64 {
                                pg_append_audit(
                                    &mut tx,
                                    &now,
                                    "scheduler",
                                    "recursive.claim_conflict",
                                    &nid,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": nid,
                                        "reason": "scheduler_capacity_exhausted",
                                        "running": recursive_running,
                                        "cap": MAX_RECURSIVE_LEASES,
                                    }),
                                )?;
                                capped_skip.push(nid);
                                continue;
                            }
                        }
                    }

                    // Check agent concurrency caps; skip capped agent_step nodes
                    if let Some((global_cap, per_run_cap)) = agent_concurrency_caps {
                        let node_task_type: String = tx
                            .query_one(
                                "SELECT task_type FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                                &[&run_id, &nid],
                            )
                            .map(|r| r.get(0))
                            .unwrap_or_default();
                        if node_task_type == "agent_step" {
                            tx.batch_execute(
                                "SELECT pg_advisory_xact_lock(734775128237)",
                            )
                            .map_err(|error| error.to_string())?;
                            let is_recursive = crate::recursive_execution::recursive_enabled()
                                && pg_workflow_node_is_recursive(&mut tx, run_id, &nid)?;
                            if is_recursive {
                                let recursive_running = pg_count_running_recursive_steps(&mut tx)?;
                                if recursive_running >= MAX_RECURSIVE_LEASES as i64 {
                                    pg_append_audit(
                                        &mut tx,
                                        &now,
                                        "scheduler",
                                        "recursive.claim_conflict",
                                        &nid,
                                        &json!({
                                            "run_id": run_id,
                                            "node_id": nid,
                                            "reason": "scheduler_capacity_exhausted",
                                            "running": recursive_running,
                                            "cap": MAX_RECURSIVE_LEASES,
                                        }),
                                    )?;
                                    capped_skip.push(nid);
                                    continue;
                                }
                            }
                            let global_running = pg_count_running_agent_steps(&mut tx)?;
                            let per_run_running = pg_count_running_agent_steps_for_run(&mut tx, run_id)?;
                            if global_running >= global_cap as i64 {
                                pg_append_audit(
                                    &mut tx,
                                    &now,
                                    "scheduler",
                                    "agent_step.claim_conflict",
                                    &nid,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": nid,
                                        "reason": "global_cap_exceeded",
                                        "running": global_running,
                                        "cap": global_cap,
                                    }),
                                )?;
                                capped_skip.push(nid);
                                continue;
                            }
                            if per_run_running >= per_run_cap as i64 {
                                pg_append_audit(
                                    &mut tx,
                                    &now,
                                    "scheduler",
                                    "agent_step.claim_conflict",
                                    &nid,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": nid,
                                        "reason": "per_run_cap_exceeded",
                                        "running": per_run_running,
                                        "cap": per_run_cap,
                                    }),
                                )?;
                                capped_skip.push(nid);
                                continue;
                            }
                            found_is_agent_step = true;
                            pg_append_audit(
                                &mut tx,
                                &now,
                                "scheduler",
                                "agent_step.claim_attempt",
                                &nid,
                                &json!({
                                    "run_id": run_id,
                                    "node_id": nid,
                                }),
                            )?;
                        }
                    }
                    break nid;
                };

                let now = self.now();
                let recursive_claim = agent_concurrency_caps.is_none()
                    && crate::recursive_execution::recursive_enabled()
                    && tx
                        .query_one(
                            "SELECT task_type FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                            &[&run_id, &node_id],
                        )
                        .map(|row| row.get::<_, String>(0) == "agent_step")
                        .unwrap_or(false)
                    && pg_workflow_node_is_recursive(&mut tx, run_id, &node_id)?;
                let updated = if recursive_claim {
                    tx.execute(
                        "UPDATE workflow_run_nodes SET status = 'running', started_at = $1, leased_at = $1, attempt_count = attempt_count + 1
                         WHERE run_id = $2 AND node_id = $3 AND status = 'pending'
                           AND (SELECT COUNT(*) FROM workflow_run_nodes
                                WHERE task_type = 'agent_step' AND status = 'running'
                                  AND (EXISTS (
                                        SELECT 1 FROM recursive_execution_nodes rn
                                        WHERE rn.node_id=workflow_run_nodes.node_id
                                          AND rn.root_run_id=workflow_run_nodes.run_id)
                                    OR node_json::jsonb ? 'recursive_node_id'
                                    OR node_json::jsonb ? 'recursive_root_node_id')) < $4",
                        &[&now, &run_id, &node_id, &(MAX_RECURSIVE_LEASES as i64)],
                    )
                    .map_err(|e| e.to_string())?
                } else {
                    tx.execute(
                        "UPDATE workflow_run_nodes SET status = 'running', started_at = $1, leased_at = $1, attempt_count = attempt_count + 1
                         WHERE run_id = $2 AND node_id = $3 AND status = 'pending'",
                        &[&now, &run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?
                };
                if updated == 0 {
                    if found_is_agent_step {
                        pg_append_audit(
                            &mut tx,
                            &now,
                            "scheduler",
                            "agent_step.claim_conflict",
                            &node_id,
                            &json!({
                                "run_id": run_id,
                                "node_id": node_id,
                                "reason": "lease_lost",
                                "metadata_only": true,
                            }),
                        )?;
                    }
                    let run = pg_get_run_row(&mut tx, run_id)?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(LeaseResult::NoReadyNode { run });
                }

                let attempt: i64 = tx
                    .query_one(
                        "SELECT attempt_count FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map(|r| i64::from(r.get::<_, i32>(0)))
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

                let node_metadata_text: String = tx
                    .query_one(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let node_metadata: Value = serde_json::from_str(&node_metadata_text)
                    .map_err(|_| "recursive_node_identity_malformed".to_string())?;
                if !node_metadata.is_object() {
                    return Err("recursive_node_identity_malformed".to_string());
                }

                let recursive_node_id = if let Some(recursive_marker) = node_metadata
                    .get("recursive_node_id")
                    .and_then(Value::as_str)
                {
                    if recursive_marker != node_id {
                        return Err("recursive_node_identity_malformed".to_string());
                    }
                    Some(recursive_marker.to_string())
                } else if let Some(root_id) = node_metadata
                    .get("recursive_root_node_id")
                    .and_then(Value::as_str)
                {
                    if crate::recursive_execution::recursive_enabled() {
                        if root_id != node_id {
                            return Err("recursive_node_identity_malformed".to_string());
                        }
                        super::recursive_execution::ensure_recursive_root_tree_pg(
                            &mut tx,
                            run_id,
                            &workflow_id,
                            &node_id,
                            &node_metadata,
                            &now,
                        )?;
                        Some(root_id.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(recursive_node_id) = recursive_node_id.as_deref() {
                    super::recursive_execution::sync_recursive_lease_pg(
                        &mut tx,
                        run_id,
                        recursive_node_id,
                        &format!("workflow:{run_id}:{node_id}:{attempt}"),
                        &now,
                    )?;
                }

                pg_insert_workflow_run_event(
                    &mut tx,
                    run_id,
                    Some(&node_id),
                    "node.leased",
                    actor,
                    &json!({"node_id": node_id, "status": "running", "attempt": attempt}),
                    &now,
                )?;

                if task_type == "agent_step" {
                    pg_append_audit(
                        &mut tx,
                        &now,
                        "scheduler",
                        "agent_step.claim_success",
                        &node_id,
                        &json!({
                            "run_id": run_id,
                            "node_id": node_id,
                            "attempt": attempt,
                        }),
                    )?;
                }

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
            LeaseResult::Terminal { action, run } => {
                let _artifact = self.record_automatic_native_scorecard_for_run(run_id, actor)?;
                Ok(json!({ "action": action, "run": run }))
            }
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
                if !node_metadata.is_object() {
                    node_metadata = json!({});
                }
                if let Some(obj) = node_metadata.as_object_mut() {
                    obj.insert("execution_attempt".to_string(), json!(attempt));
                    // Owner-derived claim identity for reserved executors (OpenCode, etc.).
                    obj.insert(
                        "scheduler_claim_id".to_string(),
                        json!(format!("workflow:{run_id}:{node_id}:{attempt}")),
                    );
                }
                if command_override.is_none()
                    && task_type != crate::external_runtime::LANGGRAPH_TASK_TYPE
                    && node_metadata.get("prompt").is_none()
                    && node_metadata.get("command").is_none()
                {
                    let exact_product_prompt = if node_metadata
                        .pointer("/managed_supervised_patch/operation")
                        .and_then(Value::as_str)
                        == Some("product_apply")
                    {
                        node_metadata
                            .get("product_task_id")
                            .and_then(Value::as_str)
                            .zip(
                                node_metadata
                                    .get("objective_fingerprint")
                                    .and_then(Value::as_str),
                            )
                            .and_then(|(task_id, fingerprint)| {
                                self.product_execution_objective(task_id, fingerprint).ok()
                            })
                            .map(Value::String)
                    } else {
                        None
                    };
                    let plan_prompt = if exact_product_prompt.is_none() {
                        self.get_workflow_run(run_id)?
                            .and_then(|run| {
                                run.get("plan_id")
                                    .and_then(Value::as_str)
                                    .map(str::to_string)
                            })
                            .map(|plan_id| self.get_workflow_plan(&plan_id))
                            .transpose()?
                            .flatten()
                            .and_then(|plan| plan.get("raw_request").cloned())
                    } else {
                        None
                    };
                    if let Some(prompt) = exact_product_prompt.or(plan_prompt) {
                        if let Some(obj) = node_metadata.as_object_mut() {
                            obj.insert("prompt".to_string(), prompt);
                        }
                    }
                }
                // Inject workspace_path from supervised_patch_workspaces if available
                if task_type != crate::external_runtime::LANGGRAPH_TASK_TYPE {
                    if let Ok(Some(workspace)) = self.get_supervised_patch_workspace_for_run(run_id)
                    {
                        if let Some(ws_path) =
                            workspace.get("workspace_path").and_then(|v| v.as_str())
                        {
                            if let Some(obj) = node_metadata.as_object_mut() {
                                obj.insert("workspace_path".to_string(), json!(ws_path));
                            }
                        }
                    }
                }
                if let Some(context_injection) =
                    self.context_injection_for_node(run_id, &node_id)?
                {
                    if let Some(obj) = node_metadata.as_object_mut() {
                        obj.insert("context_injection".to_string(), context_injection.clone());
                    } else {
                        node_metadata = json!({"context_injection": context_injection.clone()});
                    }
                    self.persist_context_injection(run_id, &node_id, &context_injection)?;
                }
                // Inject command override if provided
                if let Some(cmd) = command_override {
                    if let Some(obj) = node_metadata.as_object_mut() {
                        obj.insert("command".to_string(), json!(cmd));
                    }
                }
                let input = crate::node_executor::NodeExecutionInput {
                    node_id: node_id.clone(),
                    task_type: task_type.clone(),
                    run_id: run_id.to_string(),
                    workflow_id: workflow_id.clone(),
                    node_metadata,
                };
                if task_type == "agent_step" {
                    let _ = self.append_audit(
                        "scheduler",
                        "agent_step.execution_started",
                        &node_id,
                        &json!({
                            "run_id": run_id,
                            "node_id": node_id,
                            "attempt": attempt,
                        }),
                    );
                }
                let reserved_executor_mismatch = match task_type.as_str() {
                    "agent_step" => executor.executor_type_name() != "agent_step",
                    crate::external_runtime::LANGGRAPH_TASK_TYPE => {
                        executor.executor_type_name()
                            != crate::external_runtime::LANGGRAPH_EXECUTOR_TYPE
                    }
                    crate::opencode_runtime::OPENCODE_TASK_TYPE => {
                        executor.executor_type_name()
                            != crate::opencode_runtime::OPENCODE_EXECUTOR_TYPE
                    }
                    _ => matches!(
                        executor.executor_type_name(),
                        "agent_step"
                            | crate::external_runtime::LANGGRAPH_EXECUTOR_TYPE
                            | crate::opencode_runtime::OPENCODE_EXECUTOR_TYPE
                    ),
                };
                let output = if reserved_executor_mismatch
                    || (task_type == "agent_step") != agent_executor
                {
                    crate::node_executor::NodeExecutionOutput {
                        status: "failed".to_string(),
                        executor_type: executor.executor_type_name().to_string(),
                        output: None,
                        error_domain: Some("reserved_executor_mismatch".to_string()),
                        error_message: Some(format!(
                            "task_type {task_type} is incompatible with executor_type {}",
                            executor.executor_type_name(),
                        )),
                        input_tokens: None,
                        output_tokens: None,
                        estimated_cost: None,
                        latency_ms: Some(0),
                        process_outcome: None,
                        resolved_model: None,
                    }
                } else {
                    executor.execute_node(&input)
                };
                let token_settlement =
                    enforce_product_managed_token_budget(output, &input.node_metadata);
                let output = token_settlement.output;
                let product_managed_usage = token_settlement.usage_state;

                // Phase 3: Record result (inside lock)
                let tick_result = match &self.db {
                    DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                        let tx = rusqlite::Transaction::new_unchecked(
                            conn,
                            rusqlite::TransactionBehavior::Immediate,
                        )
                        .map_err(|error| error.to_string())?;
                        let conn: &rusqlite::Connection = &tx;
                        let now = self.now();
                        let final_status = &output.status;
                        let node_json_text: String = conn
                            .query_row(
                                "SELECT node_json FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                                params![run_id, node_id],
                                |row| row.get(0),
                            )
                            .map_err(|e| e.to_string())?;
                        let indexed_root = indexed_recursive_root_sqlite(conn, run_id, &node_id)?;
                        let mut identity = resolve_recursive_completion_identity(
                            &node_id,
                            &node_json_text,
                            indexed_root.as_deref(),
                            crate::recursive_execution::recursive_enabled(),
                        )?;
                        let recursive_identity = identity.recursive_node_id.as_ref();
                        let recursive_root_active = identity.root;
                        let mut recursive_terminal_reason = identity.terminal_reason;
                        let workflow_receipt =
                            format!("workflow:{run_id}:{node_id}:{attempt}");
                        let recursive_usage = if recursive_identity.is_some()
                            && recursive_terminal_reason.is_none()
                        {
                            let usage_result = if recursive_root_active {
                                identity
                                    .node_json
                                    .get("recursive_root_authority")
                                    .ok_or_else(|| "recursive_node_identity_malformed".to_string())
                                    .and_then(|metadata| {
                                        recursive_usage_from_output(
                                            &output,
                                            metadata,
                                            recursive_usage_mode,
                                        )
                                    })
                            } else {
                                recursive_usage_from_output(
                                    &output,
                                    &identity.node_json,
                                    recursive_usage_mode,
                                )
                            };
                            match usage_result {
                                Ok(usage) => Some(usage),
                                Err(error) => {
                                    recursive_terminal_reason = recursive_usage_failure_reason(&error);
                                    if recursive_terminal_reason.is_none() {
                                        return Err(error);
                                    }
                                    Some(RecursiveUsageEvidence {
                                        budget: crate::recursive_execution::RecursiveBudget::default(),
                                        receipt_id: None,
                                    })
                                }
                            }
                        } else {
                            recursive_identity.map(|_| RecursiveUsageEvidence {
                                budget: crate::recursive_execution::RecursiveBudget::default(),
                                receipt_id: None,
                            })
                        };
                        let recursive_terminal_failure = recursive_terminal_reason.is_some();
                        let requested_retry = !recursive_terminal_failure
                            && retryable_node_failure(&output)
                            && attempt <= if recursive_identity.is_some() {
                                1
                            } else {
                                max_retries
                            };

                        let should_retry = if requested_retry {
                            if let Some(recursive_node_id) = recursive_identity.map(String::as_str) {
                                super::recursive_execution::recursive_retry_allowed_sqlite(
                                    conn,
                                    run_id,
                                    recursive_node_id,
                                    &recursive_usage.as_ref().expect("recursive usage").budget,
                                )?
                            } else {
                                true
                            }
                        } else {
                            false
                        };

                        let actual_status = if recursive_terminal_failure {
                            "failed"
                        } else if should_retry {
                            "pending"
                        } else {
                            final_status
                        };
                        let completed_at = if matches!(actual_status, "completed" | "failed" | "cancelled" | "recovered") {
                            Some(now.as_str())
                        } else {
                            None
                        };

                        let finalized = conn.execute(
                            "UPDATE workflow_run_nodes
                             SET status = ?1, completed_at = ?2, leased_at = NULL
                             WHERE run_id = ?3 AND node_id = ?4
                               AND status = 'running' AND attempt_count = ?5",
                            params![actual_status, completed_at, run_id, node_id, attempt],
                        ).map_err(|e| e.to_string())?;
                        if finalized != 1 {
                            if let Some(recursive_node_id) = recursive_identity.map(String::as_str) {
                                if recursive_terminal_failure {
                                    append_audit_locked(
                                        conn,
                                        &now,
                                        actor,
                                        "workflow_node.late_usage_refused",
                                        &node_id,
                                        &json!({
                                            "run_id": run_id,
                                            "node_id": node_id,
                                            "attempt": attempt,
                                            "recursive_node_id": recursive_node_id,
                                            "reason": recursive_terminal_reason
                                                .expect("recursive terminal reason")
                                                .as_str(),
                                        }),
                                    )?;
                                } else {
                                let usage = recursive_usage.as_ref().expect("recursive usage");
                                let usage_receipt =
                                    usage.receipt_id.as_deref().unwrap_or(&workflow_receipt);
                                let late_usage = if recursive_root_active {
                                    super::recursive_execution::record_recursive_root_late_usage_sqlite(
                                        conn,
                                        run_id,
                                        usage_receipt,
                                        &usage.budget,
                                        &now,
                                    )
                                } else {
                                    super::recursive_execution::record_recursive_late_usage_sqlite(
                                        conn,
                                        run_id,
                                        recursive_node_id,
                                        usage_receipt,
                                        &usage.budget,
                                        &now,
                                    )
                                };
                                let _ = match late_usage {
                                    Ok(within_tree_budget) => append_audit_locked(
                                        conn,
                                        &now,
                                        actor,
                                        "workflow_node.late_usage_accounted",
                                        &node_id,
                                        &json!({
                                            "run_id": run_id,
                                            "node_id": node_id,
                                            "attempt": attempt,
                                            "recursive_node_id": recursive_node_id,
                                            "within_tree_budget": within_tree_budget,
                                        }),
                                    )?,
                                    Err(error) => append_audit_locked(
                                        conn,
                                        &now,
                                        actor,
                                        "workflow_node.late_usage_refused",
                                        &node_id,
                                        &json!({
                                            "run_id": run_id,
                                            "node_id": node_id,
                                            "attempt": attempt,
                                            "recursive_node_id": recursive_node_id,
                                            "reason": error,
                                        }),
                                    )?,
                                };
                                }
                            }
                            append_audit_locked(
                                conn,
                                &now,
                                actor,
                                "workflow_node.stale_completion_ignored",
                                &node_id,
                                &json!({
                                    "run_id": run_id,
                                    "node_id": node_id,
                                    "attempt": attempt,
                                    "executor_type": output.executor_type,
                                    "result_excluded": true,
                                }),
                            )?;
                            let run = get_run_row(conn, run_id)?;
                            tx.commit().map_err(|error| error.to_string())?;
                            return Ok(json!({
                                "action": "stale_completion_ignored",
                                "node_id": node_id,
                                "executor_type": output.executor_type,
                                "attempt": attempt,
                                "result": output.to_value(),
                                "run": run,
                            }));
                        }

                        let recursive_status = if let Some(recursive_node_id) = recursive_identity.map(String::as_str) {
                            Some(if recursive_root_active && recursive_terminal_failure {
                                super::recursive_execution::sync_recursive_root_terminal_failure_sqlite(
                                    conn,
                                    run_id,
                                    &workflow_receipt,
                                    recursive_terminal_reason.expect("recursive terminal reason"),
                                    &now,
                                )?
                            } else if recursive_root_active {
                                let usage = recursive_usage.as_ref().expect("recursive usage");
                                super::recursive_execution::sync_recursive_root_completion_sqlite_with_receipt(
                                    conn,
                                    run_id,
                                    &workflow_receipt,
                                    usage.receipt_id.as_deref().unwrap_or(&workflow_receipt),
                                    final_status == "completed",
                                    &usage.budget,
                                    &now,
                                )?
                            } else if recursive_terminal_failure {
                                super::recursive_execution::sync_recursive_terminal_failure_sqlite(
                                    conn,
                                    run_id,
                                    recursive_node_id,
                                    &workflow_receipt,
                                    recursive_terminal_reason.expect("recursive terminal reason"),
                                    &now,
                                )?
                            } else {
                            let usage = recursive_usage.as_ref().expect("recursive usage");
                            super::recursive_execution::sync_recursive_completion_sqlite_with_receipt(
                                conn,
                                run_id,
                                recursive_node_id,
                                &workflow_receipt,
                                usage.receipt_id.as_deref().unwrap_or(&workflow_receipt),
                                final_status == "completed",
                                should_retry,
                                &usage.budget,
                                &now,
                            )?
                            })
                        } else {
                            None
                        };
                        let persisted_status = recursive_status.as_deref().unwrap_or(actual_status);
                        let recursive_failure_reason = recursive_identity
                            .map(String::as_str)
                            .map(|recursive_node_id| {
                                recursive_failure_reason_sqlite(conn, run_id, recursive_node_id)
                            })
                            .transpose()?
                            .flatten();
                        if persisted_status != actual_status
                            || (recursive_identity.is_some() && persisted_status == "failed")
                        {
                            conn.execute(
                                "UPDATE workflow_run_nodes SET status=?1, blocked_reason=?2
                                 WHERE run_id=?3 AND node_id=?4",
                                params![
                                    persisted_status,
                                    (persisted_status == "failed").then(|| {
                                        recursive_failure_reason
                                            .as_deref()
                                            .unwrap_or(RecursiveFailureReason::ExecutionFailure.as_str())
                                    }),
                                    run_id,
                                    node_id,
                                ],
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        let result_json = output.to_value();
                        if identity.metadata_writable {
                        if let Some(obj) = identity.node_json.as_object_mut() {
                            obj.insert("status".to_string(), json!(persisted_status));
                            obj.insert("result".to_string(), result_json.clone());
                            if let Some(usage) = &product_managed_usage {
                                obj.insert("product_managed_usage".to_string(), usage.clone());
                            }
                            if persisted_status == "completed" {
                                obj.insert("completed_at".to_string(), json!(now));
                            }
                        }
                        conn.execute(
                            "UPDATE workflow_run_nodes SET node_json = ?1 WHERE run_id = ?2 AND node_id = ?3",
                            params![identity.node_json.to_string(), run_id, node_id],
                        )
                        .map_err(|e| e.to_string())?;
                        }

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
                            if task_type == "agent_step" {
                                append_audit_locked(
                                    conn,
                                    &now,
                                    "scheduler",
                                    "agent_step.execution_released",
                                    &node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "status": "retry",
                                        "attempt": attempt,
                                    }),
                                )?;
                            }
                        } else {
                            let event_type = match persisted_status {
                                "completed" => "node.completed",
                                "awaiting_approval" => "node.awaiting_approval",
                                _ => "node.failed",
                            };
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
                            if task_type == "agent_step" {
                                let agent_event = if final_status == "completed" {
                                    "agent_step.execution_completed"
                                } else {
                                    "agent_step.execution_failed"
                                };
                                append_audit_locked(
                                    conn,
                                    &now,
                                    "scheduler",
                                    agent_event,
                                    &node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "status": final_status,
                                        "attempt": attempt,
                                    }),
                                )?;
                            }
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
                            let terminal_audit_details =
                                workflow_run_terminal_audit_details_locked(conn, run_id)?;
                            append_audit_locked(
                                conn,
                                &now,
                                actor,
                                &format!("workflow_run.{terminal_status}"),
                                run_id,
                                &terminal_audit_details,
                            )?;
                        }

                        let run = get_run_row(conn, run_id)?;
                        let result = json!({
                            "action": if should_retry { "node_retry" } else { "node_executed" },
                            "node_id": node_id,
                            "executor_type": output.executor_type,
                            "attempt": attempt,
                            "result": output.to_value(),
                            "run": run,
                        });
                        tx.commit().map_err(|error| error.to_string())?;
                        Ok(result)
                    }),
                    #[cfg(feature = "pg")]
                    DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                        let mut tx = client.transaction().map_err(|e| e.to_string())?;

                        let now = self.now();
                        let final_status = &output.status;
                        let node_json_text: String = tx
                            .query_one(
                                "SELECT node_json FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                                &[&run_id, &node_id],
                            )
                            .map_err(|e| e.to_string())?
                            .get(0);
                        let indexed_root = indexed_recursive_root_pg(&mut tx, run_id, &node_id)?;
                        let mut identity = resolve_recursive_completion_identity(
                            &node_id,
                            &node_json_text,
                            indexed_root.as_deref(),
                            crate::recursive_execution::recursive_enabled(),
                        )?;
                        let recursive_identity = identity.recursive_node_id.as_ref();
                        let recursive_root_active = identity.root;
                        let mut recursive_terminal_reason = identity.terminal_reason;
                        let workflow_receipt =
                            format!("workflow:{run_id}:{node_id}:{attempt}");
                        let recursive_usage = if recursive_identity.is_some()
                            && recursive_terminal_reason.is_none()
                        {
                            let usage_result = if recursive_root_active {
                                identity
                                    .node_json
                                    .get("recursive_root_authority")
                                    .ok_or_else(|| "recursive_node_identity_malformed".to_string())
                                    .and_then(|metadata| {
                                        recursive_usage_from_output(
                                            &output,
                                            metadata,
                                            recursive_usage_mode,
                                        )
                                    })
                            } else {
                                recursive_usage_from_output(
                                    &output,
                                    &identity.node_json,
                                    recursive_usage_mode,
                                )
                            };
                            match usage_result {
                                Ok(usage) => Some(usage),
                                Err(error) => {
                                    recursive_terminal_reason = recursive_usage_failure_reason(&error);
                                    if recursive_terminal_reason.is_none() {
                                        return Err(error);
                                    }
                                    Some(RecursiveUsageEvidence {
                                        budget: crate::recursive_execution::RecursiveBudget::default(),
                                        receipt_id: None,
                                    })
                                }
                            }
                        } else {
                            recursive_identity.map(|_| RecursiveUsageEvidence {
                                budget: crate::recursive_execution::RecursiveBudget::default(),
                                receipt_id: None,
                            })
                        };
                        let recursive_terminal_failure = recursive_terminal_reason.is_some();
                        let requested_retry = !recursive_terminal_failure
                            && retryable_node_failure(&output)
                            && attempt <= if recursive_identity.is_some() {
                                1
                            } else {
                                max_retries
                            };

                        let should_retry = if requested_retry {
                            if let Some(recursive_node_id) = recursive_identity.map(String::as_str) {
                                super::recursive_execution::recursive_retry_allowed_pg(
                                    &mut tx,
                                    run_id,
                                    recursive_node_id,
                                    &recursive_usage.as_ref().expect("recursive usage").budget,
                                )?
                            } else {
                                true
                            }
                        } else {
                            false
                        };

                        let actual_status = if recursive_terminal_failure {
                            "failed"
                        } else if should_retry {
                            "pending"
                        } else {
                            final_status
                        };
                        let completed_at = if matches!(actual_status, "completed" | "failed" | "cancelled" | "recovered") {
                            Some(now.as_str())
                        } else {
                            None
                        };
                        let expected_attempt = i32::try_from(attempt)
                            .map_err(|_| "workflow node attempt exceeds PostgreSQL INTEGER range".to_string())?;

                        let finalized = tx.execute(
                            "UPDATE workflow_run_nodes
                             SET status = $1, completed_at = $2, leased_at = NULL
                             WHERE run_id = $3 AND node_id = $4
                               AND status = 'running' AND attempt_count = $5",
                            &[&actual_status, &completed_at, &run_id, &node_id, &expected_attempt],
                        ).map_err(|e| e.to_string())?;
                        if finalized != 1 {
                            if let Some(recursive_node_id) = recursive_identity.map(String::as_str) {
                                if recursive_terminal_failure {
                                    pg_append_audit(
                                        &mut tx,
                                        &now,
                                        actor,
                                        "workflow_node.late_usage_refused",
                                        &node_id,
                                        &json!({
                                            "run_id": run_id,
                                            "node_id": node_id,
                                            "attempt": attempt,
                                            "recursive_node_id": recursive_node_id,
                                            "reason": recursive_terminal_reason
                                                .expect("recursive terminal reason")
                                                .as_str(),
                                        }),
                                    )?;
                                } else {
                                let usage = recursive_usage.as_ref().expect("recursive usage");
                                let usage_receipt =
                                    usage.receipt_id.as_deref().unwrap_or(&workflow_receipt);
                                let late_usage = if recursive_root_active {
                                    super::recursive_execution::record_recursive_root_late_usage_pg(
                                        &mut tx,
                                        run_id,
                                        usage_receipt,
                                        &usage.budget,
                                        &now,
                                    )
                                } else {
                                    super::recursive_execution::record_recursive_late_usage_pg(
                                        &mut tx,
                                        run_id,
                                        recursive_node_id,
                                        usage_receipt,
                                        &usage.budget,
                                        &now,
                                    )
                                };
                                match late_usage {
                                    Ok(within_tree_budget) => pg_append_audit(
                                        &mut tx,
                                        &now,
                                        actor,
                                        "workflow_node.late_usage_accounted",
                                        &node_id,
                                        &json!({
                                            "run_id": run_id,
                                            "node_id": node_id,
                                            "attempt": attempt,
                                            "recursive_node_id": recursive_node_id,
                                            "within_tree_budget": within_tree_budget,
                                        }),
                                    )?,
                                    Err(error) => pg_append_audit(
                                        &mut tx,
                                        &now,
                                        actor,
                                        "workflow_node.late_usage_refused",
                                        &node_id,
                                        &json!({
                                            "run_id": run_id,
                                            "node_id": node_id,
                                            "attempt": attempt,
                                            "recursive_node_id": recursive_node_id,
                                            "reason": error,
                                        }),
                                    )?,
                                };
                                }
                            }
                            pg_append_audit(
                                &mut tx,
                                &now,
                                actor,
                                "workflow_node.stale_completion_ignored",
                                &node_id,
                                &json!({
                                    "run_id": run_id,
                                    "node_id": node_id,
                                    "attempt": attempt,
                                    "executor_type": output.executor_type,
                                    "result_excluded": true,
                                }),
                            )?;
                            let run = pg_get_run_row(&mut tx, run_id)?;
                            tx.commit().map_err(|error| error.to_string())?;
                            return Ok(json!({
                                "action": "stale_completion_ignored",
                                "node_id": node_id,
                                "executor_type": output.executor_type,
                                "attempt": attempt,
                                "result": output.to_value(),
                                "run": run,
                            }));
                        }

                        let recursive_status = if let Some(recursive_node_id) = recursive_identity.map(String::as_str) {
                            Some(if recursive_root_active && recursive_terminal_failure {
                                super::recursive_execution::sync_recursive_root_terminal_failure_pg(
                                    &mut tx,
                                    run_id,
                                    &workflow_receipt,
                                    recursive_terminal_reason.expect("recursive terminal reason"),
                                    &now,
                                )?
                            } else if recursive_root_active {
                                let usage = recursive_usage.as_ref().expect("recursive usage");
                                super::recursive_execution::sync_recursive_root_completion_pg_with_receipt(
                                    &mut tx,
                                    run_id,
                                    &workflow_receipt,
                                    usage.receipt_id.as_deref().unwrap_or(&workflow_receipt),
                                    final_status == "completed",
                                    &usage.budget,
                                    &now,
                                )?
                            } else if recursive_terminal_failure {
                                super::recursive_execution::sync_recursive_terminal_failure_pg(
                                    &mut tx,
                                    run_id,
                                    recursive_node_id,
                                    &workflow_receipt,
                                    recursive_terminal_reason.expect("recursive terminal reason"),
                                    &now,
                                )?
                            } else {
                            let usage = recursive_usage.as_ref().expect("recursive usage");
                            super::recursive_execution::sync_recursive_completion_pg_with_receipt(
                                &mut tx,
                                run_id,
                                recursive_node_id,
                                &workflow_receipt,
                                usage.receipt_id.as_deref().unwrap_or(&workflow_receipt),
                                final_status == "completed",
                                should_retry,
                                &usage.budget,
                                &now,
                            )?
                            })
                        } else {
                            None
                        };
                        let persisted_status = recursive_status.as_deref().unwrap_or(actual_status);
                        let recursive_failure_reason = recursive_identity
                            .map(String::as_str)
                            .map(|recursive_node_id| {
                                recursive_failure_reason_pg(&mut tx, run_id, recursive_node_id)
                            })
                            .transpose()?
                            .flatten();
                        if persisted_status != actual_status
                            || (recursive_identity.is_some() && persisted_status == "failed")
                        {
                            tx.execute(
                                "UPDATE workflow_run_nodes SET status=$1, blocked_reason=$2
                                 WHERE run_id=$3 AND node_id=$4",
                                &[
                                    &persisted_status,
                                    &(persisted_status == "failed").then(|| {
                                        recursive_failure_reason
                                            .as_deref()
                                            .unwrap_or(RecursiveFailureReason::ExecutionFailure.as_str())
                                    }),
                                    &run_id,
                                    &node_id,
                                ],
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        let result_json = output.to_value();
                        if identity.metadata_writable {
                        if let Some(obj) = identity.node_json.as_object_mut() {
                            obj.insert("status".to_string(), json!(persisted_status));
                            obj.insert("result".to_string(), result_json.clone());
                            if let Some(usage) = &product_managed_usage {
                                obj.insert("product_managed_usage".to_string(), usage.clone());
                            }
                            if persisted_status == "completed" {
                                obj.insert("completed_at".to_string(), json!(now));
                            }
                        }
                        tx.execute(
                            "UPDATE workflow_run_nodes SET node_json = $1 WHERE run_id = $2 AND node_id = $3",
                            &[&identity.node_json.to_string(), &run_id, &node_id],
                        )
                        .map_err(|e| e.to_string())?;
                        }

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
                            if task_type == "agent_step" {
                                pg_append_audit(
                                    &mut tx,
                                    &now,
                                    "scheduler",
                                    "agent_step.execution_released",
                                    &node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "status": "retry",
                                        "attempt": attempt,
                                    }),
                                )?;
                            }
                        } else {
                            let event_type = match persisted_status {
                                "completed" => "node.completed",
                                "awaiting_approval" => "node.awaiting_approval",
                                _ => "node.failed",
                            };
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
                            if task_type == "agent_step" {
                                let agent_event = if final_status == "completed" {
                                    "agent_step.execution_completed"
                                } else {
                                    "agent_step.execution_failed"
                                };
                                pg_append_audit(
                                    &mut tx,
                                    &now,
                                    "scheduler",
                                    agent_event,
                                    &node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "status": final_status,
                                        "attempt": attempt,
                                    }),
                                )?;
                            }
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
                            let terminal_audit_details =
                                pg_workflow_run_terminal_audit_details(&mut tx, run_id)?;
                            pg_append_audit(
                                &mut tx,
                                &now,
                                actor,
                                &format!("workflow_run.{terminal_status}"),
                                run_id,
                                &terminal_audit_details,
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

                if tick_result
                    .get("run")
                    .and_then(|run| run.get("status"))
                    .and_then(Value::as_str)
                    .map(is_run_terminal)
                    .unwrap_or(false)
                {
                    let _artifact =
                        self.record_automatic_native_scorecard_for_run(run_id, actor)?;
                }

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

                if let Err(error) = self.produce_budget_intelligence_for_run(run_id, "scheduler") {
                    let _ = self.append_audit(
                        "scheduler",
                        "budget_intelligence.production_failed",
                        run_id,
                        &json!({
                            "run_id": run_id,
                            "error_sha256": format!("{:x}", sha2::Sha256::digest(error.as_bytes())),
                            "raw_error_stored": false,
                            "workflow_result_preserved": true,
                        }),
                    );
                }

                Ok(tick_result)
            }
        }
    }

    pub(crate) fn next_ready_workflow_node_task_type_with_agent_caps(
        &self,
        run_id: &str,
        agent_concurrency_caps: Option<(usize, usize)>,
    ) -> Result<Option<String>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                ensure_run_exists_locked(conn, run_id)?;
                next_ready_task_type_sqlite(conn, run_id, agent_concurrency_caps)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                pg_ensure_run_exists(client, run_id)?;
                pg_next_ready_task_type(client, run_id, agent_concurrency_caps)
            }),
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

        let (sources, mappings) = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT e.edge_id, e.from_node_id, n.node_json, e.edge_json
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
                        let edge_json_text: String = row.get(3)?;
                        let node_json: Value =
                            serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                        let edge_json: Value =
                            serde_json::from_str(&edge_json_text).unwrap_or(Value::Null);
                        Ok((edge_id, from_node_id, node_json, edge_json))
                    })
                    .map_err(|e| e.to_string())?;
                let mut sources = Vec::new();
                let mut mappings = Vec::new();
                for row in rows {
                    let (edge_id, from_node_id, node_json, edge_json) =
                        row.map_err(|e| e.to_string())?;
                    if let Some(output) = completed_node_output(&node_json) {
                        sources.push(ContextSource {
                            edge_id,
                            from_node_id,
                            output,
                        });
                        mappings.push(edge_json.get("field_mapping").cloned());
                    }
                }
                Ok((sources, mappings))
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT e.edge_id, e.from_node_id, n.node_json, e.edge_json
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
                let mut mappings = Vec::new();
                for row in rows {
                    let node_json_text: String = row.get(2);
                    let edge_json_text: String = row.get(3);
                    let node_json: Value =
                        serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                    let edge_json: Value =
                        serde_json::from_str(&edge_json_text).unwrap_or(Value::Null);
                    if let Some(output) = completed_node_output(&node_json) {
                        sources.push(ContextSource {
                            edge_id: row.get(0),
                            from_node_id: row.get(1),
                            output,
                        });
                        mappings.push(edge_json.get("field_mapping").cloned());
                    }
                }
                Ok((sources, mappings))
            })?,
        };

        let memory_preview =
            self.memory_context_for_agent_step_node(run_id, node_id, config.max_context_tokens)?;
        let memory_estimate = memory_preview
            .as_ref()
            .and_then(|context| context.get("estimated_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let memory_budget = if memory_preview.is_none() {
            0
        } else if sources.is_empty() {
            config.max_context_tokens
        } else {
            memory_estimate
                .min((config.max_context_tokens / 4).max(1))
                .min(config.max_context_tokens)
        };
        let predecessor_budget = config.max_context_tokens.saturating_sub(memory_budget);
        let predecessor_config = ContextAssemblyConfig {
            enabled: config.enabled,
            max_context_tokens: predecessor_budget,
        };
        let assembled = if predecessor_budget == 0 {
            None
        } else {
            assemble_context_injection_with_bridge(
                node_id,
                &sources,
                &mappings,
                &predecessor_config,
            )
        };
        let memory_context = if memory_budget == 0 {
            None
        } else if memory_budget == config.max_context_tokens {
            memory_preview
        } else {
            self.memory_context_for_agent_step_node(run_id, node_id, memory_budget)?
        };
        Ok(merge_memory_context_injection(
            node_id,
            assembled,
            memory_context,
            &config,
        ))
    }

    fn memory_context_for_agent_step_node(
        &self,
        run_id: &str,
        node_id: &str,
        max_context_tokens: usize,
    ) -> Result<Option<Value>, String> {
        let Some(run) = self.get_workflow_run(run_id)? else {
            return Ok(None);
        };
        let Some(node) = run
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| {
                nodes
                    .iter()
                    .find(|node| node.get("node_id") == Some(&json!(node_id)))
            })
        else {
            return Ok(None);
        };
        if node.get("task_type").and_then(Value::as_str) != Some("agent_step") {
            return Ok(None);
        }
        let Some(agent_id) = node
            .get("agent_id")
            .and_then(Value::as_str)
            .or_else(|| node.get("assigned_agent_id").and_then(Value::as_str))
        else {
            return Ok(None);
        };
        let digest_budget = (max_context_tokens / 2).max(1);
        let memory_digest = self
            .get_agent_state(agent_id, run_id)?
            .and_then(|state| build_memory_context_for_node(&state, digest_budget));
        let tenant_id = run
            .get("tenant_id")
            .and_then(Value::as_str)
            .or_else(|| run.pointer("/boundaries/tenant_id").and_then(Value::as_str))
            .ok_or_else(|| "workflow run has no immutable tenant memory scope".to_string())?;
        let workspace_id = run
            .get("workspace_id")
            .and_then(Value::as_str)
            .or_else(|| {
                run.pointer("/boundaries/workspace_id")
                    .and_then(Value::as_str)
            })
            .ok_or_else(|| "workflow run has no immutable workspace memory scope".to_string())?;
        let query = node
            .get("objective")
            .and_then(Value::as_str)
            .or_else(|| node.pointer("/metadata/objective").and_then(Value::as_str))
            .unwrap_or(node_id);
        let retrieval_budget = max_context_tokens.saturating_sub(
            memory_digest
                .as_ref()
                .and_then(|value| value.get("included_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize,
        );
        let retrieval = if retrieval_budget == 0 {
            None
        } else {
            Some(self.retrieve_durable_memories(
                &MemoryRetrievalRequest {
                    scope: MemoryScope {
                        tenant_id: tenant_id.to_string(),
                        workspace_id: workspace_id.to_string(),
                        agent_id: Some(agent_id.to_string()),
                        task_id: None,
                    },
                    run_id: run_id.to_string(),
                    node_id: node_id.to_string(),
                    query: query.to_string(),
                    top_k: 5,
                    max_tokens: retrieval_budget,
                    max_bytes: retrieval_budget.saturating_mul(4),
                    allow_lexical_fallback: true,
                },
                "scheduler",
            )?)
        };
        if memory_digest.is_none()
            && retrieval
                .as_ref()
                .is_none_or(|value| value.selected.is_empty())
        {
            return Ok(None);
        }
        let digest_tokens = memory_digest
            .as_ref()
            .and_then(|value| value.get("included_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let retrieval_tokens = retrieval.as_ref().map_or(0, |value| value.estimated_tokens);
        let references = retrieval.as_ref().map_or_else(Vec::new, |result| {
            result
                .selected
                .iter()
                .map(|reference| {
                    json!({
                        "memory_id": reference.memory_id,
                        "version": reference.version,
                        "source_id": reference.source_id,
                        "source_sha256": reference.source_sha256,
                        "record_sha256": reference.record_sha256,
                        "score": reference.score,
                        "confidence": reference.confidence,
                        "estimated_tokens": reference.estimated_tokens,
                        "content": reference.content,
                    })
                })
                .collect()
        });
        Ok(Some(json!({
            "schema_version": "agent_memory_context.v2",
            "source": "run_digest_plus_durable_retrieval",
            "injection_surface": "node_metadata_only",
            "max_context_tokens": max_context_tokens,
            "estimated_tokens": digest_tokens.saturating_add(retrieval_tokens),
            "included_tokens": digest_tokens.saturating_add(retrieval_tokens),
            "truncated": memory_digest.as_ref().and_then(|value| value.get("truncated")).and_then(Value::as_bool).unwrap_or(false)
                || retrieval.as_ref().is_some_and(|value| value.truncated),
            "memory_digest": memory_digest.as_ref().and_then(|value| value.get("memory_digest")).cloned(),
            "retrieval": retrieval.as_ref().map(|value| json!({
                "retrieval_id": value.retrieval_id,
                "mode": value.mode,
                "embedding_provenance": value.embedding_provenance,
                "candidate_count": value.candidate_count,
                "selected_count": value.selected.len(),
                "stale_excluded": value.stale_excluded,
                "state_excluded": value.state_excluded,
                "request_sha256": value.request_sha256,
                "result_sha256": value.result_sha256,
                "read_bytes": value.read_bytes,
            })),
            "retrieved_references": references,
            "context_layers": {
                "durable_state": {"tenant_id": tenant_id, "workspace_id": workspace_id},
                "bounded_recent": [],
                "memory_digest": memory_digest.as_ref().and_then(|value| value.get("memory_digest")).cloned(),
                "retrieved_references": references,
            },
        })))
    }

    fn persist_context_injection(
        &self,
        run_id: &str,
        node_id: &str,
        context_injection: &Value,
    ) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
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
                    obj.insert("context_injection".to_string(), context_injection.clone());
                }
                conn.execute(
                    "UPDATE workflow_run_nodes SET node_json = ?1 WHERE run_id = ?2 AND node_id = ?3",
                    params![node_json.to_string(), run_id, node_id],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_one(
                        "SELECT node_json FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?;
                let node_json_text: String = row.get(0);
                let mut node_json: Value =
                    serde_json::from_str(&node_json_text).unwrap_or(Value::Null);
                if let Some(obj) = node_json.as_object_mut() {
                    obj.insert("context_injection".to_string(), context_injection.clone());
                }
                client
                    .execute(
                        "UPDATE workflow_run_nodes SET node_json = $1 WHERE run_id = $2 AND node_id = $3",
                        &[&node_json.to_string(), &run_id, &node_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    pub fn validate_approval_binding(
        &self,
        run_id: &str,
        artifact_id: &str,
    ) -> Result<Value, String> {
        let artifact = self
            .get_supervised_patch_artifact(artifact_id)?
            .ok_or_else(|| format!("artifact not found: {artifact_id}"))?;
        let artifact_run_id = artifact.get("run_id").and_then(Value::as_str).unwrap_or("");
        if artifact_run_id != run_id {
            return Ok(json!({
                "run_id": run_id,
                "artifact_id": artifact_id,
                "export_eligible": false,
                "binding_checks": [{
                    "run_match": false,
                    "artifact_run_id": artifact_run_id,
                }],
                "approving_approval": Value::Null,
            }));
        }
        let approvals = self.workflow_run_approvals(run_id, 1000)?;

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
                    .prepare(queue_lease::ACTIVE_RUN_IDS_SQL)
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
                    .query(queue_lease::ACTIVE_RUN_IDS_SQL, &[])
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
                    .prepare(queue_lease::ACTIVE_RUNS_PRIORITIZED_SQL)
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], queue_lease::prioritized_sqlite_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(queue_lease::ACTIVE_RUNS_PRIORITIZED_SQL, &[])
                    .map_err(|e| e.to_string())?;
                let values: Vec<Value> = rows.iter().map(queue_lease::prioritized_pg_row).collect();
                Ok(values)
            }),
        }
    }

    pub fn list_active_workflow_runs_prioritized_page(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        let limit = limit.clamp(0, 500);
        let offset = offset.max(0);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sql = format!(
                    "{} LIMIT ?1 OFFSET ?2",
                    queue_lease::ACTIVE_RUNS_PRIORITIZED_SQL
                );
                let mut stmt = conn.prepare(&sql).map_err(|error| error.to_string())?;
                let rows = stmt
                    .query_map(params![limit, offset], queue_lease::prioritized_sqlite_row)
                    .map_err(|error| error.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sql = format!(
                    "{} LIMIT $1 OFFSET $2",
                    queue_lease::ACTIVE_RUNS_PRIORITIZED_SQL
                );
                let rows = client
                    .query(&sql, &[&limit, &offset])
                    .map_err(|error| error.to_string())?;
                Ok(rows.iter().map(queue_lease::prioritized_pg_row).collect())
            }),
        }
    }

    pub fn count_active_workflow_runs(&self) -> Result<i64, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(queue_lease::ACTIVE_RUN_COUNT_SQL, [], |row| row.get(0))
                    .map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(queue_lease::ACTIVE_RUN_COUNT_SQL, &[])
                    .map(|row| row.get(0))
                    .map_err(|error| error.to_string())
            }),
        }
    }

    pub fn update_run_priority(&self, run_id: &str, priority: i64) -> Result<(), String> {
        let pg_priority =
            i32::try_from(priority).map_err(|_| "priority must be between 1 and 10".to_string())?;
        if !(1..=10).contains(&pg_priority) {
            return Err("priority must be between 1 and 10".to_string());
        }
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
                        &[&pg_priority, &updated, &run_id],
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
        if pause_reason == Some(API_OWNED_SUPERVISED_PATCH) {
            return Err(format!(
                "{EXECUTION_OWNER_CONFLICT_PREFIX}: the supervised-patch API owner is reserved"
            ));
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                ensure_run_exists_locked(&tx, run_id)?;
                let current_owner: Option<String> = tx
                    .query_row(
                        "SELECT pause_reason FROM workflow_runs WHERE run_id = ?1",
                        params![run_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                require_run_execution_owner(run_id, current_owner.as_deref(), None)?;
                let budget_pause_active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM budget_pause_decisions WHERE run_id = ?1 AND state = 'paused')", params![run_id], |row| row.get(0)).map_err(|e| e.to_string())?;
                if budget_pause_active { return Err("active budget auto-pause requires explicit audited recovery".to_string()); }
                let updated = self.now();
                let rows = tx
                    .execute(
                        "UPDATE workflow_runs SET pause_reason = ?1, updated_at = ?2 WHERE run_id = ?3",
                        params![pause_reason, updated, run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                pg_ensure_run_exists(&mut tx, run_id)?;
                let current_owner: Option<String> = tx
                    .query_one(
                        "SELECT pause_reason FROM workflow_runs WHERE run_id = $1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                require_run_execution_owner(run_id, current_owner.as_deref(), None)?;
                let budget_pause_active: bool = tx.query_one("SELECT EXISTS(SELECT 1 FROM budget_pause_decisions WHERE run_id = $1 AND state = 'paused')", &[&run_id]).map_err(|e| e.to_string())?.get(0);
                if budget_pause_active { return Err("active budget auto-pause requires explicit audited recovery".to_string()); }
                let updated = self.now();
                let rows = tx
                    .execute(
                        "UPDATE workflow_runs SET pause_reason = $1, updated_at = $2 WHERE run_id = $3",
                        &[&pause_reason, &updated, &run_id],
                    )
                    .map_err(|e| e.to_string())?;
                if rows == 0 {
                    return Err(format!("workflow run not found: {run_id}"));
                }
                tx.commit().map_err(|error| error.to_string())?;
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
                    .query_row(queue_lease::QUEUED_COUNT_SQL, [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                let total_running: i64 = conn
                    .query_row(queue_lease::RUNNING_COUNT_SQL, [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                let total_paused: i64 = conn
                    .query_row(queue_lease::PAUSED_COUNT_SQL, [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                let total_completed: i64 = conn
                    .query_row(queue_lease::COMPLETED_COUNT_SQL, [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                let total_failed: i64 = conn
                    .query_row(queue_lease::FAILED_COUNT_SQL, [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                let avg_priority: Value = conn
                    .query_row(queue_lease::AVG_PRIORITY_SQL, [], |row| {
                        let avg: f64 = row.get(0)?;
                        Ok(json!(avg))
                    })
                    .unwrap_or(json!(5.0));
                let overdue_count: i64 = conn
                    .query_row(queue_lease::SQLITE_OVERDUE_COUNT_SQL, [], |row| row.get(0))
                    .map_err(|e| e.to_string())?;
                Ok(queue_lease::queue_status_value(
                    total_queued,
                    total_running,
                    total_paused,
                    total_completed,
                    total_failed,
                    avg_priority,
                    overdue_count,
                ))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let total_queued: i64 = client
                    .query_one(queue_lease::QUEUED_COUNT_SQL, &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                let total_running: i64 = client
                    .query_one(queue_lease::RUNNING_COUNT_SQL, &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                let total_paused: i64 = client
                    .query_one(queue_lease::PAUSED_COUNT_SQL, &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                let total_completed: i64 = client
                    .query_one(queue_lease::COMPLETED_COUNT_SQL, &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                let total_failed: i64 = client
                    .query_one(queue_lease::FAILED_COUNT_SQL, &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                let avg_priority: Value = client
                    .query_one(queue_lease::AVG_PRIORITY_SQL, &[])
                    .map(|row| {
                        let avg: f64 = row.get(0);
                        json!(avg)
                    })
                    .unwrap_or(json!(5.0));
                let overdue_count: i64 = client
                    .query_one(queue_lease::PG_OVERDUE_COUNT_SQL, &[])
                    .map_err(|e| e.to_string())?
                    .get(0);
                Ok(queue_lease::queue_status_value(
                    total_queued,
                    total_running,
                    total_paused,
                    total_completed,
                    total_failed,
                    avg_priority,
                    overdue_count,
                ))
            }),
        }
    }

    pub fn list_tenants_with_quota(&self) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(queue_lease::TENANTS_WITH_QUOTA_SQL)
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map([], queue_lease::tenant_quota_sqlite_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(queue_lease::TENANTS_WITH_QUOTA_SQL, &[])
                    .map_err(|e| e.to_string())?;
                let values: Vec<Value> =
                    rows.iter().map(queue_lease::tenant_quota_pg_row).collect();
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
        let pg_priority =
            i32::try_from(priority).map_err(|_| "priority must be between 1 and 10".to_string())?;
        if !(1..=10).contains(&pg_priority) {
            return Err("priority must be between 1 and 10".to_string());
        }
        let pg_sla_ms = sla_ms
            .map(|value| {
                i32::try_from(value).map_err(|_| "sla_ms must fit a non-negative INT4".to_string())
            })
            .transpose()?;
        if pg_sla_ms.is_some_and(|value| value < 0) {
            return Err("sla_ms must fit a non-negative INT4".to_string());
        }
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
        let tenant_id = tenant_id.unwrap_or("local");
        let workspace_id = "local";
        let mut run_boundaries = workflow_run_boundaries_for_plan(&plan)?;
        let boundaries_object = run_boundaries
            .as_object_mut()
            .ok_or_else(|| "workflow run boundaries must be an object".to_string())?;
        boundaries_object.insert("tenant_id".to_string(), json!(tenant_id));
        boundaries_object.insert("workspace_id".to_string(), json!(workspace_id));

        let run_id = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_sequence(conn, "workflow_runs", "run_sequence")?;
                let run_id = format!("run-{sequence:04}");
                let created_at = self.now();
                let boundaries = run_boundaries.clone();
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
                    "workspace_id": workspace_id,
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
                        "workspace_id": workspace_id,
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
                        "workspace_id": workspace_id,
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
                let boundaries = run_boundaries.clone();
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
                    "workspace_id": workspace_id,
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
                        &pg_priority,
                        &deadline_at,
                        &pg_sla_ms,
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
                        "workspace_id": workspace_id,
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
                        "workspace_id": workspace_id,
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
                    .query_row(queue_lease::PENDING_NODE_FOR_TEST_SQL, [], |row| row.get(0))
                    .ok();
                let Some(node_id) = node_id else {
                    return Ok(0);
                };
                let count = conn
                    .execute(
                        queue_lease::SQLITE_SET_PENDING_NODE_RUNNING_SQL,
                        rusqlite::params![leased_at, node_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(count as i64)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(queue_lease::PENDING_NODE_FOR_TEST_SQL, &[])
                    .map_err(|e| e.to_string())?;
                let Some(row) = rows.into_iter().next() else {
                    return Ok(0);
                };
                let node_id: String = row.get(0);
                let count = client
                    .execute(
                        queue_lease::PG_SET_PENDING_NODE_RUNNING_SQL,
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
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let now = self.now();
                let stale_nodes: Vec<(String, String, String)> = {
                    let mut stmt = tx
                        .prepare(queue_lease::STALE_LEASE_SELECT_SQL)
                        .map_err(|e| e.to_string())?;
                    let nodes = stmt
                        .query_map([], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                            ))
                        })
                        .map_err(|e| e.to_string())?
                        .filter_map(|r| r.ok())
                        .filter(|(_, _, leased_at)| {
                            queue_lease::stale_lease_is_expired(leased_at, &now, lease_timeout_ms)
                        })
                        .collect();
                    nodes
                };
                let mut count = 0_i64;
                for (run_id, node_id, leased_at) in &stale_nodes {
                    let recursive_row: Option<(String, i64)> = tx
                        .query_row(
                            "SELECT node_json, attempt_count FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                            params![run_id, node_id],
                            |row| {
                                let node_json: String = row.get(0)?;
                                let attempt_count: i64 = row.get(1)?;
                                Ok((node_json, attempt_count))
                            },
                        )
                        .optional()
                        .map_err(|error| error.to_string())?;
                    let indexed_root = indexed_recursive_root_sqlite(&tx, run_id, node_id)?;
                    let recursive_state = recursive_row
                        .map(|(node_json, attempt_count)| {
                            stale_recursive_identity(
                                node_id,
                                node_json,
                                attempt_count,
                                indexed_root.as_deref(),
                            )
                        })
                        .transpose()?
                        .flatten();
                    let mut recursive_tree_missing = false;
                    let mut recursive_node_missing = false;
                    let recursive_retry = if let Some((recursive_node_id, attempt, _, _, malformed)) =
                        recursive_state.as_ref()
                    {
                        if *malformed {
                            false
                        } else { match super::recursive_execution::recursive_retry_allowed_sqlite(
                            &tx,
                            run_id,
                            recursive_node_id,
                            &recursive_retry_usage(),
                        ) {
                            Ok(allowed) => *attempt <= i64::from(MAX_RECURSIVE_RETRIES) && allowed,
                            Err(error) if error == "recursive_tree_missing" => {
                                recursive_tree_missing = true;
                                false
                            }
                            Err(error) if error == "recursive_node_missing" => {
                                recursive_node_missing = true;
                                false
                            }
                            Err(error) => return Err(error),
                        }}
                    } else {
                        false
                    };
                    let recovery_status = if recursive_state.is_some() && !recursive_retry {
                        "failed"
                    } else {
                        "pending"
                    };
                    let recovery_reason = if recursive_state
                        .as_ref()
                        .is_some_and(|(_, _, _, _, malformed)| *malformed)
                    {
                        "recursive_node_identity_malformed"
                    } else if recursive_tree_missing {
                        "recursive_tree_missing"
                    } else if recursive_node_missing {
                        "recursive_node_missing"
                    } else {
                        "recursive_retry_exhausted"
                    };
                    let mut recovered_node_json = recursive_state
                        .as_ref()
                        .and_then(|(_, _, node_json, _, _)| node_json.clone());
                    if let Some(object) = recovered_node_json
                        .as_mut()
                        .and_then(Value::as_object_mut)
                    {
                        object.insert("status".to_string(), json!(recovery_status));
                        if recovery_status == "failed" {
                            object.insert("completed_at".to_string(), json!(now));
                        } else {
                            object.remove("completed_at");
                        }
                    }
                    let completed_at = (recovery_status == "failed").then_some(now.as_str());
                    let updated = tx
                        .execute(
                            "UPDATE workflow_run_nodes SET status = ?1, completed_at = ?2,
                             blocked_reason = ?3, leased_at = NULL,
                             node_json = COALESCE(?4, node_json)
                             WHERE run_id = ?5 AND node_id = ?6 AND status = 'running'
                               AND leased_at = ?7",
                            params![
                                recovery_status,
                                completed_at,
                                (recovery_status == "failed").then_some(recovery_reason),
                                recovered_node_json.map(|value| value.to_string()),
                                run_id,
                                node_id,
                                leased_at,
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                    if updated > 0 {
                        count += updated as i64;
                        if let Some((recursive_node_id, attempt, _, recursive_root, malformed)) = recursive_state {
                            if malformed {
                                if recursive_root {
                                    super::recursive_execution::sync_recursive_root_terminal_failure_sqlite(
                                        &tx,
                                        run_id,
                                        &format!("workflow:{run_id}:{node_id}:{attempt}"),
                                        RecursiveFailureReason::RecursiveNodeIdentityMalformed,
                                        &now,
                                    )?;
                                } else {
                                    super::recursive_execution::sync_recursive_terminal_failure_sqlite(
                                        &tx,
                                        run_id,
                                        &recursive_node_id,
                                        &format!("workflow:{run_id}:{node_id}:{attempt}"),
                                        RecursiveFailureReason::RecursiveNodeIdentityMalformed,
                                        &now,
                                    )?;
                                }
                            } else if recursive_tree_missing {
                                append_audit_locked(
                                    &tx,
                                    &now,
                                    "scheduler",
                                    "recursive.tree_missing_terminalized",
                                    node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "reason": "recursive_tree_missing",
                                    }),
                                )?;
                            } else if recursive_node_missing {
                                append_audit_locked(
                                    &tx,
                                    &now,
                                    "scheduler",
                                    "recursive.node_missing_terminalized",
                                    node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "recursive_node_id": recursive_node_id,
                                        "reason": "recursive_node_missing",
                                    }),
                                )?;
                            } else {
                            super::recursive_execution::sync_recursive_stale_recovery_sqlite(
                                &tx,
                                run_id,
                                &recursive_node_id,
                                &format!("workflow:{run_id}:{node_id}:{attempt}"),
                                recursive_retry,
                                &now,
                            )?;
                            }
                        }
                        append_audit_locked(
                            &tx,
                            &now,
                            "scheduler",
                            "workflow_node.stale_lease_recovered",
                            node_id,
                            &queue_lease::stale_lease_audit_payload(
                                run_id,
                                leased_at,
                                lease_timeout_ms,
                            ),
                        )?;
                        // Emit agent_step-specific lease_expired audit
                        let task_type: Option<String> = tx
                            .query_row(
                                "SELECT task_type FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
                                params![run_id, node_id],
                                |row| row.get(0),
                            )
                            .ok();
                        if task_type.as_deref() == Some("agent_step") {
                            append_audit_locked(
                                &tx,
                                &now,
                                "scheduler",
                                "agent_step.lease_expired",
                                node_id,
                                &json!({
                                    "run_id": run_id,
                                    "node_id": node_id,
                                    "leased_at": leased_at,
                                    "lease_timeout_ms": lease_timeout_ms,
                                }),
                            )?;
                        }
                    }
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(count)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let now = self.now();
                let rows = tx
                    .query(queue_lease::STALE_LEASE_SELECT_SQL, &[])
                    .map_err(|e| e.to_string())?;
                let stale_nodes: Vec<(String, String, String)> = rows
                    .iter()
                    .map(|row| (row.get(0), row.get(1), row.get(2)))
                    .filter(|(_, _, leased_at): &(String, String, String)| {
                        queue_lease::stale_lease_is_expired(leased_at, &now, lease_timeout_ms)
                    })
                    .collect();
                let mut count = 0_i64;
                for (run_id, node_id, leased_at) in &stale_nodes {
                    let recursive_row: Option<(String, i64)> = tx
                        .query_opt(
                            "SELECT node_json, attempt_count FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                            &[run_id, node_id],
                        )
                        .map_err(|error| error.to_string())?
                        .map(|row| {
                            let node_json: String = row.get(0);
                            let attempt_count: i32 = row.get(1);
                            (node_json, i64::from(attempt_count))
                        });
                    let indexed_root = indexed_recursive_root_pg(&mut tx, run_id, node_id)?;
                    let recursive_state = recursive_row
                        .map(|(node_json, attempt_count)| {
                            stale_recursive_identity(
                                node_id,
                                node_json,
                                attempt_count,
                                indexed_root.as_deref(),
                            )
                        })
                        .transpose()?
                        .flatten();
                    let mut recursive_tree_missing = false;
                    let mut recursive_node_missing = false;
                    let recursive_retry = if let Some((recursive_node_id, attempt, _, _, malformed)) =
                        recursive_state.as_ref()
                    {
                        if *malformed {
                            false
                        } else { match super::recursive_execution::recursive_retry_allowed_pg(
                            &mut tx,
                            run_id,
                            recursive_node_id,
                            &recursive_retry_usage(),
                        ) {
                            Ok(allowed) => *attempt <= i64::from(MAX_RECURSIVE_RETRIES) && allowed,
                            Err(error) if error == "recursive_tree_missing" => {
                                recursive_tree_missing = true;
                                false
                            }
                            Err(error) if error == "recursive_node_missing" => {
                                recursive_node_missing = true;
                                false
                            }
                            Err(error) => return Err(error),
                        }}
                    } else {
                        false
                    };
                    let recovery_status = if recursive_state.is_some() && !recursive_retry {
                        "failed"
                    } else {
                        "pending"
                    };
                    let recovery_reason = if recursive_state
                        .as_ref()
                        .is_some_and(|(_, _, _, _, malformed)| *malformed)
                    {
                        "recursive_node_identity_malformed"
                    } else if recursive_tree_missing {
                        "recursive_tree_missing"
                    } else if recursive_node_missing {
                        "recursive_node_missing"
                    } else {
                        "recursive_retry_exhausted"
                    };
                    let mut recovered_node_json = recursive_state
                        .as_ref()
                        .and_then(|(_, _, node_json, _, _)| node_json.clone());
                    if let Some(object) = recovered_node_json
                        .as_mut()
                        .and_then(Value::as_object_mut)
                    {
                        object.insert("status".to_string(), json!(recovery_status));
                        if recovery_status == "failed" {
                            object.insert("completed_at".to_string(), json!(now));
                        } else {
                            object.remove("completed_at");
                        }
                    }
                    let completed_at = (recovery_status == "failed").then_some(now.as_str());
                    let updated = tx
                        .execute(
                            "UPDATE workflow_run_nodes SET status = $1, completed_at = $2,
                             blocked_reason = $3, leased_at = NULL,
                             node_json = COALESCE($4, node_json)
                             WHERE run_id = $5 AND node_id = $6 AND status = 'running'
                               AND leased_at = $7",
                            &[
                                &recovery_status,
                                &completed_at,
                                &(recovery_status == "failed").then_some(recovery_reason),
                                &recovered_node_json.map(|value| value.to_string()),
                                run_id,
                                node_id,
                                leased_at,
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                    if updated > 0 {
                        count += updated as i64;
                        if let Some((recursive_node_id, attempt, _, recursive_root, malformed)) = recursive_state {
                            if malformed {
                                if recursive_root {
                                    super::recursive_execution::sync_recursive_root_terminal_failure_pg(
                                        &mut tx,
                                        run_id,
                                        &format!("workflow:{run_id}:{node_id}:{attempt}"),
                                        RecursiveFailureReason::RecursiveNodeIdentityMalformed,
                                        &now,
                                    )?;
                                } else {
                                    super::recursive_execution::sync_recursive_terminal_failure_pg(
                                        &mut tx,
                                        run_id,
                                        &recursive_node_id,
                                        &format!("workflow:{run_id}:{node_id}:{attempt}"),
                                        RecursiveFailureReason::RecursiveNodeIdentityMalformed,
                                        &now,
                                    )?;
                                }
                            } else if recursive_tree_missing {
                                pg_append_audit(
                                    &mut tx,
                                    &now,
                                    "scheduler",
                                    "recursive.tree_missing_terminalized",
                                    node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "reason": "recursive_tree_missing",
                                    }),
                                )?;
                            } else if recursive_node_missing {
                                pg_append_audit(
                                    &mut tx,
                                    &now,
                                    "scheduler",
                                    "recursive.node_missing_terminalized",
                                    node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "recursive_node_id": recursive_node_id,
                                        "reason": "recursive_node_missing",
                                    }),
                                )?;
                            } else {
                            super::recursive_execution::sync_recursive_stale_recovery_pg(
                                &mut tx,
                                run_id,
                                &recursive_node_id,
                                &format!("workflow:{run_id}:{node_id}:{attempt}"),
                                recursive_retry,
                                &now,
                            )?;
                            }
                        }
                        pg_append_audit(
                            &mut tx,
                            &now,
                            "scheduler",
                            "workflow_node.stale_lease_recovered",
                            node_id,
                            &queue_lease::stale_lease_audit_payload(
                                run_id,
                                leased_at,
                                lease_timeout_ms,
                            ),
                        )?;
                        // Emit agent_step-specific lease_expired audit
                        let task_type_rows = tx
                            .query(
                                "SELECT task_type FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                                &[run_id, node_id],
                            )
                            .map_err(|e| e.to_string())?;
                        if let Some(task_type_row) = task_type_rows.into_iter().next() {
                            let task_type: String = task_type_row.get(0);
                            if task_type == "agent_step" {
                                pg_append_audit(
                                    &mut tx,
                                    &now,
                                    "scheduler",
                                    "agent_step.lease_expired",
                                    node_id,
                                    &json!({
                                        "run_id": run_id,
                                        "node_id": node_id,
                                        "leased_at": leased_at,
                                        "lease_timeout_ms": lease_timeout_ms,
                                    }),
                                )?;
                            }
                        }
                    }
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(count)
            }),
        }
    }

    pub fn count_running_agent_steps_global(&self) -> Result<i64, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(count_running_agent_steps_locked),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(pg_count_running_agent_steps),
        }
    }

    pub fn count_running_agent_steps_for_run(&self, run_id: &str) -> Result<i64, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => {
                self.with_conn(|conn| count_running_agent_steps_for_run_locked(conn, run_id))
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| pg_count_running_agent_steps_for_run(client, run_id))
            }
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
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                ensure_run_exists_locked(&tx, run_id)?;
                let current_owner: Option<String> = tx
                    .query_row(
                        "SELECT pause_reason FROM workflow_runs WHERE run_id = ?1",
                        params![run_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                require_run_execution_owner(run_id, current_owner.as_deref(), None)?;
                let updated_at = self.now();
                update_workflow_run_status_locked(&tx, run_id, status, &updated_at)?;
                insert_workflow_run_event_locked(
                    &tx,
                    run_id,
                    None,
                    event_type,
                    actor,
                    &json!({"reason": reason, "metadata_only": true, "execution_authority": "disabled"}),
                    &updated_at,
                )?;
                append_audit_locked(
                    &tx,
                    &updated_at,
                    actor,
                    event_type,
                    run_id,
                    &json!({"reason": reason, "metadata_only": true, "execution_authority": "disabled"}),
                )?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                pg_ensure_run_exists(&mut tx, run_id)?;
                let current_owner: Option<String> = tx
                    .query_one(
                        "SELECT pause_reason FROM workflow_runs WHERE run_id = $1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                require_run_execution_owner(run_id, current_owner.as_deref(), None)?;
                let updated_at = self.now();
                pg_update_workflow_run_status(&mut tx, run_id, status, &updated_at)?;
                pg_insert_workflow_run_event(
                    &mut tx,
                    run_id,
                    None,
                    event_type,
                    actor,
                    &json!({"reason": reason, "metadata_only": true, "execution_authority": "disabled"}),
                    &updated_at,
                )?;
                pg_append_audit(
                    &mut tx,
                    &updated_at,
                    actor,
                    event_type,
                    run_id,
                    &json!({"reason": reason, "metadata_only": true, "execution_authority": "disabled"}),
                )?;
                tx.commit().map_err(|error| error.to_string())
            }),
        }?;
        if is_run_terminal(status) || matches!(status, "blocked" | "error") {
            let _artifact = self.record_automatic_native_scorecard_for_run(run_id, actor)?;
        }
        self.get_workflow_run(run_id)?
            .ok_or_else(|| format!("workflow run not found after update: {run_id}"))
    }
}

fn valid_scope_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

pub(crate) fn insert_workflow_run_event_locked(
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

fn merge_memory_context_injection(
    node_id: &str,
    assembled: Option<Value>,
    memory_context: Option<Value>,
    config: &ContextAssemblyConfig,
) -> Option<Value> {
    let memory_context = match memory_context {
        Some(context) => context,
        None => return assembled,
    };

    let mut injection = assembled.unwrap_or_else(|| {
        json!({
            "schema_version": "context_injection.v1",
            "target_node_id": node_id,
            "source": "agent_state_memory_digest",
            "injection_surface": "node_metadata_only",
            "max_context_tokens": config.max_context_tokens,
            "total_estimated_tokens": 0,
            "included_source_count": 0,
            "truncated": false,
            "sources": [],
        })
    });

    if let Some(obj) = injection.as_object_mut() {
        let predecessor_estimated = obj
            .get("total_estimated_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let predecessor_included = obj
            .get("sources")
            .and_then(Value::as_array)
            .map(|sources| {
                sources
                    .iter()
                    .filter_map(|source| source.get("included_tokens").and_then(Value::as_u64))
                    .sum::<u64>()
            })
            .unwrap_or(0);
        let memory_estimated = memory_context
            .get("estimated_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let memory_included = memory_context
            .get("included_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let was_truncated = obj
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || memory_context
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        obj.insert(
            "max_context_tokens".to_string(),
            json!(config.max_context_tokens),
        );
        obj.insert(
            "total_estimated_tokens".to_string(),
            json!(predecessor_estimated.saturating_add(memory_estimated)),
        );
        obj.insert(
            "included_token_total".to_string(),
            json!(predecessor_included.saturating_add(memory_included)),
        );
        obj.insert("truncated".to_string(), json!(was_truncated));
        obj.insert("memory_context".to_string(), memory_context);
        if obj.get("source").and_then(Value::as_str) == Some("completed_predecessor_node_results") {
            obj.insert(
                "source".to_string(),
                json!("completed_predecessor_node_results_plus_agent_memory"),
            );
        }
    }

    Some(injection)
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
            serde_json::from_str(&text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("workflow run approval receipt is invalid JSON: {error}"),
                    )),
                )
            })
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

fn product_approval_binding_string<'a>(binding: &'a Value, field: &str) -> Result<&'a str, String> {
    binding
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("product output approval binding missing {field}"))
}

fn product_approval_verification_sha256(workspace_json: &str) -> Result<String, String> {
    let workspace: Value =
        serde_json::from_str(workspace_json).map_err(|error| error.to_string())?;
    let verification = workspace
        .get("verification")
        .ok_or_else(|| "approval blocked: current verification missing".to_string())?;
    let bytes = serde_json::to_vec(verification).map_err(|error| error.to_string())?;
    Ok(hex::encode(sha2::Sha256::digest(bytes)))
}

fn validate_product_output_approval_binding_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    node_id: &str,
    binding: &Value,
) -> Result<(), String> {
    let task_id = product_approval_binding_string(binding, "product_task_id")?;
    let expected_version = binding
        .get("expected_task_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "product output approval binding missing expected_task_version".to_string()
        })?;
    let task = conn
        .query_row(
            "SELECT status, version, run_id, workspace_record_id, source_revision, output_intent,
                    target_id, target_repo_path
             FROM product_tasks WHERE task_id = ?1",
            params![task_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("product task not found: {task_id}"))?;
    if task.0 != "awaiting_approval" || task.1 != expected_version as i64 {
        return Err("stale product task version or state at approval commit".to_string());
    }
    let workspace_id = product_approval_binding_string(binding, "workspace_record_id")?;
    let source_revision = product_approval_binding_string(binding, "source_revision")?;
    let output_intent = product_approval_binding_string(binding, "output_intent")?;
    if task.2.as_deref() != Some(run_id)
        || task.3.as_deref() != Some(workspace_id)
        || task.4 != source_revision
        || task.5 != output_intent
    {
        return Err("product output approval task binding changed".to_string());
    }
    let output_target = binding
        .get("output_target")
        .ok_or_else(|| "product output approval target missing".to_string())?;
    if output_target.get("target_id").and_then(Value::as_str) != Some(task.6.as_str())
        || output_target
            .get("target_repo_path")
            .and_then(Value::as_str)
            != Some(task.7.as_str())
    {
        return Err("product output approval target binding changed".to_string());
    }
    let node_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
            params![run_id, node_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if node_count != 1 {
        return Err("product output approval workflow node binding missing".to_string());
    }
    let workspace = conn
        .query_row(
            "SELECT status, run_id, source_revision, workspace_path, workspace_json
             FROM supervised_patch_workspaces WHERE workspace_id = ?1",
            params![workspace_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "approval blocked: workspace missing".to_string())?;
    if matches!(workspace.0.as_str(), "quarantined" | "cleaned" | "rejected")
        || workspace.1 != run_id
        || workspace.2 != source_revision
        || binding.get("workspace_path").and_then(Value::as_str) != Some(workspace.3.as_str())
    {
        return Err("product output approval workspace binding changed".to_string());
    }
    if product_approval_verification_sha256(&workspace.4)?
        != product_approval_binding_string(binding, "verification_sha256")?
    {
        return Err("product output approval verification binding changed".to_string());
    }
    let artifact_id = product_approval_binding_string(binding, "artifact_id")?;
    let artifact = conn
        .query_row(
            "SELECT workspace_id, run_id, source_revision, patch_hash, changed_files_json, target_id
             FROM supervised_patch_artifacts WHERE artifact_id = ?1",
            params![artifact_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "approval blocked: artifact missing".to_string())?;
    let changed_files: Value =
        serde_json::from_str(&artifact.4).map_err(|error| error.to_string())?;
    if artifact.0 != workspace_id
        || artifact.1 != run_id
        || artifact.2 != source_revision
        || artifact.3 != product_approval_binding_string(binding, "patch_hash")?
        || binding.get("changed_files") != Some(&changed_files)
        || artifact.5 != task.6
    {
        return Err("product output approval artifact binding changed".to_string());
    }
    Ok(())
}

fn next_sequence(conn: &rusqlite::Connection, table: &str, column: &str) -> Result<i64, String> {
    let sql = format!("SELECT COALESCE(MAX({column}), 0) + 1 FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

fn insert_agent_state_for_node_sqlite(
    conn: &rusqlite::Connection,
    run_id: &str,
    node: &Value,
    created_at: &str,
    actor: &str,
) -> Result<(), String> {
    if node.get("task_type").and_then(Value::as_str) != Some("agent_step") {
        return Ok(());
    }
    let agent_id = required_node_string(node, "agent_id")?;
    let role = required_node_string(node, "agent_role")?;
    let node_id = required_node_string(node, "node_id")?;
    let profile_id = required_node_string(node, "profile_id")?;
    let objective = validated_agent_objective(node)?;
    let capabilities = node
        .get("capability_profile")
        .and_then(Value::as_array)
        .ok_or_else(|| "agent_step node missing capability_profile".to_string())?;
    if capabilities.iter().any(|value| value.as_str().is_none()) {
        return Err("agent_step capability_profile must contain strings".to_string());
    }
    let caps_json = Value::Array(capabilities.clone()).to_string();
    let metadata_json = json!({
        "profile_id": profile_id,
        "initial_node_id": node_id,
        "decision_source": "provider_typed_action",
    })
    .to_string();
    let existing_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM agent_state WHERE agent_id=?1 AND run_id=?2",
            params![agent_id, run_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if existing_count == 1 {
        let (existing_role, existing_caps, existing_objective, existing_metadata): (
            String,
            String,
            Option<String>,
            String,
        ) = conn
            .query_row(
                "SELECT role, capability_profile_json, objective, metadata_json
                 FROM agent_state WHERE agent_id=?1 AND run_id=?2",
                params![agent_id, run_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|error| error.to_string())?;
        let existing_profile = serde_json::from_str::<Value>(&existing_metadata)
            .ok()
            .and_then(|value| value.get("profile_id").cloned())
            .and_then(|value| value.as_str().map(str::to_string));
        if existing_role != role
            || existing_caps != caps_json
            || existing_objective.as_deref() != Some(objective)
            || existing_profile.as_deref() != Some(profile_id)
        {
            return Err(format!(
                "agent_step nodes for {agent_id}/{run_id} have conflicting state identity"
            ));
        }
        append_audit_locked(
            conn,
            created_at,
            actor,
            "agent_state.reuse",
            &format!("agent_state/{agent_id}/{run_id}"),
            &json!({
                "agent_id": agent_id,
                "run_id": run_id,
                "node_id": node_id,
                "profile_id": profile_id,
                "source": "workflow_run.create",
            }),
        )?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO agent_state
         (agent_id, run_id, role, capability_profile_json, objective, status,
          scratchpad_summary, redaction_filter, metadata_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'idle', NULL, NULL, ?6, ?7, ?8)",
        params![
            agent_id,
            run_id,
            role,
            caps_json,
            objective,
            metadata_json,
            created_at,
            created_at
        ],
    )
    .map_err(|error| error.to_string())?;
    append_audit_locked(
        conn,
        created_at,
        actor,
        "agent_state.create",
        &format!("agent_state/{agent_id}/{run_id}"),
        &json!({
            "agent_id": agent_id,
            "run_id": run_id,
            "node_id": node_id,
            "profile_id": profile_id,
            "source": "workflow_run.create",
        }),
    )
    .map(|_| ())
}

#[cfg(feature = "pg")]
fn insert_agent_state_for_node_pg(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node: &Value,
    created_at: &str,
    actor: &str,
) -> Result<(), String> {
    if node.get("task_type").and_then(Value::as_str) != Some("agent_step") {
        return Ok(());
    }
    let agent_id = required_node_string(node, "agent_id")?;
    let role = required_node_string(node, "agent_role")?;
    let node_id = required_node_string(node, "node_id")?;
    let profile_id = required_node_string(node, "profile_id")?;
    let objective = validated_agent_objective(node)?;
    let capabilities = node
        .get("capability_profile")
        .and_then(Value::as_array)
        .ok_or_else(|| "agent_step node missing capability_profile".to_string())?;
    if capabilities.iter().any(|value| value.as_str().is_none()) {
        return Err("agent_step capability_profile must contain strings".to_string());
    }
    let caps_json = Value::Array(capabilities.clone()).to_string();
    let metadata_json = json!({
        "profile_id": profile_id,
        "initial_node_id": node_id,
        "decision_source": "provider_typed_action",
    })
    .to_string();
    let existing = client
        .query_opt(
            "SELECT role, capability_profile_json, objective, metadata_json
             FROM agent_state WHERE agent_id=$1 AND run_id=$2",
            &[&agent_id, &run_id],
        )
        .map_err(|error| error.to_string())?;
    if let Some(row) = existing {
        let existing_role: String = row.get(0);
        let existing_caps: String = row.get(1);
        let existing_objective: Option<String> = row.get(2);
        let existing_metadata: String = row.get(3);
        let existing_profile = serde_json::from_str::<Value>(&existing_metadata)
            .ok()
            .and_then(|value| value.get("profile_id").cloned())
            .and_then(|value| value.as_str().map(str::to_string));
        if existing_role != role
            || existing_caps != caps_json
            || existing_objective.as_deref() != Some(objective)
            || existing_profile.as_deref() != Some(profile_id)
        {
            return Err(format!(
                "agent_step nodes for {agent_id}/{run_id} have conflicting state identity"
            ));
        }
        return pg_append_audit(
            client,
            created_at,
            actor,
            "agent_state.reuse",
            &format!("agent_state/{agent_id}/{run_id}"),
            &json!({
                "agent_id": agent_id,
                "run_id": run_id,
                "node_id": node_id,
                "profile_id": profile_id,
                "source": "workflow_run.create",
            }),
        );
    }
    client
        .execute(
            "INSERT INTO agent_state
             (agent_id, run_id, role, capability_profile_json, objective, status,
              scratchpad_summary, redaction_filter, metadata_json, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, 'idle', NULL, NULL, $6, $7, $8)",
            &[
                &agent_id,
                &run_id,
                &role,
                &caps_json,
                &objective,
                &metadata_json,
                &created_at,
                &created_at,
            ],
        )
        .map_err(|error| error.to_string())?;
    pg_append_audit(
        client,
        created_at,
        actor,
        "agent_state.create",
        &format!("agent_state/{agent_id}/{run_id}"),
        &json!({
            "agent_id": agent_id,
            "run_id": run_id,
            "node_id": node_id,
            "profile_id": profile_id,
            "source": "workflow_run.create",
        }),
    )
}

fn required_node_string<'a>(node: &'a Value, field: &str) -> Result<&'a str, String> {
    node.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("agent_step node missing {field}"))
}

fn validated_agent_objective(node: &Value) -> Result<&str, String> {
    let objective = required_node_string(node, "agent_objective")?;
    if objective.len() > MAX_AGENT_OBJECTIVE_BYTES {
        return Err(format!(
            "agent_step objective exceeds {MAX_AGENT_OBJECTIVE_BYTES} byte cap"
        ));
    }
    if contains_sensitive_patterns(objective) {
        return Err("agent_step objective contains secret-shaped content".to_string());
    }
    Ok(objective)
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

#[derive(Debug)]
struct RecursiveUsageEvidence {
    budget: crate::recursive_execution::RecursiveBudget,
    receipt_id: Option<String>,
}

fn recursive_usage_from_output(
    output: &crate::node_executor::NodeExecutionOutput,
    node_metadata: &Value,
    executor_mode: crate::node_executor::RecursiveUsageMode,
) -> Result<RecursiveUsageEvidence, String> {
    let contract: crate::recursive_execution::RecursiveUsageContract = serde_json::from_value(
        node_metadata
            .get("usage_contract")
            .cloned()
            .ok_or_else(|| "fixture_usage_contract_missing".to_string())?,
    )
    .map_err(|_| "fixture_usage_contract_invalid".to_string())?;
    if matches!(
        contract,
        crate::recursive_execution::RecursiveUsageContract::Unavailable
    ) || matches!(
        executor_mode,
        crate::node_executor::RecursiveUsageMode::Unavailable
    ) {
        return Err("recursive_usage_unavailable".to_string());
    }
    match executor_mode {
        crate::node_executor::RecursiveUsageMode::Fixture => {
            if !matches!(
                contract,
                crate::recursive_execution::RecursiveUsageContract::Fixture { .. }
            ) {
                return Err("fixture_usage_contract_invalid".to_string());
            }
            let usage = contract
                .fixture_usage()
                .expect("fixture contract has fixture usage");
            if usage.calls_remaining == 0
                || usage.tokens_remaining == 0
                || usage.cost_micros_remaining == 0
                || usage.time_ms_remaining == 0
            {
                return Err("fixture_usage_contract_invalid".to_string());
            }
            return Ok(RecursiveUsageEvidence {
                budget: usage,
                receipt_id: recursive_usage_receipt_from_output(output)?,
            });
        }
        crate::node_executor::RecursiveUsageMode::Measured => {}
        crate::node_executor::RecursiveUsageMode::Unavailable => unreachable!(),
    }
    let reported_usage = output
        .output
        .as_deref()
        .and_then(|result| serde_json::from_str::<Value>(result).ok())
        .and_then(|result| result.get("provider_usage").cloned())
        .filter(|usage| {
            usage
                .get("provider_id")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty())
                && usage
                    .get("model")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.is_empty())
                && usage.get("token_provenance").and_then(Value::as_str)
                    == Some("provider_reported")
                && usage.get("cost_provenance").and_then(Value::as_str) == Some("harness_derived")
        })
        .ok_or_else(|| "recursive_usage_unavailable".to_string())?;
    let token_count = match (output.input_tokens, output.output_tokens) {
        (Some(input), Some(output)) if input >= 0 && output >= 0 => {
            (input as u64).saturating_add(output as u64)
        }
        _ => return Err("recursive_usage_unavailable".to_string()),
    };
    if reported_usage.get("input_tokens").and_then(Value::as_i64) != output.input_tokens
        || reported_usage.get("output_tokens").and_then(Value::as_i64) != output.output_tokens
    {
        return Err("recursive_usage_unavailable".to_string());
    }
    let cost_micros = output
        .estimated_cost
        .filter(|cost| cost.is_finite() && *cost >= 0.0)
        .map(|cost| (cost * 1_000_000.0).ceil() as u64)
        .ok_or_else(|| "recursive_usage_unavailable".to_string())?;
    if reported_usage
        .get("estimated_cost_usd")
        .and_then(Value::as_f64)
        != output.estimated_cost
    {
        return Err("recursive_usage_unavailable".to_string());
    }
    let time_ms = output
        .latency_ms
        .filter(|latency| *latency >= 0)
        .map(|latency| latency as u64)
        .ok_or_else(|| "recursive_usage_unavailable".to_string())?;
    let receipt_id = recursive_usage_receipt_from_output(output)?
        .ok_or_else(|| "recursive_usage_unavailable".to_string())?;
    Ok(RecursiveUsageEvidence {
        budget: crate::recursive_execution::RecursiveBudget {
            calls_remaining: 1,
            tokens_remaining: token_count,
            cost_micros_remaining: cost_micros,
            time_ms_remaining: time_ms,
        },
        receipt_id: Some(receipt_id),
    })
}

fn recursive_usage_receipt_from_output(
    output: &crate::node_executor::NodeExecutionOutput,
) -> Result<Option<String>, String> {
    let Some(receipt) = output
        .output
        .as_deref()
        .and_then(|result| serde_json::from_str::<Value>(result).ok())
        .and_then(|result| {
            result
                .get("execution_usage_receipt")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    else {
        return Ok(None);
    };
    let hash = receipt
        .strip_prefix("agent-action:")
        .ok_or_else(|| "recursive_usage_unavailable".to_string())?;
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("recursive_usage_unavailable".to_string());
    }
    Ok(Some(receipt))
}

fn recursive_usage_failure_reason(error: &str) -> Option<RecursiveFailureReason> {
    match error {
        "fixture_usage_contract_missing" => {
            Some(RecursiveFailureReason::FixtureUsageContractMissing)
        }
        "fixture_usage_contract_invalid" => {
            Some(RecursiveFailureReason::FixtureUsageContractInvalid)
        }
        "recursive_usage_unavailable" => Some(RecursiveFailureReason::RecursiveUsageUnavailable),
        "recursive_node_identity_malformed" => {
            Some(RecursiveFailureReason::RecursiveNodeIdentityMalformed)
        }
        _ => None,
    }
}

fn recursive_retry_usage() -> crate::recursive_execution::RecursiveBudget {
    crate::recursive_execution::RecursiveBudget {
        // A stale lease has no measured execution usage. Retry admission is
        // bounded by the already-held node reservation and must not spend a
        // second call merely because the first lease became stale.
        calls_remaining: 0,
        tokens_remaining: 0,
        cost_micros_remaining: 0,
        time_ms_remaining: 0,
    }
}

struct RecursiveCompletionIdentity {
    node_json: Value,
    recursive_node_id: Option<String>,
    root: bool,
    terminal_reason: Option<RecursiveFailureReason>,
    metadata_writable: bool,
}

fn resolve_recursive_completion_identity(
    node_id: &str,
    node_json_text: &str,
    indexed_root_node_id: Option<&str>,
    recursive_enabled: bool,
) -> Result<RecursiveCompletionIdentity, String> {
    let parsed = serde_json::from_str::<Value>(node_json_text).ok();
    let indexed = indexed_root_node_id.is_some();
    let root = indexed_root_node_id == Some(node_id);
    let valid_object = parsed.as_ref().is_some_and(Value::is_object);
    let marker_valid = parsed.as_ref().is_some_and(|metadata| {
        let child_value = metadata.get("recursive_node_id");
        let root_value = metadata.get("recursive_root_node_id");
        let child = child_value.and_then(Value::as_str);
        let root_marker = root_value.and_then(Value::as_str);
        let markers_well_typed = child_value
            .is_none_or(|value| value.as_str().is_some_and(|marker| !marker.is_empty()))
            && root_value
                .is_none_or(|value| value.as_str().is_some_and(|marker| !marker.is_empty()));
        if !markers_well_typed || (child_value.is_some() && root_value.is_some()) {
            return false;
        }
        if root {
            child.is_none() && root_marker == Some(node_id)
        } else if indexed {
            child == Some(node_id) && root_marker.is_none()
        } else if !recursive_enabled {
            matches!(
                (child, root_marker),
                (None, None) | (Some(_), None) | (None, Some(_))
            )
        } else {
            metadata.get("recursive_node_id").is_none()
                && metadata.get("recursive_root_node_id").is_none()
        }
    });
    if !indexed && !marker_valid {
        return Err("recursive_node_identity_malformed".to_string());
    }
    let malformed = indexed && (!valid_object || !marker_valid);
    Ok(RecursiveCompletionIdentity {
        node_json: parsed.unwrap_or(Value::Null),
        recursive_node_id: indexed.then(|| node_id.to_string()),
        root,
        terminal_reason: malformed
            .then_some(RecursiveFailureReason::RecursiveNodeIdentityMalformed),
        metadata_writable: !malformed && valid_object,
    })
}

fn indexed_recursive_root_sqlite(
    conn: &rusqlite::Connection,
    run_id: &str,
    node_id: &str,
) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT t.root_node_id
         FROM recursive_execution_nodes n
         JOIN recursive_execution_trees t ON t.root_run_id=n.root_run_id
         WHERE n.node_id=?1 AND n.root_run_id=?2",
        params![node_id, run_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| error.to_string())
}

#[cfg(feature = "pg")]
fn indexed_recursive_root_pg(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node_id: &str,
) -> Result<Option<String>, String> {
    client
        .query_opt(
            "SELECT t.root_node_id
             FROM recursive_execution_nodes n
             JOIN recursive_execution_trees t ON t.root_run_id=n.root_run_id
             WHERE n.node_id=$1 AND n.root_run_id=$2",
            &[&node_id, &run_id],
        )
        .map_err(|error| error.to_string())
        .map(|row| row.map(|row| row.get(0)))
}

type StaleRecursiveIdentity = (String, i64, Option<Value>, bool, bool);

fn stale_recursive_identity(
    node_id: &str,
    node_json: String,
    attempt_count: i64,
    indexed_root_node_id: Option<&str>,
) -> Result<Option<StaleRecursiveIdentity>, String> {
    let identity = resolve_recursive_completion_identity(
        node_id,
        &node_json,
        indexed_root_node_id,
        crate::recursive_execution::recursive_enabled(),
    )?;
    Ok(identity.recursive_node_id.map(|recursive_node_id| {
        (
            recursive_node_id,
            attempt_count,
            identity.metadata_writable.then_some(identity.node_json),
            identity.root,
            identity.terminal_reason
                == Some(RecursiveFailureReason::RecursiveNodeIdentityMalformed),
        )
    }))
}

fn recursive_failure_reason_sqlite(
    conn: &rusqlite::Connection,
    root_run_id: &str,
    recursive_node_id: &str,
) -> Result<Option<String>, String> {
    Ok(
        super::recursive_execution::load_recursive_tree_sqlite(conn, root_run_id)?.and_then(
            |tree| {
                tree.nodes.get(recursive_node_id).and_then(|node| {
                    node.failure_reason
                        .map(|reason| reason.as_str().to_string())
                })
            },
        ),
    )
}

#[cfg(feature = "pg")]
fn recursive_failure_reason_pg(
    client: &mut impl postgres::GenericClient,
    root_run_id: &str,
    recursive_node_id: &str,
) -> Result<Option<String>, String> {
    Ok(
        super::recursive_execution::load_recursive_tree_pg(client, root_run_id)?.and_then(|tree| {
            tree.nodes.get(recursive_node_id).and_then(|node| {
                node.failure_reason
                    .map(|reason| reason.as_str().to_string())
            })
        }),
    )
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

/// Count agent_step nodes currently running across all runs (inside SQLite lock).
fn count_running_agent_steps_locked(conn: &rusqlite::Connection) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM workflow_run_nodes WHERE task_type = 'agent_step' AND status = 'running'",
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

fn workflow_node_is_recursive_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    node_id: &str,
) -> Result<bool, String> {
    let node_json: String = conn
        .query_row(
            "SELECT node_json FROM workflow_run_nodes WHERE run_id = ?1 AND node_id = ?2",
            params![run_id, node_id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let indexed_root = indexed_recursive_root_sqlite(conn, run_id, node_id)?;
    recursive_identity_for_claim(node_id, &node_json, indexed_root.as_deref())
}

fn count_running_recursive_steps_locked(conn: &rusqlite::Connection) -> Result<i64, String> {
    let mut stmt = conn
        .prepare(
            "SELECT run_id, node_id FROM workflow_run_nodes
             WHERE task_type = 'agent_step' AND status = 'running'",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut count = 0_i64;
    for row in rows {
        let (run_id, node_id) = row.map_err(|e| e.to_string())?;
        if workflow_node_is_recursive_locked(conn, &run_id, &node_id)? {
            count += 1;
        }
    }
    Ok(count)
}

/// Count agent_step nodes running for a specific run (inside SQLite lock).
fn count_running_agent_steps_for_run_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id = ?1 AND task_type = 'agent_step' AND status = 'running'",
        params![run_id],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn pg_count_running_agent_steps(client: &mut impl postgres::GenericClient) -> Result<i64, String> {
    client
        .query_one(
            "SELECT COUNT(*) FROM workflow_run_nodes WHERE task_type = 'agent_step' AND status = 'running'",
            &[],
        )
        .map_err(|e| e.to_string())
        .map(|row| row.get(0))
}

#[cfg(feature = "pg")]
fn pg_workflow_node_is_recursive(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node_id: &str,
) -> Result<bool, String> {
    let row = client
        .query_one(
            "SELECT node_json FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
            &[&run_id, &node_id],
        )
        .map_err(|e| e.to_string())?;
    let node_json: String = row.get(0);
    let indexed_root = indexed_recursive_root_pg(client, run_id, node_id)?;
    recursive_identity_for_claim(node_id, &node_json, indexed_root.as_deref())
}

#[cfg(feature = "pg")]
fn pg_count_running_recursive_steps(
    client: &mut impl postgres::GenericClient,
) -> Result<i64, String> {
    let rows = client
        .query(
            "SELECT run_id, node_id FROM workflow_run_nodes
             WHERE task_type = 'agent_step' AND status = 'running'",
            &[],
        )
        .map_err(|e| e.to_string())?;
    let mut count = 0_i64;
    for row in rows {
        let run_id: String = row.get(0);
        let node_id: String = row.get(1);
        if pg_workflow_node_is_recursive(client, &run_id, &node_id)? {
            count += 1;
        }
    }
    Ok(count)
}

/// The scheduler's recursive-capacity decision must fail closed. A malformed
/// node payload is not silently treated as an ordinary agent step, otherwise a
/// corrupt marker could bypass the global three-lease cap.
fn recursive_marker_from_node_json(node_json: &str) -> Result<bool, String> {
    let metadata: Value = serde_json::from_str(node_json)
        .map_err(|_| "recursive_node_identity_malformed".to_string())?;
    let child_marker = metadata.get("recursive_node_id");
    let root_marker = metadata.get("recursive_root_node_id");
    if child_marker.is_some() && root_marker.is_some() {
        return Err("recursive_node_identity_malformed".to_string());
    }
    let Some(marker) = child_marker.or(root_marker) else {
        return Ok(false);
    };
    let marker = marker
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "recursive_node_identity_malformed".to_string())?;
    let _ = marker;
    Ok(true)
}

fn recursive_identity_for_claim(
    node_id: &str,
    node_json: &str,
    indexed_root_node_id: Option<&str>,
) -> Result<bool, String> {
    let metadata: Value = serde_json::from_str(node_json)
        .map_err(|_| "recursive_node_identity_malformed".to_string())?;
    let child = metadata.get("recursive_node_id").and_then(Value::as_str);
    let root = metadata
        .get("recursive_root_node_id")
        .and_then(Value::as_str);
    if let Some(indexed_root) = indexed_root_node_id {
        let exact = if indexed_root == node_id {
            child.is_none() && root == Some(node_id)
        } else {
            child == Some(node_id) && root.is_none()
        };
        return exact
            .then_some(true)
            .ok_or_else(|| "recursive_node_identity_malformed".to_string());
    }
    if !recursive_marker_from_node_json(node_json)? {
        return Ok(false);
    }
    if child == Some(node_id) || root == Some(node_id) {
        Ok(true)
    } else {
        Err("recursive_node_identity_malformed".to_string())
    }
}

#[cfg(feature = "pg")]
fn pg_count_running_agent_steps_for_run(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<i64, String> {
    client
        .query_one(
            "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id = $1 AND task_type = 'agent_step' AND status = 'running'",
            &[&run_id],
        )
        .map_err(|e| e.to_string())
        .map(|row| row.get(0))
}

fn find_ready_node_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
    skip: &[&str],
    agent_executor: Option<bool>,
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
        if skip.contains(&node_id.as_str()) {
            continue;
        }
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
        if agent_executor.is_some_and(|is_agent_executor| {
            (target_task_type == "agent_step") != is_agent_executor
        }) {
            continue;
        }
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

fn next_ready_task_type_sqlite(
    conn: &rusqlite::Connection,
    run_id: &str,
    agent_concurrency_caps: Option<(usize, usize)>,
) -> Result<Option<String>, String> {
    let mut skip = Vec::<String>::new();
    let mut saw_capped_agent = false;
    loop {
        let skip_refs = skip.iter().map(String::as_str).collect::<Vec<_>>();
        let Some(node_id) = find_ready_node_locked(conn, run_id, &skip_refs, None)? else {
            return Ok(saw_capped_agent.then(|| "agent_step".to_string()));
        };
        let task_type: String = conn
            .query_row(
                "SELECT task_type FROM workflow_run_nodes WHERE run_id=?1 AND node_id=?2",
                params![run_id, node_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if task_type == "agent_step" {
            if let Some((global_cap, per_run_cap)) = agent_concurrency_caps {
                let global_running = count_running_agent_steps_locked(conn)?;
                let per_run_running = count_running_agent_steps_for_run_locked(conn, run_id)?;
                if global_running >= global_cap as i64 || per_run_running >= per_run_cap as i64 {
                    saw_capped_agent = true;
                    skip.push(node_id);
                    continue;
                }
            }
        }
        return Ok(Some(task_type));
    }
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

fn retryable_node_failure(output: &crate::node_executor::NodeExecutionOutput) -> bool {
    if output.status != "failed" {
        return false;
    }
    !matches!(
        output.error_domain.as_deref(),
        Some(
            "tool_effect_outcome_unknown"
                | "tool_effect_rejected_after_execution"
                | "tool_execution_outcome_unknown"
                | "tool_execution_receipt_invalid"
                | "tool_execution_receipt_error"
                | "provider_outcome_unknown"
                | "product_token_budget_exhausted"
                | "product_call_budget_exhausted"
                | "product_token_usage_unavailable"
                | "cli_timeout"
                | "cli_wait_error"
                | "cli_stdout_reader_error"
                | "cli_stderr_reader_error"
                | "cli_combined_reader_error"
                | "cli_output_limit_exceeded"
                | "cli_process_tree_cleanup_error"
                | "cli_process_tree_containment_unavailable"
        )
    )
}

struct ProductManagedTokenSettlement {
    output: crate::node_executor::NodeExecutionOutput,
    usage_state: Option<Value>,
}

fn enforce_product_managed_token_budget(
    mut output: crate::node_executor::NodeExecutionOutput,
    node_metadata: &Value,
) -> ProductManagedTokenSettlement {
    let is_managed_product_apply = node_metadata.get("executor_class").and_then(Value::as_str)
        == Some("managed_coding")
        && node_metadata
            .pointer("/managed_supervised_patch/operation")
            .and_then(Value::as_str)
            == Some("product_apply");
    if !is_managed_product_apply {
        return ProductManagedTokenSettlement {
            output,
            usage_state: None,
        };
    }

    let Some(limit) = node_metadata
        .pointer("/product_budget/total_tokens")
        .and_then(Value::as_u64)
        .filter(|limit| *limit > 0)
    else {
        output.status = "failed".to_string();
        output.error_domain = Some("product_token_usage_unavailable".to_string());
        output.error_message = Some(
            "product token budget is unavailable; managed completion cannot be trusted".to_string(),
        );
        return ProductManagedTokenSettlement {
            output,
            usage_state: None,
        };
    };

    let execution_attempt = node_metadata
        .get("execution_attempt")
        .and_then(Value::as_u64)
        .filter(|attempt| *attempt > 0);
    let prior = node_metadata.get("product_managed_usage");
    let prior_cumulative = match (execution_attempt, prior) {
        (Some(1), None) => Some(0),
        (Some(attempt), Some(prior))
            if prior.get("schema_version").and_then(Value::as_str)
                == Some("product_managed_usage.v1")
                && prior.get("last_attempt").and_then(Value::as_u64)
                    == Some(attempt.saturating_sub(1)) =>
        {
            prior.get("cumulative_tokens").and_then(Value::as_u64)
        }
        _ => None,
    };
    let Some(prior_cumulative) = prior_cumulative else {
        output.status = "failed".to_string();
        output.error_domain = Some("product_token_usage_unavailable".to_string());
        output.error_message =
            Some("managed token usage attempt lineage is missing or stale".to_string());
        return ProductManagedTokenSettlement {
            output,
            usage_state: None,
        };
    };
    let execution_attempt = execution_attempt.expect("validated managed execution attempt");

    let measured = match (output.input_tokens, output.output_tokens) {
        (Some(input), Some(output_tokens)) if input >= 0 && output_tokens >= 0 => {
            (input as u64).checked_add(output_tokens as u64)
        }
        _ => None,
    };
    let Some(measured) = measured else {
        if output.status == "completed" || retryable_node_failure(&output) {
            output.status = "failed".to_string();
            output.error_domain = Some("product_token_usage_unavailable".to_string());
            output.error_message = Some(
                "managed executor did not provide authoritative non-negative token usage"
                    .to_string(),
            );
        }
        return ProductManagedTokenSettlement {
            output,
            usage_state: Some(json!({
                "schema_version": "product_managed_usage.v1",
                "last_attempt": execution_attempt,
                "cumulative_tokens": prior_cumulative,
                "current_attempt_tokens": Value::Null,
                "status": "unavailable",
                "raw_output_stored": false,
            })),
        };
    };

    let Some(cumulative) = prior_cumulative.checked_add(measured) else {
        output.status = "failed".to_string();
        output.error_domain = Some("product_token_budget_exhausted".to_string());
        output.error_message = Some("budget_exhausted:total_tokens overflow".to_string());
        return ProductManagedTokenSettlement {
            output,
            usage_state: None,
        };
    };

    if cumulative > limit && (output.status == "completed" || retryable_node_failure(&output)) {
        output.status = "failed".to_string();
        output.error_domain = Some("product_token_budget_exhausted".to_string());
        output.error_message = Some(format!(
            "budget_exhausted:total_tokens limit={limit} cumulative={cumulative}"
        ));
    }
    ProductManagedTokenSettlement {
        output,
        usage_state: Some(json!({
            "schema_version": "product_managed_usage.v1",
            "last_attempt": execution_attempt,
            "cumulative_tokens": cumulative,
            "current_attempt_tokens": measured,
            "limit_tokens": limit,
            "status": if cumulative > limit { "exhausted" } else { "within_budget" },
            "raw_output_stored": false,
        })),
    }
}

#[cfg(test)]
mod product_managed_token_budget_tests {
    use super::*;
    use crate::node_executor::{NodeExecutionOutput, ProcessOutcome};

    fn metadata(limit: f64) -> Value {
        json!({
            "executor_class": "managed_coding",
            "budget": limit,
            "product_budget": {"total_tokens": limit as u64},
            "execution_attempt": 1,
            "managed_supervised_patch": {"operation": "product_apply"}
        })
    }

    fn completed(input_tokens: Option<i64>, output_tokens: Option<i64>) -> NodeExecutionOutput {
        NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "codex_cli".to_string(),
            output: Some("bounded result".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens,
            output_tokens,
            estimated_cost: None,
            latency_ms: Some(25),
            process_outcome: Some(ProcessOutcome::exited(0)),
            resolved_model: None,
        }
    }

    #[test]
    fn measured_managed_usage_within_product_budget_stays_complete() {
        let output =
            enforce_product_managed_token_budget(completed(Some(40), Some(10)), &metadata(50.0))
                .output;
        assert_eq!(output.status, "completed");
        assert_eq!(output.process_outcome.unwrap().exit_code, Some(0));
    }

    #[test]
    fn managed_usage_overage_is_nonretryable_and_preserves_measurement() {
        let output =
            enforce_product_managed_token_budget(completed(Some(41), Some(10)), &metadata(50.0))
                .output;
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("product_token_budget_exhausted")
        );
        assert_eq!(output.input_tokens, Some(41));
        assert_eq!(output.output_tokens, Some(10));
        assert_eq!(output.process_outcome.as_ref().unwrap().exit_code, Some(0));
        assert!(!retryable_node_failure(&output));
    }

    #[test]
    fn missing_managed_usage_fails_closed_without_retry() {
        let output =
            enforce_product_managed_token_budget(completed(None, None), &metadata(50.0)).output;
        assert_eq!(output.status, "failed");
        assert_eq!(
            output.error_domain.as_deref(),
            Some("product_token_usage_unavailable")
        );
        assert!(!retryable_node_failure(&output));
    }

    #[test]
    fn product_call_budget_exhaustion_is_not_retryable() {
        let mut output = completed(None, None);
        output.status = "failed".to_string();
        output.error_domain = Some("product_call_budget_exhausted".to_string());
        assert!(!retryable_node_failure(&output));
    }

    #[test]
    fn managed_process_boundary_failures_are_not_retryable() {
        for error_domain in [
            "cli_timeout",
            "cli_wait_error",
            "cli_stdout_reader_error",
            "cli_stderr_reader_error",
            "cli_combined_reader_error",
            "cli_output_limit_exceeded",
            "cli_process_tree_cleanup_error",
            "cli_process_tree_containment_unavailable",
        ] {
            let mut output = completed(None, None);
            output.status = "failed".to_string();
            output.error_domain = Some(error_domain.to_string());
            assert!(
                !retryable_node_failure(&output),
                "{error_domain} must not retry after managed-process handling"
            );
        }
    }

    #[test]
    fn failed_attempt_usage_is_accumulated_before_retry() {
        let mut failed = completed(Some(30), Some(10));
        failed.status = "failed".to_string();
        failed.error_domain = Some("retryable_fixture_failure".to_string());
        let first = enforce_product_managed_token_budget(failed, &metadata(50.0));
        assert_eq!(first.output.status, "failed");
        assert_eq!(first.usage_state.as_ref().unwrap()["cumulative_tokens"], 40);

        let mut second_metadata = metadata(50.0);
        second_metadata["execution_attempt"] = json!(2);
        second_metadata["product_managed_usage"] = first.usage_state.unwrap();
        let second =
            enforce_product_managed_token_budget(completed(Some(15), Some(5)), &second_metadata);
        assert_eq!(second.output.status, "failed");
        assert_eq!(
            second.output.error_domain.as_deref(),
            Some("product_token_budget_exhausted")
        );
        assert_eq!(second.usage_state.unwrap()["cumulative_tokens"], 60);
    }
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

fn workflow_run_terminal_audit_details_locked(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<Value, String> {
    let boundaries_json: String = conn
        .query_row(
            "SELECT boundaries_json FROM workflow_runs WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let boundaries: Value = serde_json::from_str(&boundaries_json)
        .map_err(|error| format!("invalid workflow run boundaries: {error}"))?;
    Ok(workflow_run_terminal_audit_details(&boundaries))
}

fn workflow_run_terminal_audit_details(boundaries: &Value) -> Value {
    let execution_authority = boundaries
        .get("execution_authority")
        .and_then(Value::as_str)
        .unwrap_or("disabled");
    json!({
        "metadata_only": execution_authority == "disabled",
        "execution_authority": execution_authority,
    })
}

pub fn workflow_run_boundaries() -> Value {
    json!({
        "execution_authority": "disabled",
        "target_repository_writes": "disabled",
        "runtime_workers": "env_gated_supervised",
        "sandbox_process_execution": "not_implemented",
        "provider_calls": "not_invoked",
        "approval_execution_authority": "disabled",
        "resume_execution_authority": "disabled",
        "cancel_execution_authority": "disabled",
        "deploy_merge_controls": "not_available",
    })
}

fn workflow_plan_has_execution_authority(plan: &Value) -> bool {
    let declared = plan
        .pointer("/boundaries/execution_authority")
        .and_then(Value::as_str)
        .is_some_and(|authority| authority != "disabled");
    let routed_executor = matches!(
        plan.pointer("/advisory/requires_executor")
            .and_then(Value::as_str),
        Some("agent_step" | "adaptive_provider")
    );
    let executable_node = plan
        .pointer("/graph/nodes")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes.iter().any(|node| {
                node.get("task_type").and_then(Value::as_str) == Some("agent_step")
                    || node.get("adaptive_execution").is_some()
                    || node.get("managed_supervised_patch").is_some()
            })
        });
    declared || routed_executor || executable_node
}

fn workflow_run_boundaries_for_plan(plan: &Value) -> Result<Value, String> {
    if !workflow_plan_has_execution_authority(plan) {
        return Ok(workflow_run_boundaries());
    }
    let boundaries = plan
        .get("boundaries")
        .and_then(Value::as_object)
        .ok_or_else(|| "executable workflow plan is missing authority boundaries".to_string())?;
    let authority = boundaries
        .get("execution_authority")
        .and_then(Value::as_str)
        .ok_or_else(|| "executable workflow plan is missing execution_authority".to_string())?;
    if authority == "disabled" {
        return Err("executable workflow plan declares disabled execution authority".to_string());
    }
    Ok(Value::Object(boundaries.clone()))
}

// ---------------------------------------------------------------------------
// PostgreSQL helper functions
// ---------------------------------------------------------------------------

#[cfg(feature = "pg")]
fn pg_workflow_run_terminal_audit_details(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
) -> Result<Value, String> {
    let boundaries_json: String = client
        .query_one(
            "SELECT boundaries_json FROM workflow_runs WHERE run_id = $1",
            &[&run_id],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    let boundaries: Value = serde_json::from_str(&boundaries_json)
        .map_err(|error| format!("invalid workflow run boundaries: {error}"))?;
    Ok(workflow_run_terminal_audit_details(&boundaries))
}

#[cfg(feature = "pg")]
fn pg_next_sequence(
    client: &mut impl postgres::GenericClient,
    table: &str,
    column: &str,
) -> Result<i64, String> {
    let sequence_owner = format!("local_product_store_sequence:{table}:{column}");
    client
        .execute(
            "SELECT pg_advisory_xact_lock(hashtext($1))",
            &[&sequence_owner],
        )
        .map_err(|e| e.to_string())?;
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
fn pg_validate_product_output_approval_binding(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    node_id: &str,
    binding: &Value,
) -> Result<(), String> {
    let task_id = product_approval_binding_string(binding, "product_task_id")?;
    let expected_version = binding
        .get("expected_task_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "product output approval binding missing expected_task_version".to_string()
        })?;
    let task = client
        .query_opt(
            "SELECT status, version, run_id, workspace_record_id, source_revision, output_intent,
                    target_id, target_repo_path
             FROM product_tasks WHERE task_id = $1 FOR UPDATE",
            &[&task_id],
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("product task not found: {task_id}"))?;
    let status: String = task.get(0);
    let version: i64 = task.get(1);
    let task_run_id: Option<String> = task.get(2);
    let task_workspace_id: Option<String> = task.get(3);
    let task_source_revision: String = task.get(4);
    let task_output_intent: String = task.get(5);
    let task_target_id: String = task.get(6);
    let task_target_repo_path: String = task.get(7);
    if status != "awaiting_approval" || version != expected_version as i64 {
        return Err("stale product task version or state at approval commit".to_string());
    }
    let workspace_id = product_approval_binding_string(binding, "workspace_record_id")?;
    let source_revision = product_approval_binding_string(binding, "source_revision")?;
    let output_intent = product_approval_binding_string(binding, "output_intent")?;
    if task_run_id.as_deref() != Some(run_id)
        || task_workspace_id.as_deref() != Some(workspace_id)
        || task_source_revision != source_revision
        || task_output_intent != output_intent
    {
        return Err("product output approval task binding changed".to_string());
    }
    let output_target = binding
        .get("output_target")
        .ok_or_else(|| "product output approval target missing".to_string())?;
    if output_target.get("target_id").and_then(Value::as_str) != Some(task_target_id.as_str())
        || output_target
            .get("target_repo_path")
            .and_then(Value::as_str)
            != Some(task_target_repo_path.as_str())
    {
        return Err("product output approval target binding changed".to_string());
    }
    let node_count: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
            &[&run_id, &node_id],
        )
        .map_err(|error| error.to_string())?
        .get(0);
    if node_count != 1 {
        return Err("product output approval workflow node binding missing".to_string());
    }
    let workspace = client
        .query_opt(
            "SELECT status, run_id, source_revision, workspace_path, workspace_json
             FROM supervised_patch_workspaces WHERE workspace_id = $1 FOR UPDATE",
            &[&workspace_id],
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "approval blocked: workspace missing".to_string())?;
    let workspace_status: String = workspace.get(0);
    let workspace_run_id: String = workspace.get(1);
    let workspace_source_revision: String = workspace.get(2);
    let workspace_path: String = workspace.get(3);
    let workspace_json: String = workspace.get(4);
    if matches!(
        workspace_status.as_str(),
        "quarantined" | "cleaned" | "rejected"
    ) || workspace_run_id != run_id
        || workspace_source_revision != source_revision
        || binding.get("workspace_path").and_then(Value::as_str) != Some(workspace_path.as_str())
    {
        return Err("product output approval workspace binding changed".to_string());
    }
    if product_approval_verification_sha256(&workspace_json)?
        != product_approval_binding_string(binding, "verification_sha256")?
    {
        return Err("product output approval verification binding changed".to_string());
    }
    let artifact_id = product_approval_binding_string(binding, "artifact_id")?;
    let artifact = client
        .query_opt(
            "SELECT workspace_id, run_id, source_revision, patch_hash, changed_files_json, target_id
             FROM supervised_patch_artifacts WHERE artifact_id = $1 FOR UPDATE",
            &[&artifact_id],
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "approval blocked: artifact missing".to_string())?;
    let artifact_workspace_id: String = artifact.get(0);
    let artifact_run_id: String = artifact.get(1);
    let artifact_source_revision: String = artifact.get(2);
    let artifact_patch_hash: String = artifact.get(3);
    let changed_files_json: String = artifact.get(4);
    let artifact_target_id: String = artifact.get(5);
    let changed_files: Value =
        serde_json::from_str(&changed_files_json).map_err(|error| error.to_string())?;
    if artifact_workspace_id != workspace_id
        || artifact_run_id != run_id
        || artifact_source_revision != source_revision
        || artifact_patch_hash != product_approval_binding_string(binding, "patch_hash")?
        || binding.get("changed_files") != Some(&changed_files)
        || artifact_target_id != task_target_id
    {
        return Err("product output approval artifact binding changed".to_string());
    }
    Ok(())
}

#[cfg(feature = "pg")]
pub(crate) fn pg_append_audit(
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
fn pg_next_event_sequence(client: &mut impl postgres::GenericClient) -> Result<i64, String> {
    pg_next_sequence(client, "workflow_run_events", "event_sequence")
}

#[cfg(feature = "pg")]
pub(crate) fn pg_insert_workflow_run_event(
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
    skip: &[&str],
    agent_executor: Option<bool>,
) -> Result<Option<String>, String> {
    let rows = client
        .query(
            "SELECT node_id FROM workflow_run_nodes WHERE run_id = $1 AND status = 'pending' ORDER BY node_id",
            &[&run_id],
        )
        .map_err(|e| e.to_string())?;
    let pending_nodes: Vec<String> = rows.iter().map(|r| r.get(0)).collect();

    for node_id in pending_nodes {
        if skip.contains(&node_id.as_str()) {
            continue;
        }
        let target_task_type: String = client
            .query_one(
                "SELECT task_type FROM workflow_run_nodes WHERE run_id = $1 AND node_id = $2",
                &[&run_id, &node_id],
            )
            .map_err(|e| e.to_string())?
            .get(0);
        if agent_executor.is_some_and(|is_agent_executor| {
            (target_task_type == "agent_step") != is_agent_executor
        }) {
            continue;
        }
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
fn pg_next_ready_task_type(
    client: &mut impl postgres::GenericClient,
    run_id: &str,
    agent_concurrency_caps: Option<(usize, usize)>,
) -> Result<Option<String>, String> {
    let mut skip = Vec::<String>::new();
    let mut saw_capped_agent = false;
    loop {
        let skip_refs = skip.iter().map(String::as_str).collect::<Vec<_>>();
        let Some(node_id) = pg_find_ready_node(client, run_id, &skip_refs, None)? else {
            return Ok(saw_capped_agent.then(|| "agent_step".to_string()));
        };
        let task_type: String = client
            .query_one(
                "SELECT task_type FROM workflow_run_nodes WHERE run_id=$1 AND node_id=$2",
                &[&run_id, &node_id],
            )
            .map_err(|error| error.to_string())?
            .get(0);
        if task_type == "agent_step" {
            if let Some((global_cap, per_run_cap)) = agent_concurrency_caps {
                let global_running = pg_count_running_agent_steps(client)?;
                let per_run_running = pg_count_running_agent_steps_for_run(client, run_id)?;
                if global_running >= global_cap as i64 || per_run_running >= per_run_cap as i64 {
                    saw_capped_agent = true;
                    skip.push(node_id);
                    continue;
                }
            }
        }
        return Ok(Some(task_type));
    }
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
    rows.iter()
        .map(|row| {
            let text: String = row.get(0);
            serde_json::from_str(&text)
                .map_err(|error| format!("workflow run approval receipt is invalid JSON: {error}"))
        })
        .collect()
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
    rows.iter()
        .map(|row| {
            let text: String = row.get(0);
            serde_json::from_str(&text)
                .map_err(|error| format!("workflow run approval receipt is invalid JSON: {error}"))
        })
        .collect()
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

#[cfg(test)]
mod recursive_scheduler_tests {
    use super::*;
    use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
    use crate::recursive_execution::{
        recursive_root_creation_receipt_sha256, RecursiveBudget, RecursiveProposal, RecursiveScope,
        RecursiveTree, RECURSIVE_ROOT_AUTHORITY_VERSION,
    };
    use std::collections::BTreeSet;
    use std::sync::{Arc, Condvar, Mutex};

    fn bind_scheduler_tree_root(tree: &mut RecursiveTree, agent_id: &str, plan_receipt: &str) {
        let receipt = recursive_root_creation_receipt_sha256(
            plan_receipt,
            &tree.root_run_id,
            &tree.workflow_id,
            &tree.root_node_id,
            agent_id,
        );
        let root_node_id = tree.root_node_id.clone();
        tree.bind_root_identity(agent_id, &root_node_id, &receipt)
            .expect("root identity");
    }

    fn scheduler_root_authority(tree: &RecursiveTree) -> Value {
        json!({
            "schema_version": RECURSIVE_ROOT_AUTHORITY_VERSION,
            "scope": tree.root_scope,
            "capabilities": tree.root_capabilities,
            "tree_budget": tree.root_budget_limit,
            "child_budget": tree.root_child_budget_limit,
            "usage_contract": {
                "kind": "fixture",
                "calls": 1,
                "tokens": 1,
                "cost_micros": 1,
                "time_ms": 1
            }
        })
    }

    struct RecursiveCapExecutor;

    impl NodeExecutor for RecursiveCapExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some("recursive cap fixture".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(0),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn recursive_usage_mode(&self) -> crate::node_executor::RecursiveUsageMode {
            crate::node_executor::RecursiveUsageMode::Fixture
        }
    }

    struct BlockingRecursiveExecutor {
        gate: Arc<(Mutex<(usize, usize)>, Condvar)>,
        slot: usize,
    }

    struct MeasuredRecursiveExecutor;

    impl NodeExecutor for MeasuredRecursiveExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "agent_step".to_string(),
                output: Some(
                    json!({
                        "execution_usage_receipt": concat!(
                            "agent-action:",
                            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        ),
                        "provider_usage": {
                            "provider_id": "measured-provider",
                            "model": "measured-model",
                            "input_tokens": 2,
                            "output_tokens": 3,
                            "estimated_cost_usd": 0.000004,
                            "token_provenance": "provider_reported",
                            "cost_provenance": "harness_derived"
                        }
                    })
                    .to_string(),
                ),
                error_domain: None,
                error_message: None,
                input_tokens: Some(2),
                output_tokens: Some(3),
                estimated_cost: Some(0.000004),
                latency_ms: Some(9),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn recursive_usage_mode(&self) -> crate::node_executor::RecursiveUsageMode {
            crate::node_executor::RecursiveUsageMode::Measured
        }
    }

    impl NodeExecutor for BlockingRecursiveExecutor {
        fn execute_node(&self, _input: &NodeExecutionInput) -> NodeExecutionOutput {
            let (lock, ready) = &*self.gate;
            let mut state = lock.lock().expect("recursive claim gate");
            state.0 += 1;
            ready.notify_all();
            while state.1 <= self.slot {
                state = ready.wait(state).expect("recursive claim release");
            }
            RecursiveCapExecutor.execute_node(_input)
        }

        fn executor_type_name(&self) -> &str {
            "agent_step"
        }

        fn recursive_usage_mode(&self) -> crate::node_executor::RecursiveUsageMode {
            crate::node_executor::RecursiveUsageMode::Fixture
        }
    }

    fn assert_concurrent_recursive_claim_cap(
        factory: Arc<dyn Fn() -> LocalProductStore + Send + Sync>,
        suffix: &str,
    ) {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let setup = factory();
        let run_ids = (0..=MAX_RECURSIVE_LEASES)
            .map(|index| format!("recursive-concurrent-cap-run-{suffix}-{index}"))
            .collect::<Vec<_>>();
        for (index, run_id) in run_ids.iter().enumerate() {
            let node_id = format!("recursive-concurrent-cap-node-{suffix}-{index}");
            let workflow_id = format!("recursive-concurrent-cap-workflow-{suffix}-{index}");
            let agent_id = format!("recursive-concurrent-cap-agent-{suffix}-{index}");
            let plan_receipt = format!("recursive-concurrent-cap-receipt-{suffix}-{index}");
            let scope = RecursiveScope {
                repository: None,
                allowed_paths: BTreeSet::new(),
                capabilities: BTreeSet::from(["read".to_string()]),
            };
            let mut tree = RecursiveTree::new_with_root_node_id(
                run_id,
                &workflow_id,
                &node_id,
                "concurrent recursive root",
                scope.clone(),
                scope.capabilities.clone(),
                RecursiveBudget {
                    calls_remaining: 2,
                    tokens_remaining: 20,
                    cost_micros_remaining: 20,
                    time_ms_remaining: 200,
                },
            );
            tree.bind_root_execution_scope(
                Some(&format!("recursive-cap-tenant-{index}")),
                Some(&format!("recursive-cap-workspace-{index}")),
            )
            .expect("root scope");
            bind_scheduler_tree_root(&mut tree, &agent_id, &plan_receipt);
            setup
                .import_workflow_run(&json!({
                    "run_id": run_id,
                    "workflow_id": workflow_id,
                    "status": "running",
                    "boundaries": {
                        "execution_authority": "managed",
                        "tenant_id": format!("recursive-cap-tenant-{index}"),
                        "workspace_id": format!("recursive-cap-workspace-{index}")
                    },
                    "nodes": [{
                        "node_id": node_id,
                        "task_type": "agent_step",
                        "status": "pending",
                        "agent_id": agent_id,
                        "recursive_root_node_id": node_id,
                        "agent_objective": "concurrent recursive root",
                        "capability_profile": tree.root_capabilities,
                        "recursive_root_authority": scheduler_root_authority(&tree),
                        "creation_receipt_sha256": plan_receipt,
                        "decision_source": "fixture"
                    }],
                    "edges": [],
                    "events": [],
                    "approvals": []
                }))
                .expect("concurrent recursive run");
            setup
                .save_recursive_tree_with_expected_version(&tree, 0)
                .expect("concurrent recursive tree");
        }

        let gate = Arc::new((Mutex::new((0usize, 0usize)), Condvar::new()));
        let mut workers = Vec::new();
        for (index, run_id) in run_ids
            .iter()
            .take(MAX_RECURSIVE_LEASES)
            .cloned()
            .enumerate()
        {
            let worker_factory = factory.clone();
            let worker_gate = gate.clone();
            workers.push(std::thread::spawn(move || {
                worker_factory().tick_with_executor(
                    &run_id,
                    &format!("recursive-concurrent-scheduler-{index}"),
                    0,
                    &BlockingRecursiveExecutor {
                        gate: worker_gate,
                        slot: index,
                    },
                )
            }));
        }
        let (lock, ready) = &*gate;
        let mut state = lock.lock().expect("recursive claim gate");
        while state.0 < MAX_RECURSIVE_LEASES {
            state = ready.wait(state).expect("three recursive claims");
        }
        drop(state);

        let fourth = factory()
            .tick_with_executor(
                &run_ids[MAX_RECURSIVE_LEASES],
                "recursive-concurrent-scheduler-fourth",
                0,
                &RecursiveCapExecutor,
            )
            .expect("fourth recursive claim");
        assert_eq!(fourth["action"], "no_ready_node");
        let fourth_run = setup
            .get_workflow_run(&run_ids[MAX_RECURSIVE_LEASES])
            .expect("fourth run")
            .expect("fourth run exists");
        assert_eq!(fourth_run["nodes"][0]["db_status"], "pending");

        for (index, worker) in workers.into_iter().enumerate() {
            let mut state = lock.lock().expect("recursive release gate");
            state.1 = index + 1;
            ready.notify_all();
            drop(state);
            let result = worker
                .join()
                .expect("recursive claim worker")
                .expect("tick");
            assert_eq!(result["action"], "node_executed");
        }
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn recursive_usage_contract_distinguishes_fixture_measured_and_unavailable() {
        let fixture_output = NodeExecutionOutput {
            status: "completed".to_string(),
            executor_type: "agent_step".to_string(),
            output: Some("fixture".to_string()),
            error_domain: None,
            error_message: None,
            input_tokens: None,
            output_tokens: None,
            estimated_cost: None,
            latency_ms: Some(0),
            process_outcome: None,
            resolved_model: None,
        };
        let fixture = recursive_usage_from_output(
            &fixture_output,
            &json!({
                "decision_source": "fixture",
                "usage_contract": {
                    "kind": "fixture",
                    "calls": 1,
                    "tokens": 2,
                    "cost_micros": 3,
                    "time_ms": 4
                }
            }),
            crate::node_executor::RecursiveUsageMode::Fixture,
        )
        .expect("bounded fixture usage");
        assert_eq!(fixture.budget.tokens_remaining, 2);
        assert_eq!(
            recursive_usage_from_output(
                &fixture_output,
                &json!({"decision_source": "fixture"}),
                crate::node_executor::RecursiveUsageMode::Fixture,
            )
            .expect_err("missing fixture contract"),
            "fixture_usage_contract_missing"
        );
        assert_eq!(
            recursive_usage_from_output(
                &fixture_output,
                &json!({"usage_contract": {"kind": "unavailable"}}),
                crate::node_executor::RecursiveUsageMode::Fixture,
            )
            .expect_err("unavailable usage"),
            "recursive_usage_unavailable"
        );
        let measured = NodeExecutionOutput {
            executor_type: "agent_step".to_string(),
            output: Some(
                json!({
                    "execution_usage_receipt": concat!(
                        "agent-action:",
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    ),
                    "provider_usage": {
                        "provider_id": "measured-provider",
                        "model": "measured-model",
                        "input_tokens": 2,
                        "output_tokens": 3,
                        "estimated_cost_usd": 0.000004,
                        "token_provenance": "provider_reported",
                        "cost_provenance": "harness_derived"
                    }
                })
                .to_string(),
            ),
            input_tokens: Some(2),
            output_tokens: Some(3),
            estimated_cost: Some(0.000004),
            latency_ms: Some(5),
            ..fixture_output
        };
        assert_eq!(
            recursive_usage_from_output(
                &measured,
                &json!({"usage_contract": {"kind": "measured"}}),
                crate::node_executor::RecursiveUsageMode::Measured,
            )
            .expect("measured usage")
            .budget
            .tokens_remaining,
            5
        );
        let unproven = NodeExecutionOutput {
            output: Some(
                json!({
                    "provider_usage": {
                        "provider_id": "measured-provider",
                        "model": "measured-model",
                        "input_tokens": 2,
                        "output_tokens": 3,
                        "estimated_cost_usd": 0.000004,
                        "token_provenance": "unavailable",
                        "cost_provenance": "unavailable"
                    }
                })
                .to_string(),
            ),
            ..measured
        };
        assert_eq!(
            recursive_usage_from_output(
                &unproven,
                &json!({"usage_contract": {"kind": "measured"}}),
                crate::node_executor::RecursiveUsageMode::Measured,
            )
            .expect_err("unproven numeric usage must fail closed"),
            "recursive_usage_unavailable"
        );
    }

    fn assert_scheduler_admitted_fixture_child_completes(store: LocalProductStore, suffix: &str) {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        let run_id = format!("recursive-scheduler-fixture-run-{suffix}");
        let workflow_id = format!("recursive-scheduler-fixture-workflow-{suffix}");
        let root_node_id = format!("recursive-scheduler-fixture-root-{suffix}");
        let agent_id = format!("recursive-scheduler-fixture-agent-{suffix}");
        let root_receipt = format!("recursive-scheduler-fixture-root-receipt-{suffix}");
        let scope = RecursiveScope {
            repository: Some("fixture".to_string()),
            allowed_paths: BTreeSet::from(["docs/".to_string()]),
            capabilities: BTreeSet::from(["read".to_string()]),
        };
        let mut tree = RecursiveTree::new_with_root_node_id(
            &run_id,
            &workflow_id,
            &root_node_id,
            "root fixture objective",
            scope.clone(),
            scope.capabilities.clone(),
            RecursiveBudget {
                calls_remaining: 4,
                tokens_remaining: 40,
                cost_micros_remaining: 40,
                time_ms_remaining: 400,
            },
        );
        tree.bind_root_execution_scope(Some("fixture-tenant"), Some("fixture-workspace"))
            .expect("root execution scope");
        bind_scheduler_tree_root(&mut tree, &agent_id, &root_receipt);
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let admission = tree
            .admit_child(&RecursiveProposal {
                proposal_id: format!("recursive-scheduler-fixture-proposal-{suffix}"),
                parent_node_id: root_node_id.clone(),
                parent_version: tree.nodes[&root_node_id].version,
                objective: "review docs".to_string(),
                context_summary: "fixture context".to_string(),
                requested_scope: scope.clone(),
                requested_capabilities: scope.capabilities.clone(),
                budget: RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 2,
                    cost_micros_remaining: 3,
                    time_ms_remaining: 4,
                },
                receipt_sha256: format!("recursive-scheduler-fixture-action-receipt-{suffix}"),
            })
            .expect("admit child");
        let child_id = admission.node.node_id.clone();
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": workflow_id,
                "status": "running",
                "boundaries": {
                    "execution_authority": "managed",
                    "tenant_id": "fixture-tenant",
                    "workspace_id": "fixture-workspace"
                },
                "nodes": [
                    {
                        "node_id": root_node_id,
                        "task_type": "agent_step",
                        "status": "completed",
                        "agent_id": agent_id,
                        "recursive_root_node_id": root_node_id,
                        "capability_profile": tree.root_capabilities,
                        "recursive_root_authority": scheduler_root_authority(&tree),
                        "creation_receipt_sha256": root_receipt
                    },
                    {
                        "node_id": child_id,
                        "task_type": "agent_step",
                        "status": "pending",
                        "recursive_node_id": child_id,
                        "parent_node_id": root_node_id,
                        "recursive_capabilities": admission.node.capabilities,
                        "recursive_scope": serde_json::to_value(&admission.node.scope)
                            .expect("scope json"),
                        "recursive_tenant_id": admission.node.tenant_id,
                        "recursive_workspace_id": admission.node.workspace_id,
                        "decision_source": "fixture",
                        "usage_contract": {
                            "kind": "fixture",
                            "calls": 1,
                            "tokens": 2,
                            "cost_micros": 3,
                            "time_ms": 4
                        }
                    }
                ],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("workflow");
        store
            .save_recursive_tree_with_expected_version(&tree, 0)
            .expect("tree");
        let tick = store
            .tick_with_executor(&run_id, "scheduler-fixture", 0, &RecursiveCapExecutor)
            .expect("scheduler tick");
        assert_eq!(tick["action"], "node_executed");
        let loaded = store
            .load_recursive_tree(&run_id)
            .expect("tree load")
            .expect("tree exists");
        assert_eq!(loaded.nodes[&child_id].status, "completed");
        assert_eq!(loaded.nodes[&child_id].actual_usage.tokens_remaining, 2);
        assert!(loaded.active_leases.is_empty());
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn scheduler_admitted_fixture_child_leases_and_completes() {
        assert_scheduler_admitted_fixture_child_completes(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite",
        );
    }

    fn assert_root_without_children_is_accounted(
        store: LocalProductStore,
        suffix: &str,
        executor: &dyn NodeExecutor,
        expected_usage: RecursiveBudget,
    ) {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let run_id = format!("recursive-root-only-run-{suffix}");
        let workflow_id = format!("recursive-root-only-workflow-{suffix}");
        let root_id = format!("recursive-root-only-node-{suffix}");
        let agent_id = format!("recursive-root-only-agent-{suffix}");
        let plan_receipt = format!("recursive-root-only-receipt-{suffix}");
        let scope = RecursiveScope {
            repository: None,
            allowed_paths: BTreeSet::new(),
            capabilities: BTreeSet::from(["read".to_string()]),
        };
        let tree_budget = RecursiveBudget {
            calls_remaining: 2,
            tokens_remaining: 20,
            cost_micros_remaining: 20,
            time_ms_remaining: 200,
        };
        let child_budget = RecursiveBudget {
            calls_remaining: 1,
            tokens_remaining: 10,
            cost_micros_remaining: 10,
            time_ms_remaining: 100,
        };
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": workflow_id,
                "status": "running",
                "boundaries": {
                    "execution_authority": "managed",
                    "tenant_id": "root-only-tenant",
                    "workspace_id": "root-only-workspace"
                },
                "nodes": [{
                    "node_id": root_id,
                    "task_type": "agent_step",
                    "status": "pending",
                    "agent_id": agent_id,
                    "agent_objective": "finish the bounded root",
                    "recursive_root_node_id": root_id,
                    "capability_profile": scope.capabilities,
                    "recursive_root_authority": {
                        "schema_version": RECURSIVE_ROOT_AUTHORITY_VERSION,
                        "scope": scope,
                        "capabilities": ["read"],
                        "tree_budget": tree_budget,
                        "child_budget": child_budget,
                        "usage_contract": {
                            "kind": "fixture",
                            "calls": 1,
                            "tokens": 1,
                            "cost_micros": 1,
                            "time_ms": 1
                        }
                    },
                    "creation_receipt_sha256": plan_receipt,
                    "decision_source": "fixture"
                }],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("root-only workflow");
        assert!(store
            .load_recursive_tree(&run_id)
            .expect("tree read")
            .is_none());
        let tick = store
            .tick_with_executor(&run_id, "root-only-scheduler", 0, executor)
            .expect("root-only tick");
        assert_eq!(tick["action"], "node_executed");
        let tree = store
            .load_recursive_tree(&run_id)
            .expect("tree read")
            .expect("root tree initialized");
        assert_eq!(tree.root_node_id, root_id);
        assert_eq!(tree.nodes[&root_id].status, "completed");
        assert_eq!(tree.nodes[&root_id].actual_usage, expected_usage);
        assert!(tree.active_leases.is_empty());
        assert_eq!(
            tree.execution_state,
            crate::recursive_execution::RecursiveExecutionState::Completed
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn sqlite_fixture_root_without_children_is_leased_and_accounted() {
        assert_root_without_children_is_accounted(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite",
            &RecursiveCapExecutor,
            RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 1,
                cost_micros_remaining: 1,
                time_ms_remaining: 1,
            },
        );
    }

    #[test]
    fn sqlite_measured_executor_cannot_use_fixture_accounting() {
        assert_root_without_children_is_accounted(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite-measured",
            &MeasuredRecursiveExecutor,
            RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 5,
                cost_micros_remaining: 4,
                time_ms_remaining: 9,
            },
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_fixture_root_without_children_is_leased_and_accounted() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        assert_root_without_children_is_accounted(
            LocalProductStore::new_postgres(&url, || "2026-07-19T00:00:00Z".to_string())
                .expect("PostgreSQL store"),
            &uuid::Uuid::new_v4().to_string(),
            &RecursiveCapExecutor,
            RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 1,
                cost_micros_remaining: 1,
                time_ms_remaining: 1,
            },
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_measured_executor_cannot_use_fixture_accounting() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        assert_root_without_children_is_accounted(
            LocalProductStore::new_postgres(&url, || "2026-07-19T00:00:00Z".to_string())
                .expect("PostgreSQL store"),
            &uuid::Uuid::new_v4().to_string(),
            &MeasuredRecursiveExecutor,
            RecursiveBudget {
                calls_remaining: 1,
                tokens_remaining: 5,
                cost_micros_remaining: 4,
                time_ms_remaining: 9,
            },
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_scheduler_admitted_fixture_child_leases_and_completes() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        assert_scheduler_admitted_fixture_child_completes(
            LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
                .expect("PostgreSQL store"),
            &uuid::Uuid::new_v4().to_string(),
        );
    }

    fn assert_invalid_recursive_usage_terminalizes(
        store: LocalProductStore,
        suffix: &str,
        usage_contract: Value,
        expected_reason: RecursiveFailureReason,
    ) {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let run_id = format!("recursive-usage-unavailable-run-{suffix}");
        let workflow_id = format!("recursive-usage-unavailable-workflow-{suffix}");
        let root_node_id = format!("recursive-usage-unavailable-root-{suffix}");
        let agent_id = format!("recursive-usage-unavailable-agent-{suffix}");
        let root_receipt = format!("recursive-usage-unavailable-receipt-{suffix}");
        let scope = RecursiveScope {
            repository: Some("fixture".to_string()),
            allowed_paths: BTreeSet::from(["docs/".to_string()]),
            capabilities: BTreeSet::from(["read".to_string()]),
        };
        let mut tree = RecursiveTree::new_with_root_node_id(
            &run_id,
            &workflow_id,
            &root_node_id,
            "root objective",
            scope.clone(),
            scope.capabilities.clone(),
            RecursiveBudget {
                calls_remaining: 2,
                tokens_remaining: 20,
                cost_micros_remaining: 20,
                time_ms_remaining: 200,
            },
        );
        bind_scheduler_tree_root(&mut tree, &agent_id, &root_receipt);
        let admission = tree
            .admit_child(&RecursiveProposal {
                proposal_id: format!("recursive-usage-unavailable-proposal-{suffix}"),
                parent_node_id: root_node_id.clone(),
                parent_version: tree.nodes[&root_node_id].version,
                objective: "provider child".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: scope.clone(),
                requested_capabilities: scope.capabilities.clone(),
                budget: RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 10,
                    cost_micros_remaining: 10,
                    time_ms_remaining: 100,
                },
                receipt_sha256: format!("recursive-usage-unavailable-action-{suffix}"),
            })
            .expect("admit child");
        let child_id = admission.node.node_id.clone();
        let sibling = tree
            .admit_child(&RecursiveProposal {
                proposal_id: format!("recursive-usage-unavailable-sibling-proposal-{suffix}"),
                parent_node_id: root_node_id.clone(),
                parent_version: tree.nodes[&root_node_id].version,
                objective: "independent provider sibling".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: scope.clone(),
                requested_capabilities: scope.capabilities.clone(),
                budget: RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 10,
                    cost_micros_remaining: 10,
                    time_ms_remaining: 100,
                },
                receipt_sha256: format!("recursive-usage-unavailable-sibling-action-{suffix}"),
            })
            .expect("admit sibling");
        let sibling_id = sibling.node.node_id.clone();
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": workflow_id,
                "status": "running",
                "boundaries": {"execution_authority": "managed"},
                "nodes": [
                    {
                        "node_id": root_node_id,
                        "task_type": "agent_step",
                        "status": "completed",
                        "agent_id": agent_id,
                        "recursive_root_node_id": root_node_id,
                        "capability_profile": tree.root_capabilities,
                        "recursive_root_authority": scheduler_root_authority(&tree),
                        "creation_receipt_sha256": root_receipt
                    },
                    {
                        "node_id": child_id,
                        "task_type": "agent_step",
                        "status": "pending",
                        "recursive_node_id": child_id,
                        "parent_node_id": root_node_id,
                        "recursive_capabilities": admission.node.capabilities,
                        "recursive_scope": admission.node.scope,
                        "recursive_tenant_id": admission.node.tenant_id,
                        "recursive_workspace_id": admission.node.workspace_id,
                        "decision_source": "provider",
                        "usage_contract": usage_contract
                    },
                    {
                        "node_id": sibling_id,
                        "task_type": "agent_step",
                        "status": "pending",
                        "recursive_node_id": sibling_id,
                        "parent_node_id": root_node_id,
                        "recursive_capabilities": sibling.node.capabilities,
                        "recursive_scope": sibling.node.scope,
                        "recursive_tenant_id": sibling.node.tenant_id,
                        "recursive_workspace_id": sibling.node.workspace_id,
                        "decision_source": "provider",
                        "usage_contract": usage_contract
                    }
                ],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("workflow");
        store
            .save_recursive_tree_with_expected_version(&tree, 0)
            .expect("tree");
        let tick = store
            .tick_with_executor(
                &run_id,
                "scheduler-usage-unavailable",
                0,
                &RecursiveCapExecutor,
            )
            .expect("terminal tick");
        assert_eq!(tick["action"], "node_executed");
        let failed_node_id = tick["node_id"].as_str().expect("executed node id");
        let terminalized_node_id = if failed_node_id == child_id {
            sibling_id.as_str()
        } else {
            assert_eq!(failed_node_id, sibling_id);
            child_id.as_str()
        };
        let loaded = store
            .load_recursive_tree(&run_id)
            .expect("tree")
            .expect("tree exists");
        assert_eq!(
            loaded.execution_state,
            crate::recursive_execution::RecursiveExecutionState::TerminalFailed
        );
        assert_eq!(
            loaded.nodes[failed_node_id].failure_reason.as_deref(),
            Some(expected_reason.as_str())
        );
        assert_eq!(loaded.nodes[terminalized_node_id].status, "failed");
        assert_eq!(
            loaded.nodes[terminalized_node_id].failure_reason.as_deref(),
            Some(RecursiveFailureReason::TerminalFailed.as_str())
        );
        assert!(loaded.active_leases.is_empty());
        assert_eq!(loaded.reserved_budget, RecursiveBudget::default());
        let run = store
            .get_workflow_run(&run_id)
            .expect("run")
            .expect("run exists");
        let failed_node = run["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|node| node["node_id"] == failed_node_id)
            .expect("failed node");
        assert_eq!(failed_node["db_status"], "failed");
        assert_eq!(failed_node["blocked_reason"], expected_reason.as_str());
        let terminalized_node = run["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|node| node["node_id"] == terminalized_node_id)
            .expect("terminalized node");
        assert_eq!(terminalized_node["db_status"], "failed");
        assert_eq!(
            terminalized_node["blocked_reason"],
            RecursiveFailureReason::TerminalFailed.as_str()
        );
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn unavailable_recursive_usage_terminalizes_sqlite() {
        assert_invalid_recursive_usage_terminalizes(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite",
            json!({"kind": "unavailable"}),
            RecursiveFailureReason::RecursiveUsageUnavailable,
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn unavailable_recursive_usage_terminalizes_postgres() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        assert_invalid_recursive_usage_terminalizes(
            LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
                .expect("PostgreSQL store"),
            &uuid::Uuid::new_v4().to_string(),
            json!({"kind": "unavailable"}),
            RecursiveFailureReason::RecursiveUsageUnavailable,
        );
    }

    #[test]
    fn invalid_fixture_usage_contract_terminalizes_sqlite_with_exact_reason() {
        assert_invalid_recursive_usage_terminalizes(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite-invalid-fixture",
            json!({"kind": "fixture", "calls": 0, "tokens": 1, "cost_micros": 1, "time_ms": 1}),
            RecursiveFailureReason::FixtureUsageContractInvalid,
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn invalid_fixture_usage_contract_terminalizes_postgres_with_exact_reason() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        assert_invalid_recursive_usage_terminalizes(
            LocalProductStore::new_postgres(&url, || "2026-07-19T00:00:00Z".to_string())
                .expect("PostgreSQL store"),
            &uuid::Uuid::new_v4().to_string(),
            json!({"kind": "fixture", "calls": 0, "tokens": 1, "cost_micros": 1, "time_ms": 1}),
            RecursiveFailureReason::FixtureUsageContractInvalid,
        );
    }

    fn assert_one_stale_retry_then_terminal(store: LocalProductStore, suffix: &str) {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let run_id = format!("recursive-stale-run-{suffix}");
        let workflow_id = format!("recursive-stale-workflow-{suffix}");
        let root_id = format!("recursive-stale-root-{suffix}");
        let agent_id = format!("recursive-stale-agent-{suffix}");
        let root_receipt = format!("recursive-stale-root-receipt-{suffix}");
        let scope = RecursiveScope {
            repository: Some("fixture".to_string()),
            allowed_paths: BTreeSet::from(["docs/".to_string()]),
            capabilities: BTreeSet::from(["read".to_string()]),
        };
        let mut tree = RecursiveTree::new_with_root_node_id(
            &run_id,
            &workflow_id,
            &root_id,
            "root objective",
            scope.clone(),
            scope.capabilities.clone(),
            RecursiveBudget {
                calls_remaining: 3,
                tokens_remaining: 30,
                cost_micros_remaining: 30,
                time_ms_remaining: 300,
            },
        );
        bind_scheduler_tree_root(&mut tree, &agent_id, &root_receipt);
        let child = tree
            .admit_child(&RecursiveProposal {
                proposal_id: format!("recursive-stale-proposal-{suffix}"),
                parent_node_id: root_id.clone(),
                parent_version: 1,
                objective: "stale child".to_string(),
                context_summary: "fixture".to_string(),
                requested_scope: scope,
                requested_capabilities: BTreeSet::from(["read".to_string()]),
                budget: RecursiveBudget {
                    calls_remaining: 1,
                    tokens_remaining: 10,
                    cost_micros_remaining: 10,
                    time_ms_remaining: 100,
                },
                receipt_sha256: format!("recursive-stale-action-{suffix}"),
            })
            .expect("child");
        let child_id = child.node.node_id.clone();
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": workflow_id,
                "status": "running",
                "boundaries": {"execution_authority": "managed"},
                "nodes": [
                    {
                        "node_id": root_id,
                        "task_type": "agent_step",
                        "status": "completed",
                        "agent_id": agent_id,
                        "recursive_root_node_id": root_id,
                        "capability_profile": tree.root_capabilities,
                        "recursive_root_authority": scheduler_root_authority(&tree),
                        "creation_receipt_sha256": root_receipt
                    },
                    {
                        "node_id": child_id,
                        "task_type": "agent_step",
                        "status": "pending",
                        "attempt_count": 0,
                        "recursive_node_id": child_id,
                        "recursive_capabilities": child.node.capabilities,
                        "recursive_scope": child.node.scope,
                        "recursive_tenant_id": child.node.tenant_id,
                        "recursive_workspace_id": child.node.workspace_id,
                        "usage_contract": {"kind": "fixture", "calls": 1, "tokens": 1, "cost_micros": 1, "time_ms": 1},
                        "decision_source": "fixture"
                    }
                ],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("workflow");
        store
            .save_recursive_tree_with_expected_version(&tree, 0)
            .expect("tree");

        let seed_attempt = |store: &LocalProductStore, attempt: i64| -> Result<(), String> {
            let lease_id = format!("workflow:{run_id}:{child_id}:{attempt}");
            match &store.db {
                DatabaseConnection::Sqlite(_) => store.with_conn(|conn| {
                    conn.execute(
                        "UPDATE workflow_run_nodes SET status='running', attempt_count=?1,
                         leased_at='2026-07-17T00:00:00Z' WHERE run_id=?2 AND node_id=?3",
                        params![attempt, run_id, child_id],
                    )
                    .map_err(|error| error.to_string())?;
                    super::super::recursive_execution::sync_recursive_lease_sqlite(
                        conn,
                        &run_id,
                        &child_id,
                        &lease_id,
                        "2026-07-17T00:00:00Z",
                    )
                }),
                #[cfg(feature = "pg")]
                DatabaseConnection::Pg(_) => store.with_pg_conn(|client| {
                    let mut tx = client.transaction().map_err(|error| error.to_string())?;
                    let attempt = i32::try_from(attempt).map_err(|error| error.to_string())?;
                    tx.execute(
                        "UPDATE workflow_run_nodes SET status='running', attempt_count=$1,
                         leased_at='2026-07-17T00:00:00Z' WHERE run_id=$2 AND node_id=$3",
                        &[&attempt, &run_id, &child_id],
                    )
                    .map_err(|error| error.to_string())?;
                    super::super::recursive_execution::sync_recursive_lease_pg(
                        &mut tx,
                        &run_id,
                        &child_id,
                        &lease_id,
                        "2026-07-17T00:00:00Z",
                    )?;
                    tx.commit().map_err(|error| error.to_string())
                }),
            }
        };

        seed_attempt(&store, 1).expect("first attempt");
        assert!(store.recover_stale_leases(0).expect("first recovery") >= 1);
        let after_first = store
            .load_recursive_tree(&run_id)
            .expect("tree")
            .expect("tree exists");
        assert_eq!(after_first.nodes[&child_id].retry_count, 1);
        assert_eq!(after_first.nodes[&child_id].status, "ready");
        assert_eq!(
            after_first.nodes[&child_id].actual_usage,
            RecursiveBudget::default()
        );
        assert!(
            after_first.usage_receipts.is_empty(),
            "stale recovery must leave the attempt receipt available for measured late usage"
        );
        let stale_lease_id = format!("workflow:{run_id}:{child_id}:1");
        let late_completion = match &store.db {
            DatabaseConnection::Sqlite(_) => store.with_conn(|conn| {
                super::super::recursive_execution::sync_recursive_completion_sqlite(
                    conn,
                    &run_id,
                    &child_id,
                    &stale_lease_id,
                    true,
                    false,
                    &RecursiveBudget {
                        calls_remaining: 1,
                        tokens_remaining: 1,
                        cost_micros_remaining: 1,
                        time_ms_remaining: 1,
                    },
                    "2026-07-18T00:00:00Z",
                )
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => store.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                super::super::recursive_execution::sync_recursive_completion_pg(
                    &mut tx,
                    &run_id,
                    &child_id,
                    &stale_lease_id,
                    true,
                    false,
                    &RecursiveBudget {
                        calls_remaining: 1,
                        tokens_remaining: 1,
                        cost_micros_remaining: 1,
                        time_ms_remaining: 1,
                    },
                    "2026-07-18T00:00:00Z",
                )
            }),
        };
        assert!(late_completion.is_err(), "stale completion must be fenced");
        let after_late = store
            .load_recursive_tree(&run_id)
            .expect("tree")
            .expect("tree exists");
        assert_eq!(after_late.nodes[&child_id].retry_count, 1);
        assert_eq!(after_late.nodes[&child_id].status, "ready");
        assert_eq!(
            after_late.nodes[&child_id].actual_usage,
            RecursiveBudget::default()
        );

        seed_attempt(&store, 2).expect("replacement attempt");
        assert!(store.recover_stale_leases(0).expect("second recovery") >= 1);
        let terminal = store
            .load_recursive_tree(&run_id)
            .expect("tree")
            .expect("tree exists");
        assert_eq!(terminal.nodes[&child_id].status, "failed");
        assert_eq!(
            terminal.nodes[&child_id].failure_reason.as_deref(),
            Some(RecursiveFailureReason::RetryExhausted.as_str())
        );
        assert_eq!(
            terminal.nodes[&child_id].actual_usage,
            RecursiveBudget::default()
        );
        assert!(terminal.active_leases.is_empty());
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn sqlite_scheduler_retries_one_stale_recursive_lease() {
        assert_one_stale_retry_then_terminal(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite",
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_scheduler_retries_one_stale_recursive_lease() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        assert_one_stale_retry_then_terminal(
            LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
                .expect("PostgreSQL store"),
            &uuid::Uuid::new_v4().to_string(),
        );
    }

    fn assert_root_stale_retry_is_bounded_and_late_usage_is_fenced(
        store: LocalProductStore,
        suffix: &str,
    ) {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let run_id = format!("recursive-root-stale-run-{suffix}");
        let workflow_id = format!("recursive-root-stale-workflow-{suffix}");
        let root_id = format!("recursive-root-stale-node-{suffix}");
        let agent_id = format!("recursive-root-stale-agent-{suffix}");
        let plan_receipt = format!("recursive-root-stale-receipt-{suffix}");
        let scope = RecursiveScope {
            repository: Some("fixture".to_string()),
            allowed_paths: BTreeSet::from(["docs/".to_string()]),
            capabilities: BTreeSet::from(["read".to_string()]),
        };
        let mut tree = RecursiveTree::new_with_root_node_id(
            &run_id,
            &workflow_id,
            &root_id,
            "root stale objective",
            scope.clone(),
            scope.capabilities.clone(),
            RecursiveBudget {
                calls_remaining: 3,
                tokens_remaining: 30,
                cost_micros_remaining: 30,
                time_ms_remaining: 300,
            },
        );
        bind_scheduler_tree_root(&mut tree, &agent_id, &plan_receipt);
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": workflow_id,
                "status": "running",
                "boundaries": {"execution_authority": "managed"},
                "nodes": [{
                    "node_id": root_id,
                    "task_type": "agent_step",
                    "status": "pending",
                    "attempt_count": 0,
                    "agent_id": agent_id,
                    "recursive_root_node_id": root_id,
                    "capability_profile": tree.root_capabilities,
                    "recursive_root_authority": scheduler_root_authority(&tree),
                    "creation_receipt_sha256": plan_receipt,
                    "decision_source": "fixture",
                    "usage_contract": {"kind": "fixture", "calls": 1, "tokens": 1, "cost_micros": 1, "time_ms": 1}
                }],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("root workflow");
        store
            .save_recursive_tree_with_expected_version(&tree, 0)
            .expect("root tree");

        let seed_attempt = |attempt: i64| -> Result<(), String> {
            let lease_id = format!("workflow:{run_id}:{root_id}:{attempt}");
            match &store.db {
                DatabaseConnection::Sqlite(_) => store.with_conn(|conn| {
                    conn.execute(
                        "UPDATE workflow_run_nodes SET status='running', attempt_count=?1,
                         leased_at='2026-07-17T00:00:00Z' WHERE run_id=?2 AND node_id=?3",
                        params![attempt, run_id, root_id],
                    )
                    .map_err(|error| error.to_string())?;
                    super::super::recursive_execution::sync_recursive_lease_sqlite(
                        conn,
                        &run_id,
                        &root_id,
                        &lease_id,
                        "2026-07-17T00:00:00Z",
                    )
                }),
                #[cfg(feature = "pg")]
                DatabaseConnection::Pg(_) => store.with_pg_conn(|client| {
                    let mut tx = client.transaction().map_err(|error| error.to_string())?;
                    let attempt = i32::try_from(attempt).map_err(|error| error.to_string())?;
                    tx.execute(
                        "UPDATE workflow_run_nodes SET status='running', attempt_count=$1,
                         leased_at='2026-07-17T00:00:00Z' WHERE run_id=$2 AND node_id=$3",
                        &[&attempt, &run_id, &root_id],
                    )
                    .map_err(|error| error.to_string())?;
                    super::super::recursive_execution::sync_recursive_lease_pg(
                        &mut tx,
                        &run_id,
                        &root_id,
                        &lease_id,
                        "2026-07-17T00:00:00Z",
                    )?;
                    tx.commit().map_err(|error| error.to_string())
                }),
            }
        };

        seed_attempt(1).expect("first root attempt");
        assert!(store.recover_stale_leases(0).expect("first recovery") >= 1);
        let retried = store
            .load_recursive_tree(&run_id)
            .expect("load root tree")
            .expect("root tree exists");
        assert_eq!(retried.nodes[&root_id].retry_count, 1);
        assert_eq!(retried.nodes[&root_id].status, "ready");

        seed_attempt(2).expect("replacement root attempt");
        let replacement_lease = format!("workflow:{run_id}:{root_id}:2");
        let stale_receipt = format!("workflow:{run_id}:{root_id}:1");
        let usage = RecursiveBudget {
            calls_remaining: 1,
            tokens_remaining: 1,
            cost_micros_remaining: 1,
            time_ms_remaining: 1,
        };
        for _ in 0..2 {
            match &store.db {
                DatabaseConnection::Sqlite(_) => store
                    .with_conn(|conn| {
                        super::super::recursive_execution::record_recursive_root_late_usage_sqlite(
                            conn,
                            &run_id,
                            &stale_receipt,
                            &usage,
                            "2026-07-18T00:00:00Z",
                        )
                    })
                    .expect("record SQLite root late usage"),
                #[cfg(feature = "pg")]
                DatabaseConnection::Pg(_) => store
                    .with_pg_conn(|client| {
                        let mut tx = client.transaction().map_err(|error| error.to_string())?;
                        let recorded =
                            super::super::recursive_execution::record_recursive_root_late_usage_pg(
                                &mut tx,
                                &run_id,
                                &stale_receipt,
                                &usage,
                                "2026-07-18T00:00:00Z",
                            )?;
                        tx.commit().map_err(|error| error.to_string())?;
                        Ok(recorded)
                    })
                    .expect("record PostgreSQL root late usage"),
            };
        }
        let after_late = store
            .load_recursive_tree(&run_id)
            .expect("load after late usage")
            .expect("root tree exists");
        assert_eq!(after_late.nodes[&root_id].actual_usage, usage);
        assert_eq!(
            after_late.nodes[&root_id].lease_id.as_deref(),
            Some(replacement_lease.as_str())
        );
        assert!(after_late.active_leases.contains(&replacement_lease));
        assert_eq!(after_late.usage_receipts.len(), 1);

        assert!(store.recover_stale_leases(0).expect("second recovery") >= 1);
        let terminal = store
            .load_recursive_tree(&run_id)
            .expect("load terminal root")
            .expect("root tree exists");
        assert_eq!(terminal.nodes[&root_id].status, "failed");
        assert_eq!(terminal.nodes[&root_id].retry_count, 1);
        assert_eq!(
            terminal.nodes[&root_id].failure_reason.as_deref(),
            Some(RecursiveFailureReason::RetryExhausted.as_str())
        );
        assert_eq!(terminal.nodes[&root_id].actual_usage, usage);
        assert!(terminal.active_leases.is_empty());
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[test]
    fn sqlite_scheduler_retries_one_stale_recursive_root_lease() {
        assert_root_stale_retry_is_bounded_and_late_usage_is_fenced(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite",
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_scheduler_retries_one_stale_recursive_root_lease() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        assert_root_stale_retry_is_bounded_and_late_usage_is_fenced(
            LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
                .expect("PostgreSQL store"),
            &uuid::Uuid::new_v4().to_string(),
        );
    }

    fn assert_malformed_stale_recursive_identity_fails_closed(
        store: LocalProductStore,
        suffix: &str,
    ) {
        let run_id = format!("malformed-stale-recursive-run-{suffix}");
        let node_id = format!("malformed-stale-recursive-node-{suffix}");
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": format!("malformed-stale-recursive-workflow-{suffix}"),
                "status": "running",
                "boundaries": {"execution_authority": "managed"},
                "nodes": [{
                    "node_id": node_id,
                    "task_type": "agent_step",
                    "status": "pending",
                    "attempt_count": 0,
                    "recursive_node_id": 7
                }],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("workflow");
        match &store.db {
            DatabaseConnection::Sqlite(_) => store
                .with_conn(|conn| {
                    conn.execute(
                        "UPDATE workflow_run_nodes SET status='running', attempt_count=1,
                         leased_at='2026-07-17T00:00:00Z' WHERE run_id=?1 AND node_id=?2",
                        params![run_id, node_id],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
                })
                .expect("seed stale node"),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => store
                .with_pg_conn(|client| {
                    client
                        .execute(
                            "UPDATE workflow_run_nodes SET status='running', attempt_count=1,
                             leased_at='2026-07-17T00:00:00Z' WHERE run_id=$1 AND node_id=$2",
                            &[&run_id, &node_id],
                        )
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
                .expect("seed stale node"),
        }
        let error = store
            .recover_stale_leases(0)
            .expect_err("malformed recursive identity must fail closed");
        assert!(error.contains("recursive_node_identity_malformed"));
        let run = store
            .get_workflow_run(&run_id)
            .expect("run")
            .expect("run exists");
        assert_eq!(run["nodes"][0]["db_status"], "running");
        match &store.db {
            DatabaseConnection::Sqlite(_) => store
                .with_conn(|conn| {
                    conn.execute("DELETE FROM workflow_run_nodes WHERE run_id=?1", [&run_id])
                        .map_err(|error| error.to_string())?;
                    conn.execute("DELETE FROM workflow_runs WHERE run_id=?1", [&run_id])
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
                .expect("clean malformed fixture"),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => store
                .with_pg_conn(|client| {
                    let mut tx = client.transaction().map_err(|error| error.to_string())?;
                    tx.execute("DELETE FROM workflow_run_nodes WHERE run_id=$1", &[&run_id])
                        .map_err(|error| error.to_string())?;
                    tx.execute("DELETE FROM workflow_runs WHERE run_id=$1", &[&run_id])
                        .map_err(|error| error.to_string())?;
                    tx.commit().map_err(|error| error.to_string())
                })
                .expect("clean malformed fixture"),
        }
    }

    #[test]
    fn sqlite_malformed_stale_recursive_identity_fails_closed() {
        assert_malformed_stale_recursive_identity_fails_closed(
            LocalProductStore::new(":memory:").expect("store"),
            "sqlite",
        );
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_malformed_stale_recursive_identity_fails_closed() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        assert_malformed_stale_recursive_identity_fails_closed(
            LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
                .expect("PostgreSQL store"),
            &uuid::Uuid::new_v4().to_string(),
        );
    }

    #[test]
    fn global_recursive_lease_counter_ignores_non_recursive_agent_steps() {
        let conn = rusqlite::Connection::open_in_memory().expect("sqlite");
        conn.execute(
            "CREATE TABLE workflow_run_nodes (
                run_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                task_type TEXT NOT NULL,
                status TEXT NOT NULL,
                node_json TEXT NOT NULL
            )",
            [],
        )
        .expect("schema");
        conn.execute(
            "CREATE TABLE recursive_execution_nodes (node_id TEXT, root_run_id TEXT)",
            [],
        )
        .expect("recursive index schema");
        conn.execute(
            "CREATE TABLE recursive_execution_trees (root_run_id TEXT, root_node_id TEXT)",
            [],
        )
        .expect("recursive tree schema");
        for index in 0..3 {
            conn.execute(
                "INSERT INTO workflow_run_nodes
                 (run_id, node_id, task_type, status, node_json)
                 VALUES (?1, ?2, 'agent_step', 'running', ?3)",
                params![
                    format!("run-{index}"),
                    format!("node-{index}"),
                    if index == 0 {
                        json!({"recursive_root_node_id": format!("node-{index}")}).to_string()
                    } else {
                        json!({"recursive_node_id": format!("node-{index}")}).to_string()
                    }
                ],
            )
            .expect("recursive row");
        }
        conn.execute(
            "INSERT INTO workflow_run_nodes
             (run_id, node_id, task_type, status, node_json)
             VALUES ('ordinary-run', 'ordinary-node', 'agent_step', 'running', '{}')",
            [],
        )
        .expect("ordinary row");

        assert_eq!(
            count_running_recursive_steps_locked(&conn).expect("count"),
            3
        );
        assert!(workflow_node_is_recursive_locked(&conn, "run-0", "node-0").expect("marker"));
        assert!(
            !workflow_node_is_recursive_locked(&conn, "ordinary-run", "ordinary-node")
                .expect("marker")
        );
    }

    #[test]
    fn disabled_recursive_feature_does_not_apply_recursive_capacity() {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
        let store = LocalProductStore::new(":memory:").expect("store");
        store
            .import_workflow_run(&json!({
                "run_id": "recursive-disabled-cap-run",
                "workflow_id": "recursive-disabled-cap-workflow",
                "status": "running",
                "boundaries": {"execution_authority": "managed"},
                "nodes": [
                    {"node_id": "occupied-1", "task_type": "agent_step", "status": "running", "recursive_root_node_id": "occupied-1"},
                    {"node_id": "occupied-2", "task_type": "agent_step", "status": "running", "recursive_root_node_id": "occupied-2"},
                    {"node_id": "occupied-3", "task_type": "agent_step", "status": "running", "recursive_root_node_id": "occupied-3"},
                    {"node_id": "ordinary-root", "task_type": "agent_step", "status": "pending", "recursive_root_node_id": "ordinary-root"}
                ],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("workflow");

        let result = store
            .tick_with_executor(
                "recursive-disabled-cap-run",
                "recursive-disabled-scheduler",
                0,
                &RecursiveCapExecutor,
            )
            .expect("ordinary root executes while recursive feature is disabled");
        assert_eq!(result["action"], "node_executed");
        assert_eq!(result["node_id"], "ordinary-root");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_disabled_recursive_feature_does_not_apply_recursive_capacity() {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        let store = LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
            .expect("PostgreSQL store");
        let suffix = uuid::Uuid::new_v4().to_string();
        let run_id = format!("recursive-disabled-pg-cap-run-{suffix}");
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": format!("recursive-disabled-pg-cap-workflow-{suffix}"),
                "status": "running",
                "boundaries": {"execution_authority": "managed"},
                "nodes": [
                    {"node_id": "occupied-1", "task_type": "agent_step", "status": "running", "recursive_root_node_id": "occupied-1"},
                    {"node_id": "occupied-2", "task_type": "agent_step", "status": "running", "recursive_root_node_id": "occupied-2"},
                    {"node_id": "occupied-3", "task_type": "agent_step", "status": "running", "recursive_root_node_id": "occupied-3"},
                    {"node_id": "ordinary-root", "task_type": "agent_step", "status": "pending", "recursive_root_node_id": "ordinary-root"}
                ],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("workflow");

        let result = store
            .tick_with_executor(
                &run_id,
                "recursive-disabled-pg-scheduler",
                0,
                &RecursiveCapExecutor,
            )
            .expect("ordinary root executes while recursive feature is disabled");
        assert_eq!(result["action"], "node_executed");
        assert_eq!(result["node_id"], "ordinary-root");
        store
            .with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE workflow_run_nodes SET status='completed' WHERE run_id=$1",
                        &[&run_id],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .expect("release disabled-feature PostgreSQL fixture capacity");
    }

    #[test]
    fn malformed_recursive_metadata_cannot_bypass_global_lease_cap() {
        let conn = rusqlite::Connection::open_in_memory().expect("sqlite");
        conn.execute(
            "CREATE TABLE workflow_run_nodes (
                run_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                task_type TEXT NOT NULL,
                status TEXT NOT NULL,
                node_json TEXT NOT NULL
            )",
            [],
        )
        .expect("schema");
        conn.execute(
            "CREATE TABLE recursive_execution_nodes (node_id TEXT, root_run_id TEXT)",
            [],
        )
        .expect("recursive index schema");
        conn.execute(
            "CREATE TABLE recursive_execution_trees (root_run_id TEXT, root_node_id TEXT)",
            [],
        )
        .expect("recursive tree schema");
        conn.execute(
            "INSERT INTO workflow_run_nodes
             (run_id, node_id, task_type, status, node_json)
             VALUES ('malformed-run', 'malformed-node', 'agent_step', 'running', '{')",
            [],
        )
        .expect("malformed row");
        assert_eq!(
            count_running_recursive_steps_locked(&conn)
                .expect_err("malformed metadata must fail closed"),
            "recursive_node_identity_malformed"
        );
    }

    #[test]
    fn indexed_recursive_claim_requires_the_exact_persisted_marker() {
        assert_eq!(
            recursive_identity_for_claim("root-node", "{}", Some("root-node"))
                .expect_err("indexed root missing marker must fail closed"),
            "recursive_node_identity_malformed"
        );
        assert_eq!(
            recursive_identity_for_claim("child-node", "{}", Some("root-node"))
                .expect_err("indexed child missing marker must fail closed"),
            "recursive_node_identity_malformed"
        );
        assert!(recursive_identity_for_claim(
            "child-node",
            r#"{"recursive_node_id":"child-node"}"#,
            Some("root-node")
        )
        .expect("exact indexed child marker"));
        assert_eq!(
            recursive_identity_for_claim(
                "child-node",
                r#"{"recursive_node_id":"other-node"}"#,
                Some("root-node")
            )
            .expect_err("mismatched indexed child marker must fail closed"),
            "recursive_node_identity_malformed"
        );
    }

    #[test]
    fn concurrent_sqlite_claims_enforce_global_three_lease_cap() {
        let suffix = uuid::Uuid::new_v4().to_string();
        let path = std::env::temp_dir().join(format!("recursive-cap-{suffix}.sqlite3"));
        let path_text = path.to_string_lossy().to_string();
        let factory: Arc<dyn Fn() -> LocalProductStore + Send + Sync> =
            Arc::new(move || LocalProductStore::new(&path_text).expect("concurrent SQLite store"));
        assert_concurrent_recursive_claim_cap(factory, &suffix);
        std::fs::remove_file(path).expect("remove concurrent SQLite fixture");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn concurrent_postgres_claims_enforce_global_three_lease_cap() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        let factory: Arc<dyn Fn() -> LocalProductStore + Send + Sync> = Arc::new(move || {
            LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
                .expect("concurrent PostgreSQL store")
        });
        assert_concurrent_recursive_claim_cap(factory, &uuid::Uuid::new_v4().to_string());
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_malformed_recursive_metadata_cannot_bypass_global_lease_cap() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        let store = LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
            .expect("PostgreSQL store");
        let suffix = uuid::Uuid::new_v4().to_string();
        let run_id = format!("recursive-pg-malformed-run-{suffix}");
        let node_id = format!("recursive-pg-malformed-node-{suffix}");
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": format!("recursive-pg-malformed-workflow-{suffix}"),
                "status": "running",
                "boundaries": {"execution_authority": "managed"},
                "nodes": [{
                    "node_id": node_id,
                    "task_type": "agent_step",
                    "status": "running",
                    "agent_id": format!("recursive-pg-malformed-agent-{suffix}")
                }],
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("workflow");
        store
            .with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute(
                    "UPDATE workflow_run_nodes SET node_json='{' WHERE run_id=$1 AND node_id=$2",
                    &[&run_id, &node_id],
                )
                .map_err(|error| error.to_string())?;
                assert_eq!(
                    pg_count_running_recursive_steps(&mut tx)
                        .expect_err("malformed metadata must fail closed"),
                    "recursive_node_identity_malformed"
                );
                tx.rollback().map_err(|error| error.to_string())
            })
            .expect("malformed identity check");
        store
            .with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE workflow_run_nodes SET status='completed' WHERE run_id=$1 AND node_id=$2",
                        &[&run_id, &node_id],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .expect("release malformed recursive fixture capacity");
    }

    #[test]
    fn recursive_scheduler_claim_refuses_fourth_running_lease() {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let store = LocalProductStore::new(":memory:").expect("store");
        let plan = store
            .create_workflow_plan("recursive lease cap", "test", "actor", |ids, _| {
                let nodes: Vec<Value> = (0..=MAX_RECURSIVE_LEASES)
                    .map(|index| {
                        json!({
                            "node_id": format!("recursive-cap-node-{index}"),
                            "task_type": "agent_step",
                            "status": "pending",
                            "agent_id": format!("recursive-cap-agent-{index}"),
                            "assigned_agent_id": format!("recursive-cap-agent-{index}"),
                            "agent_role": "fixture-agent",
                            "agent_objective": "test recursive lease cap",
                            "capability_profile": ["fixture"],
                            "profile_id": "recursive-cap-profile",
                            "decision_source": "fixture",
                            "max_actions": 1,
                            "recursive_node_id": format!("recursive-cap-node-{index}"),
                        })
                    })
                    .collect();
                Ok(json!({
                    "schema_version": "read_only_plan.v1",
                    "plan_id": ids.plan_id,
                    "status": "planned_read_only",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": {"analysis_id": "recursive-cap-analysis", "task_domain": "test"},
                    "graph": {
                        "schema_version": "workflow_graph.v1",
                        "workflow_id": ids.workflow_id,
                        "dispatch_id": ids.dispatch_id,
                        "status": "decomposed",
                        "created_at": "2026-07-18T00:00:00Z",
                        "updated_at": "2026-07-18T00:00:00Z",
                        "nodes": nodes,
                        "edges": [],
                    },
                    "boundaries": {
                        "execution_authority": "managed",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "disabled",
                    },
                }))
            })
            .expect("plan");
        let run = store
            .create_workflow_run_from_plan(plan["plan_id"].as_str().expect("plan id"), "actor")
            .expect("run");
        let run_id = run["run_id"].as_str().expect("run id").to_string();

        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE workflow_runs SET status='running' WHERE run_id=?1",
                    [&run_id],
                )
                .map_err(|error| error.to_string())?;
                for index in 0..=MAX_RECURSIVE_LEASES {
                    conn.execute(
                        "UPDATE workflow_run_nodes
                         SET status=?1, started_at=?2, leased_at=?2
                         WHERE run_id=?3 AND node_id=?4",
                        rusqlite::params![
                            if index < MAX_RECURSIVE_LEASES {
                                "running"
                            } else {
                                "pending"
                            },
                            "2026-07-18T00:00:00Z",
                            run_id,
                            format!("recursive-cap-node-{index}"),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                }
                Ok(())
            })
            .expect("seed running recursive leases");

        let result = store
            .tick_with_executor(&run_id, "scheduler-test", 0, &RecursiveCapExecutor)
            .expect("tick");
        assert_eq!(result["action"], "no_ready_node");
        let run = store
            .get_workflow_run(&run_id)
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            run["nodes"]
                .as_array()
                .expect("nodes")
                .iter()
                .find(|node| node["node_id"] == "recursive-cap-node-3")
                .expect("fourth node")["db_status"],
            "pending"
        );
        let conflicts: Vec<_> = store
            .audit_events(50)
            .expect("audit")
            .into_iter()
            .filter(|event| {
                event.get("action").and_then(Value::as_str) == Some("recursive.claim_conflict")
                    && event.pointer("/details/reason").and_then(Value::as_str)
                        == Some("scheduler_capacity_exhausted")
            })
            .collect();
        assert_eq!(conflicts.len(), 1, "expected one bounded-cap conflict");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn postgres_recursive_scheduler_claim_refuses_fourth_running_lease() {
        let _env_lock = crate::recursive_execution::test_env_lock()
            .lock()
            .expect("recursive environment lock");
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            if std::env::var("CI").as_deref() == Ok("true") {
                panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
            }
            return;
        };
        std::env::set_var("ACP_RECURSIVE_EXECUTION_ENABLED", "1");
        let store = LocalProductStore::new_postgres(&url, || "2026-07-18T00:00:00Z".to_string())
            .expect("PostgreSQL store");
        let suffix = uuid::Uuid::new_v4().to_string();
        let run_id = format!("recursive-pg-cap-run-{suffix}");
        let nodes = (0..=MAX_RECURSIVE_LEASES)
            .map(|index| {
                let node_id = format!("recursive-pg-cap-node-{suffix}-{index}");
                let marker = if index == 0 {
                    json!({"recursive_root_node_id": node_id})
                } else {
                    json!({"recursive_node_id": node_id})
                };
                let mut node = json!({
                    "node_id": node_id,
                    "task_type": "agent_step",
                    "status": if index < MAX_RECURSIVE_LEASES { "running" } else { "pending" },
                    "agent_id": format!("recursive-pg-cap-agent-{suffix}-{index}"),
                    "leased_at": "2026-07-18T00:00:00Z",
                });
                node.as_object_mut()
                    .expect("node object")
                    .extend(marker.as_object().expect("marker object").clone());
                node
            })
            .collect::<Vec<_>>();
        store
            .import_workflow_run(&json!({
                "run_id": run_id,
                "workflow_id": format!("recursive-pg-cap-workflow-{suffix}"),
                "status": "running",
                "boundaries": {"execution_authority": "managed"},
                "nodes": nodes,
                "edges": [],
                "events": [],
                "approvals": []
            }))
            .expect("workflow");
        let result = store
            .tick_with_executor(&run_id, "scheduler-test", 0, &RecursiveCapExecutor)
            .expect("tick");
        assert_eq!(result["action"], "no_ready_node");
        let run = store
            .get_workflow_run(&run_id)
            .expect("load run")
            .expect("run exists");
        let fourth = format!("recursive-pg-cap-node-{suffix}-3");
        assert_eq!(
            run["nodes"]
                .as_array()
                .expect("nodes")
                .iter()
                .find(|node| node["node_id"] == fourth)
                .expect("fourth node")["db_status"],
            "pending"
        );
        store
            .with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE workflow_run_nodes SET status='completed' WHERE run_id=$1",
                        &[&run_id],
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            })
            .expect("release recursive capacity after assertion");
        std::env::remove_var("ACP_RECURSIVE_EXECUTION_ENABLED");
    }
}
#[cfg(test)]
fn assert_nonrecursive_import_preserves_upsert_compatibility(
    store: LocalProductStore,
    suffix: &str,
) {
    let run_id = format!("nonrecursive-import-upsert-{suffix}");
    store
        .import_workflow_run(&json!({
            "run_id": run_id,
            "workflow_id": format!("nonrecursive-import-workflow-{suffix}"),
            "status": "completed",
            "boundaries": {"execution_authority": "disabled"},
            "nodes": [
                {"node_id": "same-node", "task_type": "noop", "status": "pending"},
                {"node_id": "same-node", "task_type": "noop", "status": "completed"}
            ],
            "edges": [],
            "events": [],
            "approvals": []
        }))
        .expect("legacy import upsert");
    let run = store
        .get_workflow_run(&run_id)
        .expect("run")
        .expect("run exists");
    assert_eq!(run["nodes"].as_array().expect("nodes").len(), 1);
    assert_eq!(run["nodes"][0]["db_status"], "completed");
}

#[test]
fn sqlite_nonrecursive_import_preserves_upsert_compatibility() {
    assert_nonrecursive_import_preserves_upsert_compatibility(
        LocalProductStore::new(":memory:").expect("store"),
        "sqlite",
    );
}

#[cfg(feature = "pg-tests")]
#[test]
fn postgres_nonrecursive_import_preserves_upsert_compatibility() {
    let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
        if std::env::var("CI").as_deref() == Ok("true") {
            panic!("ACP_TEST_DATABASE_URL is required for PostgreSQL CI evidence");
        }
        return;
    };
    assert_nonrecursive_import_preserves_upsert_compatibility(
        LocalProductStore::new_postgres(&url, || "2026-07-19T00:00:00Z".to_string())
            .expect("PostgreSQL store"),
        &uuid::Uuid::new_v4().to_string(),
    );
}
