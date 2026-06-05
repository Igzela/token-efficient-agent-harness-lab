use rusqlite::{params, Row};
use serde_json::{json, Value};

use super::{append_audit_locked, collect_values, LocalProductStore};

pub const WORKFLOW_RUN_SCHEMA_VERSION: &str = "workflow_run.v1";

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

        let run_id = self.with_conn(|conn| {
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
                  dispatch_id, started_at, completed_at, result_json, boundaries_json, run_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, NULL, ?9, ?10)",
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
        })?;

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
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                            workflow_id, dispatch_id, started_at, completed_at, result_json,
                            boundaries_json, run_json
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
        })
    }

    pub fn list_workflow_runs_with_offset(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                            workflow_id, dispatch_id, started_at, completed_at, result_json,
                            boundaries_json, run_json
                     FROM workflow_runs
                     ORDER BY run_sequence DESC
                     LIMIT ?1 OFFSET ?2",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![limit, offset], workflow_run_summary_row)
                .map_err(|e| e.to_string())?;
            collect_values(rows)
        })
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
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT run_sequence, run_id, plan_id, created_at, updated_at, status,
                            workflow_id, dispatch_id, started_at, completed_at, result_json,
                            boundaries_json, run_json
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
        })
    }

    pub fn append_workflow_run_event(
        &self,
        run_id: &str,
        node_id: Option<&str>,
        event_type: &str,
        details: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        self.with_conn(|conn| {
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
        })
    }

    pub fn workflow_run_events(&self, run_id: &str, limit: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            ensure_run_exists_locked(conn, run_id)?;
            workflow_run_events_locked(conn, run_id, limit)
        })
    }

    pub fn record_workflow_run_approval(
        &self,
        run_id: &str,
        node_id: &str,
        decision: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Value, String> {
        if !matches!(decision, "requested" | "approved" | "rejected") {
            return Err(format!("invalid workflow approval decision: {decision}"));
        }
        self.with_conn(|conn| {
            ensure_run_exists_locked(conn, run_id)?;
            let sequence = next_sequence(conn, "workflow_run_approvals", "approval_sequence")?;
            let approval_id = format!("workflow-approval-{sequence:04}");
            let created_at = self.now();
            let approval = json!({
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
        })
    }

    pub fn workflow_run_approvals(&self, run_id: &str, limit: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
            ensure_run_exists_locked(conn, run_id)?;
            workflow_run_approvals_locked(conn, run_id, limit)
        })
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

        self.with_conn(|conn| {
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
        })
    }

    fn update_workflow_run_status_with_event(
        &self,
        run_id: &str,
        status: &str,
        event_type: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Value, String> {
        self.with_conn(|conn| {
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
        })?;
        self.get_workflow_run(run_id)?
            .ok_or_else(|| format!("workflow run not found after update: {run_id}"))
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
    conn.execute(
        "INSERT OR REPLACE INTO workflow_run_nodes
         (run_id, node_id, task_type, status, node_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![run_id, node_id, task_type, status, node.to_string()],
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
    let run_text: String = row.get(12)?;
    let run: Value = serde_json::from_str(&run_text).unwrap_or(Value::Null);
    let boundaries_text: String = row.get(11)?;
    let boundaries: Value =
        serde_json::from_str(&boundaries_text).unwrap_or_else(|_| workflow_run_boundaries());
    Ok(workflow_run_value(
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
        &boundaries,
        &run,
    ))
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
        .prepare("SELECT node_json FROM workflow_run_nodes WHERE run_id = ?1 ORDER BY rowid ASC")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![run_id], |row| {
            let text: String = row.get(0)?;
            Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
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
