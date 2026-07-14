use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use crate::target_repo_output::{
    inspect_git_patch, patch_hash as target_patch_hash, remove_git_worktree, stage_and_build_patch,
    staged_changed_files, TargetRepoOutputConfig,
};

#[cfg(test)]
use super::workflow_runs::is_execution_owner_conflict;
use super::workflow_runs::API_OWNED_SUPERVISED_PATCH;
use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

pub(crate) mod fs_utils;
use self::fs_utils::*;

pub const SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION: &str = "supervised_patch_workspace.v1";
pub const SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION: &str = "supervised_patch_artifact.v1";

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
            "evidence_recorded" | "verification_failed" | "approval_required"
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
        if !matches!(operation, "verify" | "repair") {
            return Err(format!(
                "unsupported managed supervised-patch operation: {operation}"
            ));
        }
        if attempt == 0 || attempt > 5 {
            return Err("managed supervised-patch attempt must be between 1 and 5".to_string());
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
