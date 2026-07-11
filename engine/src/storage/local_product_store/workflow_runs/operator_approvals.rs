use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use super::{ensure_run_exists_locked, next_sequence};
#[cfg(feature = "pg")]
use super::{pg_append_audit, pg_ensure_run_exists, pg_next_sequence};
use super::super::{append_audit_locked, DatabaseConnection, LocalProductStore};

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
                            "SELECT node_id, approval_sequence
                             FROM workflow_run_approvals
                             WHERE run_id = ?1 AND approval_id = ?2 AND decision = 'requested'",
                            params![run_id, requested_approval_id],
                            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
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
                    let approval = json!({
                        "approval_sequence": sequence,
                        "approval_id": approval_id,
                        "run_id": run_id,
                        "node_id": requested.0,
                        "decision": resolution,
                        "actor": actor,
                        "reason": reason,
                        "created_at": created_at,
                        "resolved_request_id": requested_approval_id,
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
                            requested.0,
                            resolution,
                            actor,
                            reason,
                            created_at,
                            approval.to_string(),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
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
                            "metadata_only": true,
                            "execution_authority": "disabled",
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
                        "SELECT node_id, approval_sequence
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

                let sequence =
                    pg_next_sequence(&mut tx, "workflow_run_approvals", "approval_sequence")?;
                let approval_id = format!("workflow-approval-{sequence:04}");
                let created_at = self.now();
                let approval = json!({
                    "approval_sequence": sequence,
                    "approval_id": approval_id,
                    "run_id": run_id,
                    "node_id": node_id,
                    "decision": resolution,
                    "actor": actor,
                    "reason": reason,
                    "created_at": created_at,
                    "resolved_request_id": requested_approval_id,
                    "metadata_only": true,
                    "execution_authority": "disabled",
                });
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
                        "metadata_only": true,
                        "execution_authority": "disabled",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(approval)
            }),
        }
    }
}
