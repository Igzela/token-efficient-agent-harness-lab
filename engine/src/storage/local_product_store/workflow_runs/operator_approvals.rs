use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use super::super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use super::{ensure_run_exists_locked, next_sequence};
#[cfg(feature = "pg")]
use super::{pg_append_audit, pg_ensure_run_exists, pg_next_sequence};

impl LocalProductStore {
    pub fn resolve_requested_workflow_run_approval(
        &self,
        run_id: &str,
        requested_approval_id: &str,
        resolution: &str,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<Value, String> {
        if !matches!(resolution, "approved" | "rejected") {
            return Err(format!(
                "invalid workflow approval resolution: {resolution}"
            ));
        }

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
                    .map_err(|error| error.to_string())?;
                let result = (|| {
                    ensure_run_exists_locked(conn, run_id)?;
                    let requested = conn
                        .query_row(
                            "SELECT node_id, approval_sequence, approval_json
                             FROM workflow_run_approvals
                             WHERE run_id = ?1 AND approval_id = ?2 AND decision = 'requested'",
                            params![run_id, requested_approval_id],
                            |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, i64>(1)?,
                                    row.get::<_, String>(2)?,
                                ))
                            },
                        )
                        .optional()
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| {
                            format!(
                                "requested workflow approval is no longer current: {requested_approval_id}"
                            )
                        })?;
                    let latest_sequence: i64 = conn
                        .query_row(
                            "SELECT COALESCE(MAX(approval_sequence), 0)
                             FROM workflow_run_approvals
                             WHERE run_id = ?1 AND node_id = ?2",
                            params![run_id, requested.0],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    if latest_sequence != requested.1 {
                        return Err(format!(
                            "requested workflow approval has been superseded: {requested_approval_id}"
                        ));
                    }

                    let sequence =
                        next_sequence(conn, "workflow_run_approvals", "approval_sequence")?;
                    let approval_id = format!("workflow-approval-{sequence:04}");
                    let created_at = self.now();
                    let requested_json: Value = serde_json::from_str(&requested.2)
                        .map_err(|error| format!("invalid requested approval JSON: {error}"))?;
                    let is_tool_execution = requested_json
                        .get("approval_kind")
                        .and_then(Value::as_str)
                        == Some("tool_execution");
                    if is_tool_execution {
                        let binding: Option<(String, String, String, String)> = conn
                            .query_row(
                                "SELECT action_sha256, tool_name, profile_id, status
                                 FROM tool_execution_authorizations
                                 WHERE run_id = ?1 AND node_id = ?2
                                   AND requested_approval_id = ?3",
                                params![run_id, requested.0, requested_approval_id],
                                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                            )
                            .optional()
                            .map_err(|error| error.to_string())?;
                        let Some((action_hash, tool_name, profile_id, status)) = binding else {
                            return Err("tool execution approval binding is missing".to_string());
                        };
                        if status != "requested"
                            || requested_json.get("action_sha256").and_then(Value::as_str)
                                != Some(action_hash.as_str())
                            || requested_json.get("tool_name").and_then(Value::as_str)
                                != Some(tool_name.as_str())
                            || requested_json.get("profile_id").and_then(Value::as_str)
                                != Some(profile_id.as_str())
                        {
                            return Err("tool execution approval binding changed".to_string());
                        }
                    }
                    let mut approval = json!({
                        "approval_sequence": sequence,
                        "approval_id": approval_id,
                        "run_id": run_id,
                        "node_id": requested.0,
                        "decision": resolution,
                        "actor": actor,
                        "reason": reason,
                        "created_at": created_at,
                        "resolved_request_id": requested_approval_id,
                        "metadata_only": !is_tool_execution,
                        "execution_authority": if is_tool_execution { "single_tool_invocation" } else { "disabled" },
                    });
                    if is_tool_execution {
                        let object = approval
                            .as_object_mut()
                            .ok_or_else(|| "approval must be an object".to_string())?;
                        object.insert("approval_kind".to_string(), json!("tool_execution"));
                        for field in ["tool_name", "profile_id", "action_sha256"] {
                            object.insert(field.to_string(), requested_json[field].clone());
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
                            requested.0,
                            resolution,
                            actor,
                            reason,
                            created_at,
                            approval.to_string(),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    if is_tool_execution {
                        let changed = conn
                            .execute(
                                "UPDATE tool_execution_authorizations
                                 SET status = ?1, resolved_by = ?2, updated_at = ?3
                                 WHERE run_id = ?4 AND node_id = ?5
                                   AND requested_approval_id = ?6 AND status = 'requested'",
                                params![
                                    resolution,
                                    actor,
                                    created_at,
                                    run_id,
                                    requested.0,
                                    requested_approval_id,
                                ],
                            )
                            .map_err(|error| error.to_string())?;
                        if changed != 1 {
                            return Err("tool execution approval changed concurrently".to_string());
                        }
                        let node_changed = conn
                            .execute(
                                "UPDATE workflow_run_nodes
                                 SET status = 'pending', completed_at = NULL, blocked_reason = NULL
                                 WHERE run_id = ?1 AND node_id = ?2
                                   AND status = 'awaiting_approval'",
                                params![run_id, requested.0],
                            )
                            .map_err(|error| error.to_string())?;
                        if node_changed != 1 {
                            return Err(
                                "tool approval node is no longer awaiting approval".to_string()
                            );
                        }
                        let node_json_text: String = conn
                            .query_row(
                                "SELECT node_json FROM workflow_run_nodes
                                 WHERE run_id = ?1 AND node_id = ?2",
                                params![run_id, requested.0],
                                |row| row.get(0),
                            )
                            .map_err(|error| error.to_string())?;
                        let mut node_json: Value = serde_json::from_str(&node_json_text)
                            .map_err(|error| format!("invalid workflow node JSON: {error}"))?;
                        if let Some(object) = node_json.as_object_mut() {
                            object.insert("status".to_string(), json!("pending"));
                        }
                        conn.execute(
                            "UPDATE workflow_run_nodes SET node_json = ?1
                             WHERE run_id = ?2 AND node_id = ?3",
                            params![node_json.to_string(), run_id, requested.0],
                        )
                        .map_err(|error| error.to_string())?;
                    }
                    append_audit_locked(
                        conn,
                        &created_at,
                        actor,
                        "workflow_run.approval_record",
                        run_id,
                        &json!({
                            "node_id": approval["node_id"],
                            "decision": resolution,
                            "resolved_request_id": requested_approval_id,
                            "metadata_only": !is_tool_execution,
                            "execution_authority": if is_tool_execution { "single_tool_invocation" } else { "disabled" },
                        }),
                    )?;
                    Ok(approval)
                })();

                match result {
                    Ok(value) => {
                        conn.execute_batch("COMMIT")
                            .map_err(|error| error.to_string())?;
                        Ok(value)
                    }
                    Err(error) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Err(error)
                    }
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                pg_ensure_run_exists(&mut tx, run_id)?;
                tx.query_one(
                    "SELECT run_id FROM workflow_runs WHERE run_id = $1 FOR UPDATE",
                    &[&run_id],
                )
                .map_err(|error| error.to_string())?;
                let row = tx
                    .query_opt(
                        "SELECT node_id, approval_sequence, approval_json
                         FROM workflow_run_approvals
                         WHERE run_id = $1 AND approval_id = $2 AND decision = 'requested'",
                        &[&run_id, &requested_approval_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        format!(
                            "requested workflow approval is no longer current: {requested_approval_id}"
                        )
                    })?;
                let node_id: String = row.get(0);
                let requested_sequence: i64 = row.get(1);
                let requested_json_text: String = row.get(2);
                let latest_sequence: i64 = tx
                    .query_one(
                        "SELECT COALESCE(MAX(approval_sequence), 0)
                         FROM workflow_run_approvals
                         WHERE run_id = $1 AND node_id = $2",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                if latest_sequence != requested_sequence {
                    return Err(format!(
                        "requested workflow approval has been superseded: {requested_approval_id}"
                    ));
                }

                let requested_json: Value = serde_json::from_str(&requested_json_text)
                    .map_err(|error| format!("invalid requested approval JSON: {error}"))?;
                let is_tool_execution = requested_json
                    .get("approval_kind")
                    .and_then(Value::as_str)
                    == Some("tool_execution");
                if is_tool_execution {
                    let binding = tx
                        .query_opt(
                            "SELECT action_sha256, tool_name, profile_id, status
                             FROM tool_execution_authorizations
                             WHERE run_id = $1 AND node_id = $2
                               AND requested_approval_id = $3 FOR UPDATE",
                            &[&run_id, &node_id, &requested_approval_id],
                        )
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "tool execution approval binding is missing".to_string())?;
                    let action_hash: String = binding.get(0);
                    let tool_name: String = binding.get(1);
                    let profile_id: String = binding.get(2);
                    let status: String = binding.get(3);
                    if status != "requested"
                        || requested_json.get("action_sha256").and_then(Value::as_str)
                            != Some(action_hash.as_str())
                        || requested_json.get("tool_name").and_then(Value::as_str)
                            != Some(tool_name.as_str())
                        || requested_json.get("profile_id").and_then(Value::as_str)
                            != Some(profile_id.as_str())
                    {
                        return Err("tool execution approval binding changed".to_string());
                    }
                }

                tx.batch_execute(
                    "LOCK TABLE workflow_run_approvals IN SHARE ROW EXCLUSIVE MODE",
                )
                .map_err(|error| error.to_string())?;
                let sequence =
                    pg_next_sequence(&mut tx, "workflow_run_approvals", "approval_sequence")?;
                let approval_id = format!("workflow-approval-{sequence:04}");
                let created_at = self.now();
                let mut approval = json!({
                    "approval_sequence": sequence,
                    "approval_id": approval_id,
                    "run_id": run_id,
                    "node_id": node_id,
                    "decision": resolution,
                    "actor": actor,
                    "reason": reason,
                    "created_at": created_at,
                    "resolved_request_id": requested_approval_id,
                    "metadata_only": !is_tool_execution,
                    "execution_authority": if is_tool_execution { "single_tool_invocation" } else { "disabled" },
                });
                if is_tool_execution {
                    let object = approval
                        .as_object_mut()
                        .ok_or_else(|| "approval must be an object".to_string())?;
                    object.insert("approval_kind".to_string(), json!("tool_execution"));
                    for field in ["tool_name", "profile_id", "action_sha256"] {
                        object.insert(field.to_string(), requested_json[field].clone());
                    }
                }
                tx.execute(
                    "INSERT INTO workflow_run_approvals
                     (approval_sequence, approval_id, run_id, node_id, decision, actor, reason,
                      created_at, approval_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                    &[
                        &sequence,
                        &approval_id,
                        &run_id,
                        &node_id,
                        &resolution,
                        &actor,
                        &reason,
                        &created_at,
                        &approval.to_string(),
                    ],
                )
                .map_err(|error| error.to_string())?;
                if is_tool_execution {
                    let changed = tx
                        .execute(
                            "UPDATE tool_execution_authorizations
                             SET status = $1, resolved_by = $2, updated_at = $3
                             WHERE run_id = $4 AND node_id = $5
                               AND requested_approval_id = $6 AND status = 'requested'",
                            &[
                                &resolution,
                                &actor,
                                &created_at,
                                &run_id,
                                &node_id,
                                &requested_approval_id,
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    if changed != 1 {
                        return Err("tool execution approval changed concurrently".to_string());
                    }
                    let node_changed = tx
                        .execute(
                            "UPDATE workflow_run_nodes
                             SET status = 'pending', completed_at = NULL, blocked_reason = NULL
                             WHERE run_id = $1 AND node_id = $2
                               AND status = 'awaiting_approval'",
                            &[&run_id, &node_id],
                        )
                        .map_err(|error| error.to_string())?;
                    if node_changed != 1 {
                        return Err(
                            "tool approval node is no longer awaiting approval".to_string()
                        );
                    }
                    let node_json_text: String = tx
                        .query_one(
                            "SELECT node_json FROM workflow_run_nodes
                             WHERE run_id = $1 AND node_id = $2",
                            &[&run_id, &node_id],
                        )
                        .map_err(|error| error.to_string())?
                        .get(0);
                    let mut node_json: Value = serde_json::from_str(&node_json_text)
                        .map_err(|error| format!("invalid workflow node JSON: {error}"))?;
                    if let Some(object) = node_json.as_object_mut() {
                        object.insert("status".to_string(), json!("pending"));
                    }
                    tx.execute(
                        "UPDATE workflow_run_nodes SET node_json = $1
                         WHERE run_id = $2 AND node_id = $3",
                        &[&node_json.to_string(), &run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?;
                }
                pg_append_audit(
                    &mut tx,
                    &created_at,
                    actor,
                    "workflow_run.approval_record",
                    run_id,
                    &json!({
                        "node_id": node_id,
                        "decision": resolution,
                        "resolved_request_id": requested_approval_id,
                        "metadata_only": !is_tool_execution,
                        "execution_authority": if is_tool_execution { "single_tool_invocation" } else { "disabled" },
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(approval)
            }),
        }
    }
}
