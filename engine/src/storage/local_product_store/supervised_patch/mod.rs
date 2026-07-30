use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::provider::redaction::redact_sensitive_patterns;
use crate::target_repo_output::{
    inspect_git_patch, patch_hash as target_patch_hash, remove_git_worktree, stage_and_build_patch,
    staged_changed_files, TargetRepoOutputConfig,
};

use super::product_tasks::validate_product_terminal_evidence_content_hash;
#[cfg(test)]
use super::workflow_runs::is_execution_owner_conflict;
use super::workflow_runs::API_OWNED_SUPERVISED_PATCH;
use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

pub(crate) mod fs_utils;
use self::fs_utils::*;

pub const SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION: &str = "supervised_patch_workspace.v1";
pub const SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION: &str = "supervised_patch_artifact.v1";
// Covers the complete bounded branch-publish sequence (multiple individually capped git
// subprocesses) with recovery margin, not merely one subprocess timeout.
const PRODUCT_OUTPUT_PHASE_LEASE_SECONDS: i64 = 15 * 60;

#[derive(Debug, Clone, PartialEq)]
pub enum TargetOutputClaim {
    Claimed,
    Reused(Value),
    ReconciliationRequired(String),
}

impl LocalProductStore {
    pub fn create_workspace_directory(
        &self,
        workspace_id: &str,
        target_repo_path: &str,
    ) -> Result<String, String> {
        validate_workspace_id(workspace_id)?;
        let db_dir = self
            .db_path()
            .parent()
            .ok_or_else(|| "store has no parent directory".to_string())?;
        let workspaces_dir = db_dir.join("workspaces");
        std::fs::create_dir_all(&workspaces_dir).map_err(|e| e.to_string())?;
        let workspaces_canonical =
            std::fs::canonicalize(&workspaces_dir).map_err(|e| e.to_string())?;
        let workspace_dir = workspaces_canonical.join(workspace_id);
        if workspace_dir.exists() {
            let existing = std::fs::canonicalize(&workspace_dir).map_err(|e| e.to_string())?;
            if !existing.starts_with(&workspaces_canonical) {
                return Err("workspace directory escaped app-owned workspace root".to_string());
            }
            return Ok(existing.to_string_lossy().into_owned());
        }

        let target_canonical =
            std::fs::canonicalize(target_repo_path).map_err(|e| e.to_string())?;
        let planned_workspace =
            canonicalize_planned_path(&workspace_dir.to_string_lossy(), "workspace_path")?;
        if !planned_workspace.starts_with(&workspaces_canonical) {
            return Err(
                "workspace directory must stay inside app-owned workspace root".to_string(),
            );
        }
        if workspaces_canonical.starts_with(&target_canonical)
            || planned_workspace.starts_with(&target_canonical)
        {
            return Err("workspace directory must be outside target repository".to_string());
        }

        if let Err(e) = copy_dir_contents(&target_canonical, &workspace_dir) {
            let _ = std::fs::remove_dir_all(&workspace_dir);
            return Err(e);
        }

        let source_manifest = compute_manifest(&target_canonical)?;
        let manifest_path = workspace_dir.join(".source_manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&source_manifest).unwrap_or_default(),
        )
        .map_err(|e| e.to_string())?;

        let canonical_workspace =
            std::fs::canonicalize(&workspace_dir).map_err(|e| e.to_string())?;
        Ok(canonical_workspace.to_string_lossy().into_owned())
    }

    pub fn update_workspace_status(
        &self,
        workspace_id: &str,
        new_status: &str,
        actor: &str,
    ) -> Result<Value, String> {
        if !is_valid_workspace_status(new_status) {
            return Err(format!("invalid workspace status: {new_status}"));
        }
        let workspace = self
            .get_supervised_patch_workspace(workspace_id)?
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let current_status = workspace
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("requested");
        if !is_valid_workspace_transition(current_status, new_status) {
            return Err(format!(
                "invalid workspace status transition: {current_status} -> {new_status}"
            ));
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE supervised_patch_workspaces SET status = ?1, updated_at = ?2 WHERE workspace_id = ?3",
                    params![new_status, now, workspace_id],
                ).map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "supervised_patch.workspace_status_update",
                    workspace_id,
                    &json!({"from": current_status, "to": new_status, "metadata_only": true}),
                )?;
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client.execute(
                    "UPDATE supervised_patch_workspaces SET status = $1, updated_at = $2 WHERE workspace_id = $3",
                    &[&new_status, &now, &workspace_id],
                ).map_err(|e| e.to_string())?;
                let audit_details =
                    json!({"from": current_status, "to": new_status, "metadata_only": true}).to_string();
                pg_append_audit(client, &now, actor, "supervised_patch.workspace_status_update", workspace_id, &audit_details)?;
                Ok(())
            })?,
        }
        self.get_supervised_patch_workspace(workspace_id)?
            .ok_or_else(|| format!("workspace not found after update: {workspace_id}"))
    }

    pub fn cleanup_workspace(&self, workspace_id: &str, actor: &str) -> Result<Value, String> {
        let workspace = self
            .get_supervised_patch_workspace(workspace_id)?
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let workspace_path = workspace
            .get("workspace_path")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !workspace_path.is_empty() {
            let path = Path::new(workspace_path);
            if path.exists() {
                let workspace_mode = workspace
                    .get("workspace_mode")
                    .and_then(Value::as_str)
                    .unwrap_or("copy");
                if workspace_mode == "git_worktree" {
                    let target_repo_path = required_str(&workspace, "target_repo_path")?;
                    remove_git_worktree(
                        &TargetRepoOutputConfig::from_env(),
                        Path::new(target_repo_path),
                        path,
                    )?;
                } else {
                    std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
                }
            }
        }
        self.update_workspace_status(workspace_id, "cleaned", actor)
    }

    pub fn quarantine_workspace(&self, workspace_id: &str, actor: &str) -> Result<Value, String> {
        self.update_workspace_status(workspace_id, "quarantined", actor)
    }

    pub fn record_workspace_verification(
        &self,
        workspace_id: &str,
        verification: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        if !verification.is_object() {
            return Err("workspace verification must be an object".to_string());
        }
        let verification_status = required_str(verification, "status")?;
        if !matches!(
            verification_status,
            "evidence_recorded"
                | "verification_failed"
                | "approval_required"
                | "authority_lost"
                | "outcome_unknown"
        ) {
            return Err(format!(
                "invalid workspace verification status: {verification_status}"
            ));
        }

        let mut workspace = self
            .get_supervised_patch_workspace(workspace_id)?
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let now = self.now();
        let object = workspace
            .as_object_mut()
            .ok_or_else(|| "workspace record must be an object".to_string())?;
        object.insert("verification".to_string(), verification.clone());
        object.insert("updated_at".to_string(), json!(now.clone()));
        let workspace_json = workspace.to_string();

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE supervised_patch_workspaces
                     SET workspace_json = ?1, updated_at = ?2
                     WHERE workspace_id = ?3",
                    params![workspace_json, now, workspace_id],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "supervised_patch.workspace_verification",
                    workspace_id,
                    &json!({
                        "status": verification_status,
                        "command": verification.get("command"),
                        "attempt": verification.get("attempt"),
                    }),
                )?;
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE supervised_patch_workspaces
                         SET workspace_json = $1, updated_at = $2
                         WHERE workspace_id = $3",
                        &[&workspace_json, &now, &workspace_id],
                    )
                    .map_err(|e| e.to_string())?;
                let audit_details = json!({
                    "status": verification_status,
                    "command": verification.get("command"),
                    "attempt": verification.get("attempt"),
                })
                .to_string();
                pg_append_audit(
                    client,
                    &now,
                    actor,
                    "supervised_patch.workspace_verification",
                    workspace_id,
                    &audit_details,
                )?;
                Ok(())
            })?,
        }

        self.get_supervised_patch_workspace(workspace_id)?
            .ok_or_else(|| format!("workspace not found after verification: {workspace_id}"))
    }

    pub(crate) fn ensure_managed_supervised_patch_run(
        &self,
        workspace_id: &str,
        operation: &str,
        attempt: u64,
        binding_sha256: &str,
        node_metadata: &Value,
        actor: &str,
    ) -> Result<String, String> {
        if !matches!(operation, "verify" | "repair" | "product_verify") {
            return Err(format!(
                "unsupported managed supervised-patch operation: {operation}"
            ));
        }
        let max_attempt = if operation == "product_verify" { 8 } else { 5 };
        if attempt == 0 || attempt > max_attempt {
            return Err(format!(
                "managed supervised-patch {operation} attempt must be between 1 and {max_attempt}"
            ));
        }
        if binding_sha256.len() != 64
            || !binding_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(
                "managed supervised-patch binding must be a SHA-256 hex digest".to_string(),
            );
        }
        validate_managed_supervised_patch_metadata(
            workspace_id,
            operation,
            attempt,
            binding_sha256,
            node_metadata,
        )?;

        let identity = managed_supervised_patch_identity(workspace_id, operation, attempt)?;
        let node_id = format!("supervised-{operation}-{attempt}");
        let run_id = format!("managed-run-{}", identity);
        let workflow_id = format!("managed-workflow-{}", identity);
        let dispatch_id = format!("managed-dispatch-{}", identity);

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
                    .map_err(|error| error.to_string())?;
                let result = (|| {
                    let workspace_exists: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM supervised_patch_workspaces WHERE workspace_id = ?1",
                            params![workspace_id],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    if workspace_exists != 1 {
                        return Err(format!("workspace not found: {workspace_id}"));
                    }

                    let existing_node: Option<(String, Option<String>)> = conn
                        .query_row(
                            "SELECT n.node_json, r.pause_reason
                             FROM workflow_run_nodes n
                             JOIN workflow_runs r ON r.run_id = n.run_id
                             WHERE n.run_id = ?1 AND n.node_id = ?2",
                            params![run_id, node_id],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()
                        .map_err(|error| error.to_string())?;
                    if let Some((existing_node, pause_reason)) = existing_node {
                        validate_existing_managed_supervised_patch_node(
                            &existing_node,
                            workspace_id,
                            operation,
                            attempt,
                            binding_sha256,
                            node_metadata,
                        )?;
                        match pause_reason.as_deref() {
                            Some(API_OWNED_SUPERVISED_PATCH) => {}
                            None => {
                                let updated = conn
                                    .execute(
                                        "UPDATE workflow_runs
                                         SET pause_reason = ?1
                                         WHERE run_id = ?2 AND pause_reason IS NULL",
                                        params![API_OWNED_SUPERVISED_PATCH, run_id],
                                    )
                                    .map_err(|error| error.to_string())?;
                                if updated != 1 {
                                    return Err(
                                        "managed supervised-patch execution owner changed"
                                            .to_string(),
                                    );
                                }
                            }
                            Some(_) => {
                                return Err(
                                    "managed supervised-patch execution owner changed".to_string()
                                );
                            }
                        }
                        let now = self.now();
                        append_audit_locked(
                            conn,
                            &now,
                            actor,
                            "supervised_patch.managed_run_reused",
                            &run_id,
                            &json!({
                                "workspace_id": workspace_id,
                                "operation": operation,
                                "attempt": attempt,
                                "binding_sha256": binding_sha256,
                                "canonical": true,
                                "content_excluded": true,
                            }),
                        )?;
                        return Ok(run_id.clone());
                    }

                    let conflicting_run_exists: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM workflow_runs WHERE run_id = ?1",
                            params![run_id],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    if conflicting_run_exists != 0 {
                        return Err(
                            "managed supervised-patch canonical run is missing its bound node"
                                .to_string(),
                        );
                    }

                    let sequence = next_sequence(conn, "workflow_runs", "run_sequence")?;
                    let event_sequence =
                        next_sequence(conn, "workflow_run_events", "event_sequence")?;
                    let created_at = self.now();
                    let (node, graph, boundaries, run) = build_managed_supervised_patch_run(
                        sequence,
                        &run_id,
                        &workflow_id,
                        &dispatch_id,
                        &node_id,
                        operation,
                        node_metadata,
                        &created_at,
                    )?;
                    conn.execute(
                        "INSERT INTO workflow_runs
                         (run_sequence, run_id, plan_id, created_at, updated_at, status,
                          workflow_id, dispatch_id, started_at, completed_at, result_json,
                          boundaries_json, run_json, priority, deadline_at, sla_ms, tenant_id,
                          queue_position, pause_reason, degrade_mode)
                         VALUES (?1, ?2, NULL, ?3, ?3, 'created', ?4, ?5, NULL, NULL, NULL,
                                 ?6, ?7, 5, NULL, NULL, NULL, NULL, ?8, NULL)",
                        params![
                            sequence,
                            run_id,
                            created_at,
                            workflow_id,
                            dispatch_id,
                            boundaries.to_string(),
                            run.to_string(),
                            API_OWNED_SUPERVISED_PATCH,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    conn.execute(
                        "INSERT INTO workflow_run_nodes
                         (run_id, node_id, task_type, status, node_json, started_at, completed_at,
                          attempt_count, timeout_ms, blocked_reason, leased_at, profile_id)
                         VALUES (?1, ?2, ?3, 'pending', ?4, NULL, NULL, 0, ?5, NULL, NULL, ?6)",
                        params![
                            run_id,
                            node_id,
                            format!("workspace_{operation}"),
                            node.to_string(),
                            node.get("executor_timeout_ms").and_then(Value::as_i64),
                            node.get("profile_id").and_then(Value::as_str),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    let event_id = format!("workflow-event-{event_sequence:04}");
                    let event_details = json!({
                        "workspace_id": workspace_id,
                        "operation": operation,
                        "attempt": attempt,
                        "binding_sha256": binding_sha256,
                        "canonical": true,
                        "metadata_only": false,
                        "execution_authority": "bounded_trusted_local",
                        "content_excluded": true,
                    });
                    conn.execute(
                        "INSERT INTO workflow_run_events
                         (event_sequence, event_id, run_id, node_id, event_type, actor,
                          created_at, details_json)
                         VALUES (?1, ?2, ?3, NULL, 'workflow_run.created', ?4, ?5, ?6)",
                        params![
                            event_sequence,
                            event_id,
                            run_id,
                            actor,
                            created_at,
                            event_details.to_string(),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    append_audit_locked(
                        conn,
                        &created_at,
                        actor,
                        "supervised_patch.managed_run_created",
                        &run_id,
                        &json!({
                            "workspace_id": workspace_id,
                            "operation": operation,
                            "attempt": attempt,
                            "binding_sha256": binding_sha256,
                            "workflow_id": workflow_id,
                            "dispatch_id": dispatch_id,
                            "graph_sha256": hex::encode(Sha256::digest(graph.to_string().as_bytes())),
                            "canonical": true,
                            "content_excluded": true,
                        }),
                    )?;
                    Ok(run_id.clone())
                })();
                match result {
                    Ok(run_id) => {
                        conn.execute_batch("COMMIT")
                            .map_err(|error| error.to_string())?;
                        Ok(run_id)
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
                tx.batch_execute(
                    "LOCK TABLE workflow_runs IN SHARE ROW EXCLUSIVE MODE;
                     LOCK TABLE workflow_run_events IN SHARE ROW EXCLUSIVE MODE;",
                )
                .map_err(|error| error.to_string())?;
                if tx
                    .query_opt(
                        "SELECT workspace_id FROM supervised_patch_workspaces
                         WHERE workspace_id = $1 FOR UPDATE",
                        &[&workspace_id],
                    )
                    .map_err(|error| error.to_string())?
                    .is_none()
                {
                    return Err(format!("workspace not found: {workspace_id}"));
                }

                let existing_node = tx
                    .query_opt(
                        "SELECT n.node_json, r.pause_reason
                         FROM workflow_run_nodes n
                         JOIN workflow_runs r ON r.run_id = n.run_id
                         WHERE n.run_id = $1 AND n.node_id = $2
                         FOR UPDATE OF n, r",
                        &[&run_id, &node_id],
                    )
                    .map_err(|error| error.to_string())?;
                if let Some(existing_row) = existing_node {
                    let existing_node: String = existing_row.get(0);
                    let pause_reason: Option<String> = existing_row.get(1);
                    validate_existing_managed_supervised_patch_node(
                        &existing_node,
                        workspace_id,
                        operation,
                        attempt,
                        binding_sha256,
                        node_metadata,
                    )?;
                    match pause_reason.as_deref() {
                        Some(API_OWNED_SUPERVISED_PATCH) => {}
                        None => {
                            let updated = tx
                                .execute(
                                    "UPDATE workflow_runs
                                     SET pause_reason = $1
                                     WHERE run_id = $2 AND pause_reason IS NULL",
                                    &[&API_OWNED_SUPERVISED_PATCH, &run_id],
                                )
                                .map_err(|error| error.to_string())?;
                            if updated != 1 {
                                return Err(
                                    "managed supervised-patch execution owner changed".to_string()
                                );
                            }
                        }
                        Some(_) => {
                            return Err(
                                "managed supervised-patch execution owner changed".to_string()
                            );
                        }
                    }
                    let now = self.now();
                    let details = json!({
                        "workspace_id": workspace_id,
                        "operation": operation,
                        "attempt": attempt,
                        "binding_sha256": binding_sha256,
                        "canonical": true,
                        "content_excluded": true,
                    })
                    .to_string();
                    pg_append_audit(
                        &mut tx,
                        &now,
                        actor,
                        "supervised_patch.managed_run_reused",
                        &run_id,
                        &details,
                    )?;
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(run_id);
                }
                if tx
                    .query_opt(
                        "SELECT run_id FROM workflow_runs WHERE run_id = $1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .is_some()
                {
                    return Err(
                        "managed supervised-patch canonical run is missing its bound node"
                            .to_string(),
                    );
                }

                let sequence = pg_next_sequence(&mut tx, "workflow_runs", "run_sequence")?;
                let event_sequence =
                    pg_next_sequence(&mut tx, "workflow_run_events", "event_sequence")?;
                let created_at = self.now();
                let (node, graph, boundaries, run) = build_managed_supervised_patch_run(
                    sequence,
                    &run_id,
                    &workflow_id,
                    &dispatch_id,
                    &node_id,
                    operation,
                    node_metadata,
                    &created_at,
                )?;
                tx.execute(
                    "INSERT INTO workflow_runs
                     (run_sequence, run_id, plan_id, created_at, updated_at, status,
                      workflow_id, dispatch_id, started_at, completed_at, result_json,
                      boundaries_json, run_json, priority, deadline_at, sla_ms, tenant_id,
                      queue_position, pause_reason, degrade_mode)
                     VALUES ($1, $2, NULL, $3, $3, 'created', $4, $5, NULL, NULL, NULL,
                             $6, $7, 5, NULL, NULL, NULL, NULL, $8, NULL)",
                    &[
                        &sequence,
                        &run_id,
                        &created_at,
                        &workflow_id,
                        &dispatch_id,
                        &boundaries.to_string(),
                        &run.to_string(),
                        &API_OWNED_SUPERVISED_PATCH,
                    ],
                )
                .map_err(|error| error.to_string())?;
                let task_type = format!("workspace_{operation}");
                let node_json = node.to_string();
                let profile_id = node.get("profile_id").and_then(Value::as_str);
                let executor_timeout_ms = node
                    .get("executor_timeout_ms")
                    .and_then(Value::as_u64)
                    .and_then(|value| i32::try_from(value).ok());
                tx.execute(
                    "INSERT INTO workflow_run_nodes
                     (run_id, node_id, task_type, status, node_json, started_at, completed_at,
                      attempt_count, timeout_ms, blocked_reason, leased_at, profile_id)
                     VALUES ($1, $2, $3, 'pending', $4, NULL, NULL, 0, $5, NULL, NULL, $6)",
                    &[
                        &run_id,
                        &node_id,
                        &task_type,
                        &node_json,
                        &executor_timeout_ms,
                        &profile_id,
                    ],
                )
                .map_err(|error| error.to_string())?;
                let event_id = format!("workflow-event-{event_sequence:04}");
                let event_details = json!({
                    "workspace_id": workspace_id,
                    "operation": operation,
                    "attempt": attempt,
                    "binding_sha256": binding_sha256,
                    "canonical": true,
                    "metadata_only": false,
                    "execution_authority": "bounded_trusted_local",
                    "content_excluded": true,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO workflow_run_events
                     (event_sequence, event_id, run_id, node_id, event_type, actor,
                      created_at, details_json)
                     VALUES ($1, $2, $3, NULL, 'workflow_run.created', $4, $5, $6)",
                    &[
                        &event_sequence,
                        &event_id,
                        &run_id,
                        &actor,
                        &created_at,
                        &event_details,
                    ],
                )
                .map_err(|error| error.to_string())?;
                let audit_details = json!({
                    "workspace_id": workspace_id,
                    "operation": operation,
                    "attempt": attempt,
                    "binding_sha256": binding_sha256,
                    "workflow_id": workflow_id,
                    "dispatch_id": dispatch_id,
                    "graph_sha256": hex::encode(Sha256::digest(graph.to_string().as_bytes())),
                    "canonical": true,
                    "content_excluded": true,
                })
                .to_string();
                pg_append_audit(
                    &mut tx,
                    &created_at,
                    actor,
                    "supervised_patch.managed_run_created",
                    &run_id,
                    &audit_details,
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(run_id)
            }),
        }
    }

    pub fn capture_patch(&self, workspace_id: &str, actor: &str) -> Result<Value, String> {
        self.capture_patch_inner(workspace_id, actor)
    }

    fn capture_patch_inner(&self, workspace_id: &str, actor: &str) -> Result<Value, String> {
        let workspace = self
            .get_supervised_patch_workspace(workspace_id)?
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let workspace_path = workspace
            .get("workspace_path")
            .and_then(Value::as_str)
            .ok_or_else(|| "workspace missing workspace_path".to_string())?;
        let path = Path::new(workspace_path);
        if !path.exists() {
            return Err(format!(
                "workspace directory does not exist: {workspace_path}"
            ));
        }

        let workspace_mode = workspace
            .get("workspace_mode")
            .and_then(Value::as_str)
            .unwrap_or("copy");
        let config = TargetRepoOutputConfig::from_env();
        let (added, modified, deleted, changed_files, review_diff, patch_hash) = if workspace_mode
            == "git_worktree"
        {
            let changes = staged_changed_files(&config, path)?;
            if changes.changed_files.is_empty() {
                return Err("no changes detected against source revision".to_string());
            }
            let patch = stage_and_build_patch(&config, path)?;
            let hash = target_patch_hash(&patch);
            (
                changes.added,
                changes.modified,
                changes.deleted,
                changes.changed_files,
                truncate_text(patch, MAX_REVIEW_DIFF_BYTES),
                hash,
            )
        } else {
            let manifest_path = path.join(".source_manifest.json");
            let manifest: Value = if manifest_path.exists() {
                let content = std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
                serde_json::from_str(&content).unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            let (added, modified, deleted) = if manifest.is_object() {
                diff_against_manifest(path, &manifest)?
            } else {
                let (files, _) = collect_workspace_files(path)?;
                (files, Vec::new(), Vec::new())
            };
            let mut changed_files = Vec::new();
            changed_files.extend(added.iter().map(|file| format!("+{file}")));
            changed_files.extend(modified.iter().map(|file| format!("~{file}")));
            changed_files.extend(deleted.iter().map(|file| format!("-{file}")));
            let review_diff = generate_review_diff(path, &added, &modified, &deleted);
            let hash = target_patch_hash(&review_diff);
            (added, modified, deleted, changed_files, review_diff, hash)
        };

        if changed_files.is_empty() {
            return Err("no changes detected against source snapshot".to_string());
        }
        let secret_findings = scan_for_secrets(path)?;
        let redaction_status = if secret_findings.is_empty() {
            "redacted"
        } else {
            "failed"
        };
        let secret_scan_status = if secret_findings.is_empty() {
            "passed"
        } else {
            "blocked"
        };

        let review_diff = if secret_findings.is_empty() {
            review_diff
        } else {
            "review diff suppressed: secret scan failed".to_string()
        };
        let run_id = required_str(&workspace, "run_id")?;
        let verification = workspace
            .get("verification")
            .cloned()
            .unwrap_or_else(|| self.workflow_verification_evidence(run_id));

        let artifact_request = json!({
            "workspace_id": workspace_id,
            "patch_hash": patch_hash,
            "changed_files": changed_files,
            "redaction_status": redaction_status,
            "secret_scan_status": secret_scan_status,
            "review_diff": review_diff,
            "evidence_bundle": {
                "schema_version": "target_repo_evidence.v1",
                "run_id": run_id,
                "source_revision": workspace.get("source_revision"),
                "patch_hash": patch_hash,
                "changed_files": changed_files,
                "verification": verification,
                "secret_scan_status": secret_scan_status,
                "redaction_status": redaction_status,
            },
            "safety": {
                "workspace_confinement": "app_owned_directory",
                "secret_scan": secret_scan_status,
                "review_diff": if secret_findings.is_empty() { "generated" } else { "suppressed" },
                "target_repository_writes": if workspace_mode == "git_worktree" { "approval_bound_branch_only" } else { "disabled" },
            },
        });
        if workspace_mode == "git_worktree" {
            let confirmed_changes = staged_changed_files(&config, path)?;
            let confirmed_patch = inspect_git_patch(&config, path)?;
            let confirmed_hash = target_patch_hash(&confirmed_patch);
            if confirmed_hash != patch_hash || confirmed_changes.changed_files != changed_files {
                return Err("verified patch identity changed during artifact capture".to_string());
            }
        }
        let artifact = self.record_supervised_patch_artifact(&artifact_request, actor)?;

        self.update_workspace_status(workspace_id, "patch_prepared", actor)?;

        let mut result = artifact;
        if let Some(obj) = result.as_object_mut() {
            obj.insert("secret_findings".to_string(), json!(secret_findings));
            obj.insert("added".to_string(), json!(added));
            obj.insert("modified".to_string(), json!(modified));
            obj.insert("deleted".to_string(), json!(deleted));
        }
        Ok(result)
    }

    /// Atomically bind the verified Git snapshot to one artifact and advance the exact
    /// product-task version. The database row locks remain held while the bounded patch
    /// snapshot is prepared. The automatic API caller holds its scheduler-control
    /// authority guard around this entire method, so pause/kill cannot interleave
    /// between artifact persistence and the awaiting-approval transition.
    pub(crate) fn finalize_product_verification_artifact(
        &self,
        task_id: &str,
        expected_task_version: u64,
        workspace_id: &str,
        expected_patch_hash: &str,
        actor: &str,
    ) -> Result<(Value, Value), String> {
        let now = self.now();
        let artifact = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")
                    .map_err(|error| error.to_string())?;
                let result = (|| {
                    let (status, version, bound_workspace, intake_json): (
                        String,
                        i64,
                        Option<String>,
                        String,
                    ) = conn
                        .query_row(
                            "SELECT status, version, workspace_record_id, intake_json FROM product_tasks
                             WHERE task_id = ?1",
                            params![task_id],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                        )
                        .map_err(|_| format!("product task not found: {task_id}"))?;
                    if status != "verifying"
                        || version as u64 != expected_task_version
                        || bound_workspace.as_deref() != Some(workspace_id)
                    {
                        return Err("product verification artifact authority is stale".to_string());
                    }
                    let workspace_json: String = conn
                        .query_row(
                            "SELECT workspace_json FROM supervised_patch_workspaces
                             WHERE workspace_id = ?1",
                            params![workspace_id],
                            |row| row.get(0),
                        )
                        .map_err(|_| format!("workspace not found: {workspace_id}"))?;
                    let mut workspace: Value = serde_json::from_str(&workspace_json)
                        .map_err(|_| "workspace record is corrupt".to_string())?;
                    let prepared = prepare_product_artifact_fields(
                        &workspace,
                        workspace_id,
                        expected_patch_hash,
                    )?;
                    validate_product_artifact_allowed_paths(&prepared, &intake_json)?;
                    let sequence =
                        next_sequence(conn, "supervised_patch_artifacts", "artifact_sequence")?;
                    let artifact_id = format!("patch-artifact-{sequence:04}");
                    let mut artifact = prepared.artifact(sequence, &artifact_id, &now);
                    artifact
                        .as_object_mut()
                        .ok_or_else(|| "prepared product artifact must be an object".to_string())?
                        .extend([
                            ("product_task_id".to_string(), json!(task_id)),
                            (
                                "verification_task_version".to_string(),
                                json!(expected_task_version),
                            ),
                        ]);
                    conn.execute(
                        "INSERT INTO supervised_patch_artifacts
                         (artifact_sequence, artifact_id, workspace_id, run_id, plan_id, target_id,
                          source_revision, artifact_type, patch_hash, changed_files_json,
                          redaction_status, created_at, artifact_json)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'patch_diff', ?8, ?9, ?10, ?11, ?12)",
                        params![
                            sequence,
                            artifact_id,
                            workspace_id,
                            prepared.run_id,
                            prepared.plan_id,
                            prepared.target_id,
                            prepared.source_revision,
                            prepared.patch_hash,
                            prepared.changed_files.to_string(),
                            prepared.redaction_status,
                            now,
                            artifact.to_string(),
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    workspace["status"] = json!("patch_prepared");
                    workspace["updated_at"] = json!(now);
                    conn.execute(
                        "UPDATE supervised_patch_workspaces
                         SET status = 'patch_prepared', updated_at = ?1, workspace_json = ?2
                         WHERE workspace_id = ?3",
                        params![now, workspace.to_string(), workspace_id],
                    )
                    .map_err(|error| error.to_string())?;
                    let updated = conn
                        .execute(
                            "UPDATE product_tasks SET status = 'awaiting_approval', version = ?1,
                             updated_at = ?2, failure_code = NULL, failure_detail = NULL
                             WHERE task_id = ?3 AND status = 'verifying' AND version = ?4
                               AND workspace_record_id = ?5",
                            params![
                                (expected_task_version + 1) as i64,
                                now,
                                task_id,
                                expected_task_version as i64,
                                workspace_id,
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    if updated != 1 {
                        return Err("product verification artifact authority changed".to_string());
                    }
                    append_audit_locked(
                        conn,
                        &now,
                        actor,
                        "supervised_patch.artifact_record",
                        &artifact_id,
                        &json!({
                            "workspace_id": workspace_id,
                            "run_id": prepared.run_id,
                            "target_id": prepared.target_id,
                            "artifact_type": "patch_diff",
                            "metadata_only": true,
                            "execution_authority": "disabled",
                            "patch_apply_authority": "disabled",
                            "secret_scan_status": prepared.secret_scan_status,
                            "product_task_id": task_id,
                            "atomic_product_transition": true,
                        }),
                    )?;
                    append_audit_locked(
                        conn,
                        &now,
                        actor,
                        "product_task.transition",
                        task_id,
                        &json!({
                            "from": "verifying",
                            "to": "awaiting_approval",
                            "version": expected_task_version + 1,
                            "execution_admitted": false,
                            "failure_code": null,
                            "artifact_id": artifact_id,
                        }),
                    )?;
                    Ok(artifact)
                })();
                match result {
                    Ok(artifact) => {
                        conn.execute_batch("COMMIT")
                            .map_err(|error| error.to_string())?;
                        Ok(artifact)
                    }
                    Err(error) => {
                        let _ = conn.execute_batch("ROLLBACK");
                        Err(error)
                    }
                }
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let task = tx
                    .query_opt(
                        "SELECT status, version, workspace_record_id, intake_json FROM product_tasks
                         WHERE task_id = $1 FOR UPDATE",
                        &[&task_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("product task not found: {task_id}"))?;
                let status: String = task.get(0);
                let version: i64 = task.get(1);
                let bound_workspace: Option<String> = task.get(2);
                let intake_json: String = task.get(3);
                if status != "verifying"
                    || version as u64 != expected_task_version
                    || bound_workspace.as_deref() != Some(workspace_id)
                {
                    return Err("product verification artifact authority is stale".to_string());
                }
                let row = tx
                    .query_opt(
                        "SELECT workspace_json FROM supervised_patch_workspaces
                         WHERE workspace_id = $1 FOR UPDATE",
                        &[&workspace_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
                let workspace_json: String = row.get(0);
                let mut workspace: Value = serde_json::from_str(&workspace_json)
                    .map_err(|_| "workspace record is corrupt".to_string())?;
                let prepared =
                    prepare_product_artifact_fields(&workspace, workspace_id, expected_patch_hash)?;
                validate_product_artifact_allowed_paths(&prepared, &intake_json)?;
                tx.batch_execute(
                    "LOCK TABLE supervised_patch_artifacts IN SHARE ROW EXCLUSIVE MODE",
                )
                .map_err(|error| error.to_string())?;
                let sequence =
                    pg_next_sequence(&mut tx, "supervised_patch_artifacts", "artifact_sequence")?;
                let artifact_id = format!("patch-artifact-{sequence:04}");
                let mut artifact = prepared.artifact(sequence, &artifact_id, &now);
                artifact
                    .as_object_mut()
                    .ok_or_else(|| "prepared product artifact must be an object".to_string())?
                    .extend([
                        ("product_task_id".to_string(), json!(task_id)),
                        (
                            "verification_task_version".to_string(),
                            json!(expected_task_version),
                        ),
                    ]);
                tx.execute(
                    "INSERT INTO supervised_patch_artifacts
                     (artifact_sequence, artifact_id, workspace_id, run_id, plan_id, target_id,
                      source_revision, artifact_type, patch_hash, changed_files_json,
                      redaction_status, created_at, artifact_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, 'patch_diff', $8, $9, $10, $11, $12)",
                    &[
                        &sequence,
                        &artifact_id,
                        &workspace_id,
                        &prepared.run_id,
                        &prepared.plan_id,
                        &prepared.target_id,
                        &prepared.source_revision,
                        &prepared.patch_hash,
                        &prepared.changed_files.to_string(),
                        &prepared.redaction_status,
                        &now,
                        &artifact.to_string(),
                    ],
                )
                .map_err(|error| error.to_string())?;
                workspace["status"] = json!("patch_prepared");
                workspace["updated_at"] = json!(now);
                tx.execute(
                    "UPDATE supervised_patch_workspaces
                     SET status = 'patch_prepared', updated_at = $1, workspace_json = $2
                     WHERE workspace_id = $3",
                    &[&now, &workspace.to_string(), &workspace_id],
                )
                .map_err(|error| error.to_string())?;
                let updated = tx
                    .execute(
                        "UPDATE product_tasks SET status = 'awaiting_approval', version = $1,
                         updated_at = $2, failure_code = NULL, failure_detail = NULL
                         WHERE task_id = $3 AND status = 'verifying' AND version = $4
                           AND workspace_record_id = $5",
                        &[
                            &((expected_task_version + 1) as i64),
                            &now,
                            &task_id,
                            &(expected_task_version as i64),
                            &workspace_id,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err("product verification artifact authority changed".to_string());
                }
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "supervised_patch.artifact_record",
                    &artifact_id,
                    &json!({
                        "workspace_id": workspace_id,
                        "run_id": prepared.run_id,
                        "target_id": prepared.target_id,
                        "artifact_type": "patch_diff",
                        "metadata_only": true,
                        "execution_authority": "disabled",
                        "patch_apply_authority": "disabled",
                        "secret_scan_status": prepared.secret_scan_status,
                        "product_task_id": task_id,
                        "atomic_product_transition": true,
                    })
                    .to_string(),
                )?;
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "product_task.transition",
                    task_id,
                    &json!({
                        "from": "verifying",
                        "to": "awaiting_approval",
                        "version": expected_task_version + 1,
                        "execution_admitted": false,
                        "failure_code": null,
                        "artifact_id": artifact_id,
                    })
                    .to_string(),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(artifact)
            })?,
        };
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| "product task missing after artifact transition".to_string())?;
        Ok((artifact, task))
    }

    fn workflow_verification_evidence(&self, run_id: &str) -> Value {
        let run = self.get_workflow_run(run_id).ok().flatten();
        let nodes = run
            .as_ref()
            .and_then(|value| value.get("nodes"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let node_results: Vec<Value> = nodes
            .iter()
            .map(|node| {
                json!({
                    "node_id": node.get("node_id"),
                    "task_type": node.get("task_type"),
                    "status": node.get("status"),
                    "executor_type": node.get("execution_result").and_then(|result| result.get("executor_type")),
                    "result_status": node.get("execution_result").and_then(|result| result.get("status")),
                    "latency_ms": node.get("execution_result").and_then(|result| result.get("latency_ms")),
                })
            })
            .collect();
        let passed = node_results.iter().filter(|node| {
            matches!(
                node.get("status").and_then(Value::as_str),
                Some("completed" | "succeeded")
            ) || matches!(
                node.get("result_status").and_then(Value::as_str),
                Some("completed" | "succeeded" | "success")
            )
        });
        let passed_count = passed.count();
        let run_status = run
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str);
        let verification_status = if run_status == Some("completed")
            && !node_results.is_empty()
            && passed_count == node_results.len()
        {
            "evidence_recorded"
        } else if run.is_some() && !node_results.is_empty() {
            "verification_failed"
        } else {
            "not_run"
        };
        json!({
            "schema_version": "workflow_verification_evidence.v1",
            "run_id": run_id,
            "run_status": run_status,
            "node_count": node_results.len(),
            "passed_count": passed_count,
            "status": verification_status,
            "nodes": node_results,
        })
    }

    pub fn validate_artifact_integrity(&self, artifact_id: &str) -> Result<Value, String> {
        let artifact = self
            .get_supervised_patch_artifact(artifact_id)?
            .ok_or_else(|| format!("artifact not found: {artifact_id}"))?;
        let workspace_id = artifact
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("");
        let workspace = self.get_supervised_patch_workspace(workspace_id)?;
        let workspace_path = workspace
            .as_ref()
            .and_then(|w| w.get("workspace_path"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let patch_hash = artifact
            .get("patch_hash")
            .and_then(Value::as_str)
            .unwrap_or("");
        let changed_files = artifact
            .get("changed_files")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let redaction_status = artifact
            .get("redaction_status")
            .and_then(Value::as_str)
            .unwrap_or("pending");

        let mut checks = Vec::new();

        checks.push(json!({
            "check": "patch_hash_non_empty",
            "passed": !patch_hash.is_empty(),
            "message": if patch_hash.is_empty() { "patch hash is empty" } else { "ok" }
        }));
        checks.push(json!({
            "check": "changed_files_non_empty",
            "passed": !changed_files.is_empty(),
            "message": if changed_files.is_empty() { "no changed files" } else { "ok" }
        }));
        checks.push(json!({
            "check": "redaction_not_failed",
            "passed": redaction_status != "failed",
            "message": if redaction_status == "failed" { "redaction failed" } else { "ok" }
        }));

        let workspace_exists = !workspace_path.is_empty() && Path::new(workspace_path).exists();
        checks.push(json!({
            "check": "workspace_exists",
            "passed": workspace_exists,
            "message": if workspace_exists { "ok" } else { "workspace directory missing" }
        }));

        let mut current_hash = String::new();
        let mut hash_message = "workspace content could not be hashed".to_string();
        if workspace_exists {
            let workspace_mode = workspace
                .as_ref()
                .and_then(|value| value.get("workspace_mode"))
                .and_then(Value::as_str)
                .unwrap_or("copy");
            if workspace_mode == "git_worktree" {
                match inspect_git_patch(
                    &TargetRepoOutputConfig::from_env(),
                    Path::new(workspace_path),
                ) {
                    Ok(patch) => {
                        current_hash = target_patch_hash(&patch);
                        hash_message = "ok".to_string();
                    }
                    Err(error) => hash_message = error,
                }
            } else {
                let manifest_path = Path::new(workspace_path).join(".source_manifest.json");
                if manifest_path.exists() {
                    let manifest_content =
                        std::fs::read_to_string(&manifest_path).unwrap_or_default();
                    let manifest: Value =
                        serde_json::from_str(&manifest_content).unwrap_or(Value::Null);
                    if manifest.is_object() {
                        let (added, modified, deleted) =
                            diff_against_manifest(Path::new(workspace_path), &manifest)?;
                        let review_diff = generate_review_diff(
                            Path::new(workspace_path),
                            &added,
                            &modified,
                            &deleted,
                        );
                        current_hash = target_patch_hash(&review_diff);
                        hash_message = "ok".to_string();
                    }
                }
            }
        }
        let hash_unchanged = !current_hash.is_empty() && current_hash == patch_hash;
        checks.push(json!({
            "check": "patch_hash_unchanged",
            "passed": hash_unchanged,
            "message": if hash_unchanged {
                "ok".to_string()
            } else if current_hash.is_empty() {
                hash_message
            } else {
                format!("hash changed: recorded={} current={}", patch_hash, current_hash)
            }
        }));

        let all_passed = checks
            .iter()
            .all(|c| c["passed"].as_bool().unwrap_or(false));
        Ok(json!({
            "artifact_id": artifact_id,
            "integrity_ok": all_passed,
            "checks": checks,
        }))
    }

    pub fn record_supervised_patch_workspace(
        &self,
        request: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        let run_id = required_str(request, "run_id")?;
        let plan_id = optional_str(request, "plan_id");
        let target_id = required_str(request, "target_id")?;
        let target_repo_path = required_str(request, "target_repo_path")?;
        let workspace_path = required_str(request, "workspace_path")?;
        let source_revision = required_str(request, "source_revision")?;
        let source_tree_hash = optional_str(request, "source_tree_hash");
        let workspace_mode = optional_str(request, "workspace_mode").unwrap_or("copy");
        if !matches!(workspace_mode, "copy" | "git_worktree") {
            return Err(format!(
                "invalid supervised patch workspace mode: {workspace_mode}"
            ));
        }
        let git = request.get("git").cloned().unwrap_or(Value::Null);
        let status = optional_str(request, "status").unwrap_or("requested");
        if !is_valid_workspace_status(status) {
            return Err(format!(
                "invalid supervised patch workspace status: {status}"
            ));
        }

        let boundary = supervised_patch_boundary(target_repo_path, workspace_path, workspace_mode)?;
        let target_repo_canonical_path = required_str(&boundary, "target_repo_canonical_path")?;
        let workspace_canonical_path = required_str(&boundary, "workspace_canonical_path")?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence =
                    next_sequence(conn, "supervised_patch_workspaces", "workspace_sequence")?;
                let workspace_id = format!("patch-workspace-{sequence:04}");
                let created_at = self.now();
                let workspace = json!({
                    "schema_version": SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION,
                    "workspace_sequence": sequence,
                    "workspace_id": workspace_id.clone(),
                    "plan_id": plan_id,
                    "run_id": run_id,
                    "target_id": target_id,
                    "target_repo_path": target_repo_path,
                    "target_repo_canonical_path": target_repo_canonical_path,
                    "workspace_path": workspace_path,
                    "workspace_canonical_path": workspace_canonical_path,
                    "source_revision": source_revision,
                    "source_tree_hash": source_tree_hash,
                    "workspace_mode": workspace_mode,
                    "git": git,
                    "status": status,
                    "created_at": created_at,
                    "updated_at": created_at,
                    "boundary": boundary.clone(),
                    "metadata_only": true,
                    "execution_authority": "disabled",
                    "verification_execution_authority": "allowlisted_commands",
                    "target_output_authority": if workspace_mode == "git_worktree" { "approval_bound" } else { "disabled" },
                    "safety": workspace_safety_profile(workspace_mode),
                });
                conn.execute(
                    "INSERT INTO supervised_patch_workspaces
                     (workspace_sequence, workspace_id, plan_id, run_id, target_id,
                      target_repo_path, target_repo_canonical_path, workspace_path,
                      workspace_canonical_path, source_revision, source_tree_hash, status,
                      created_at, updated_at, boundary_json, workspace_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    params![
                        sequence,
                        workspace_id,
                        plan_id,
                        run_id,
                        target_id,
                        target_repo_path,
                        target_repo_canonical_path,
                        workspace_path,
                        workspace_canonical_path,
                        source_revision,
                        source_tree_hash,
                        status,
                        created_at,
                        created_at,
                        boundary.to_string(),
                        workspace.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "supervised_patch.workspace_record",
                    &workspace_id,
                    &json!({
                        "run_id": run_id,
                        "plan_id": plan_id,
                        "target_id": target_id,
                        "source_revision": source_revision,
                        "metadata_only": true,
                    "workspace_mode": workspace_mode,
                    "target_repository_writes": if workspace_mode == "git_worktree" { "approval_bound_branch_only" } else { "disabled" },
                    "registered_git_worktree": if workspace_mode == "git_worktree" { "controlled" } else { "forbidden" },
                    "workspace_directory_creation": boundary
                        .get("workspace_directory_creation")
                        .and_then(Value::as_str)
                        .unwrap_or("not_performed"),
                    "execution_authority": "disabled",
                    "workspace_confinement": boundary.get("workspace_confinement"),
                    "kill_switch": "quarantine_or_cleanup",
                }),
                )?;
                Ok(workspace)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence =
                    pg_next_sequence(client, "supervised_patch_workspaces", "workspace_sequence")?;
                let workspace_id = format!("patch-workspace-{sequence:04}");
                let created_at = self.now();
                let workspace = json!({
                    "schema_version": SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION,
                    "workspace_sequence": sequence,
                    "workspace_id": workspace_id.clone(),
                    "plan_id": plan_id,
                    "run_id": run_id,
                    "target_id": target_id,
                    "target_repo_path": target_repo_path,
                    "target_repo_canonical_path": target_repo_canonical_path,
                    "workspace_path": workspace_path,
                    "workspace_canonical_path": workspace_canonical_path,
                    "source_revision": source_revision,
                    "source_tree_hash": source_tree_hash,
                    "workspace_mode": workspace_mode,
                    "git": git,
                    "status": status,
                    "created_at": created_at,
                    "updated_at": created_at,
                    "boundary": boundary.clone(),
                    "metadata_only": true,
                    "execution_authority": "disabled",
                    "verification_execution_authority": "allowlisted_commands",
                    "target_output_authority": if workspace_mode == "git_worktree" { "approval_bound" } else { "disabled" },
                    "safety": workspace_safety_profile(workspace_mode),
                });
                client.execute(
                    "INSERT INTO supervised_patch_workspaces
                     (workspace_sequence, workspace_id, plan_id, run_id, target_id,
                      target_repo_path, target_repo_canonical_path, workspace_path,
                      workspace_canonical_path, source_revision, source_tree_hash, status,
                      created_at, updated_at, boundary_json, workspace_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
                    &[
                        &sequence,
                        &workspace_id,
                        &plan_id,
                        &run_id,
                        &target_id,
                        &target_repo_path,
                        &target_repo_canonical_path,
                        &workspace_path,
                        &workspace_canonical_path,
                        &source_revision,
                        &source_tree_hash,
                        &status,
                        &created_at,
                        &created_at,
                        &boundary.to_string(),
                        &workspace.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                let audit_details = json!({
                    "run_id": run_id,
                    "plan_id": plan_id,
                    "target_id": target_id,
                    "source_revision": source_revision,
                    "metadata_only": true,
                    "workspace_mode": workspace_mode,
                    "target_repository_writes": if workspace_mode == "git_worktree" { "approval_bound_branch_only" } else { "disabled" },
                    "registered_git_worktree": if workspace_mode == "git_worktree" { "controlled" } else { "forbidden" },
                    "workspace_directory_creation": boundary
                        .get("workspace_directory_creation")
                        .and_then(Value::as_str)
                        .unwrap_or("not_performed"),
                    "execution_authority": "disabled",
                    "workspace_confinement": boundary.get("workspace_confinement"),
                    "kill_switch": "quarantine_or_cleanup",
                }).to_string();
                pg_append_audit(client, &created_at, actor, "supervised_patch.workspace_record", &workspace_id, &audit_details)?;
                Ok(workspace)
            }),
        }
    }

    pub fn get_supervised_patch_workspace_for_run(
        &self,
        run_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT workspace_sequence, workspace_id, plan_id, run_id, target_id,
                                target_repo_path, target_repo_canonical_path, workspace_path,
                                workspace_canonical_path, source_revision, source_tree_hash, status,
                                created_at, updated_at, boundary_json, workspace_json
                         FROM supervised_patch_workspaces
                         WHERE run_id = ?1
                         ORDER BY workspace_sequence DESC
                         LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![run_id], supervised_patch_workspace_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(Ok(value)) => Ok(Some(value)),
                    Some(Err(e)) => Err(e.to_string()),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT workspace_sequence, workspace_id, plan_id, run_id, target_id,
                                target_repo_path, target_repo_canonical_path, workspace_path,
                                workspace_canonical_path, source_revision, source_tree_hash, status,
                                created_at, updated_at, boundary_json, workspace_json
                         FROM supervised_patch_workspaces
                         WHERE run_id = $1
                         ORDER BY workspace_sequence DESC
                         LIMIT 1",
                        &[&run_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_supervised_patch_workspace_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub(crate) fn supervised_patch_workspace_count_for_run(
        &self,
        run_id: &str,
    ) -> Result<i64, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM supervised_patch_workspaces WHERE run_id=?1",
                    params![run_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_one(
                        "SELECT COUNT(*) FROM supervised_patch_workspaces WHERE run_id=$1",
                        &[&run_id],
                    )
                    .map(|row| row.get(0))
                    .map_err(|error| error.to_string())
            }),
        }
    }

    pub fn get_supervised_patch_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT workspace_sequence, workspace_id, plan_id, run_id, target_id,
                                target_repo_path, target_repo_canonical_path, workspace_path,
                                workspace_canonical_path, source_revision, source_tree_hash, status,
                                created_at, updated_at, boundary_json, workspace_json
                         FROM supervised_patch_workspaces
                         WHERE workspace_id = ?1
                         LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![workspace_id], supervised_patch_workspace_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(Ok(value)) => Ok(Some(value)),
                    Some(Err(e)) => Err(e.to_string()),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT workspace_sequence, workspace_id, plan_id, run_id, target_id,
                                target_repo_path, target_repo_canonical_path, workspace_path,
                                workspace_canonical_path, source_revision, source_tree_hash, status,
                                created_at, updated_at, boundary_json, workspace_json
                         FROM supervised_patch_workspaces
                         WHERE workspace_id = $1
                         LIMIT 1",
                        &[&workspace_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_supervised_patch_workspace_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn supervised_patch_workspaces(&self, limit: i64) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT workspace_sequence, workspace_id, plan_id, run_id, target_id,
                                target_repo_path, target_repo_canonical_path, workspace_path,
                                workspace_canonical_path, source_revision, source_tree_hash, status,
                                created_at, updated_at, boundary_json, workspace_json
                         FROM supervised_patch_workspaces
                         ORDER BY workspace_sequence DESC
                         LIMIT ?1",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit], supervised_patch_workspace_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT workspace_sequence, workspace_id, plan_id, run_id, target_id,
                                target_repo_path, target_repo_canonical_path, workspace_path,
                                workspace_canonical_path, source_revision, source_tree_hash, status,
                                created_at, updated_at, boundary_json, workspace_json
                         FROM supervised_patch_workspaces
                         ORDER BY workspace_sequence DESC
                         LIMIT $1",
                        &[&limit],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(rows.iter().map(pg_supervised_patch_workspace_row).collect())
            }),
        }
    }

    pub fn export_supervised_patch_workspaces(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.supervised_patch_workspaces(limit)
    }

    pub fn import_supervised_patch_workspace(&self, workspace: &Value) -> Result<bool, String> {
        ensure_schema_version(
            workspace,
            "supervised patch workspace",
            SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION,
        )?;
        ensure_optional_bool_field(workspace, "metadata_only", true)?;
        ensure_optional_string_field(workspace, "execution_authority", "disabled")?;
        let workspace_id = required_str(workspace, "workspace_id")?;
        if self.get_supervised_patch_workspace(workspace_id)?.is_some() {
            return Ok(false);
        }
        let run_id = required_str(workspace, "run_id")?;
        let plan_id = optional_str(workspace, "plan_id");
        let target_id = required_str(workspace, "target_id")?;
        let target_repo_path = required_str(workspace, "target_repo_path")?;
        let target_repo_canonical_path = required_str(workspace, "target_repo_canonical_path")?;
        let workspace_path = required_str(workspace, "workspace_path")?;
        let workspace_canonical_path = required_str(workspace, "workspace_canonical_path")?;
        let source_revision = required_str(workspace, "source_revision")?;
        let source_tree_hash = optional_str(workspace, "source_tree_hash");
        let workspace_mode = optional_str(workspace, "workspace_mode").unwrap_or("copy");
        if !matches!(workspace_mode, "copy" | "git_worktree") {
            return Err(format!(
                "invalid supervised patch workspace mode: {workspace_mode}"
            ));
        }
        let status = optional_str(workspace, "status").unwrap_or("requested");
        if !is_valid_workspace_status(status) {
            return Err(format!(
                "invalid supervised patch workspace status: {status}"
            ));
        }
        let boundary = workspace.get("boundary").cloned().unwrap_or_else(|| {
            json!({
                "metadata_only": true,
                "execution_authority": "disabled",
                "verification_execution_authority": "allowlisted_commands",
                "workspace_directory_creation": if workspace_mode == "git_worktree" { "git_worktree" } else { "not_performed" },
                "target_repository_writes": if workspace_mode == "git_worktree" { "approval_bound_branch_only" } else { "disabled" },
                "registered_git_worktree": if workspace_mode == "git_worktree" { "controlled" } else { "forbidden" },
                "git_worktree_add": if workspace_mode == "git_worktree" { "performed" } else { "forbidden" },
                "process_execution": "allowlisted_verification_only",
                "provider_calls": "disabled",
                "push_merge_deploy_apply": if workspace_mode == "git_worktree" { "approval_bound_push_only" } else { "disabled" },
                "target_repo_canonical_path": target_repo_canonical_path,
                "workspace_canonical_path": workspace_canonical_path,
            })
        });
        validate_imported_workspace_boundary(
            target_repo_path,
            target_repo_canonical_path,
            workspace_path,
            workspace_canonical_path,
            workspace_mode,
            &boundary,
        )?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence =
                    next_sequence(conn, "supervised_patch_workspaces", "workspace_sequence")?;
                let created_at = optional_str(workspace, "created_at")
                    .map(str::to_string)
                    .unwrap_or_else(|| self.now());
                let updated_at = optional_str(workspace, "updated_at")
                    .map(str::to_string)
                    .unwrap_or_else(|| created_at.clone());
                let workspace_record = build_import_workspace_record(
                    workspace, workspace_id, sequence, plan_id, run_id, target_id,
                    target_repo_path, target_repo_canonical_path, workspace_path,
                    workspace_canonical_path, source_revision, source_tree_hash,
                    status, &boundary, &created_at, &updated_at,
                )?;
                conn.execute(
                    "INSERT INTO supervised_patch_workspaces
                     (workspace_sequence, workspace_id, plan_id, run_id, target_id,
                      target_repo_path, target_repo_canonical_path, workspace_path,
                      workspace_canonical_path, source_revision, source_tree_hash, status,
                      created_at, updated_at, boundary_json, workspace_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                    params![
                        sequence,
                        workspace_id,
                        plan_id,
                        run_id,
                        target_id,
                        target_repo_path,
                        target_repo_canonical_path,
                        workspace_path,
                        workspace_canonical_path,
                        source_revision,
                        source_tree_hash,
                        status,
                        created_at,
                        updated_at,
                        boundary.to_string(),
                        workspace_record.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &self.now(),
                    "import",
                    "supervised_patch.workspace_import",
                    workspace_id,
                    &json!({"run_id": run_id, "metadata_only": true}),
                )?;
                Ok(true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence =
                    pg_next_sequence(client, "supervised_patch_workspaces", "workspace_sequence")?;
                let created_at = optional_str(workspace, "created_at")
                    .map(str::to_string)
                    .unwrap_or_else(|| self.now());
                let updated_at = optional_str(workspace, "updated_at")
                    .map(str::to_string)
                    .unwrap_or_else(|| created_at.clone());
                let workspace_record = build_import_workspace_record(
                    workspace, workspace_id, sequence, plan_id, run_id, target_id,
                    target_repo_path, target_repo_canonical_path, workspace_path,
                    workspace_canonical_path, source_revision, source_tree_hash,
                    status, &boundary, &created_at, &updated_at,
                )?;
                client.execute(
                    "INSERT INTO supervised_patch_workspaces
                     (workspace_sequence, workspace_id, plan_id, run_id, target_id,
                      target_repo_path, target_repo_canonical_path, workspace_path,
                      workspace_canonical_path, source_revision, source_tree_hash, status,
                      created_at, updated_at, boundary_json, workspace_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
                    &[
                        &sequence,
                        &workspace_id,
                        &plan_id,
                        &run_id,
                        &target_id,
                        &target_repo_path,
                        &target_repo_canonical_path,
                        &workspace_path,
                        &workspace_canonical_path,
                        &source_revision,
                        &source_tree_hash,
                        &status,
                        &created_at,
                        &updated_at,
                        &boundary.to_string(),
                        &workspace_record.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                let audit_details = json!({"run_id": run_id, "metadata_only": true}).to_string();
                pg_append_audit(client, &self.now(), "import", "supervised_patch.workspace_import", workspace_id, &audit_details)?;
                Ok(true)
            }),
        }
    }

    pub fn record_supervised_patch_artifact(
        &self,
        request: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        let workspace_id = required_str(request, "workspace_id")?;
        let workspace = self
            .get_supervised_patch_workspace(workspace_id)?
            .ok_or_else(|| format!("supervised patch workspace not found: {workspace_id}"))?;
        let artifact_type = optional_str(request, "artifact_type").unwrap_or("patch_diff");
        if artifact_type != "patch_diff" {
            return Err(format!(
                "invalid supervised patch artifact type: {artifact_type}"
            ));
        }
        let patch_hash = required_str(request, "patch_hash")?;
        let changed_files = normalize_changed_files(request.get("changed_files"))?;
        let redaction_status = optional_str(request, "redaction_status").unwrap_or("pending");
        if !matches!(redaction_status, "pending" | "redacted" | "failed") {
            return Err(format!(
                "invalid supervised patch artifact redaction_status: {redaction_status}"
            ));
        }
        let secret_scan_status = optional_str(request, "secret_scan_status").unwrap_or("pending");
        if !matches!(secret_scan_status, "pending" | "passed" | "blocked") {
            return Err(format!(
                "invalid supervised patch artifact secret_scan_status: {secret_scan_status}"
            ));
        }
        let review_diff = request
            .get("review_diff")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        validate_review_diff_redaction(redaction_status, &review_diff)?;
        let storage_refs = request
            .get("storage_refs")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let retention_expires_at = optional_str(request, "retention_expires_at");
        let safety = request
            .get("safety")
            .cloned()
            .unwrap_or_else(artifact_safety_profile);
        let evidence_bundle = request
            .get("evidence_bundle")
            .cloned()
            .unwrap_or(Value::Null);
        let run_id = required_str(&workspace, "run_id")?;
        let plan_id = optional_str(&workspace, "plan_id");
        let target_id = required_str(&workspace, "target_id")?;
        let source_revision = required_str(&workspace, "source_revision")?;

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence =
                    next_sequence(conn, "supervised_patch_artifacts", "artifact_sequence")?;
                let artifact_id = format!("patch-artifact-{sequence:04}");
                let created_at = self.now();
                let artifact = json!({
                    "schema_version": SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION,
                    "artifact_sequence": sequence,
                    "artifact_id": artifact_id.clone(),
                    "workspace_id": workspace_id,
                    "run_id": run_id,
                    "plan_id": plan_id,
                    "target_id": target_id,
                    "source_revision": source_revision,
                    "artifact_type": artifact_type,
                    "patch_hash": patch_hash,
                    "changed_files": changed_files.clone(),
                    "redaction_status": redaction_status,
                    "secret_scan_status": secret_scan_status,
                    "review_diff": review_diff,
                    "storage_refs": storage_refs.clone(),
                    "evidence_bundle": evidence_bundle.clone(),
                    "safety": safety.clone(),
                    "retention_expires_at": retention_expires_at,
                    "created_at": created_at,
                    "metadata_only": true,
                    "execution_authority": "disabled",
                    "patch_apply_authority": "disabled",
                    "artifact_file_created": false,
                });
                conn.execute(
                    "INSERT INTO supervised_patch_artifacts
                     (artifact_sequence, artifact_id, workspace_id, run_id, plan_id, target_id,
                      source_revision, artifact_type, patch_hash, changed_files_json,
                      redaction_status, created_at, artifact_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        sequence,
                        artifact_id,
                        workspace_id,
                        run_id,
                        plan_id,
                        target_id,
                        source_revision,
                        artifact_type,
                        patch_hash,
                        changed_files.to_string(),
                        redaction_status,
                        created_at,
                        artifact.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "supervised_patch.artifact_record",
                    &artifact_id,
                    &json!({
                        "workspace_id": workspace_id,
                        "run_id": run_id,
                        "target_id": target_id,
                        "artifact_type": artifact_type,
                        "metadata_only": true,
                        "execution_authority": "disabled",
                        "patch_apply_authority": "disabled",
                        "secret_scan_status": secret_scan_status,
                    }),
                )?;
                Ok(artifact)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence =
                    pg_next_sequence(client, "supervised_patch_artifacts", "artifact_sequence")?;
                let artifact_id = format!("patch-artifact-{sequence:04}");
                let created_at = self.now();
                let artifact = json!({
                    "schema_version": SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION,
                    "artifact_sequence": sequence,
                    "artifact_id": artifact_id.clone(),
                    "workspace_id": workspace_id,
                    "run_id": run_id,
                    "plan_id": plan_id,
                    "target_id": target_id,
                    "source_revision": source_revision,
                    "artifact_type": artifact_type,
                    "patch_hash": patch_hash,
                    "changed_files": changed_files.clone(),
                    "redaction_status": redaction_status,
                    "secret_scan_status": secret_scan_status,
                    "review_diff": review_diff,
                    "storage_refs": storage_refs.clone(),
                    "evidence_bundle": evidence_bundle.clone(),
                    "safety": safety.clone(),
                    "retention_expires_at": retention_expires_at,
                    "created_at": created_at,
                    "metadata_only": true,
                    "execution_authority": "disabled",
                    "patch_apply_authority": "disabled",
                    "artifact_file_created": false,
                });
                client
                    .execute(
                        "INSERT INTO supervised_patch_artifacts
                     (artifact_sequence, artifact_id, workspace_id, run_id, plan_id, target_id,
                      source_revision, artifact_type, patch_hash, changed_files_json,
                      redaction_status, created_at, artifact_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                        &[
                            &sequence,
                            &artifact_id,
                            &workspace_id,
                            &run_id,
                            &plan_id,
                            &target_id,
                            &source_revision,
                            &artifact_type,
                            &patch_hash,
                            &changed_files.to_string(),
                            &redaction_status,
                            &created_at,
                            &artifact.to_string(),
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let audit_details = json!({
                    "workspace_id": workspace_id,
                    "run_id": run_id,
                    "target_id": target_id,
                    "artifact_type": artifact_type,
                    "metadata_only": true,
                    "execution_authority": "disabled",
                    "patch_apply_authority": "disabled",
                    "secret_scan_status": secret_scan_status,
                })
                .to_string();
                pg_append_audit(
                    client,
                    &created_at,
                    actor,
                    "supervised_patch.artifact_record",
                    &artifact_id,
                    &audit_details,
                )?;
                Ok(artifact)
            }),
        }
    }

    pub fn get_supervised_patch_artifact(
        &self,
        artifact_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT artifact_sequence, artifact_id, workspace_id, run_id, plan_id,
                                target_id, source_revision, artifact_type, patch_hash,
                                changed_files_json, redaction_status, created_at, artifact_json
                         FROM supervised_patch_artifacts
                         WHERE artifact_id = ?1
                         LIMIT 1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![artifact_id], supervised_patch_artifact_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(Ok(value)) => Ok(Some(value)),
                    Some(Err(e)) => Err(e.to_string()),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT artifact_sequence, artifact_id, workspace_id, run_id, plan_id,
                                target_id, source_revision, artifact_type, patch_hash,
                                changed_files_json, redaction_status, created_at, artifact_json
                         FROM supervised_patch_artifacts
                         WHERE artifact_id = $1
                         LIMIT 1",
                        &[&artifact_id],
                    )
                    .map_err(|e| e.to_string())?;
                match rows.into_iter().next() {
                    Some(row) => Ok(Some(pg_supervised_patch_artifact_row(&row))),
                    None => Ok(None),
                }
            }),
        }
    }

    pub fn claim_target_output(
        &self,
        artifact_id: &str,
        request_binding: &Value,
        request_sha256: &str,
        actor: &str,
    ) -> Result<TargetOutputClaim, String> {
        validate_target_output_request_hash(request_binding, request_sha256)?;
        let claimed_at = self.now();
        let claim = |mut artifact: Value| -> Result<(Value, TargetOutputClaim, bool), String> {
            validate_target_output_binding(&artifact, artifact_id, request_binding)?;
            if let Some(existing) = artifact.get("target_output_receipt") {
                validate_target_output_receipt(&artifact, existing)?;
                if existing.get("request_sha256").and_then(Value::as_str) != Some(request_sha256)
                    || existing.get("request_binding") != Some(request_binding)
                {
                    return Err("target output request does not match durable receipt".to_string());
                }
                return match existing.get("state").and_then(Value::as_str) {
                    Some("completed") => Ok((
                        artifact.clone(),
                        TargetOutputClaim::Reused(existing.get("output").cloned().ok_or_else(
                            || "completed target output receipt is missing output".to_string(),
                        )?),
                        false,
                    )),
                    Some(state @ ("sending" | "outcome_unknown")) => Ok((
                        artifact.clone(),
                        TargetOutputClaim::ReconciliationRequired(state.to_string()),
                        false,
                    )),
                    _ => Err("target output receipt has invalid state".to_string()),
                };
            }
            let receipt = json!({
                "schema_version": "target_repo_output_receipt.v1",
                "state": "sending",
                "artifact_id": artifact_id,
                "workspace_id": artifact.get("workspace_id"),
                "run_id": artifact.get("run_id"),
                "target_id": artifact.get("target_id"),
                "source_revision": artifact.get("source_revision"),
                "patch_hash": artifact.get("patch_hash"),
                "request_binding": request_binding,
                "request_sha256": request_sha256,
                "output": Value::Null,
                "output_sha256": Value::Null,
                "claimed_at": claimed_at,
                "completed_at": Value::Null,
            });
            validate_target_output_receipt(&artifact, &receipt)?;
            artifact
                .as_object_mut()
                .ok_or_else(|| "supervised patch artifact must be an object".to_string())?
                .insert("target_output_receipt".to_string(), receipt);
            Ok((artifact, TargetOutputClaim::Claimed, true))
        };

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let raw: String = tx.query_row(
                    "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = ?1",
                    params![artifact_id],
                    |row| row.get(0),
                ).map_err(|error| error.to_string())?;
                let (artifact, result, changed) = claim(
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?
                )?;
                if changed {
                    tx.execute(
                        "UPDATE supervised_patch_artifacts SET artifact_json = ?1 WHERE artifact_id = ?2",
                        params![artifact.to_string(), artifact_id],
                    ).map_err(|error| error.to_string())?;
                    append_audit_locked(&tx, &claimed_at, actor,
                        "supervised_patch.target_output_claimed", artifact_id,
                        &json!({"request_sha256": request_sha256, "receipt_state": "sending"}))?;
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(result)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx.query_one(
                    "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = $1 FOR UPDATE",
                    &[&artifact_id],
                ).map_err(|error| error.to_string())?;
                let raw: String = row.get(0);
                let (artifact, result, changed) = claim(
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?
                )?;
                if changed {
                    tx.execute(
                        "UPDATE supervised_patch_artifacts SET artifact_json = $1 WHERE artifact_id = $2",
                        &[&artifact.to_string(), &artifact_id],
                    ).map_err(|error| error.to_string())?;
                    let details = json!({"request_sha256": request_sha256, "receipt_state": "sending"}).to_string();
                    pg_append_audit(&mut tx, &claimed_at, actor,
                        "supervised_patch.target_output_claimed", artifact_id, &details)?;
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(result)
            }),
        }
    }

    pub fn record_target_output_receipt(
        &self,
        artifact_id: &str,
        request_binding: &Value,
        request_sha256: &str,
        output: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        validate_target_output_request_hash(request_binding, request_sha256)?;
        let completed_at = self.now();
        let update = |mut artifact: Value| -> Result<(Value, Value), String> {
            validate_target_output_binding(&artifact, artifact_id, request_binding)?;
            let existing = artifact
                .get("target_output_receipt")
                .cloned()
                .ok_or_else(|| "target output must be claimed before finalization".to_string())?;
            validate_target_output_receipt(&artifact, &existing)?;
            if existing.get("request_sha256").and_then(Value::as_str) != Some(request_sha256)
                || existing.get("request_binding") != Some(request_binding)
            {
                return Err("target output finalization binding changed".to_string());
            }
            if existing.get("state").and_then(Value::as_str) == Some("completed") {
                let durable_output = existing.get("output").cloned().unwrap_or(Value::Null);
                if &durable_output != output {
                    return Err("target output completed under a different result".to_string());
                }
                return Ok((artifact, existing));
            }
            if existing.get("state").and_then(Value::as_str) != Some("sending") {
                return Err("target output receipt is not finalizable".to_string());
            }
            let output_sha256 = target_output_json_sha256(output)?;
            let receipt = json!({
                "schema_version": "target_repo_output_receipt.v1",
                "state": "completed",
                "artifact_id": artifact_id,
                "workspace_id": artifact.get("workspace_id"),
                "run_id": artifact.get("run_id"),
                "target_id": artifact.get("target_id"),
                "source_revision": artifact.get("source_revision"),
                "patch_hash": artifact.get("patch_hash"),
                "request_binding": request_binding,
                "request_sha256": request_sha256,
                "output": output,
                "output_sha256": output_sha256,
                "claimed_at": existing.get("claimed_at"),
                "completed_at": completed_at,
            });
            validate_target_output_receipt(&artifact, &receipt)?;
            artifact
                .as_object_mut()
                .ok_or_else(|| "supervised patch artifact must be an object".to_string())?
                .insert("target_output_receipt".to_string(), receipt.clone());
            Ok((artifact, receipt))
        };

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let raw: String = tx
                    .query_row(
                        "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = ?1",
                        params![artifact_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let artifact: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                let (artifact, receipt) = update(artifact)?;
                tx.execute(
                    "UPDATE supervised_patch_artifacts SET artifact_json = ?1 WHERE artifact_id = ?2",
                    params![artifact.to_string(), artifact_id],
                )
                .map_err(|error| error.to_string())?;
                append_audit_locked(
                    &tx,
                    &completed_at,
                    actor,
                    "supervised_patch.target_output_success",
                    artifact_id,
                    &json!({"request_sha256": request_sha256, "receipt_state": "completed"}),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(receipt)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx
                    .query_one(
                        "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = $1 FOR UPDATE",
                        &[&artifact_id],
                    )
                    .map_err(|error| error.to_string())?;
                let raw: String = row.get(0);
                let artifact: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                let (artifact, receipt) = update(artifact)?;
                tx
                    .execute(
                        "UPDATE supervised_patch_artifacts SET artifact_json = $1 WHERE artifact_id = $2",
                        &[&artifact.to_string(), &artifact_id],
                    )
                    .map_err(|error| error.to_string())?;
                let details = json!({"request_sha256": request_sha256, "receipt_state": "completed"}).to_string();
                pg_append_audit(
                    &mut tx,
                    &completed_at,
                    actor,
                    "supervised_patch.target_output_success",
                    artifact_id,
                    &details,
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(receipt)
            }),
        }
    }

    pub fn mark_target_output_outcome_unknown(
        &self,
        artifact_id: &str,
        request_binding: &Value,
        request_sha256: &str,
        actor: &str,
        reason: &str,
    ) -> Result<(), String> {
        validate_target_output_request_hash(request_binding, request_sha256)?;
        let now = self.now();
        let update = |mut artifact: Value| -> Result<Value, String> {
            validate_target_output_binding(&artifact, artifact_id, request_binding)?;
            let receipt = artifact
                .get_mut("target_output_receipt")
                .and_then(Value::as_object_mut)
                .ok_or_else(|| "target output receipt is missing".to_string())?;
            if receipt.get("request_sha256").and_then(Value::as_str) != Some(request_sha256)
                || receipt.get("request_binding") != Some(request_binding)
            {
                return Err("target output outcome binding changed".to_string());
            }
            match receipt.get("state").and_then(Value::as_str) {
                Some("sending") => {
                    receipt.insert("state".to_string(), json!("outcome_unknown"));
                    receipt.insert("outcome_unknown_at".to_string(), json!(now));
                }
                Some("outcome_unknown") => {}
                _ => return Err("target output receipt cannot become outcome unknown".to_string()),
            }
            Ok(artifact)
        };
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let raw: String = tx.query_row(
                    "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = ?1",
                    params![artifact_id], |row| row.get(0),
                ).map_err(|error| error.to_string())?;
                let artifact = update(serde_json::from_str(&raw).map_err(|error| error.to_string())?)?;
                tx.execute(
                    "UPDATE supervised_patch_artifacts SET artifact_json = ?1 WHERE artifact_id = ?2",
                    params![artifact.to_string(), artifact_id],
                ).map_err(|error| error.to_string())?;
                append_audit_locked(&tx, &now, actor,
                    "supervised_patch.target_output_outcome_unknown", artifact_id,
                    &json!({"request_sha256": request_sha256, "reason": reason}))?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let row = tx.query_one(
                    "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = $1 FOR UPDATE",
                    &[&artifact_id],
                ).map_err(|error| error.to_string())?;
                let raw: String = row.get(0);
                let artifact = update(serde_json::from_str(&raw).map_err(|error| error.to_string())?)?;
                tx.execute(
                    "UPDATE supervised_patch_artifacts SET artifact_json = $1 WHERE artifact_id = $2",
                    &[&artifact.to_string(), &artifact_id],
                ).map_err(|error| error.to_string())?;
                let details = json!({"request_sha256": request_sha256, "reason": reason}).to_string();
                pg_append_audit(&mut tx, &now, actor,
                    "supervised_patch.target_output_outcome_unknown", artifact_id, &details)?;
                tx.commit().map_err(|error| error.to_string())
            }),
        }
    }

    pub fn claim_product_output_operation(
        &self,
        artifact_id: &str,
        request: &Value,
        request_sha256: &str,
        expected_task_version: u64,
        actor: &str,
    ) -> Result<Value, String> {
        validate_target_output_request_hash(request, request_sha256)?;
        let authority_request = json!({
            "schema_version": "product_output_authority_request.v1",
            "product_task_id": request.get("product_task_id"),
            "artifact_id": request.get("artifact_id"),
            "approval_id": request.get("approval_id"),
            "output_intent": request.get("output_intent"),
            "expected_task_version": expected_task_version,
        });
        self.mutate_product_output_operation(
            artifact_id,
            actor,
            "product_task.output_operation_claimed",
            Some(&authority_request),
            |artifact, now| {
                validate_product_output_operation_request(
                    artifact,
                    artifact_id,
                    request,
                    request_sha256,
                )?;
                if let Some(existing) = artifact.get("product_output_operation") {
                    validate_product_output_operation(artifact, existing)?;
                    if existing.get("request_sha256").and_then(Value::as_str)
                        != Some(request_sha256)
                        || existing.get("request") != Some(request)
                        || existing
                            .get("expected_task_version")
                            .and_then(Value::as_u64)
                            != Some(expected_task_version)
                    {
                        return Err(
                            "product output request does not match durable operation".to_string()
                        );
                    }
                    let mut operation = existing.clone();
                    let branch_status = operation
                        .pointer("/branch_push/status")
                        .and_then(Value::as_str)
                        .unwrap_or("pending");
                    let pr_status = operation
                        .pointer("/pr_create/status")
                        .and_then(Value::as_str)
                        .unwrap_or("pending");
                    let action = if operation.get("state").and_then(Value::as_str)
                        == Some("completed")
                    {
                        "reused"
                    } else if branch_status == "completed" && pr_status == "completed" {
                        "reconciliation_required"
                    } else if branch_status == "completed" {
                        let pr = operation
                            .get("pr_create")
                            .ok_or_else(|| "product output PR phase missing".to_string())?;
                        if pr_status == "in_progress"
                            && product_output_phase_claim_is_current(pr, now)?
                        {
                            "operation_in_progress"
                        } else {
                            claim_product_output_phase(&mut operation, "pr_create", actor, now)?;
                            "create_or_reconcile_pr"
                        }
                    } else if matches!(
                        branch_status,
                        "in_progress" | "outcome_unknown" | "failed_known"
                    ) {
                        let branch = operation
                            .get("branch_push")
                            .ok_or_else(|| "product output branch phase missing".to_string())?;
                        if branch_status == "in_progress"
                            && product_output_phase_claim_is_current(branch, now)?
                        {
                            "operation_in_progress"
                        } else {
                            claim_product_output_phase(&mut operation, "branch_push", actor, now)?;
                            "push_or_reconcile_branch"
                        }
                    } else {
                        "reconciliation_required"
                    };
                    if matches!(
                        action,
                        "create_or_reconcile_pr" | "push_or_reconcile_branch"
                    ) {
                        artifact
                            .as_object_mut()
                            .ok_or_else(|| {
                                "supervised patch artifact must be an object".to_string()
                            })?
                            .insert("product_output_operation".to_string(), operation.clone());
                    }
                    operation
                        .as_object_mut()
                        .ok_or_else(|| "product output operation must be an object".to_string())?
                        .insert("claim_action".to_string(), json!(action));
                    return Ok(operation);
                }
                let operation_id =
                    format!("product-output-{}-{}", artifact_id, &request_sha256[..12]);
                let operation = json!({
                    "schema_version": "product_output_operation.v1",
                    "operation_id": operation_id,
                    "product_task_id": request.get("product_task_id"),
                    "artifact_id": artifact_id,
                    "approval_id": request.get("approval_id"),
                    "expected_task_version": expected_task_version,
                    "request_sha256": request_sha256,
                    "request": request,
                    "target_repository": request.get("target_repository"),
                    "source_revision": artifact.get("source_revision"),
                    "base_branch": request.get("base_branch"),
                    "head_branch": request.get("head_branch"),
                    "state": "active",
                    "attempt": 1,
                    "current_version": 1,
                    "branch_push": {
                        "status": "in_progress",
                        "claimed_at": now,
                        "claimed_by": actor,
                        "commit_sha": Value::Null,
                        "completed_at": Value::Null,
                    },
                    "pr_create": {
                        "status": "pending",
                        "draft": true,
                        "number": Value::Null,
                        "url": Value::Null,
                        "completed_at": Value::Null,
                    },
                    "created_at": now,
                    "updated_at": now,
                    "created_by": actor,
                    "updated_by": actor,
                });
                validate_product_output_operation(artifact, &operation)?;
                artifact
                    .as_object_mut()
                    .ok_or_else(|| "supervised patch artifact must be an object".to_string())?
                    .insert("product_output_operation".to_string(), operation.clone());
                let mut response = operation;
                response
                    .as_object_mut()
                    .expect("operation object")
                    .insert("claim_action".to_string(), json!("push_branch"));
                Ok(response)
            },
        )
    }

    pub fn record_product_nonnetwork_output_receipt(
        &self,
        artifact_id: &str,
        product_task_id: &str,
        approval_id: &str,
        output_intent: &str,
        expected_task_version: u64,
        output: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        if !matches!(
            output_intent,
            "artifact_only" | "export_patch" | "apply_local_changes"
        ) {
            return Err("nonnetwork output receipt intent is invalid".to_string());
        }
        // Create/reuse path keeps strict expected-current authority. A concurrent
        // winner may terminalize the task (version V -> V+1, status completed)
        // between receipt creation and this caller's rebind; that race is recovered
        // by replaying the already-committed canonical receipt only.
        let authority_request = json!({
            "schema_version": "product_output_authority_request.v1",
            "product_task_id": product_task_id,
            "artifact_id": artifact_id,
            "approval_id": approval_id,
            "output_intent": output_intent,
            "expected_task_version": expected_task_version,
        });
        match self.mutate_product_output_operation(
            artifact_id,
            actor,
            "product_task.nonnetwork_output_completed",
            Some(&authority_request),
            |artifact, now| {
                record_or_reuse_nonnetwork_output_receipt(
                    artifact,
                    product_task_id,
                    artifact_id,
                    approval_id,
                    output_intent,
                    expected_task_version,
                    output,
                    actor,
                    now,
                    /*allow_create=*/ true,
                )
            },
        ) {
            Ok(receipt) => Ok(receipt),
            Err(error) if is_product_output_authority_race_error(&error) => self
                .reuse_completed_nonnetwork_output_receipt(
                    artifact_id,
                    product_task_id,
                    approval_id,
                    output_intent,
                    expected_task_version,
                    output,
                )
                .map_err(|reuse_error| {
                    if reuse_error.contains("canonical completed output missing") {
                        error
                    } else {
                        reuse_error
                    }
                }),
            Err(error) => Err(error),
        }
    }

    /// Replay a non-network receipt after another caller already won terminal CAS.
    ///
    /// Authority remains current-state: only completed@expected+1 with a matching
    /// durable receipt may be reconstructed. Conflicting identities stay fail-closed.
    fn reuse_completed_nonnetwork_output_receipt(
        &self,
        artifact_id: &str,
        product_task_id: &str,
        approval_id: &str,
        output_intent: &str,
        expected_task_version: u64,
        output: &Value,
    ) -> Result<Value, String> {
        let authority_request = json!({
            "schema_version": "product_output_authority_request.v1",
            "product_task_id": product_task_id,
            "artifact_id": artifact_id,
            "approval_id": approval_id,
            "output_intent": output_intent,
            "expected_task_version": expected_task_version,
            "allow_completed_idempotent": true,
        });
        self.mutate_product_output_operation(
            artifact_id,
            "canonical-replay",
            "product_task.nonnetwork_output_completed",
            Some(&authority_request),
            |artifact, now| {
                // Create is forbidden on the completed-idempotent recovery path.
                // If the winner committed terminal evidence without a receipt the
                // store is corrupt; refuse rather than invent a second effect.
                record_or_reuse_nonnetwork_output_receipt(
                    artifact,
                    product_task_id,
                    artifact_id,
                    approval_id,
                    output_intent,
                    expected_task_version,
                    output,
                    "canonical-replay",
                    now,
                    /*allow_create=*/ false,
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn complete_product_task_output_authorized(
        &self,
        product_task_id: &str,
        artifact_id: &str,
        approval_id: &str,
        output_intent: &str,
        expected_task_version: u64,
        terminal_evidence: &Value,
        actor: &str,
    ) -> Result<(Value, Value), String> {
        let authority_request = json!({
            "schema_version": "product_output_authority_request.v1",
            "product_task_id": product_task_id,
            "artifact_id": artifact_id,
            "approval_id": approval_id,
            "output_intent": output_intent,
            "expected_task_version": expected_task_version,
            "allow_completed_idempotent": true,
        });
        let now = self.now();
        let stored_evidence = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let (task, approval) =
                    validate_product_output_request_authority_sqlite(&tx, &authority_request)?;
                let raw: String = tx
                    .query_row(
                        "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = ?1",
                        params![artifact_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let artifact: Value =
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                validate_product_output_approval_artifact(
                    &artifact,
                    &authority_request,
                    &approval,
                )?;
                validate_terminal_product_output_record(
                    &artifact,
                    &authority_request,
                    output_intent,
                )?;
                let next_version = expected_task_version.saturating_add(1);
                if task.get("status").and_then(Value::as_str) == Some("completed") {
                    let raw: String = tx
                        .query_row(
                            "SELECT evidence_json FROM product_task_terminal_evidence
                             WHERE product_task_id = ?1 AND task_version = ?2",
                            params![product_task_id, next_version as i64],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    let evidence: Value =
                        serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                    validate_product_terminal_evidence_content_hash(&evidence)?;
                    validate_product_terminal_evidence_candidate(
                        &evidence,
                        &task,
                        &artifact,
                        &approval,
                        next_version,
                    )?;
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(evidence);
                }
                validate_terminal_product_task_status(&task, output_intent)?;
                validate_product_terminal_evidence_candidate(
                    terminal_evidence,
                    &task,
                    &artifact,
                    &approval,
                    next_version,
                )?;
                let updated = tx
                    .execute(
                        "UPDATE product_tasks SET status = 'completed', version = ?1,
                                updated_at = ?2, failure_code = NULL, failure_detail = NULL
                         WHERE task_id = ?3 AND version = ?4 AND status = ?5",
                        params![
                            next_version as i64,
                            now,
                            product_task_id,
                            expected_task_version as i64,
                            required_str(&task, "status")?,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err(
                        "product output terminal expected-current update conflict".to_string()
                    );
                }
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "product_task.transition",
                    product_task_id,
                    &json!({
                        "from": task.get("status"),
                        "to": "completed",
                        "version": next_version,
                        "execution_admitted": false,
                        "failure_code": Value::Null,
                        "artifact_id": artifact_id,
                        "approval_id": approval_id,
                        "output_intent": output_intent,
                        "terminal_authority_revalidated": true,
                    }),
                )?;
                let evidence_id = required_str(terminal_evidence, "evidence_id")?;
                let evidence_audit_id = append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "product_task.terminal_evidence_committed",
                    product_task_id,
                    &json!({
                        "schema_version": "product_task_terminal_evidence_audit.v1",
                        "evidence_id": evidence_id,
                        "task_version": next_version,
                        "artifact_id": artifact_id,
                        "approval_id": approval_id,
                        "output_intent": output_intent,
                    }),
                )?;
                let evidence = finalize_product_terminal_evidence(
                    terminal_evidence,
                    evidence_audit_id,
                    &now,
                    actor,
                )?;
                insert_product_terminal_evidence_sqlite(&tx, &evidence)?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(evidence)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let (task, approval) =
                    pg_validate_product_output_request_authority(&mut tx, &authority_request)?;
                let row = tx
                    .query_one(
                        "SELECT artifact_json FROM supervised_patch_artifacts
                         WHERE artifact_id = $1 FOR UPDATE",
                        &[&artifact_id],
                    )
                    .map_err(|error| error.to_string())?;
                let raw: String = row.get(0);
                let artifact: Value =
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                validate_product_output_approval_artifact(
                    &artifact,
                    &authority_request,
                    &approval,
                )?;
                validate_terminal_product_output_record(
                    &artifact,
                    &authority_request,
                    output_intent,
                )?;
                let next_version = expected_task_version.saturating_add(1);
                if task.get("status").and_then(Value::as_str) == Some("completed") {
                    let row = tx
                        .query_one(
                            "SELECT evidence_json FROM product_task_terminal_evidence
                             WHERE product_task_id = $1 AND task_version = $2",
                            &[&product_task_id, &(next_version as i64)],
                        )
                        .map_err(|error| error.to_string())?;
                    let raw: String = row.get(0);
                    let evidence: Value =
                        serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                    validate_product_terminal_evidence_content_hash(&evidence)?;
                    validate_product_terminal_evidence_candidate(
                        &evidence,
                        &task,
                        &artifact,
                        &approval,
                        next_version,
                    )?;
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(evidence);
                }
                validate_terminal_product_task_status(&task, output_intent)?;
                validate_product_terminal_evidence_candidate(
                    terminal_evidence,
                    &task,
                    &artifact,
                    &approval,
                    next_version,
                )?;
                let current_status = required_str(&task, "status")?;
                let updated = tx
                    .execute(
                        "UPDATE product_tasks SET status = 'completed', version = $1,
                                updated_at = $2, failure_code = NULL, failure_detail = NULL
                         WHERE task_id = $3 AND version = $4 AND status = $5",
                        &[
                            &(next_version as i64),
                            &now,
                            &product_task_id,
                            &(expected_task_version as i64),
                            &current_status,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err(
                        "product output terminal expected-current update conflict".to_string()
                    );
                }
                let details = json!({
                    "from": current_status,
                    "to": "completed",
                    "version": next_version,
                    "execution_admitted": false,
                    "failure_code": Value::Null,
                    "artifact_id": artifact_id,
                    "approval_id": approval_id,
                    "output_intent": output_intent,
                    "terminal_authority_revalidated": true,
                })
                .to_string();
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "product_task.transition",
                    product_task_id,
                    &details,
                )?;
                let evidence_id = required_str(terminal_evidence, "evidence_id")?;
                let evidence_details = json!({
                    "schema_version": "product_task_terminal_evidence_audit.v1",
                    "evidence_id": evidence_id,
                    "task_version": next_version,
                    "artifact_id": artifact_id,
                    "approval_id": approval_id,
                    "output_intent": output_intent,
                })
                .to_string();
                let evidence_audit_id: i64 = tx
                    .query_one(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1,$2,'product_task.terminal_evidence_committed',$3,$4)
                         RETURNING audit_id",
                        &[&now, &actor, &product_task_id, &evidence_details],
                    )
                    .map_err(|error| error.to_string())?
                    .get(0);
                let evidence = finalize_product_terminal_evidence(
                    terminal_evidence,
                    evidence_audit_id,
                    &now,
                    actor,
                )?;
                insert_product_terminal_evidence_pg(&mut tx, &evidence)?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(evidence)
            })?,
        };
        let task = self
            .get_product_task(product_task_id)?
            .ok_or_else(|| "product task missing after authorized completion".to_string())?;
        Ok((task, stored_evidence))
    }

    pub fn record_product_output_branch_pushed(
        &self,
        artifact_id: &str,
        operation_id: &str,
        expected_operation_version: u64,
        commit_sha: &str,
        actor: &str,
    ) -> Result<Value, String> {
        self.mutate_product_output_operation(
            artifact_id,
            actor,
            "product_task.output_branch_pushed",
            None,
            |artifact, now| {
                let operation = artifact
                    .get_mut("product_output_operation")
                    .ok_or_else(|| "product output operation missing".to_string())?;
                if operation.get("operation_id").and_then(Value::as_str) != Some(operation_id) {
                    return Err("product output operation identity mismatch".to_string());
                }
                require_product_output_operation_version(operation, expected_operation_version)?;
                let branch = operation
                    .get_mut("branch_push")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| "product output branch phase missing".to_string())?;
                match branch.get("status").and_then(Value::as_str) {
                    Some("completed")
                        if branch.get("commit_sha").and_then(Value::as_str) == Some(commit_sha) =>
                    {
                        return Ok(operation.clone())
                    }
                    Some("in_progress") => {}
                    _ => return Err("product output branch phase is not finalizable".to_string()),
                }
                branch.insert("status".to_string(), json!("completed"));
                branch.insert("commit_sha".to_string(), json!(commit_sha));
                branch.insert("completed_at".to_string(), json!(now));
                branch.remove("claimed_at");
                branch.remove("claimed_by");
                increment_product_output_operation_version(operation)?;
                operation["updated_at"] = json!(now);
                operation["updated_by"] = json!(actor);
                let snapshot = operation.clone();
                validate_product_output_operation(artifact, &snapshot)?;
                Ok(snapshot)
            },
        )
    }

    pub fn complete_product_output_draft_pr(
        &self,
        artifact_id: &str,
        operation_id: &str,
        expected_operation_version: u64,
        pull_request: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        if pull_request.get("draft").and_then(Value::as_bool) != Some(true)
            || pull_request.get("number").and_then(Value::as_u64).is_none()
            || pull_request.get("url").and_then(Value::as_str).is_none()
            || pull_request
                .get("repository")
                .and_then(Value::as_str)
                .is_none()
            || pull_request
                .get("base_branch")
                .and_then(Value::as_str)
                .is_none()
            || pull_request
                .get("head_branch")
                .and_then(Value::as_str)
                .is_none()
            || pull_request
                .get("head_sha")
                .and_then(Value::as_str)
                .is_none()
        {
            return Err("Draft PR receipt is incomplete".to_string());
        }
        self.mutate_product_output_operation(
            artifact_id,
            actor,
            "product_task.output_draft_pr_completed",
            None,
            |artifact, now| {
                let operation = artifact
                    .get_mut("product_output_operation")
                    .ok_or_else(|| "product output operation missing".to_string())?;
                if operation.get("operation_id").and_then(Value::as_str) != Some(operation_id) {
                    return Err("product output operation identity mismatch".to_string());
                }
                if operation.get("state").and_then(Value::as_str) == Some("completed")
                    && operation.pointer("/pr_create/number") == pull_request.get("number")
                    && operation.pointer("/pr_create/url") == pull_request.get("url")
                    && operation.pointer("/pr_create/repository")
                        == pull_request.get("repository")
                    && operation.pointer("/pr_create/base_branch")
                        == pull_request.get("base_branch")
                    && operation.pointer("/pr_create/head_branch")
                        == pull_request.get("head_branch")
                    && operation.pointer("/pr_create/head_sha") == pull_request.get("head_sha")
                {
                    return Ok(operation.clone());
                }
                require_product_output_operation_version(operation, expected_operation_version)?;
                if operation
                    .pointer("/branch_push/status")
                    .and_then(Value::as_str)
                    != Some("completed")
                {
                    return Err("Draft PR cannot complete before branch push".to_string());
                }
                let target_repository = operation
                    .get("target_repository")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "product output target repository missing".to_string())?;
                let expected_base = operation
                    .get("base_branch")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "product output base branch missing".to_string())?;
                let expected_head = operation
                    .get("head_branch")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "product output head branch missing".to_string())?;
                let expected_head_sha = operation
                    .pointer("/branch_push/commit_sha")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "product output branch commit missing".to_string())?;
                let expected_url_prefix = format!(
                    "https://github.com/{}/pull/",
                    target_repository.trim_end_matches(".git")
                );
                if pull_request
                    .get("url")
                    .and_then(Value::as_str)
                    .is_none_or(|url| !url.starts_with(&expected_url_prefix))
                {
                    return Err(
                        "Draft PR receipt URL does not match the admitted repository".to_string(),
                    );
                }
                if pull_request.get("repository").and_then(Value::as_str)
                    != Some(target_repository.trim_end_matches(".git"))
                    || pull_request.get("base_branch").and_then(Value::as_str)
                        != Some(expected_base)
                    || pull_request.get("head_branch").and_then(Value::as_str)
                        != Some(expected_head)
                    || pull_request.get("head_sha").and_then(Value::as_str)
                        != Some(expected_head_sha)
                {
                    return Err(
                        "Draft PR receipt does not match the durable repository/base/head/commit binding"
                            .to_string(),
                    );
                }
                let pr = operation
                    .get_mut("pr_create")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| "product output PR phase missing".to_string())?;
                if pr.get("status").and_then(Value::as_str) != Some("in_progress") {
                    return Err("product output PR phase is not finalizable".to_string());
                }
                pr.insert("status".to_string(), json!("completed"));
                pr.insert("draft".to_string(), json!(true));
                pr.insert("number".to_string(), pull_request["number"].clone());
                pr.insert("url".to_string(), pull_request["url"].clone());
                pr.insert("reused".to_string(), pull_request["reused"].clone());
                pr.insert(
                    "repository".to_string(),
                    pull_request["repository"].clone(),
                );
                pr.insert(
                    "base_branch".to_string(),
                    pull_request["base_branch"].clone(),
                );
                pr.insert(
                    "head_branch".to_string(),
                    pull_request["head_branch"].clone(),
                );
                pr.insert("head_sha".to_string(), pull_request["head_sha"].clone());
                pr.insert("completed_at".to_string(), json!(now));
                pr.remove("claimed_at");
                pr.remove("claimed_by");
                increment_product_output_operation_version(operation)?;
                operation["state"] = json!("completed");
                operation["updated_at"] = json!(now);
                operation["updated_by"] = json!(actor);
                let snapshot = operation.clone();
                validate_product_output_operation(artifact, &snapshot)?;
                Ok(snapshot)
            },
        )
    }

    pub fn mark_product_output_pr_outcome_unknown(
        &self,
        artifact_id: &str,
        operation_id: &str,
        expected_operation_version: u64,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        self.mutate_product_output_operation(
            artifact_id,
            actor,
            "product_task.output_draft_pr_outcome_unknown",
            None,
            |artifact, now| {
                let operation = artifact
                    .get_mut("product_output_operation")
                    .ok_or_else(|| "product output operation missing".to_string())?;
                if operation.get("operation_id").and_then(Value::as_str) != Some(operation_id) {
                    return Err("product output operation identity mismatch".to_string());
                }
                require_product_output_operation_version(operation, expected_operation_version)?;
                let pr = operation
                    .get_mut("pr_create")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| "product output PR phase missing".to_string())?;
                if pr.get("status").and_then(Value::as_str) != Some("in_progress") {
                    return Err("product output PR phase is not current".to_string());
                }
                pr.insert("status".to_string(), json!("outcome_unknown"));
                pr.insert(
                    "reason".to_string(),
                    json!(redact_sensitive_patterns(reason)),
                );
                pr.insert("updated_at".to_string(), json!(now));
                pr.remove("claimed_at");
                pr.remove("claimed_by");
                increment_product_output_operation_version(operation)?;
                operation["state"] = json!("outcome_unknown");
                operation["updated_at"] = json!(now);
                operation["updated_by"] = json!(actor);
                let snapshot = operation.clone();
                validate_product_output_operation(artifact, &snapshot)?;
                Ok(snapshot)
            },
        )
    }

    pub fn mark_product_output_branch_outcome_unknown(
        &self,
        artifact_id: &str,
        operation_id: &str,
        expected_operation_version: u64,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        self.mutate_product_output_operation(
            artifact_id,
            actor,
            "product_task.output_branch_outcome_unknown",
            None,
            |artifact, now| {
                let operation = artifact
                    .get_mut("product_output_operation")
                    .ok_or_else(|| "product output operation missing".to_string())?;
                if operation.get("operation_id").and_then(Value::as_str) != Some(operation_id) {
                    return Err("product output operation identity mismatch".to_string());
                }
                require_product_output_operation_version(operation, expected_operation_version)?;
                let branch = operation
                    .get_mut("branch_push")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| "product output branch phase missing".to_string())?;
                if branch.get("status").and_then(Value::as_str) != Some("in_progress") {
                    return Err("product output branch phase is not current".to_string());
                }
                branch.insert("status".to_string(), json!("outcome_unknown"));
                branch.insert(
                    "reason".to_string(),
                    json!(redact_sensitive_patterns(reason)),
                );
                branch.insert("updated_at".to_string(), json!(now));
                branch.remove("claimed_at");
                branch.remove("claimed_by");
                increment_product_output_operation_version(operation)?;
                operation["state"] = json!("outcome_unknown");
                operation["updated_at"] = json!(now);
                operation["updated_by"] = json!(actor);
                let snapshot = operation.clone();
                validate_product_output_operation(artifact, &snapshot)?;
                Ok(snapshot)
            },
        )
    }

    pub fn mark_product_output_branch_failed_known(
        &self,
        artifact_id: &str,
        operation_id: &str,
        expected_operation_version: u64,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        self.mutate_product_output_operation(
            artifact_id,
            actor,
            "product_task.output_branch_failed_known",
            None,
            |artifact, now| {
                let operation = artifact
                    .get_mut("product_output_operation")
                    .ok_or_else(|| "product output operation missing".to_string())?;
                if operation.get("operation_id").and_then(Value::as_str) != Some(operation_id) {
                    return Err("product output operation identity mismatch".to_string());
                }
                require_product_output_operation_version(operation, expected_operation_version)?;
                let branch = operation
                    .get_mut("branch_push")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| "product output branch phase missing".to_string())?;
                if branch.get("status").and_then(Value::as_str) != Some("in_progress") {
                    return Err("product output branch phase is not current".to_string());
                }
                branch.insert("status".to_string(), json!("failed_known"));
                branch.insert(
                    "reason".to_string(),
                    json!(redact_sensitive_patterns(reason)),
                );
                branch.insert("updated_at".to_string(), json!(now));
                branch.remove("claimed_at");
                branch.remove("claimed_by");
                increment_product_output_operation_version(operation)?;
                operation["state"] = json!("failed");
                operation["updated_at"] = json!(now);
                operation["updated_by"] = json!(actor);
                let snapshot = operation.clone();
                validate_product_output_operation(artifact, &snapshot)?;
                Ok(snapshot)
            },
        )
    }

    pub fn mark_product_output_pr_failed_known(
        &self,
        artifact_id: &str,
        operation_id: &str,
        expected_operation_version: u64,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        self.mutate_product_output_operation(
            artifact_id,
            actor,
            "product_task.output_draft_pr_failed_known",
            None,
            |artifact, now| {
                let operation = artifact
                    .get_mut("product_output_operation")
                    .ok_or_else(|| "product output operation missing".to_string())?;
                if operation.get("operation_id").and_then(Value::as_str) != Some(operation_id) {
                    return Err("product output operation identity mismatch".to_string());
                }
                require_product_output_operation_version(operation, expected_operation_version)?;
                let pr = operation
                    .get_mut("pr_create")
                    .and_then(Value::as_object_mut)
                    .ok_or_else(|| "product output PR phase missing".to_string())?;
                if pr.get("status").and_then(Value::as_str) != Some("in_progress") {
                    return Err("product output PR phase is not current".to_string());
                }
                pr.insert("status".to_string(), json!("failed_known"));
                pr.insert(
                    "reason".to_string(),
                    json!(redact_sensitive_patterns(reason)),
                );
                pr.insert("updated_at".to_string(), json!(now));
                pr.remove("claimed_at");
                pr.remove("claimed_by");
                increment_product_output_operation_version(operation)?;
                operation["state"] = json!("active");
                operation["updated_at"] = json!(now);
                operation["updated_by"] = json!(actor);
                let snapshot = operation.clone();
                validate_product_output_operation(artifact, &snapshot)?;
                Ok(snapshot)
            },
        )
    }

    fn mutate_product_output_operation<F>(
        &self,
        artifact_id: &str,
        actor: &str,
        audit_action: &str,
        authority_request: Option<&Value>,
        mutate: F,
    ) -> Result<Value, String>
    where
        F: Fn(&mut Value, &str) -> Result<Value, String>,
    {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let approval = authority_request
                    .map(|request| {
                        validate_product_output_request_authority_sqlite(&tx, request)
                    })
                    .transpose()?;
                let raw: String = tx
                    .query_row(
                        "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = ?1",
                        params![artifact_id],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let mut artifact: Value =
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                if let Some((request, (_, approval))) = authority_request.zip(approval.as_ref()) {
                    validate_product_output_approval_artifact(&artifact, request, approval)?;
                }
                let persisted_artifact = artifact.clone();
                let result = mutate(&mut artifact, &now)?;
                if artifact == persisted_artifact {
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(result);
                }
                tx.execute(
                    "UPDATE supervised_patch_artifacts SET artifact_json = ?1 WHERE artifact_id = ?2",
                    params![artifact.to_string(), artifact_id],
                )
                .map_err(|error| error.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    audit_action,
                    artifact_id,
                    &json!({
                        "operation_id": result.get("operation_id"),
                        "product_task_id": result.get("product_task_id"),
                        "state": result.get("state"),
                        "branch_push_status": result.pointer("/branch_push/status"),
                        "pr_create_status": result.pointer("/pr_create/status"),
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(result)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let approval = authority_request
                    .map(|request| pg_validate_product_output_request_authority(&mut tx, request))
                    .transpose()?;
                let row = tx
                    .query_one(
                        "SELECT artifact_json FROM supervised_patch_artifacts WHERE artifact_id = $1 FOR UPDATE",
                        &[&artifact_id],
                    )
                    .map_err(|error| error.to_string())?;
                let raw: String = row.get(0);
                let mut artifact: Value =
                    serde_json::from_str(&raw).map_err(|error| error.to_string())?;
                if let Some((request, (_, approval))) = authority_request.zip(approval.as_ref()) {
                    validate_product_output_approval_artifact(&artifact, request, approval)?;
                }
                let persisted_artifact = artifact.clone();
                let result = mutate(&mut artifact, &now)?;
                if artifact == persisted_artifact {
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(result);
                }
                tx.execute(
                    "UPDATE supervised_patch_artifacts SET artifact_json = $1 WHERE artifact_id = $2",
                    &[&artifact.to_string(), &artifact_id],
                )
                .map_err(|error| error.to_string())?;
                let audit_details = json!({
                    "operation_id": result.get("operation_id"),
                    "product_task_id": result.get("product_task_id"),
                    "state": result.get("state"),
                    "branch_push_status": result.pointer("/branch_push/status"),
                    "pr_create_status": result.pointer("/pr_create/status"),
                })
                .to_string();
                pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    audit_action,
                    artifact_id,
                    &audit_details,
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(result)
            }),
        }
    }

    pub fn supervised_patch_artifacts(&self, limit: i64) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT artifact_sequence, artifact_id, workspace_id, run_id, plan_id,
                                target_id, source_revision, artifact_type, patch_hash,
                                changed_files_json, redaction_status, created_at, artifact_json
                         FROM supervised_patch_artifacts
                         ORDER BY artifact_sequence DESC
                         LIMIT ?1",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![limit], supervised_patch_artifact_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT artifact_sequence, artifact_id, workspace_id, run_id, plan_id,
                                target_id, source_revision, artifact_type, patch_hash,
                                changed_files_json, redaction_status, created_at, artifact_json
                         FROM supervised_patch_artifacts
                         ORDER BY artifact_sequence DESC
                         LIMIT $1",
                        &[&limit],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(rows.iter().map(pg_supervised_patch_artifact_row).collect())
            }),
        }
    }

    pub fn export_supervised_patch_artifacts(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.supervised_patch_artifacts(limit)
    }

    pub fn import_supervised_patch_artifact(&self, artifact: &Value) -> Result<bool, String> {
        ensure_schema_version(
            artifact,
            "supervised patch artifact",
            SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION,
        )?;
        ensure_optional_bool_field(artifact, "metadata_only", true)?;
        ensure_optional_string_field(artifact, "execution_authority", "disabled")?;
        ensure_optional_string_field(artifact, "patch_apply_authority", "disabled")?;
        ensure_optional_bool_field(artifact, "artifact_file_created", false)?;
        let artifact_id = required_str(artifact, "artifact_id")?;
        if self.get_supervised_patch_artifact(artifact_id)?.is_some() {
            return Ok(false);
        }
        let workspace_id = required_str(artifact, "workspace_id")?;
        if self.get_supervised_patch_workspace(workspace_id)?.is_none() {
            return Err(format!(
                "supervised patch workspace not found: {workspace_id}"
            ));
        }
        let run_id = required_str(artifact, "run_id")?;
        let plan_id = optional_str(artifact, "plan_id");
        let target_id = required_str(artifact, "target_id")?;
        let source_revision = required_str(artifact, "source_revision")?;
        let artifact_type = optional_str(artifact, "artifact_type").unwrap_or("patch_diff");
        if artifact_type != "patch_diff" {
            return Err(format!(
                "invalid supervised patch artifact type: {artifact_type}"
            ));
        }
        let patch_hash = required_str(artifact, "patch_hash")?;
        let changed_files = normalize_changed_files(artifact.get("changed_files"))?;
        let redaction_status = optional_str(artifact, "redaction_status").unwrap_or("pending");
        if !matches!(redaction_status, "pending" | "redacted" | "failed") {
            return Err(format!(
                "invalid supervised patch artifact redaction_status: {redaction_status}"
            ));
        }

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence =
                    next_sequence(conn, "supervised_patch_artifacts", "artifact_sequence")?;
                let created_at = optional_str(artifact, "created_at")
                    .map(str::to_string)
                    .unwrap_or_else(|| self.now());
                let artifact_record = build_import_artifact_record(
                    artifact,
                    artifact_id,
                    sequence,
                    workspace_id,
                    run_id,
                    plan_id,
                    target_id,
                    source_revision,
                    artifact_type,
                    patch_hash,
                    &changed_files,
                    redaction_status,
                    &created_at,
                )?;
                conn.execute(
                    "INSERT INTO supervised_patch_artifacts
                     (artifact_sequence, artifact_id, workspace_id, run_id, plan_id, target_id,
                      source_revision, artifact_type, patch_hash, changed_files_json,
                      redaction_status, created_at, artifact_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        sequence,
                        artifact_id,
                        workspace_id,
                        run_id,
                        plan_id,
                        target_id,
                        source_revision,
                        artifact_type,
                        patch_hash,
                        changed_files.to_string(),
                        redaction_status,
                        created_at,
                        artifact_record.to_string(),
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &self.now(),
                    "import",
                    "supervised_patch.artifact_import",
                    artifact_id,
                    &json!({"workspace_id": workspace_id, "metadata_only": true}),
                )?;
                Ok(true)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence =
                    pg_next_sequence(client, "supervised_patch_artifacts", "artifact_sequence")?;
                let created_at = optional_str(artifact, "created_at")
                    .map(str::to_string)
                    .unwrap_or_else(|| self.now());
                let artifact_record = build_import_artifact_record(
                    artifact,
                    artifact_id,
                    sequence,
                    workspace_id,
                    run_id,
                    plan_id,
                    target_id,
                    source_revision,
                    artifact_type,
                    patch_hash,
                    &changed_files,
                    redaction_status,
                    &created_at,
                )?;
                client
                    .execute(
                        "INSERT INTO supervised_patch_artifacts
                     (artifact_sequence, artifact_id, workspace_id, run_id, plan_id, target_id,
                      source_revision, artifact_type, patch_hash, changed_files_json,
                      redaction_status, created_at, artifact_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
                        &[
                            &sequence,
                            &artifact_id,
                            &workspace_id,
                            &run_id,
                            &plan_id,
                            &target_id,
                            &source_revision,
                            &artifact_type,
                            &patch_hash,
                            &changed_files.to_string(),
                            &redaction_status,
                            &created_at,
                            &artifact_record.to_string(),
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let audit_details =
                    json!({"workspace_id": workspace_id, "metadata_only": true}).to_string();
                pg_append_audit(
                    client,
                    &self.now(),
                    "import",
                    "supervised_patch.artifact_import",
                    artifact_id,
                    &audit_details,
                )?;
                Ok(true)
            }),
        }
    }
}

fn supervised_patch_workspace_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let boundary_text: String = row.get(14)?;
    let workspace_text: String = row.get(15)?;
    let boundary: Value = serde_json::from_str(&boundary_text).unwrap_or(Value::Null);
    let mut workspace: Value = serde_json::from_str(&workspace_text).unwrap_or(Value::Null);
    if let Some(object) = workspace.as_object_mut() {
        object.insert(
            "workspace_sequence".to_string(),
            json!(row.get::<_, i64>(0)?),
        );
        object.insert("workspace_id".to_string(), json!(row.get::<_, String>(1)?));
        object.insert("plan_id".to_string(), optional_row_string(row, 2)?);
        object.insert("run_id".to_string(), json!(row.get::<_, String>(3)?));
        object.insert("target_id".to_string(), json!(row.get::<_, String>(4)?));
        object.insert(
            "target_repo_path".to_string(),
            json!(row.get::<_, String>(5)?),
        );
        object.insert(
            "target_repo_canonical_path".to_string(),
            json!(row.get::<_, String>(6)?),
        );
        object.insert(
            "workspace_path".to_string(),
            json!(row.get::<_, String>(7)?),
        );
        object.insert(
            "workspace_canonical_path".to_string(),
            json!(row.get::<_, String>(8)?),
        );
        object.insert(
            "source_revision".to_string(),
            json!(row.get::<_, String>(9)?),
        );
        object.insert(
            "source_tree_hash".to_string(),
            optional_row_string(row, 10)?,
        );
        object.insert("status".to_string(), json!(row.get::<_, String>(11)?));
        object.insert("created_at".to_string(), json!(row.get::<_, String>(12)?));
        object.insert("updated_at".to_string(), json!(row.get::<_, String>(13)?));
        object.insert("boundary".to_string(), boundary);
        object.insert("metadata_only".to_string(), json!(true));
        object.insert("execution_authority".to_string(), json!("disabled"));
    }
    Ok(workspace)
}

fn supervised_patch_artifact_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let changed_files_text: String = row.get(9)?;
    let artifact_text: String = row.get(12)?;
    let changed_files: Value = serde_json::from_str(&changed_files_text).unwrap_or(Value::Null);
    let mut artifact: Value = serde_json::from_str(&artifact_text).unwrap_or(Value::Null);
    if let Some(object) = artifact.as_object_mut() {
        object.insert(
            "artifact_sequence".to_string(),
            json!(row.get::<_, i64>(0)?),
        );
        object.insert("artifact_id".to_string(), json!(row.get::<_, String>(1)?));
        object.insert("workspace_id".to_string(), json!(row.get::<_, String>(2)?));
        object.insert("run_id".to_string(), json!(row.get::<_, String>(3)?));
        object.insert("plan_id".to_string(), optional_row_string(row, 4)?);
        object.insert("target_id".to_string(), json!(row.get::<_, String>(5)?));
        object.insert(
            "source_revision".to_string(),
            json!(row.get::<_, String>(6)?),
        );
        object.insert("artifact_type".to_string(), json!(row.get::<_, String>(7)?));
        object.insert("patch_hash".to_string(), json!(row.get::<_, String>(8)?));
        object.insert("changed_files".to_string(), changed_files);
        object.insert(
            "redaction_status".to_string(),
            json!(row.get::<_, String>(10)?),
        );
        object.insert("created_at".to_string(), json!(row.get::<_, String>(11)?));
        object.insert("metadata_only".to_string(), json!(true));
        object.insert("execution_authority".to_string(), json!("disabled"));
        object.insert("patch_apply_authority".to_string(), json!("disabled"));
    }
    Ok(artifact)
}

fn supervised_patch_boundary(
    target_repo_path: &str,
    workspace_path: &str,
    workspace_mode: &str,
) -> Result<Value, String> {
    let target_repo_canonical = canonicalize_existing_path(target_repo_path, "target_repo_path")?;
    let workspace_canonical = canonicalize_planned_path(workspace_path, "workspace_path")?;
    if workspace_canonical.starts_with(&target_repo_canonical) {
        return Err(format!(
            "workspace_path must be outside registered target repository: {}",
            workspace_canonical.display()
        ));
    }
    Ok(json!({
        "metadata_only": true,
        "execution_authority": "disabled",
        "verification_execution_authority": "allowlisted_commands",
        "workspace_directory_creation": if workspace_mode == "git_worktree" {
            "git_worktree"
        } else if Path::new(workspace_path).exists() {
            "app_owned_copy"
        } else {
            "not_performed"
        },
        "workspace_confinement": "app_owned_directory",
        "workspace_root_policy": "canonical_app_store_root",
        "symlink_policy": "skip",
        "copy_resource_limits": {
            "max_files": MAX_WORKSPACE_COPY_FILES,
            "max_total_bytes": MAX_WORKSPACE_COPY_BYTES,
            "max_file_bytes": MAX_WORKSPACE_COPY_FILE_BYTES,
        },
        "secret_scan": "required_before_artifact",
        "kill_switch": "quarantine_or_cleanup",
        "target_repository_writes": if workspace_mode == "git_worktree" { "approval_bound_branch_only" } else { "disabled" },
        "registered_git_worktree": if workspace_mode == "git_worktree" { "controlled" } else { "forbidden" },
        "git_worktree_add": if workspace_mode == "git_worktree" { "performed" } else { "forbidden" },
        "process_execution": "allowlisted_verification_only",
        "provider_calls": "disabled",
        "push_merge_deploy_apply": if workspace_mode == "git_worktree" { "approval_bound_push_only" } else { "disabled" },
        "target_repo_canonical_path": path_string(&target_repo_canonical),
        "workspace_canonical_path": path_string(&workspace_canonical),
    }))
}

fn workspace_safety_profile(workspace_mode: &str) -> Value {
    json!({
        "workspace_confinement": "app_owned_directory",
        "workspace_root_policy": "canonical_app_store_root",
        "symlink_policy": "skip",
        "copy_resource_limits": {
            "max_files": MAX_WORKSPACE_COPY_FILES,
            "max_total_bytes": MAX_WORKSPACE_COPY_BYTES,
            "max_file_bytes": MAX_WORKSPACE_COPY_FILE_BYTES,
        },
        "secret_scan": "required_before_artifact",
        "verification_execution": "allowlisted_commands",
        "kill_switch": "quarantine_or_cleanup",
        "target_repository_writes": if workspace_mode == "git_worktree" { "approval_bound_branch_only" } else { "disabled" },
        "branch_policy": if workspace_mode == "git_worktree" { "acp_prefix_only_no_main" } else { "not_applicable" },
    })
}

fn artifact_safety_profile() -> Value {
    json!({
        "secret_scan": "pending",
        "review_diff": "pending",
        "target_repository_writes": "disabled",
        "patch_apply_authority": "disabled",
    })
}

fn validate_workspace_id(workspace_id: &str) -> Result<(), String> {
    let valid = !workspace_id.trim().is_empty()
        && workspace_id.len() <= 128
        && workspace_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err("workspace_id must contain only ASCII letters, digits, '-' or '_'".to_string())
    }
}

fn validate_review_diff_redaction(redaction_status: &str, review_diff: &str) -> Result<(), String> {
    if let Some(pattern) = secret_pattern(&review_diff.to_lowercase()) {
        return Err(format!(
            "review_diff contains sensitive pattern and must be suppressed: {pattern}"
        ));
    }
    if redaction_status == "failed" && !review_diff.contains("suppressed") {
        return Err("review_diff must be suppressed when redaction_status is failed".to_string());
    }
    Ok(())
}

fn canonicalize_existing_path(value: &str, field: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    ensure_absolute_clean_path(path, field)?;
    std::fs::canonicalize(path).map_err(|e| format!("{field} must exist: {e}"))
}

fn canonicalize_planned_path(value: &str, field: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    ensure_absolute_clean_path(path, field)?;
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|e| format!("{field} invalid: {e}"));
    }

    let mut suffix: Vec<OsString> = Vec::new();
    let mut cursor = path;
    while !cursor.exists() {
        let file_name = cursor
            .file_name()
            .ok_or_else(|| format!("{field} has no existing parent"))?;
        suffix.push(file_name.to_os_string());
        cursor = cursor
            .parent()
            .ok_or_else(|| format!("{field} has no existing parent"))?;
    }
    let mut canonical = std::fs::canonicalize(cursor)
        .map_err(|e| format!("{field} existing parent invalid: {e}"))?;
    for part in suffix.iter().rev() {
        canonical.push(part);
    }
    Ok(canonical)
}

fn ensure_absolute_clean_path(path: &Path, field: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("{field} must be absolute"));
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            return Err(format!("{field} must not contain . or .. components"));
        }
    }
    Ok(())
}

fn normalize_changed_files(value: Option<&Value>) -> Result<Value, String> {
    let files = value
        .and_then(Value::as_array)
        .ok_or_else(|| "changed_files must be an array".to_string())?;
    if files.is_empty() {
        return Err("changed_files must not be empty".to_string());
    }
    let mut normalized = Vec::new();
    for file in files {
        let path = file
            .as_str()
            .ok_or_else(|| "changed_files entries must be strings".to_string())?;
        if path.trim().is_empty() {
            return Err("changed_files entries must not be empty".to_string());
        }
        if path.contains('\\') {
            return Err(format!("changed file must use forward slashes: {path}"));
        }
        let path_ref = Path::new(path);
        if path_ref.is_absolute() {
            return Err(format!("changed file must be relative: {path}"));
        }
        for component in path_ref.components() {
            if !matches!(component, Component::Normal(_)) {
                return Err(format!("changed file must be normalized: {path}"));
            }
        }
        normalized.push(json!(path));
    }
    Ok(Value::Array(normalized))
}

fn is_valid_workspace_status(status: &str) -> bool {
    matches!(
        status,
        "requested"
            | "source_recorded"
            | "workspace_created"
            | "patch_prepared"
            | "review_blocked"
            | "approved_for_artifact_export"
            | "rejected"
            | "quarantined"
            | "cleaned"
    )
}

fn is_valid_workspace_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("requested", "source_recorded" | "rejected" | "quarantined")
            | (
                "source_recorded",
                "workspace_created" | "rejected" | "quarantined"
            )
            | (
                "workspace_created",
                "patch_prepared" | "rejected" | "quarantined" | "cleaned"
            )
            | (
                "patch_prepared",
                "review_blocked"
                    | "approved_for_artifact_export"
                    | "rejected"
                    | "quarantined"
                    | "cleaned"
            )
            | (
                "review_blocked",
                "approved_for_artifact_export" | "rejected" | "quarantined"
            )
            | (
                "approved_for_artifact_export",
                "rejected" | "quarantined" | "cleaned"
            )
            | ("rejected", "quarantined" | "cleaned")
            | ("quarantined", "cleaned")
    )
}

fn validate_imported_workspace_boundary(
    target_repo_path: &str,
    target_repo_canonical_path: &str,
    workspace_path: &str,
    workspace_canonical_path: &str,
    workspace_mode: &str,
    boundary: &Value,
) -> Result<(), String> {
    ensure_absolute_clean_path(Path::new(target_repo_path), "target_repo_path")?;
    ensure_absolute_clean_path(
        Path::new(target_repo_canonical_path),
        "target_repo_canonical_path",
    )?;
    ensure_absolute_clean_path(Path::new(workspace_path), "workspace_path")?;
    ensure_absolute_clean_path(
        Path::new(workspace_canonical_path),
        "workspace_canonical_path",
    )?;
    if Path::new(workspace_canonical_path).starts_with(Path::new(target_repo_canonical_path)) {
        return Err(format!(
            "workspace_path must be outside registered target repository: {workspace_canonical_path}"
        ));
    }
    ensure_optional_bool_field(boundary, "metadata_only", true)?;
    ensure_optional_string_field(boundary, "execution_authority", "disabled")?;
    if let Some(actual) = boundary.get("workspace_directory_creation") {
        let value = actual.as_str().unwrap_or("");
        if !matches!(value, "not_performed" | "app_owned_copy" | "git_worktree") {
            return Err(
                "workspace_directory_creation must be not_performed, app_owned_copy, or git_worktree"
                    .to_string(),
            );
        }
    }
    let (target_writes, registered_worktree, worktree_add, push_policy) =
        if workspace_mode == "git_worktree" {
            (
                "approval_bound_branch_only",
                "controlled",
                "performed",
                "approval_bound_push_only",
            )
        } else {
            ("disabled", "forbidden", "forbidden", "disabled")
        };
    ensure_optional_string_field(boundary, "target_repository_writes", target_writes)?;
    ensure_optional_string_field(boundary, "registered_git_worktree", registered_worktree)?;
    ensure_optional_string_field(boundary, "git_worktree_add", worktree_add)?;
    if let Some(actual) = boundary.get("process_execution") {
        let value = actual.as_str().unwrap_or("");
        if !matches!(value, "disabled" | "allowlisted_verification_only") {
            return Err(
                "process_execution must be disabled or allowlisted_verification_only".to_string(),
            );
        }
    }
    ensure_optional_string_field(boundary, "provider_calls", "disabled")?;
    ensure_optional_string_field(boundary, "push_merge_deploy_apply", push_policy)
}

fn ensure_schema_version(value: &Value, label: &str, expected: &str) -> Result<(), String> {
    let schema_version = optional_str(value, "schema_version").unwrap_or(expected);
    if schema_version != expected {
        return Err(format!(
            "unsupported {label} schema_version: {schema_version} (expected {expected})"
        ));
    }
    Ok(())
}

fn ensure_optional_string_field(value: &Value, field: &str, expected: &str) -> Result<(), String> {
    if let Some(actual) = value.get(field) {
        if actual.as_str() != Some(expected) {
            return Err(format!("{field} must be {expected}"));
        }
    }
    Ok(())
}

fn ensure_optional_bool_field(value: &Value, field: &str, expected: bool) -> Result<(), String> {
    if let Some(actual) = value.get(field) {
        if actual.as_bool() != Some(expected) {
            return Err(format!("{field} must be {expected}"));
        }
    }
    Ok(())
}

fn managed_supervised_patch_identity(
    workspace_id: &str,
    operation: &str,
    attempt: u64,
) -> Result<String, String> {
    let encoded = serde_json::to_vec(&json!({
        "schema_version": "managed_supervised_patch_identity.v1",
        "workspace_id": workspace_id,
        "operation": operation,
        "attempt": attempt,
    }))
    .map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn validate_managed_supervised_patch_metadata(
    workspace_id: &str,
    operation: &str,
    attempt: u64,
    binding_sha256: &str,
    node_metadata: &Value,
) -> Result<(), String> {
    let binding = node_metadata
        .get("managed_supervised_patch")
        .and_then(Value::as_object)
        .ok_or_else(|| "managed supervised-patch metadata is missing its binding".to_string())?;
    if binding.get("schema_version").and_then(Value::as_str) != Some("managed_supervised_patch.v1")
        || binding.get("workspace_id").and_then(Value::as_str) != Some(workspace_id)
        || binding.get("operation").and_then(Value::as_str) != Some(operation)
        || binding.get("attempt").and_then(Value::as_u64) != Some(attempt)
        || binding.get("binding_sha256").and_then(Value::as_str) != Some(binding_sha256)
    {
        return Err("managed supervised-patch metadata binding changed".to_string());
    }
    Ok(())
}

fn validate_existing_managed_supervised_patch_node(
    existing_node: &str,
    workspace_id: &str,
    operation: &str,
    attempt: u64,
    binding_sha256: &str,
    requested_metadata: &Value,
) -> Result<(), String> {
    let existing_node: Value = serde_json::from_str(existing_node)
        .map_err(|_| "managed supervised-patch canonical node is corrupt".to_string())?;
    validate_managed_supervised_patch_metadata(
        workspace_id,
        operation,
        attempt,
        binding_sha256,
        &existing_node,
    )
    .map_err(|_| {
        "managed supervised-patch binding changed for canonical workspace operation attempt"
            .to_string()
    })?;
    let requested = requested_metadata.as_object().ok_or_else(|| {
        "managed supervised-patch requested metadata must be an object".to_string()
    })?;
    if requested
        .iter()
        .any(|(field, value)| existing_node.get(field) != Some(value))
    {
        return Err(
            "managed supervised-patch binding changed for canonical workspace operation attempt"
                .to_string(),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_managed_supervised_patch_run(
    sequence: i64,
    run_id: &str,
    workflow_id: &str,
    dispatch_id: &str,
    node_id: &str,
    operation: &str,
    node_metadata: &Value,
    created_at: &str,
) -> Result<(Value, Value, Value, Value), String> {
    let mut node = node_metadata.clone();
    let object = node
        .as_object_mut()
        .ok_or_else(|| "managed supervised-patch node metadata must be an object".to_string())?;
    object.insert("schema_version".to_string(), json!("workflow_node.v1"));
    object.insert("node_id".to_string(), json!(node_id));
    object.insert("workflow_id".to_string(), json!(workflow_id));
    object.insert(
        "task_type".to_string(),
        json!(format!("workspace_{operation}")),
    );
    object.insert("status".to_string(), json!("pending"));
    let graph = json!({
        "schema_version": "workflow_graph.v1",
        "workflow_id": workflow_id,
        "dispatch_id": dispatch_id,
        "status": "decomposed",
        "created_at": created_at,
        "updated_at": created_at,
        "nodes": [node.clone()],
        "edges": [],
    });
    let boundaries = json!({
        "execution_authority": "bounded_trusted_local",
        "scheduler_authority": "rust_engine",
        "approval_authority": "workflow_run_approvals",
    });
    let run = json!({
        "schema_version": "workflow_run.v1",
        "run_sequence": sequence,
        "run_id": run_id,
        "plan_id": null,
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
    Ok((node, graph, boundaries, run))
}

struct PreparedProductArtifact {
    workspace_id: String,
    run_id: String,
    plan_id: Option<String>,
    target_id: String,
    source_revision: String,
    patch_hash: String,
    changed_files: Value,
    review_diff: String,
    verification: Value,
    redaction_status: String,
    secret_scan_status: String,
    secret_findings: Value,
    added: Value,
    modified: Value,
    deleted: Value,
}

impl PreparedProductArtifact {
    fn artifact(&self, sequence: i64, artifact_id: &str, created_at: &str) -> Value {
        json!({
            "schema_version": SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION,
            "artifact_sequence": sequence,
            "artifact_id": artifact_id,
            "workspace_id": self.workspace_id,
            "run_id": self.run_id,
            "plan_id": self.plan_id,
            "target_id": self.target_id,
            "source_revision": self.source_revision,
            "artifact_type": "patch_diff",
            "patch_hash": self.patch_hash,
            "changed_files": self.changed_files,
            "redaction_status": self.redaction_status,
            "secret_scan_status": self.secret_scan_status,
            "review_diff": self.review_diff,
            "storage_refs": {},
            "evidence_bundle": {
                "schema_version": "target_repo_evidence.v1",
                "run_id": self.run_id,
                "source_revision": self.source_revision,
                "patch_hash": self.patch_hash,
                "changed_files": self.changed_files,
                "verification": self.verification,
                "secret_scan_status": self.secret_scan_status,
                "redaction_status": self.redaction_status,
            },
            "safety": {
                "workspace_confinement": "app_owned_directory",
                "secret_scan": self.secret_scan_status,
                "review_diff": if self.secret_scan_status == "passed" { "generated" } else { "suppressed" },
                "target_repository_writes": "approval_bound_branch_only",
            },
            "retention_expires_at": null,
            "created_at": created_at,
            "metadata_only": true,
            "execution_authority": "disabled",
            "patch_apply_authority": "disabled",
            "artifact_file_created": false,
            "secret_findings": self.secret_findings,
            "added": self.added,
            "modified": self.modified,
            "deleted": self.deleted,
        })
    }
}

fn validate_product_artifact_allowed_paths(
    artifact: &PreparedProductArtifact,
    intake_json: &str,
) -> Result<(), String> {
    let intake: Value = serde_json::from_str(intake_json)
        .map_err(|_| "product task intake is corrupt during artifact capture".to_string())?;
    let allowed_paths = intake
        .get("allowed_paths")
        .and_then(Value::as_array)
        .filter(|paths| !paths.is_empty())
        .ok_or_else(|| {
            "product task allowed_paths are missing during artifact capture".to_string()
        })?
        .iter()
        .map(|path| {
            let path = path
                .as_str()
                .filter(|path| !path.is_empty())
                .ok_or_else(|| {
                    "product task allowed_paths are invalid during artifact capture".to_string()
                })?;
            normalized_product_artifact_path(path, "allowed_paths")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let changed_files = artifact
        .changed_files
        .as_array()
        .ok_or_else(|| "product artifact changed_files are invalid".to_string())?;
    for changed_file in changed_files {
        let changed_file = changed_file
            .as_str()
            .ok_or_else(|| "product artifact changed_files are invalid".to_string())?;
        let marker = changed_file
            .chars()
            .next()
            .filter(|marker| matches!(marker, '+' | '~' | '-'))
            .ok_or_else(|| "product artifact changed_files are invalid".to_string())?;
        let path = changed_file
            .get(marker.len_utf8()..)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| "product artifact changed_files are invalid".to_string())?;
        let changed_components = normalized_product_artifact_path(path, "changed_files")?;
        let admitted = allowed_paths
            .iter()
            .any(|allowed| changed_components.starts_with(allowed));
        if !admitted {
            return Err(format!(
                "product artifact path is outside product task allowed_paths: {path}"
            ));
        }
    }
    Ok(())
}

fn normalized_product_artifact_path(path: &str, field: &str) -> Result<Vec<String>, String> {
    let mut components = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => components.push(
                part.to_str()
                    .filter(|part| !part.is_empty())
                    .ok_or_else(|| {
                        format!("product task {field} path is invalid during artifact capture")
                    })?
                    .to_string(),
            ),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "product task {field} path is invalid during artifact capture"
                ));
            }
        }
    }
    if components.is_empty() {
        return Err(format!(
            "product task {field} path is invalid during artifact capture"
        ));
    }
    Ok(components)
}

fn prepare_product_artifact_fields(
    workspace: &Value,
    workspace_id: &str,
    expected_patch_hash: &str,
) -> Result<PreparedProductArtifact, String> {
    if workspace.get("workspace_id").and_then(Value::as_str) != Some(workspace_id)
        || !matches!(
            workspace.get("workspace_mode").and_then(Value::as_str),
            Some("git_worktree" | "copy")
        )
        || matches!(
            workspace.get("status").and_then(Value::as_str),
            Some("rejected" | "quarantined" | "cleaned") | None
        )
    {
        return Err("product artifact workspace binding is invalid".to_string());
    }
    let workspace_path = required_str(workspace, "workspace_path")?;
    let path = Path::new(workspace_path);
    if workspace.get("workspace_mode").and_then(Value::as_str) == Some("copy") {
        let source_path = required_str(workspace, "target_repo_path")?;
        let exclusions = crate::local_folder_source::configured_local_folder_exclusions()?;
        let source_manifest = crate::local_folder_source::capture_local_folder_manifest(
            Path::new(source_path),
            &exclusions,
        )?;
        if source_manifest.tree_sha256 != required_str(workspace, "source_revision")? {
            return Err("local-folder source revision changed before artifact capture".to_string());
        }
        let summary = crate::local_folder_source::summarize_local_folder_changes(
            &source_manifest,
            path,
            &exclusions,
        )?;
        if summary.changed_relative_paths.is_empty() {
            return Err("no changes detected against source revision".to_string());
        }
        if summary.change_sha256 != expected_patch_hash {
            return Err(
                "verified patch identity changed before atomic artifact commit".to_string(),
            );
        }
        let secret_findings = scan_for_secrets(path)?;
        if !secret_findings.is_empty() {
            return Err("local-folder artifact secret scan failed".to_string());
        }
        let confirmed = crate::local_folder_source::summarize_local_folder_changes(
            &source_manifest,
            path,
            &exclusions,
        )?;
        if confirmed != summary {
            return Err(
                "verified patch identity changed during atomic artifact commit".to_string(),
            );
        }
        let verification = workspace
            .get("verification")
            .cloned()
            .ok_or_else(|| "product artifact workspace verification is missing".to_string())?;
        if verification.get("status").and_then(Value::as_str) != Some("evidence_recorded")
            || verification.get("trustworthy").and_then(Value::as_bool) != Some(true)
        {
            return Err("product artifact workspace verification is not trustworthy".to_string());
        }
        let review_diff = truncate_text(
            format!(
                "local-folder change bundle: added={} modified={} deleted={}",
                summary.added_relative_paths.len(),
                summary.modified_relative_paths.len(),
                summary.deleted_relative_paths.len()
            ),
            MAX_REVIEW_DIFF_BYTES,
        );
        let changed_files = summary
            .added_relative_paths
            .iter()
            .map(|path| format!("+{path}"))
            .chain(
                summary
                    .modified_relative_paths
                    .iter()
                    .map(|path| format!("~{path}")),
            )
            .chain(
                summary
                    .deleted_relative_paths
                    .iter()
                    .map(|path| format!("-{path}")),
            )
            .collect::<Vec<_>>();
        return Ok(PreparedProductArtifact {
            workspace_id: workspace_id.to_string(),
            run_id: required_str(workspace, "run_id")?.to_string(),
            plan_id: optional_str(workspace, "plan_id").map(str::to_string),
            target_id: required_str(workspace, "target_id")?.to_string(),
            source_revision: required_str(workspace, "source_revision")?.to_string(),
            patch_hash: summary.change_sha256,
            changed_files: json!(changed_files),
            review_diff,
            verification,
            redaction_status: "redacted".to_string(),
            secret_scan_status: "passed".to_string(),
            secret_findings: json!([]),
            added: json!(summary.added_relative_paths),
            modified: json!(summary.modified_relative_paths),
            deleted: json!(summary.deleted_relative_paths),
        });
    }
    let config = TargetRepoOutputConfig::from_env();
    let changes = staged_changed_files(&config, path)?;
    if changes.changed_files.is_empty() {
        return Err("no changes detected against source revision".to_string());
    }
    let patch = inspect_git_patch(&config, path)?;
    let patch_hash = target_patch_hash(&patch);
    if patch_hash != expected_patch_hash {
        return Err("verified patch identity changed before atomic artifact commit".to_string());
    }
    let secret_findings = scan_for_secrets(path)?;
    let secret_scan_status = if secret_findings.is_empty() {
        "passed"
    } else {
        "blocked"
    };
    let redaction_status = if secret_findings.is_empty() {
        "redacted"
    } else {
        "failed"
    };
    let review_diff = if secret_findings.is_empty() {
        truncate_text(patch, MAX_REVIEW_DIFF_BYTES)
    } else {
        "review diff suppressed: secret scan failed".to_string()
    };

    // A second add/diff while the task/workspace rows remain locked catches writes that
    // race with secret scanning or review generation. Only this confirmed identity commits.
    let confirmed_changes = staged_changed_files(&config, path)?;
    let confirmed_patch = inspect_git_patch(&config, path)?;
    if target_patch_hash(&confirmed_patch) != patch_hash
        || confirmed_changes.changed_files != changes.changed_files
    {
        return Err("verified patch identity changed during atomic artifact commit".to_string());
    }
    let verification = workspace
        .get("verification")
        .cloned()
        .ok_or_else(|| "product artifact workspace verification is missing".to_string())?;
    if verification.get("status").and_then(Value::as_str) != Some("evidence_recorded")
        || verification.get("trustworthy").and_then(Value::as_bool) != Some(true)
    {
        return Err("product artifact workspace verification is not trustworthy".to_string());
    }
    Ok(PreparedProductArtifact {
        workspace_id: workspace_id.to_string(),
        run_id: required_str(workspace, "run_id")?.to_string(),
        plan_id: optional_str(workspace, "plan_id").map(str::to_string),
        target_id: required_str(workspace, "target_id")?.to_string(),
        source_revision: required_str(workspace, "source_revision")?.to_string(),
        patch_hash,
        changed_files: json!(changes.changed_files),
        review_diff,
        verification,
        redaction_status: redaction_status.to_string(),
        secret_scan_status: secret_scan_status.to_string(),
        secret_findings: json!(secret_findings),
        added: json!(changes.added),
        modified: json!(changes.modified),
        deleted: json!(changes.deleted),
    })
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{field} is required"))
}

fn optional_str<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

fn optional_row_string(row: &Row<'_>, index: usize) -> rusqlite::Result<Value> {
    let value: Option<String> = row.get(index)?;
    Ok(value.map(Value::String).unwrap_or(Value::Null))
}

fn next_sequence(conn: &rusqlite::Connection, table: &str, column: &str) -> Result<i64, String> {
    let sql = format!("SELECT COALESCE(MAX({column}), 0) + 1 FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
        .map_err(|e| e.to_string())
}

fn build_import_workspace_record(
    workspace: &Value,
    workspace_id: &str,
    sequence: i64,
    plan_id: Option<&str>,
    run_id: &str,
    target_id: &str,
    target_repo_path: &str,
    target_repo_canonical_path: &str,
    workspace_path: &str,
    workspace_canonical_path: &str,
    source_revision: &str,
    source_tree_hash: Option<&str>,
    status: &str,
    boundary: &Value,
    created_at: &str,
    updated_at: &str,
) -> Result<Value, String> {
    let mut workspace_record = workspace.clone();
    let object = workspace_record
        .as_object_mut()
        .ok_or_else(|| "supervised patch workspace must be an object".to_string())?;
    object.insert(
        "schema_version".to_string(),
        json!(SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION),
    );
    object.insert("workspace_sequence".to_string(), json!(sequence));
    object.insert("workspace_id".to_string(), json!(workspace_id));
    object.insert(
        "plan_id".to_string(),
        plan_id.map(Value::from).unwrap_or(Value::Null),
    );
    object.insert("run_id".to_string(), json!(run_id));
    object.insert("target_id".to_string(), json!(target_id));
    object.insert("target_repo_path".to_string(), json!(target_repo_path));
    object.insert(
        "target_repo_canonical_path".to_string(),
        json!(target_repo_canonical_path),
    );
    object.insert("workspace_path".to_string(), json!(workspace_path));
    object.insert(
        "workspace_canonical_path".to_string(),
        json!(workspace_canonical_path),
    );
    object.insert("source_revision".to_string(), json!(source_revision));
    object.insert(
        "source_tree_hash".to_string(),
        source_tree_hash.map(Value::from).unwrap_or(Value::Null),
    );
    object.insert("status".to_string(), json!(status));
    object.insert("created_at".to_string(), json!(created_at));
    object.insert("updated_at".to_string(), json!(updated_at));
    object.insert("boundary".to_string(), boundary.clone());
    object.insert("metadata_only".to_string(), json!(true));
    object.insert("execution_authority".to_string(), json!("disabled"));
    let workspace_mode = optional_str(workspace, "workspace_mode").unwrap_or("copy");
    object.insert("workspace_mode".to_string(), json!(workspace_mode));
    object.insert(
        "safety".to_string(),
        workspace_safety_profile(workspace_mode),
    );
    Ok(workspace_record)
}

fn build_import_artifact_record(
    artifact: &Value,
    artifact_id: &str,
    sequence: i64,
    workspace_id: &str,
    run_id: &str,
    plan_id: Option<&str>,
    target_id: &str,
    source_revision: &str,
    artifact_type: &str,
    patch_hash: &str,
    changed_files: &Value,
    redaction_status: &str,
    created_at: &str,
) -> Result<Value, String> {
    let mut artifact_record = artifact.clone();
    let object = artifact_record
        .as_object_mut()
        .ok_or_else(|| "supervised patch artifact must be an object".to_string())?;
    object.insert(
        "schema_version".to_string(),
        json!(SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION),
    );
    object.insert("artifact_sequence".to_string(), json!(sequence));
    object.insert("artifact_id".to_string(), json!(artifact_id));
    object.insert("workspace_id".to_string(), json!(workspace_id));
    object.insert("run_id".to_string(), json!(run_id));
    object.insert(
        "plan_id".to_string(),
        plan_id.map(Value::from).unwrap_or(Value::Null),
    );
    object.insert("target_id".to_string(), json!(target_id));
    object.insert("source_revision".to_string(), json!(source_revision));
    object.insert("artifact_type".to_string(), json!(artifact_type));
    object.insert("patch_hash".to_string(), json!(patch_hash));
    object.insert("changed_files".to_string(), changed_files.clone());
    object.insert("redaction_status".to_string(), json!(redaction_status));
    object.insert("created_at".to_string(), json!(created_at));
    object.insert("metadata_only".to_string(), json!(true));
    object.insert("execution_authority".to_string(), json!("disabled"));
    object.insert("patch_apply_authority".to_string(), json!("disabled"));
    object.insert("artifact_file_created".to_string(), json!(false));
    if !object.contains_key("secret_scan_status") {
        object.insert("secret_scan_status".to_string(), json!("pending"));
    }
    if !object.contains_key("safety") {
        object.insert("safety".to_string(), artifact_safety_profile());
    }
    Ok(artifact_record)
}

fn target_output_json_sha256(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn validate_product_terminal_evidence_candidate(
    evidence: &Value,
    task: &Value,
    artifact: &Value,
    approval: &Value,
    terminal_task_version: u64,
) -> Result<(), String> {
    for (valid, field) in [
        (
            evidence.get("schema_version").and_then(Value::as_str)
                == Some("product_task_terminal_evidence.v2"),
            "schema_version",
        ),
        (
            evidence.get("task_status").and_then(Value::as_str) == Some("completed"),
            "task_status",
        ),
        (
            evidence.get("task_version").and_then(Value::as_u64) == Some(terminal_task_version),
            "task_version",
        ),
        (
            evidence.get("creation_version").and_then(Value::as_u64) == Some(terminal_task_version),
            "creation_version",
        ),
        (
            evidence.get("product_task_id") == task.get("task_id"),
            "product_task_id",
        ),
        (
            evidence.get("tenant_id") == task.get("tenant_id"),
            "tenant_id",
        ),
        (
            evidence.get("workspace_scope_id") == task.get("workspace_id"),
            "workspace_scope_id",
        ),
        (
            evidence.get("intake_contract_sha256") == task.get("intake_contract_sha256"),
            "intake_contract_sha256",
        ),
        (evidence.get("plan_id") == task.get("plan_id"), "plan_id"),
        (evidence.get("run_id") == task.get("run_id"), "run_id"),
        (
            evidence.get("workspace_record_id") == task.get("workspace_record_id"),
            "workspace_record_id",
        ),
        (
            evidence.get("source_revision") == task.get("source_revision"),
            "source_revision",
        ),
        (
            evidence.pointer("/node/node_id") == approval.get("node_id"),
            "node_id",
        ),
        (
            evidence.pointer("/verification/verification_sha256")
                == approval.get("verification_sha256"),
            "verification_sha256",
        ),
        (
            evidence.pointer("/artifact/artifact_id") == artifact.get("artifact_id"),
            "artifact_id",
        ),
        (
            evidence.pointer("/artifact/patch_hash") == artifact.get("patch_hash"),
            "patch_hash",
        ),
        (
            evidence.pointer("/approval/approval_id") == approval.get("approval_id"),
            "approval_id",
        ),
        (
            evidence.pointer("/approval/approved_by") == approval.get("approved_by"),
            "approved_by",
        ),
        (
            evidence.pointer("/output/intent") == task.get("output_intent"),
            "output_intent",
        ),
    ] {
        if !valid {
            return Err(format!(
                "terminal evidence candidate {field} binding mismatch"
            ));
        }
    }
    let evidence_id = required_str(evidence, "evidence_id")?;
    let task_id = required_str(task, "task_id")?;
    if evidence
        .pointer("/approval/approval_sha256")
        .and_then(Value::as_str)
        != Some(target_output_json_sha256(approval)?.as_str())
    {
        return Err("terminal evidence approval content hash mismatch".to_string());
    }
    let output_result_sha256 = evidence
        .pointer("/output/result_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "terminal evidence output result hash missing".to_string())?;
    if output_result_sha256.len() != 64
        || evidence_id
            != format!(
                "product-terminal-{task_id}-{terminal_task_version}-{}",
                &output_result_sha256[..12]
            )
    {
        return Err("terminal evidence deterministic identity mismatch".to_string());
    }
    match required_str(task, "output_intent")? {
        "artifact_only" | "export_patch" | "apply_local_changes" => {
            let receipt = artifact
                .get("product_output_receipt")
                .ok_or_else(|| "terminal evidence output receipt missing".to_string())?;
            if evidence.pointer("/output/receipt_id") != receipt.get("receipt_id")
                || evidence
                    .pointer("/output/operation_id")
                    .is_some_and(|value| !value.is_null())
                || receipt.pointer("/output_sha256").and_then(Value::as_str)
                    != Some(output_result_sha256)
            {
                return Err("terminal evidence receipt binding mismatch".to_string());
            }
        }
        "draft_pr" => {
            let operation = artifact
                .get("product_output_operation")
                .ok_or_else(|| "terminal evidence output operation missing".to_string())?;
            let expected_output =
                super::product_tasks::product_draft_pr_output_from_operation(task_id, operation);
            let expected_output_sha256 = target_output_json_sha256(&expected_output)?;
            if evidence.pointer("/output/operation_id") != operation.get("operation_id")
                || evidence
                    .pointer("/output/receipt_id")
                    .is_some_and(|value| !value.is_null())
                || output_result_sha256 != expected_output_sha256
                || operation.get("state").and_then(Value::as_str) != Some("completed")
                || operation
                    .pointer("/branch_push/status")
                    .and_then(Value::as_str)
                    != Some("completed")
                || operation
                    .pointer("/pr_create/status")
                    .and_then(Value::as_str)
                    != Some("completed")
                || evidence.pointer("/output/branch") != operation.get("head_branch")
                || evidence.pointer("/output/pushed_commit")
                    != operation.pointer("/branch_push/commit_sha")
                || evidence.pointer("/output/draft_pr/number")
                    != operation.pointer("/pr_create/number")
                || evidence.pointer("/output/draft_pr/url") != operation.pointer("/pr_create/url")
                || evidence.pointer("/output/draft_pr/repository")
                    != operation.pointer("/pr_create/repository")
                || evidence.pointer("/output/draft_pr/base_branch")
                    != operation.pointer("/pr_create/base_branch")
                || evidence.pointer("/output/draft_pr/head_branch")
                    != operation.pointer("/pr_create/head_branch")
                || evidence.pointer("/output/draft_pr/head_sha")
                    != operation.pointer("/pr_create/head_sha")
                || evidence
                    .pointer("/output/draft_pr/draft")
                    .and_then(Value::as_bool)
                    != Some(true)
            {
                return Err("terminal evidence Draft PR operation binding mismatch".to_string());
            }
        }
        _ => return Err("terminal evidence output intent is invalid".to_string()),
    }
    Ok(())
}

fn finalize_product_terminal_evidence(
    candidate: &Value,
    audit_id: i64,
    now: &str,
    actor: &str,
) -> Result<Value, String> {
    let mut evidence = candidate.clone();
    let object = evidence
        .as_object_mut()
        .ok_or_else(|| "terminal evidence candidate must be an object".to_string())?;
    object.insert(
        "audit_reference".to_string(),
        json!({
            "audit_id": audit_id,
            "action": "product_task.terminal_evidence_committed",
        }),
    );
    object.insert("created_at".to_string(), json!(now));
    object.insert("created_by".to_string(), json!(actor));
    object.insert("content_sha256".to_string(), Value::Null);
    let content_sha256 = target_output_json_sha256(&evidence)?;
    evidence
        .as_object_mut()
        .ok_or_else(|| "terminal evidence candidate stopped being an object".to_string())?
        .insert("content_sha256".to_string(), json!(content_sha256));
    Ok(evidence)
}

fn insert_product_terminal_evidence_sqlite(
    conn: &rusqlite::Connection,
    evidence: &Value,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO product_task_terminal_evidence
         (evidence_id, product_task_id, tenant_id, workspace_id, task_version,
          output_result_sha256, artifact_id, approval_id, output_operation_id,
          output_receipt_id, audit_id, content_sha256, evidence_json, created_at, created_by)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            required_str(evidence, "evidence_id")?,
            required_str(evidence, "product_task_id")?,
            required_str(evidence, "tenant_id")?,
            required_str(evidence, "workspace_scope_id")?,
            evidence
                .get("task_version")
                .and_then(Value::as_u64)
                .ok_or_else(|| "terminal evidence task version missing".to_string())?
                as i64,
            evidence
                .pointer("/output/result_sha256")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal evidence output hash missing".to_string())?,
            evidence
                .pointer("/artifact/artifact_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal evidence artifact missing".to_string())?,
            evidence
                .pointer("/approval/approval_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "terminal evidence approval missing".to_string())?,
            evidence
                .pointer("/output/operation_id")
                .and_then(Value::as_str),
            evidence
                .pointer("/output/receipt_id")
                .and_then(Value::as_str),
            evidence
                .pointer("/audit_reference/audit_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| "terminal evidence audit reference missing".to_string())?,
            required_str(evidence, "content_sha256")?,
            evidence.to_string(),
            required_str(evidence, "created_at")?,
            required_str(evidence, "created_by")?,
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn insert_product_terminal_evidence_pg(
    client: &mut impl postgres::GenericClient,
    evidence: &Value,
) -> Result<(), String> {
    let task_version = evidence
        .get("task_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "terminal evidence task version missing".to_string())?
        as i64;
    let audit_id = evidence
        .pointer("/audit_reference/audit_id")
        .and_then(Value::as_i64)
        .ok_or_else(|| "terminal evidence audit reference missing".to_string())?;
    let evidence_json = evidence.to_string();
    client
        .execute(
            "INSERT INTO product_task_terminal_evidence
             (evidence_id, product_task_id, tenant_id, workspace_id, task_version,
              output_result_sha256, artifact_id, approval_id, output_operation_id,
              output_receipt_id, audit_id, content_sha256, evidence_json, created_at, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
            &[
                &required_str(evidence, "evidence_id")?,
                &required_str(evidence, "product_task_id")?,
                &required_str(evidence, "tenant_id")?,
                &required_str(evidence, "workspace_scope_id")?,
                &task_version,
                &evidence
                    .pointer("/output/result_sha256")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "terminal evidence output hash missing".to_string())?,
                &evidence
                    .pointer("/artifact/artifact_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "terminal evidence artifact missing".to_string())?,
                &evidence
                    .pointer("/approval/approval_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "terminal evidence approval missing".to_string())?,
                &evidence
                    .pointer("/output/operation_id")
                    .and_then(Value::as_str),
                &evidence
                    .pointer("/output/receipt_id")
                    .and_then(Value::as_str),
                &audit_id,
                &required_str(evidence, "content_sha256")?,
                &evidence_json,
                &required_str(evidence, "created_at")?,
                &required_str(evidence, "created_by")?,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn validate_product_output_request_authority_sqlite(
    conn: &rusqlite::Connection,
    request: &Value,
) -> Result<(Value, Value), String> {
    let task_id = required_str(request, "product_task_id")?;
    let approval_id = required_str(request, "approval_id")?;
    let output_intent = required_str(request, "output_intent")?;
    let task = conn
        .query_row(
            "SELECT status, version, run_id, workspace_record_id, source_revision, output_intent,
                    target_id, target_repo_path, tenant_id, workspace_id, plan_id,
                    intake_contract_sha256
             FROM product_tasks WHERE task_id = ?1",
            params![task_id],
            |row| {
                Ok(json!({
                    "task_id": task_id,
                    "status": row.get::<_, String>(0)?,
                    "version": row.get::<_, i64>(1)?,
                    "run_id": row.get::<_, Option<String>>(2)?,
                    "workspace_record_id": row.get::<_, Option<String>>(3)?,
                    "source_revision": row.get::<_, String>(4)?,
                    "output_intent": row.get::<_, String>(5)?,
                    "target_id": row.get::<_, String>(6)?,
                    "target_repo_path": row.get::<_, String>(7)?,
                    "tenant_id": row.get::<_, String>(8)?,
                    "workspace_id": row.get::<_, String>(9)?,
                    "plan_id": row.get::<_, Option<String>>(10)?,
                    "intake_contract_sha256": row.get::<_, String>(11)?,
                }))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "product output task authority missing".to_string())?;
    let run_id = required_str(&task, "run_id")?;
    let workspace_id = required_str(&task, "workspace_record_id")?;
    validate_product_output_request_task(request, &task, output_intent)?;
    let approval_raw: String = conn
        .query_row(
            "SELECT approval_json FROM workflow_run_approvals
             WHERE approval_id = ?1 AND run_id = ?2",
            params![approval_id, run_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "product output approval authority missing".to_string())?;
    let approval: Value = serde_json::from_str(&approval_raw).map_err(|error| error.to_string())?;
    validate_product_output_request_approval(request, &task, &approval)?;
    let workspace_raw: String = conn
        .query_row(
            "SELECT workspace_json FROM supervised_patch_workspaces
             WHERE workspace_id = ?1 AND run_id = ?2 AND source_revision = ?3
               AND status NOT IN ('quarantined', 'cleaned', 'rejected')",
            params![
                workspace_id,
                run_id,
                required_str(&task, "source_revision")?
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "product output workspace authority missing or inactive".to_string())?;
    validate_product_output_request_verification(&workspace_raw, &approval)?;
    Ok((task, approval))
}

#[cfg(feature = "pg")]
fn pg_validate_product_output_request_authority(
    client: &mut impl postgres::GenericClient,
    request: &Value,
) -> Result<(Value, Value), String> {
    let task_id = required_str(request, "product_task_id")?;
    let approval_id = required_str(request, "approval_id")?;
    let output_intent = required_str(request, "output_intent")?;
    let row = client
        .query_opt(
            "SELECT status, version, run_id, workspace_record_id, source_revision, output_intent,
                    target_id, target_repo_path, tenant_id, workspace_id, plan_id,
                    intake_contract_sha256
             FROM product_tasks WHERE task_id = $1 FOR UPDATE",
            &[&task_id],
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "product output task authority missing".to_string())?;
    let task = json!({
        "task_id": task_id,
        "status": row.get::<_, String>(0),
        "version": row.get::<_, i64>(1),
        "run_id": row.get::<_, Option<String>>(2),
        "workspace_record_id": row.get::<_, Option<String>>(3),
        "source_revision": row.get::<_, String>(4),
        "output_intent": row.get::<_, String>(5),
        "target_id": row.get::<_, String>(6),
        "target_repo_path": row.get::<_, String>(7),
        "tenant_id": row.get::<_, String>(8),
        "workspace_id": row.get::<_, String>(9),
        "plan_id": row.get::<_, Option<String>>(10),
        "intake_contract_sha256": row.get::<_, String>(11),
    });
    let run_id = required_str(&task, "run_id")?;
    let workspace_id = required_str(&task, "workspace_record_id")?;
    validate_product_output_request_task(request, &task, output_intent)?;
    let approval_row = client
        .query_opt(
            "SELECT approval_json FROM workflow_run_approvals
             WHERE approval_id = $1 AND run_id = $2 FOR UPDATE",
            &[&approval_id, &run_id],
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "product output approval authority missing".to_string())?;
    let approval_raw: String = approval_row.get(0);
    let approval: Value = serde_json::from_str(&approval_raw).map_err(|error| error.to_string())?;
    validate_product_output_request_approval(request, &task, &approval)?;
    let source_revision = required_str(&task, "source_revision")?;
    let workspace_row = client
        .query_opt(
            "SELECT workspace_json FROM supervised_patch_workspaces
             WHERE workspace_id = $1 AND run_id = $2 AND source_revision = $3
               AND status NOT IN ('quarantined', 'cleaned', 'rejected') FOR UPDATE",
            &[&workspace_id, &run_id, &source_revision],
        )
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "product output workspace authority missing or inactive".to_string())?;
    let workspace_raw: String = workspace_row.get(0);
    validate_product_output_request_verification(&workspace_raw, &approval)?;
    Ok((task, approval))
}

fn is_product_output_authority_race_error(error: &str) -> bool {
    error.contains("stale product task version at output authority boundary")
        || error.contains("stale product task version at output")
        || error.contains("product output terminal expected-current update conflict")
        || error.contains("product task expected-current update conflict")
        || error.contains("expected-current")
}

fn record_or_reuse_nonnetwork_output_receipt(
    artifact: &mut Value,
    product_task_id: &str,
    artifact_id: &str,
    approval_id: &str,
    output_intent: &str,
    expected_task_version: u64,
    output: &Value,
    actor: &str,
    now: &str,
    allow_create: bool,
) -> Result<Value, String> {
    if output.get("product_task_id").and_then(Value::as_str) != Some(product_task_id)
        || output.get("artifact_id").and_then(Value::as_str) != Some(artifact_id)
    {
        return Err("nonnetwork output result identity changed".to_string());
    }
    if output_intent == "artifact_only" {
        if output.get("status").and_then(Value::as_str) != Some("artifact_only")
            || output.get("target_mutation").and_then(Value::as_bool) != Some(false)
        {
            return Err("artifact-only output result is not non-mutating".to_string());
        }
    } else if output_intent == "apply_local_changes" {
        if output.get("status").and_then(Value::as_str) != Some("applied_local_changes")
            || output.get("patch_hash") != artifact.get("patch_hash")
            || output
                .get("rollback_bundle_present")
                .and_then(Value::as_bool)
                != Some(true)
        {
            return Err("local apply output result does not match approved artifact".to_string());
        }
    } else if output.get("status").and_then(Value::as_str) != Some("exported")
        || output.get("patch_hash") != artifact.get("patch_hash")
    {
        return Err("export output result does not match approved artifact".to_string());
    }
    let request = json!({
        "schema_version": "product_nonnetwork_output_request.v1",
        "product_task_id": product_task_id,
        "artifact_id": artifact_id,
        "approval_id": approval_id,
        "output_intent": output_intent,
        "expected_task_version": expected_task_version,
        "source_revision": artifact.get("source_revision"),
        "patch_hash": artifact.get("patch_hash"),
    });
    let request_sha256 = target_output_json_sha256(&request)?;
    let output_sha256 = target_output_json_sha256(output)?;
    if let Some(existing) = artifact.get("product_output_receipt") {
        // Canonical receipt identity is the request binding. Output payload may be
        // reconstructed identically by concurrent callers; require exact match so a
        // different export identity cannot silently overwrite or reuse another effect.
        if existing.get("request") == Some(&request)
            && existing.get("request_sha256").and_then(Value::as_str)
                == Some(request_sha256.as_str())
            && existing.get("output") == Some(output)
            && existing.get("output_sha256").and_then(Value::as_str) == Some(output_sha256.as_str())
            && existing.get("state").and_then(Value::as_str) == Some("completed")
        {
            return Ok(existing.clone());
        }
        return Err("nonnetwork output receipt already exists with another binding".to_string());
    }
    if !allow_create {
        return Err("canonical completed output missing matching nonnetwork receipt".to_string());
    }
    let receipt = json!({
        "schema_version": "product_output_receipt.v1",
        "receipt_id": format!("product-output-receipt-{artifact_id}-{}", &request_sha256[..12]),
        "state": "completed",
        "product_task_id": product_task_id,
        "artifact_id": artifact_id,
        "approval_id": approval_id,
        "output_intent": output_intent,
        "expected_task_version": expected_task_version,
        "source_revision": artifact.get("source_revision"),
        "patch_hash": artifact.get("patch_hash"),
        "request": request,
        "request_sha256": request_sha256,
        "output": output,
        "output_sha256": output_sha256,
        "created_at": now,
        "created_by": actor,
    });
    artifact
        .as_object_mut()
        .ok_or_else(|| "supervised patch artifact must be an object".to_string())?
        .insert("product_output_receipt".to_string(), receipt.clone());
    Ok(receipt)
}

fn validate_product_output_request_task(
    request: &Value,
    task: &Value,
    output_intent: &str,
) -> Result<(), String> {
    let expected_task_version = request
        .get("expected_task_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "product output expected task version missing".to_string())?;
    if !matches!(
        task.get("status").and_then(Value::as_str),
        Some("awaiting_approval" | "output_pending" | "outcome_unknown" | "completed")
    ) || task.get("output_intent").and_then(Value::as_str) != Some(output_intent)
    {
        return Err("product output task state or intent authority changed".to_string());
    }
    let task_status = task.get("status").and_then(Value::as_str);
    let task_version = task.get("version").and_then(Value::as_u64);
    let completed_idempotent = request
        .get("allow_completed_idempotent")
        .and_then(Value::as_bool)
        == Some(true)
        && task_status == Some("completed")
        && task_version == Some(expected_task_version.saturating_add(1));
    if task_version != Some(expected_task_version) && !completed_idempotent {
        return Err("stale product task version at output authority boundary".to_string());
    }
    for field in [
        "run_id",
        "workspace_record_id",
        "source_revision",
        "target_id",
    ] {
        if request.get(field).is_some() && request.get(field) != task.get(field) {
            return Err(format!("product output request {field} authority changed"));
        }
    }
    if request.get("workspace_id").is_some()
        && request.get("workspace_id") != task.get("workspace_record_id")
    {
        return Err("product output request workspace authority changed".to_string());
    }
    Ok(())
}

fn validate_product_output_request_approval(
    request: &Value,
    task: &Value,
    approval: &Value,
) -> Result<(), String> {
    if approval.get("schema_version").and_then(Value::as_str) != Some("product_output_approval.v1")
        || approval.get("decision").and_then(Value::as_str) != Some("approved")
        || approval.get("approval_kind").and_then(Value::as_str) != Some("product_output")
        || approval.get("output_authority").and_then(Value::as_str) != Some("product_output")
        || approval.get("execution_authority").and_then(Value::as_str) != Some("disabled")
        || approval.get("product_task_id") != request.get("product_task_id")
        || approval.get("approval_id") != request.get("approval_id")
        || approval.get("artifact_id") != request.get("artifact_id")
        || approval.get("output_intent") != request.get("output_intent")
        || approval.get("run_id") != task.get("run_id")
        || approval.get("workspace_record_id") != task.get("workspace_record_id")
        || approval.get("source_revision") != task.get("source_revision")
        || approval
            .get("expected_task_version")
            .and_then(Value::as_i64)
            .is_none_or(|approved| {
                task.get("version")
                    .and_then(Value::as_i64)
                    .is_none_or(|current| approved > current)
            })
    {
        return Err("product output approval authority binding changed".to_string());
    }
    let target = approval
        .get("output_target")
        .ok_or_else(|| "product output approval target missing".to_string())?;
    if target.get("target_id") != task.get("target_id")
        || target.get("target_repo_path") != task.get("target_repo_path")
    {
        return Err("product output approval target authority changed".to_string());
    }
    Ok(())
}

fn validate_product_output_request_verification(
    workspace_raw: &str,
    approval: &Value,
) -> Result<(), String> {
    let workspace: Value =
        serde_json::from_str(workspace_raw).map_err(|error| error.to_string())?;
    let verification = workspace
        .get("verification")
        .ok_or_else(|| "product output verification authority missing".to_string())?;
    if target_output_json_sha256(verification)? != required_str(approval, "verification_sha256")? {
        return Err("product output verification authority changed".to_string());
    }
    Ok(())
}

fn validate_product_output_approval_artifact(
    artifact: &Value,
    request: &Value,
    approval: &Value,
) -> Result<(), String> {
    if artifact.get("artifact_id") != request.get("artifact_id")
        || artifact.get("artifact_id") != approval.get("artifact_id")
        || artifact.get("run_id") != approval.get("run_id")
        || artifact.get("workspace_id") != approval.get("workspace_record_id")
        || artifact.get("source_revision") != approval.get("source_revision")
        || artifact.get("patch_hash") != approval.get("patch_hash")
        || artifact.get("changed_files") != approval.get("changed_files")
    {
        return Err("product output approval artifact authority changed".to_string());
    }
    Ok(())
}

fn validate_terminal_product_task_status(task: &Value, output_intent: &str) -> Result<(), String> {
    let status = required_str(task, "status")?;
    let valid = if output_intent == "draft_pr" {
        matches!(status, "output_pending" | "outcome_unknown")
    } else {
        matches!(
            status,
            "awaiting_approval" | "output_pending" | "outcome_unknown"
        )
    };
    if !valid {
        return Err(format!(
            "product output terminal authority rejects task status {status}"
        ));
    }
    Ok(())
}

fn validate_terminal_product_output_record(
    artifact: &Value,
    authority_request: &Value,
    output_intent: &str,
) -> Result<(), String> {
    if output_intent == "draft_pr" {
        let operation = artifact
            .get("product_output_operation")
            .ok_or_else(|| "completed Draft PR operation missing at terminal CAS".to_string())?;
        validate_product_output_operation(artifact, operation)?;
        if operation.get("state").and_then(Value::as_str) != Some("completed")
            || operation.get("product_task_id") != authority_request.get("product_task_id")
            || operation.get("artifact_id") != authority_request.get("artifact_id")
            || operation.get("approval_id") != authority_request.get("approval_id")
            || operation.get("expected_task_version")
                != authority_request.get("expected_task_version")
            || operation
                .pointer("/request/output_intent")
                .and_then(Value::as_str)
                != Some("draft_pr")
            || operation.pointer("/request/expected_task_version")
                != authority_request.get("expected_task_version")
            || operation
                .pointer("/branch_push/status")
                .and_then(Value::as_str)
                != Some("completed")
            || operation
                .pointer("/pr_create/status")
                .and_then(Value::as_str)
                != Some("completed")
            || operation
                .pointer("/pr_create/draft")
                .and_then(Value::as_bool)
                != Some(true)
        {
            return Err("completed Draft PR operation is stale at terminal CAS".to_string());
        }
        return Ok(());
    }

    let receipt = artifact
        .get("product_output_receipt")
        .ok_or_else(|| "completed nonnetwork output receipt missing at terminal CAS".to_string())?;
    if receipt.get("schema_version").and_then(Value::as_str) != Some("product_output_receipt.v1")
        || receipt.get("state").and_then(Value::as_str) != Some("completed")
        || receipt.get("product_task_id") != authority_request.get("product_task_id")
        || receipt.get("artifact_id") != authority_request.get("artifact_id")
        || receipt.get("approval_id") != authority_request.get("approval_id")
        || receipt.get("output_intent").and_then(Value::as_str) != Some(output_intent)
        || receipt.get("expected_task_version") != authority_request.get("expected_task_version")
        || receipt.get("source_revision") != artifact.get("source_revision")
        || receipt.get("patch_hash") != artifact.get("patch_hash")
    {
        return Err("completed nonnetwork output receipt is stale at terminal CAS".to_string());
    }
    let request = receipt
        .get("request")
        .ok_or_else(|| "nonnetwork output request missing at terminal CAS".to_string())?;
    let output = receipt
        .get("output")
        .ok_or_else(|| "nonnetwork output result missing at terminal CAS".to_string())?;
    if request.get("expected_task_version") != authority_request.get("expected_task_version")
        || target_output_json_sha256(request)? != required_str(receipt, "request_sha256")?
        || target_output_json_sha256(output)? != required_str(receipt, "output_sha256")?
    {
        return Err("nonnetwork output receipt hash changed at terminal CAS".to_string());
    }
    let output_status = output.get("status").and_then(Value::as_str);
    if (output_intent == "artifact_only" && output_status != Some("artifact_only"))
        || (output_intent == "export_patch" && output_status != Some("exported"))
    {
        return Err("nonnetwork output receipt status changed at terminal CAS".to_string());
    }
    Ok(())
}

fn product_output_phase_claim_is_current(phase: &Value, now: &str) -> Result<bool, String> {
    let claimed_at = phase
        .get("claimed_at")
        .and_then(Value::as_str)
        .ok_or_else(|| "in-progress product output phase is missing claimed_at".to_string())?;
    let claimed = chrono::DateTime::parse_from_rfc3339(claimed_at)
        .map_err(|_| "product output phase claimed_at is invalid".to_string())?;
    let current = chrono::DateTime::parse_from_rfc3339(now)
        .map_err(|_| "product output store time is invalid".to_string())?;
    let age = current.signed_duration_since(claimed).num_seconds();
    if age < 0 {
        return Err("product output phase claim is from the future".to_string());
    }
    Ok(age < PRODUCT_OUTPUT_PHASE_LEASE_SECONDS)
}

fn claim_product_output_phase(
    operation: &mut Value,
    phase_name: &str,
    actor: &str,
    now: &str,
) -> Result<(), String> {
    let prior_attempt = operation
        .get("attempt")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if prior_attempt >= 4 {
        return Err("product output operation exhausted its four bounded attempts".to_string());
    }
    operation["attempt"] = json!(prior_attempt + 1);
    increment_product_output_operation_version(operation)?;
    operation["state"] = json!("active");
    operation["updated_at"] = json!(now);
    operation["updated_by"] = json!(actor);
    let phase = operation
        .get_mut(phase_name)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("product output {phase_name} phase missing"))?;
    phase.insert("status".to_string(), json!("in_progress"));
    phase.insert("claimed_at".to_string(), json!(now));
    phase.insert("claimed_by".to_string(), json!(actor));
    Ok(())
}

fn require_product_output_operation_version(
    operation: &Value,
    expected: u64,
) -> Result<(), String> {
    if operation.get("current_version").and_then(Value::as_u64) != Some(expected) {
        return Err("stale product output operation version".to_string());
    }
    Ok(())
}

fn increment_product_output_operation_version(operation: &mut Value) -> Result<(), String> {
    let current = operation
        .get("current_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "product output operation current_version missing".to_string())?;
    operation["current_version"] = json!(current.saturating_add(1));
    Ok(())
}

fn validate_product_output_operation_request(
    artifact: &Value,
    artifact_id: &str,
    request: &Value,
    request_sha256: &str,
) -> Result<(), String> {
    if request.get("schema_version").and_then(Value::as_str)
        != Some("product_draft_pr_output_request.v1")
        || request.get("artifact_id").and_then(Value::as_str) != Some(artifact_id)
    {
        return Err("product Draft PR request schema or artifact binding is invalid".to_string());
    }
    if request
        .get("expected_task_version")
        .and_then(Value::as_u64)
        .is_none()
    {
        return Err("product Draft PR request expected task version is missing".to_string());
    }
    for field in [
        "workspace_id",
        "run_id",
        "target_id",
        "source_revision",
        "patch_hash",
    ] {
        if request.get(field) != artifact.get(field) {
            return Err(format!("product Draft PR request {field} binding changed"));
        }
    }
    for field in [
        "product_task_id",
        "approval_id",
        "output_intent",
        "target_repository",
        "base_branch",
        "head_branch",
    ] {
        if request
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .is_none()
        {
            return Err(format!("product Draft PR request missing {field}"));
        }
    }
    if request.get("output_intent").and_then(Value::as_str) != Some("draft_pr") {
        return Err("product Draft PR request requires draft_pr output intent".to_string());
    }
    if request
        .get("head_branch")
        .and_then(Value::as_str)
        .is_none_or(|branch| !branch.starts_with("acp/"))
    {
        return Err("product Draft PR head branch must use acp/*".to_string());
    }
    validate_target_output_request_hash(request, request_sha256)
}

fn validate_product_output_operation(artifact: &Value, operation: &Value) -> Result<(), String> {
    if operation.get("schema_version").and_then(Value::as_str)
        != Some("product_output_operation.v1")
        || operation
            .get("operation_id")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("product-output-"))
            .is_none()
    {
        return Err("product output operation identity is invalid".to_string());
    }
    for field in ["artifact_id", "source_revision"] {
        if operation.get(field) != artifact.get(field) {
            return Err(format!("product output operation {field} binding changed"));
        }
    }
    let request = operation
        .get("request")
        .ok_or_else(|| "product output operation request missing".to_string())?;
    let request_sha256 = operation
        .get("request_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "product output operation request hash missing".to_string())?;
    validate_product_output_operation_request(
        artifact,
        artifact
            .get("artifact_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "artifact identity missing".to_string())?,
        request,
        request_sha256,
    )?;
    if operation.get("product_task_id") != request.get("product_task_id")
        || operation.get("approval_id") != request.get("approval_id")
        || operation.get("expected_task_version") != request.get("expected_task_version")
        || operation.get("target_repository") != request.get("target_repository")
        || operation.get("base_branch") != request.get("base_branch")
        || operation.get("head_branch") != request.get("head_branch")
    {
        return Err("product output operation request identity changed".to_string());
    }
    if !(1..=4).contains(
        &operation
            .get("attempt")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    ) || operation
        .get("current_version")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        == 0
    {
        return Err("product output operation attempt/version is invalid".to_string());
    }
    let branch_status = operation
        .pointer("/branch_push/status")
        .and_then(Value::as_str)
        .ok_or_else(|| "product output branch status missing".to_string())?;
    let pr_status = operation
        .pointer("/pr_create/status")
        .and_then(Value::as_str)
        .ok_or_else(|| "product output PR status missing".to_string())?;
    if !matches!(
        branch_status,
        "in_progress" | "completed" | "failed_known" | "outcome_unknown"
    ) || !matches!(
        pr_status,
        "pending" | "in_progress" | "completed" | "failed_known" | "outcome_unknown"
    ) {
        return Err("product output phase status is invalid".to_string());
    }
    for (phase_name, phase_status) in [("branch_push", branch_status), ("pr_create", pr_status)] {
        if phase_status == "in_progress" {
            let phase = operation
                .get(phase_name)
                .ok_or_else(|| format!("product output {phase_name} phase missing"))?;
            if phase
                .get("claimed_at")
                .and_then(Value::as_str)
                .filter(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok())
                .is_none()
                || phase
                    .get("claimed_by")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .is_none()
            {
                return Err(format!(
                    "in-progress product output {phase_name} phase claim is invalid"
                ));
            }
        }
    }
    if pr_status == "completed" {
        let pr = operation
            .get("pr_create")
            .ok_or_else(|| "product output PR phase missing".to_string())?;
        for field in [
            "number",
            "url",
            "repository",
            "base_branch",
            "head_branch",
            "head_sha",
            "completed_at",
        ] {
            if pr.get(field).is_none_or(Value::is_null) {
                return Err(format!("completed product output PR missing {field}"));
            }
        }
        if pr.get("draft").and_then(Value::as_bool) != Some(true)
            || operation.get("state").and_then(Value::as_str) != Some("completed")
            || branch_status != "completed"
        {
            return Err("completed product output PR state is inconsistent".to_string());
        }
    }
    if branch_status == "completed"
        && operation
            .pointer("/branch_push/commit_sha")
            .and_then(Value::as_str)
            .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .is_none()
    {
        return Err("completed branch phase is missing commit SHA".to_string());
    }
    if operation
        .pointer("/pr_create/draft")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return Err("product output operation may create Draft PRs only".to_string());
    }
    if pr_status == "completed"
        && (operation
            .pointer("/pr_create/number")
            .and_then(Value::as_u64)
            .is_none()
            || operation
                .pointer("/pr_create/url")
                .and_then(Value::as_str)
                .is_none())
    {
        return Err("completed Draft PR phase is missing identity".to_string());
    }
    let state = operation
        .get("state")
        .and_then(Value::as_str)
        .ok_or_else(|| "product output operation state missing".to_string())?;
    if !matches!(state, "active" | "completed" | "outcome_unknown" | "failed")
        || (state == "completed" && (branch_status != "completed" || pr_status != "completed"))
    {
        return Err("product output operation terminal state is inconsistent".to_string());
    }
    Ok(())
}

fn validate_target_output_request_hash(
    request_binding: &Value,
    request_sha256: &str,
) -> Result<(), String> {
    if request_sha256.len() != 64
        || !request_sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || target_output_json_sha256(request_binding)? != request_sha256
    {
        return Err("target output request hash is invalid".to_string());
    }
    Ok(())
}

fn validate_target_output_binding(
    artifact: &Value,
    artifact_id: &str,
    request_binding: &Value,
) -> Result<(), String> {
    let required = [
        ("workspace_id", artifact.get("workspace_id")),
        ("run_id", artifact.get("run_id")),
        ("target_id", artifact.get("target_id")),
        ("source_revision", artifact.get("source_revision")),
        ("patch_hash", artifact.get("patch_hash")),
    ];
    if request_binding
        .get("schema_version")
        .and_then(Value::as_str)
        != Some("target_repo_output_request.v1")
        || request_binding.get("artifact_id").and_then(Value::as_str) != Some(artifact_id)
        || required
            .iter()
            .any(|(field, expected)| request_binding.get(*field) != *expected)
    {
        return Err("target output request is not bound to its artifact owner".to_string());
    }
    Ok(())
}

fn validate_target_output_receipt(artifact: &Value, receipt: &Value) -> Result<(), String> {
    let request_binding = receipt
        .get("request_binding")
        .ok_or_else(|| "target output receipt is missing request binding".to_string())?;
    let artifact_id = artifact
        .get("artifact_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "target output artifact is missing identity".to_string())?;
    validate_target_output_binding(artifact, artifact_id, request_binding)?;
    let request_sha256 = receipt
        .get("request_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "target output receipt is missing request hash".to_string())?;
    validate_target_output_request_hash(request_binding, request_sha256)?;
    for field in [
        "artifact_id",
        "workspace_id",
        "run_id",
        "target_id",
        "source_revision",
        "patch_hash",
    ] {
        if receipt.get(field) != artifact.get(field) {
            return Err(format!("target output receipt {field} binding changed"));
        }
    }
    if receipt.get("schema_version").and_then(Value::as_str)
        != Some("target_repo_output_receipt.v1")
    {
        return Err("target output receipt schema is invalid".to_string());
    }
    match receipt.get("state").and_then(Value::as_str) {
        Some("sending" | "outcome_unknown") => {
            if !receipt.get("output").is_none_or(Value::is_null)
                || !receipt.get("output_sha256").is_none_or(Value::is_null)
                || !receipt.get("completed_at").is_none_or(Value::is_null)
            {
                return Err("nonterminal target output receipt contains a result".to_string());
            }
        }
        Some("completed") => {
            let output = receipt
                .get("output")
                .filter(|value| !value.is_null())
                .ok_or_else(|| "completed target output receipt is missing output".to_string())?;
            let expected_hash = receipt
                .get("output_sha256")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "completed target output receipt is missing output hash".to_string()
                })?;
            if receipt
                .get("completed_at")
                .and_then(Value::as_str)
                .is_none()
            {
                return Err(
                    "completed target output receipt is missing completion time".to_string()
                );
            }
            if target_output_json_sha256(output)? != expected_hash {
                return Err("target output receipt result hash changed".to_string());
            }
            for field in ["source_revision", "patch_hash"] {
                if output.get(field) != artifact.get(field) {
                    return Err(format!("target output result {field} binding changed"));
                }
            }
            if request_binding.get("mode").and_then(Value::as_str) == Some("push_branch") {
                for field in ["branch_name", "remote"] {
                    if output.get(field) != request_binding.get(field) {
                        return Err(format!("target output result {field} binding changed"));
                    }
                }
            }
        }
        _ => return Err("target output receipt has invalid state".to_string()),
    }
    Ok(())
}

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
fn pg_append_audit(
    client: &mut impl postgres::GenericClient,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &str,
) -> Result<(), String> {
    client
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
             VALUES ($1, $2, $3, $4, $5)",
            &[&now, &actor, &action, &resource, &details],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_supervised_patch_workspace_row(row: &postgres::Row) -> Value {
    let boundary_text: String = row.get(14);
    let workspace_text: String = row.get(15);
    let boundary: Value = serde_json::from_str(&boundary_text).unwrap_or(Value::Null);
    let mut workspace: Value = serde_json::from_str(&workspace_text).unwrap_or(Value::Null);
    if let Some(object) = workspace.as_object_mut() {
        object.insert(
            "workspace_sequence".to_string(),
            json!(row.get::<_, i64>(0)),
        );
        object.insert("workspace_id".to_string(), json!(row.get::<_, String>(1)));
        object.insert("plan_id".to_string(), pg_optional_row_string(row, 2));
        object.insert("run_id".to_string(), json!(row.get::<_, String>(3)));
        object.insert("target_id".to_string(), json!(row.get::<_, String>(4)));
        object.insert(
            "target_repo_path".to_string(),
            json!(row.get::<_, String>(5)),
        );
        object.insert(
            "target_repo_canonical_path".to_string(),
            json!(row.get::<_, String>(6)),
        );
        object.insert("workspace_path".to_string(), json!(row.get::<_, String>(7)));
        object.insert(
            "workspace_canonical_path".to_string(),
            json!(row.get::<_, String>(8)),
        );
        object.insert(
            "source_revision".to_string(),
            json!(row.get::<_, String>(9)),
        );
        object.insert(
            "source_tree_hash".to_string(),
            pg_optional_row_string(row, 10),
        );
        object.insert("status".to_string(), json!(row.get::<_, String>(11)));
        object.insert("created_at".to_string(), json!(row.get::<_, String>(12)));
        object.insert("updated_at".to_string(), json!(row.get::<_, String>(13)));
        object.insert("boundary".to_string(), boundary);
        object.insert("metadata_only".to_string(), json!(true));
        object.insert("execution_authority".to_string(), json!("disabled"));
    }
    workspace
}

#[cfg(feature = "pg")]
fn pg_supervised_patch_artifact_row(row: &postgres::Row) -> Value {
    let changed_files_text: String = row.get(9);
    let artifact_text: String = row.get(12);
    let changed_files: Value = serde_json::from_str(&changed_files_text).unwrap_or(Value::Null);
    let mut artifact: Value = serde_json::from_str(&artifact_text).unwrap_or(Value::Null);
    if let Some(object) = artifact.as_object_mut() {
        object.insert("artifact_sequence".to_string(), json!(row.get::<_, i64>(0)));
        object.insert("artifact_id".to_string(), json!(row.get::<_, String>(1)));
        object.insert("workspace_id".to_string(), json!(row.get::<_, String>(2)));
        object.insert("run_id".to_string(), json!(row.get::<_, String>(3)));
        object.insert("plan_id".to_string(), pg_optional_row_string(row, 4));
        object.insert("target_id".to_string(), json!(row.get::<_, String>(5)));
        object.insert(
            "source_revision".to_string(),
            json!(row.get::<_, String>(6)),
        );
        object.insert("artifact_type".to_string(), json!(row.get::<_, String>(7)));
        object.insert("patch_hash".to_string(), json!(row.get::<_, String>(8)));
        object.insert("changed_files".to_string(), changed_files);
        object.insert(
            "redaction_status".to_string(),
            json!(row.get::<_, String>(10)),
        );
        object.insert("created_at".to_string(), json!(row.get::<_, String>(11)));
        object.insert("metadata_only".to_string(), json!(true));
        object.insert("execution_authority".to_string(), json!("disabled"));
        object.insert("patch_apply_authority".to_string(), json!("disabled"));
    }
    artifact
}

#[cfg(feature = "pg")]
fn pg_optional_row_string(row: &postgres::Row, index: usize) -> Value {
    let value: Option<String> = row.get(index);
    value.map(Value::String).unwrap_or(Value::Null)
}

#[cfg(test)]
mod managed_owner_tests {
    use super::*;
    use crate::scheduler::{SchedulerConfig, WorkflowScheduler};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;

    struct HoldingExecutor {
        entered: std::sync::mpsc::Sender<()>,
        release: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    }

    #[test]
    fn product_output_phase_claims_stop_at_four_attempts() {
        let mut operation = json!({
            "attempt": 4,
            "current_version": 8,
            "state": "failed",
            "branch_push": {"status": "failed_known"},
            "pr_create": {"status": "pending"},
        });
        let error = claim_product_output_phase(
            &mut operation,
            "branch_push",
            "output-operator",
            "2026-07-22T00:00:00Z",
        )
        .unwrap_err();
        assert!(error.contains("four bounded attempts"));
        assert_eq!(operation["attempt"], 4);
        assert_eq!(operation["current_version"], 8);
        assert_eq!(operation["branch_push"]["status"], "failed_known");
    }

    #[test]
    fn product_output_phase_lease_covers_the_bounded_effect_window() {
        let still_owned = json!({"claimed_at": "2026-07-22T00:00:01Z"});
        assert!(
            product_output_phase_claim_is_current(&still_owned, "2026-07-22T00:15:00Z").unwrap()
        );
        let expired = json!({"claimed_at": "2026-07-22T00:00:00Z"});
        assert!(!product_output_phase_claim_is_current(&expired, "2026-07-22T00:15:00Z").unwrap());
    }

    impl crate::node_executor::NodeExecutor for HoldingExecutor {
        fn execute_node(
            &self,
            _input: &crate::node_executor::NodeExecutionInput,
        ) -> crate::node_executor::NodeExecutionOutput {
            self.entered.send(()).unwrap();
            let (lock, condition) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = condition.wait(released).unwrap();
            }
            crate::node_executor::NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: "holding_fixture".to_string(),
                output: Some("held managed execution completed".to_string()),
                error_domain: None,
                error_message: None,
                input_tokens: None,
                output_tokens: None,
                estimated_cost: None,
                latency_ms: Some(1),
                process_outcome: None,
                resolved_model: None,
            }
        }

        fn executor_type_name(&self) -> &str {
            "holding_fixture"
        }
    }

    #[test]
    fn managed_run_is_api_owned_and_both_scheduler_queue_modes_cannot_claim_it() {
        let dir = tempdir().unwrap();
        let target_dir = tempdir().unwrap();
        let workspace_dir = tempdir().unwrap();
        let target_path = target_dir.path().to_string_lossy().to_string();
        let workspace_path = workspace_dir.path().to_string_lossy().to_string();
        let store = Arc::new(LocalProductStore::new(dir.path().join("managed.db")).unwrap());
        store
            .import_supervised_patch_workspace(&json!({
                "schema_version": SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION,
                "workspace_id": "managed-owner-workspace",
                "run_id": "source-run",
                "target_id": "target",
                "target_repo_path": target_path,
                "target_repo_canonical_path": target_path,
                "workspace_path": workspace_path,
                "workspace_canonical_path": workspace_path,
                "source_revision": "fixture",
                "status": "requested",
                "metadata_only": true,
                "execution_authority": "disabled",
            }))
            .unwrap();
        let binding_sha256 = "ab".repeat(32);
        let metadata = json!({
            "profile_id": "supervised_patch_verification",
            "command": "python3 --version",
            "workspace_path": workspace_path,
            "workspace_root": workspace_path,
            "executor_timeout_ms": 120_000,
            "managed_supervised_patch": {
                "schema_version": "managed_supervised_patch.v1",
                "workspace_id": "managed-owner-workspace",
                "operation": "verify",
                "attempt": 1,
                "binding_sha256": binding_sha256,
                "content_excluded": true,
            },
        });
        let run_id = store
            .ensure_managed_supervised_patch_run(
                "managed-owner-workspace",
                "verify",
                1,
                &binding_sha256,
                &metadata,
                "test",
            )
            .unwrap();

        let run = store.get_workflow_run(&run_id).unwrap().unwrap();
        assert_eq!(run["pause_reason"], API_OWNED_SUPERVISED_PATCH);
        assert!(!store
            .list_active_workflow_run_ids()
            .unwrap()
            .contains(&run_id));
        assert!(!store
            .list_active_workflow_runs_prioritized()
            .unwrap()
            .iter()
            .any(|run| run["run_id"] == run_id));
        assert_eq!(store.count_active_workflow_runs().unwrap(), 0);
        assert_eq!(store.get_queue_status().unwrap()["total_paused"], 0);

        for result in [
            store.update_run_pause_reason(&run_id, None),
            store.update_run_pause_reason(&run_id, Some("operator_hold")),
        ] {
            let error = result.expect_err("generic pause mutation must preserve API ownership");
            assert!(is_execution_owner_conflict(&error), "{error}");
        }
        for result in [
            store
                .request_workflow_run_resume(&run_id, "operator", Some("generic resume"))
                .map(|_| ()),
            store
                .request_workflow_run_cancel(&run_id, "operator", Some("generic cancel"))
                .map(|_| ()),
        ] {
            let error = result.expect_err("generic run mutation must preserve API ownership");
            assert!(is_execution_owner_conflict(&error), "{error}");
        }
        let error = store
            .tick_workflow_run(&run_id, "generic-tick")
            .expect_err("generic tick must not claim API-owned run");
        assert!(is_execution_owner_conflict(&error), "{error}");

        for queue_enabled in [false, true] {
            let config = SchedulerConfig {
                interval_ms: 10,
                max_concurrent: 1,
                lease_timeout_ms: 300_000,
                executor_type: "noop".to_string(),
                queue_enabled,
                supervised_workers_enabled: true,
                worker_count: 1,
                ..Default::default()
            };
            let mut scheduler = WorkflowScheduler::new(store.clone(), config);
            scheduler.start().unwrap();
            for _ in 0..20 {
                if !scheduler.status()["last_tick_at"].is_null() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(!scheduler.status()["last_tick_at"].is_null());
            scheduler.stop().unwrap();
            let run = store.get_workflow_run(&run_id).unwrap().unwrap();
            assert_eq!(run["status"], "created");
            assert_eq!(run["nodes"][0]["status"], "pending");
        }

        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let release = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let tick_store = store.clone();
        let tick_run_id = run_id.clone();
        let tick_release = release.clone();
        let tick_thread = std::thread::spawn(move || {
            tick_store.tick_managed_supervised_patch_with_executor(
                &tick_run_id,
                "api-owner",
                &HoldingExecutor {
                    entered: entered_tx,
                    release: tick_release,
                },
            )
        });
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        for result in [
            store.update_run_pause_reason(&run_id, Some("concurrent operator hold")),
            store
                .request_workflow_run_cancel(&run_id, "operator", Some("concurrent generic cancel"))
                .map(|_| ()),
            store
                .tick_workflow_run(&run_id, "concurrent-generic-tick")
                .map(|_| ()),
        ] {
            let error = result.expect_err("concurrent generic mutation must preserve API owner");
            assert!(is_execution_owner_conflict(&error), "{error}");
        }
        let (lock, condition) = &*release;
        *lock.lock().unwrap() = true;
        condition.notify_all();
        let tick = tick_thread.join().unwrap().unwrap();
        assert_eq!(tick["node_id"], "supervised-verify-1");
        assert_eq!(tick["result"]["executor_type"], "holding_fixture");
        assert_eq!(store.get_queue_status().unwrap()["total_paused"], 0);
    }
}

#[cfg(all(test, feature = "pg-tests"))]
mod pg_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn managed_run_creation_is_atomic_and_binding_safe_on_postgres() {
        let Ok(url) = std::env::var("ACP_TEST_DATABASE_URL") else {
            eprintln!("ACP_TEST_DATABASE_URL not set; skipping pg-tests");
            return;
        };
        let store = Arc::new(
            LocalProductStore::new_postgres(&url, || {
                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
            })
            .unwrap(),
        );
        let tag = uuid::Uuid::new_v4().to_string();
        let workspace_id = format!("managed-pg-workspace-{tag}");
        let workspace_path = format!("/var/tmp/managed-pg-workspace-{tag}");
        store
            .import_supervised_patch_workspace(&json!({
                "schema_version": SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION,
                "workspace_id": workspace_id,
                "run_id": format!("source-run-{tag}"),
                "target_id": format!("target-{tag}"),
                "target_repo_path": "/tmp",
                "target_repo_canonical_path": "/tmp",
                "workspace_path": workspace_path,
                "workspace_canonical_path": workspace_path,
                "source_revision": "fixture",
                "status": "requested",
                "metadata_only": true,
                "execution_authority": "disabled",
            }))
            .unwrap();
        let binding_sha256 = "ab".repeat(32);
        let metadata = json!({
            "profile_id": "supervised_patch_verification",
            "command": "python3 --version",
            "workspace_path": workspace_path,
            "workspace_root": workspace_path,
            "executor_timeout_ms": 120_000,
            "managed_supervised_patch": {
                "schema_version": "managed_supervised_patch.v1",
                "workspace_id": workspace_id,
                "operation": "verify",
                "attempt": 1,
                "binding_sha256": binding_sha256,
                "content_excluded": true,
            },
        });

        let first_store = store.clone();
        let first_workspace = workspace_id.clone();
        let first_binding = binding_sha256.clone();
        let first_metadata = metadata.clone();
        let first = std::thread::spawn(move || {
            first_store.ensure_managed_supervised_patch_run(
                &first_workspace,
                "verify",
                1,
                &first_binding,
                &first_metadata,
                "pg-test",
            )
        });
        let second_store = store.clone();
        let second_workspace = workspace_id.clone();
        let second_binding = binding_sha256.clone();
        let second_metadata = metadata.clone();
        let second = std::thread::spawn(move || {
            second_store.ensure_managed_supervised_patch_run(
                &second_workspace,
                "verify",
                1,
                &second_binding,
                &second_metadata,
                "pg-test",
            )
        });
        let first_run = first.join().unwrap().unwrap();
        let second_run = second.join().unwrap().unwrap();
        assert_eq!(first_run, second_run);
        let run = store.get_workflow_run(&first_run).unwrap().unwrap();
        assert_eq!(run["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(run["pause_reason"], API_OWNED_SUPERVISED_PATCH);
        assert!(!store
            .list_active_workflow_run_ids()
            .unwrap()
            .contains(&first_run));
        assert!(!store
            .list_active_workflow_runs_prioritized()
            .unwrap()
            .iter()
            .any(|run| run["run_id"] == first_run));
        for result in [
            store.update_run_pause_reason(&first_run, None),
            store.update_run_pause_reason(&first_run, Some("operator_hold")),
        ] {
            let error = result.expect_err("generic pause mutation must preserve API ownership");
            assert!(is_execution_owner_conflict(&error), "{error}");
        }
        for result in [
            store
                .request_workflow_run_resume(&first_run, "operator", Some("generic resume"))
                .map(|_| ()),
            store
                .request_workflow_run_cancel(&first_run, "operator", Some("generic cancel"))
                .map(|_| ()),
        ] {
            let error = result.expect_err("generic run mutation must preserve API ownership");
            assert!(is_execution_owner_conflict(&error), "{error}");
        }
        let error = store
            .tick_workflow_run(&first_run, "generic-tick")
            .expect_err("generic tick must not claim API-owned run");
        assert!(is_execution_owner_conflict(&error), "{error}");
        let tick = store
            .tick_managed_supervised_patch_with_executor(
                &first_run,
                "api-owner",
                &crate::node_executor::NoopNodeExecutor,
            )
            .unwrap();
        assert_eq!(tick["node_id"], "supervised-verify-1");

        let changed_binding = "cd".repeat(32);
        let mut changed_metadata = metadata;
        changed_metadata["command"] = json!("python3 -V");
        changed_metadata["managed_supervised_patch"]["binding_sha256"] = json!(changed_binding);
        let error = store
            .ensure_managed_supervised_patch_run(
                &workspace_id,
                "verify",
                1,
                changed_metadata["managed_supervised_patch"]["binding_sha256"]
                    .as_str()
                    .unwrap(),
                &changed_metadata,
                "pg-test",
            )
            .unwrap_err();
        assert!(error.contains("binding changed"), "{error}");
    }
}
