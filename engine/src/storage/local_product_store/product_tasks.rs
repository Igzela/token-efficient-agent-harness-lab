//! Canonical product-task persistence and worktree-first intake orchestration (G1).

use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};
use std::path::Path;

use crate::product_golden_path::{
    compile_product_executable_graph, is_valid_product_task_transition, planned_workspace_path,
    product_gate_enabled, provisional_run_id_for_task, redacted_intake_json,
    resolve_admitted_executor, validate_source_revision_format, workspace_content_hash,
    ProductExecutorPolicy, ProductTaskStatus, ProductWorkspaceBinding, ValidatedProductTaskIntake,
    PRODUCT_EXECUTABLE_GRAPH_SCHEMA_VERSION, PRODUCT_TASK_SCHEMA_VERSION,
    PRODUCT_TASK_WORKSPACE_BINDING_SCHEMA_VERSION,
};
use crate::read_only_planner::{ReadOnlyPlanner, READ_ONLY_PLAN_SCHEMA_VERSION};
use crate::target_repo_output::{
    prepare_git_worktree, remove_git_worktree, TargetRepoOutputConfig,
};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

const PRODUCT_TASK_SELECT: &str = "SELECT schema_version, task_id, tenant_id, workspace_id,
    idempotency_key, status, version, objective_fingerprint, target_id, target_repo_path,
    source_revision, source_tree_hash, output_intent, risk_class, approval_required,
    confirm_execution, confirm_output, intake_contract_sha256, intake_json,
    workspace_binding_json, plan_id, run_id, workspace_record_id, failure_code,
    failure_detail, created_at, updated_at, created_by
 FROM product_tasks";

impl LocalProductStore {
    /// Authenticated intake: reserve canonical task under idempotency, prepare controlled
    /// worktree, verify bindings, and finalize to `workspace_bound` without admitting execution.
    pub fn admit_product_task(
        &self,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        validate_source_revision_format(&intake.source_revision)?;

        let reserved = self.reserve_product_task(intake, actor)?;
        let status = reserved
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("admitted");
        if matches!(
            status,
            "workspace_bound"
                | "graph_ready"
                | "running"
                | "awaiting_approval"
                | "output_pending"
                | "completed"
        ) {
            return Ok(reserved);
        }
        if matches!(status, "failed" | "blocked" | "killed" | "budget_exhausted") {
            return Ok(reserved);
        }

        let task_id = reserved
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "reserved product task missing task_id".to_string())?
            .to_string();
        let version = reserved.get("version").and_then(Value::as_u64).unwrap_or(1);

        if status == ProductTaskStatus::Admitted.as_str() {
            self.transition_product_task(
                &task_id,
                ProductTaskStatus::WorkspacePreparing,
                Some(version),
                actor,
                None,
                None,
                None,
                None,
                None,
            )?;
        }

        match self.prepare_product_task_worktree(&task_id, intake, actor) {
            Ok(task) => Ok(task),
            Err(error) => {
                let _ = self.fail_product_task_and_compensate(
                    &task_id,
                    "worktree_prepare_failed",
                    &error,
                    actor,
                );
                Err(error)
            }
        }
    }

    pub fn get_product_task(&self, task_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    &format!("{PRODUCT_TASK_SELECT} WHERE task_id = ?1"),
                    params![task_id],
                    map_product_task_row,
                )
                .optional()
                .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_opt(
                        &format!("{PRODUCT_TASK_SELECT} WHERE task_id = $1"),
                        &[&task_id],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| product_task_row_to_json_pg(&r)))
            }),
        }
    }

    pub fn get_product_task_by_idempotency(
        &self,
        tenant_id: &str,
        workspace_id: &str,
        idempotency_key: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    &format!(
                        "{PRODUCT_TASK_SELECT}
                         WHERE tenant_id = ?1 AND workspace_id = ?2 AND idempotency_key = ?3"
                    ),
                    params![tenant_id, workspace_id, idempotency_key],
                    map_product_task_row,
                )
                .optional()
                .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_opt(
                        &format!(
                            "{PRODUCT_TASK_SELECT}
                             WHERE tenant_id = $1 AND workspace_id = $2 AND idempotency_key = $3"
                        ),
                        &[&tenant_id, &workspace_id, &idempotency_key],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(row.map(|r| product_task_row_to_json_pg(&r)))
            }),
        }
    }

    fn reserve_product_task(
        &self,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
    ) -> Result<Value, String> {
        if let Some(existing) = self.get_product_task_by_idempotency(
            &intake.tenant_id,
            &intake.workspace_id,
            &intake.idempotency_key,
        )? {
            if let Some(expected) = intake.expected_version {
                let current = existing.get("version").and_then(Value::as_u64).unwrap_or(0);
                if current != expected {
                    return Err(format!(
                        "stale expected_version: current={current} expected={expected}"
                    ));
                }
            }
            let existing_sha = existing
                .get("intake_contract_sha256")
                .and_then(Value::as_str)
                .unwrap_or("");
            if existing_sha != intake.intake_contract_sha256 {
                return Err(
                    "idempotency key already bound to a different intake contract".to_string(),
                );
            }
            return Ok(existing);
        }

        let now = self.now();
        let task_id = allocate_task_id(&now);
        let intake_json = redacted_intake_json(intake).to_string();
        let status = ProductTaskStatus::Admitted.as_str();

        match &self.db {
            DatabaseConnection::Sqlite(_) => {
                self.with_conn(|conn| {
                    match conn.execute(
                        "INSERT INTO product_tasks (
                            task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                            status, version, objective_fingerprint, target_id, target_repo_path,
                            source_revision, source_tree_hash, output_intent, risk_class,
                            approval_required, confirm_execution, confirm_output,
                            intake_contract_sha256, intake_json, workspace_binding_json,
                            plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                            created_at, updated_at, created_by
                         ) VALUES (
                            ?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                            ?14, ?15, ?16, ?17, ?18, NULL, NULL, NULL, NULL, NULL, NULL,
                            ?19, ?19, ?20
                         )",
                        params![
                            task_id,
                            PRODUCT_TASK_SCHEMA_VERSION,
                            intake.tenant_id,
                            intake.workspace_id,
                            intake.idempotency_key,
                            status,
                            intake.objective_fingerprint,
                            intake.target_id,
                            intake.target_repo_path,
                            intake.source_revision,
                            intake.source_tree_hash,
                            intake.output_intent.as_str(),
                            intake.risk_class,
                            intake.approval_required as i64,
                            intake.confirm_execution as i64,
                            intake.confirm_output as i64,
                            intake.intake_contract_sha256,
                            intake_json,
                            now,
                            actor,
                        ],
                    ) {
                        Ok(_) => {
                            append_audit_locked(
                                conn,
                                &now,
                                actor,
                                "product_task.admit",
                                &task_id,
                                &json!({
                                    "status": status,
                                    "tenant_id": intake.tenant_id,
                                    "workspace_id": intake.workspace_id,
                                    "idempotency_key": intake.idempotency_key,
                                    "intake_contract_sha256": intake.intake_contract_sha256,
                                    "execution_admitted": false,
                                }),
                            )?;
                            Ok(())
                        }
                        Err(rusqlite::Error::SqliteFailure(code, _))
                            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
                        {
                            Ok(())
                        }
                        Err(e) => Err(e.to_string()),
                    }
                })?;
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| {
                    let n = client
                        .execute(
                            "INSERT INTO product_tasks (
                                task_id, schema_version, tenant_id, workspace_id, idempotency_key,
                                status, version, objective_fingerprint, target_id, target_repo_path,
                                source_revision, source_tree_hash, output_intent, risk_class,
                                approval_required, confirm_execution, confirm_output,
                                intake_contract_sha256, intake_json, workspace_binding_json,
                                plan_id, run_id, workspace_record_id, failure_code, failure_detail,
                                created_at, updated_at, created_by
                             ) VALUES (
                                $1, $2, $3, $4, $5, $6, 1, $7, $8, $9, $10, $11, $12, $13,
                                $14, $15, $16, $17, $18, NULL, NULL, NULL, NULL, NULL, NULL,
                                $19, $19, $20
                             )
                             ON CONFLICT (tenant_id, workspace_id, idempotency_key) DO NOTHING",
                            &[
                                &task_id,
                                &PRODUCT_TASK_SCHEMA_VERSION,
                                &intake.tenant_id,
                                &intake.workspace_id,
                                &intake.idempotency_key,
                                &status,
                                &intake.objective_fingerprint,
                                &intake.target_id,
                                &intake.target_repo_path,
                                &intake.source_revision,
                                &intake.source_tree_hash,
                                &intake.output_intent.as_str(),
                                &intake.risk_class,
                                &(intake.approval_required as i32),
                                &(intake.confirm_execution as i32),
                                &(intake.confirm_output as i32),
                                &intake.intake_contract_sha256,
                                &intake_json,
                                &now,
                                &actor,
                            ],
                        )
                        .map_err(|e| e.to_string())?;
                    if n > 0 {
                        let audit_details = json!({
                            "status": status,
                            "tenant_id": intake.tenant_id,
                            "workspace_id": intake.workspace_id,
                            "idempotency_key": intake.idempotency_key,
                            "intake_contract_sha256": intake.intake_contract_sha256,
                            "execution_admitted": false,
                        })
                        .to_string();
                        client
                            .execute(
                                "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                                 VALUES ($1, $2, 'product_task.admit', $3, $4)",
                                &[&now, &actor, &task_id, &audit_details],
                            )
                            .map_err(|e| e.to_string())?;
                    }
                    Ok(())
                })?;
            }
        }

        self.get_product_task_by_idempotency(
            &intake.tenant_id,
            &intake.workspace_id,
            &intake.idempotency_key,
        )?
        .ok_or_else(|| "product task reservation failed".to_string())
    }

    #[allow(clippy::too_many_arguments)]
    fn transition_product_task(
        &self,
        task_id: &str,
        to: ProductTaskStatus,
        expected_version: Option<u64>,
        actor: &str,
        workspace_binding: Option<&ProductWorkspaceBinding>,
        workspace_record_id: Option<&str>,
        failure_code: Option<&str>,
        failure_detail: Option<&str>,
        provisional_run_id: Option<&str>,
    ) -> Result<Value, String> {
        let current = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let from =
            ProductTaskStatus::parse(current.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if from != to && !is_valid_product_task_transition(from, to) {
            return Err(format!(
                "invalid product task transition: {} -> {}",
                from.as_str(),
                to.as_str()
            ));
        }
        let current_version = current.get("version").and_then(Value::as_u64).unwrap_or(0);
        if let Some(expected) = expected_version {
            if current_version != expected {
                return Err(format!(
                    "stale product task version: current={current_version} expected={expected}"
                ));
            }
        }
        if from == to {
            return Ok(current);
        }
        let next_version = current_version + 1;
        let now = self.now();
        let binding_json = workspace_binding.map(|b| serde_json::to_string(b).unwrap_or_default());
        let status = to.as_str();

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let updated = conn
                    .execute(
                        "UPDATE product_tasks SET
                            status = ?1,
                            version = ?2,
                            updated_at = ?3,
                            workspace_binding_json = COALESCE(?4, workspace_binding_json),
                            workspace_record_id = COALESCE(?5, workspace_record_id),
                            run_id = COALESCE(?6, run_id),
                            failure_code = ?7,
                            failure_detail = ?8
                         WHERE task_id = ?9 AND version = ?10",
                        params![
                            status,
                            next_version as i64,
                            now,
                            binding_json,
                            workspace_record_id,
                            provisional_run_id,
                            failure_code,
                            failure_detail,
                            task_id,
                            current_version as i64,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("product task expected-current update conflict".to_string());
                }
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "product_task.transition",
                    task_id,
                    &json!({
                        "from": from.as_str(),
                        "to": status,
                        "version": next_version,
                        "execution_admitted": to.admits_execution(),
                        "failure_code": failure_code,
                    }),
                )?;
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let updated = client
                    .execute(
                        "UPDATE product_tasks SET
                            status = $1,
                            version = $2,
                            updated_at = $3,
                            workspace_binding_json = COALESCE($4, workspace_binding_json),
                            workspace_record_id = COALESCE($5, workspace_record_id),
                            run_id = COALESCE($6, run_id),
                            failure_code = $7,
                            failure_detail = $8
                         WHERE task_id = $9 AND version = $10",
                        &[
                            &status,
                            &(next_version as i64),
                            &now,
                            &binding_json,
                            &workspace_record_id,
                            &provisional_run_id,
                            &failure_code,
                            &failure_detail,
                            &task_id,
                            &(current_version as i64),
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                if updated != 1 {
                    return Err("product task expected-current update conflict".to_string());
                }
                let audit_details = json!({
                    "from": from.as_str(),
                    "to": status,
                    "version": next_version,
                    "execution_admitted": to.admits_execution(),
                    "failure_code": failure_code,
                })
                .to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, 'product_task.transition', $3, $4)",
                        &[&now, &actor, &task_id, &audit_details],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            })?,
        }
        self.get_product_task(task_id)?
            .ok_or_else(|| "product task missing after transition".to_string())
    }

    fn prepare_product_task_worktree(
        &self,
        task_id: &str,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
    ) -> Result<Value, String> {
        let config = TargetRepoOutputConfig::from_env();
        config.require_enabled()?;

        let target_repo = Path::new(&intake.target_repo_path);
        if !target_repo.is_absolute() {
            return Err("target_repo_path must be absolute".to_string());
        }
        if !target_repo.is_dir() {
            return Err("target_repo_path is not a directory".to_string());
        }
        let target_canonical = std::fs::canonicalize(target_repo).map_err(|e| e.to_string())?;

        let workspace_fs_id = format!(
            "pt-{}",
            task_id
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        );
        let workspace_path = planned_workspace_path(self.db_path(), &workspace_fs_id)?;

        if workspace_path.exists() {
            let _ = remove_git_worktree(&config, &target_canonical, &workspace_path);
            if workspace_path.exists() {
                let _ = std::fs::remove_dir_all(&workspace_path);
            }
        }

        let prepared = prepare_git_worktree(
            &config,
            &target_canonical,
            &workspace_path,
            &intake.source_revision,
        )
        .map_err(|e| format!("prepare_git_worktree failed: {e}"))?;

        let workspaces_root = self
            .db_path()
            .parent()
            .ok_or_else(|| "store has no parent".to_string())?
            .join("workspaces");
        let workspaces_root = std::fs::canonicalize(&workspaces_root).unwrap_or(workspaces_root);
        let ws = std::fs::canonicalize(Path::new(&prepared.workspace_path))
            .map_err(|e| e.to_string())?;
        if !ws.starts_with(&workspaces_root) {
            let _ = remove_git_worktree(
                &config,
                &target_canonical,
                Path::new(&prepared.workspace_path),
            );
            return Err("workspace escaped app-owned workspace root".to_string());
        }

        let content_hash = match workspace_content_hash(Path::new(&prepared.workspace_path)) {
            Ok(hash) => hash,
            Err(e) => {
                let _ = remove_git_worktree(
                    &config,
                    &target_canonical,
                    Path::new(&prepared.workspace_path),
                );
                return Err(e);
            }
        };

        if let Some(expected_tree) = intake.source_tree_hash.as_deref() {
            if expected_tree.len() == 64 && expected_tree != content_hash {
                let _ = remove_git_worktree(
                    &config,
                    &target_canonical,
                    Path::new(&prepared.workspace_path),
                );
                return Err("source_tree_hash mismatch against prepared workspace".to_string());
            }
        }

        let provisional_run_id = provisional_run_id_for_task(task_id);
        let workspace_request = json!({
            "run_id": provisional_run_id,
            "plan_id": Value::Null,
            "target_id": intake.target_id,
            "target_repo_path": target_canonical.to_string_lossy(),
            "workspace_path": prepared.workspace_path,
            "source_revision": prepared.source_revision,
            "source_tree_hash": content_hash,
            "workspace_mode": "git_worktree",
            "git": {
                "default_branch": prepared.default_branch,
                "source_revision": prepared.source_revision,
            },
            "status": "workspace_created",
            "product_task_id": task_id,
            "allowed_paths": intake.allowed_paths,
        });

        let workspace = match self.record_supervised_patch_workspace(&workspace_request, actor) {
            Ok(ws) => ws,
            Err(e) => {
                let _ = remove_git_worktree(
                    &config,
                    &target_canonical,
                    Path::new(&prepared.workspace_path),
                );
                return Err(format!("record supervised workspace failed: {e}"));
            }
        };

        let workspace_record_id = workspace
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let binding = ProductWorkspaceBinding {
            schema_version: PRODUCT_TASK_WORKSPACE_BINDING_SCHEMA_VERSION.to_string(),
            workspace_id: workspace_record_id.clone(),
            workspace_path: prepared.workspace_path.clone(),
            workspace_canonical_path: workspace
                .get("workspace_canonical_path")
                .and_then(Value::as_str)
                .unwrap_or(&prepared.workspace_path)
                .to_string(),
            target_repo_canonical_path: target_canonical.to_string_lossy().into_owned(),
            source_revision: prepared.source_revision.clone(),
            source_tree_hash: Some(content_hash.clone()),
            workspace_content_hash: content_hash,
            workspace_mode: "git_worktree".to_string(),
            provisional_run_id: provisional_run_id.clone(),
            allowed_paths: intake.allowed_paths.clone(),
            bound_at: self.now(),
        };

        let current = self
            .get_product_task(task_id)?
            .ok_or_else(|| "task missing before finalize".to_string())?;
        let version = current.get("version").and_then(Value::as_u64).unwrap_or(0);
        self.transition_product_task(
            task_id,
            ProductTaskStatus::WorkspaceBound,
            Some(version),
            actor,
            Some(&binding),
            Some(&workspace_record_id),
            None,
            None,
            Some(&provisional_run_id),
        )
    }

    fn fail_product_task_and_compensate(
        &self,
        task_id: &str,
        code: &str,
        detail: &str,
        actor: &str,
    ) -> Result<Value, String> {
        let provisional = provisional_run_id_for_task(task_id);
        if let Ok(workspaces) = self.supervised_patch_workspaces(50) {
            for ws in workspaces {
                if ws.get("run_id").and_then(Value::as_str) == Some(provisional.as_str()) {
                    if let Some(ws_id) = ws.get("workspace_id").and_then(Value::as_str) {
                        let _ = self.cleanup_workspace(ws_id, actor);
                    }
                }
            }
        }
        // Also clean planned fs path if present without a workspace record.
        let workspace_fs_id = format!(
            "pt-{}",
            task_id
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect::<String>()
        );
        if let Ok(path) = planned_workspace_path(self.db_path(), &workspace_fs_id) {
            if path.exists() {
                let _ = std::fs::remove_dir_all(&path);
            }
        }

        let current = self.get_product_task(task_id)?;
        let version = current
            .as_ref()
            .and_then(|t| t.get("version").and_then(Value::as_u64));
        let status = current
            .as_ref()
            .and_then(|t| t.get("status").and_then(Value::as_str))
            .unwrap_or("");
        if status == ProductTaskStatus::Failed.as_str() {
            return current.ok_or_else(|| "failed task missing".to_string());
        }
        self.transition_product_task(
            task_id,
            ProductTaskStatus::Failed,
            version,
            actor,
            None,
            None,
            Some(code),
            Some(detail),
            None,
        )
    }

    /// G2: compile executable graph from a workspace-bound product task and create a
    /// scheduler-eligible workflow run through existing plan/run owners.
    ///
    /// `available_executors` is the live pool's registered types; missing admission fails closed.
    pub fn compile_and_schedule_product_task(
        &self,
        task_id: &str,
        actor: &str,
        available_executors: &[String],
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let status =
            ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if matches!(
            status,
            ProductTaskStatus::GraphReady | ProductTaskStatus::Running
        ) {
            // Idempotent: return current task + bound run if already scheduled.
            return Ok(json!({
                "task": task,
                "reused": true,
                "execution_admitted": status.admits_execution(),
            }));
        }
        if status != ProductTaskStatus::WorkspaceBound {
            return Err(format!(
                "compile requires workspace_bound task; status={}",
                status.as_str()
            ));
        }

        // Verify worktree still exists and matches binding before admitting execution.
        let binding = task
            .get("workspace_binding")
            .ok_or_else(|| "workspace_binding missing".to_string())?;
        let workspace_path = binding
            .get("workspace_path")
            .and_then(Value::as_str)
            .ok_or_else(|| "workspace_path missing".to_string())?;
        if !Path::new(workspace_path).is_dir() {
            return Err("bound worktree is missing; zero execution effect".to_string());
        }
        let workspace_record_id = task
            .get("workspace_record_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workspace_record_id missing".to_string())?;
        let workspace = self
            .get_supervised_patch_workspace(workspace_record_id)?
            .ok_or_else(|| "supervised workspace record missing".to_string())?;
        let ws_status = workspace
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("");
        if matches!(ws_status, "quarantined" | "cleaned" | "rejected") {
            return Err(format!(
                "workspace status {ws_status} blocks execution; zero execution effect"
            ));
        }

        let executor_policy: ProductExecutorPolicy = serde_json::from_value(
            task.pointer("/intake/executor_policy")
                .cloned()
                .unwrap_or(json!({"allowed_executors":["command"]})),
        )
        .map_err(|e| format!("executor_policy malformed: {e}"))?;
        let resolved = resolve_admitted_executor(&executor_policy)?;
        if !available_executors.iter().any(|e| e == &resolved) {
            return Err(format!(
                "admitted executor '{resolved}' is unavailable in the live executor pool"
            ));
        }

        let tenant_id = task
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("local");
        let workspace_scope = task
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("default");
        let objective_preview = task
            .pointer("/intake/objective_preview")
            .and_then(Value::as_str)
            .unwrap_or("product golden path task");

        let planner = ReadOnlyPlanner::new();
        let plan = self.create_workflow_plan(
            objective_preview,
            "product_golden_path",
            actor,
            |ids, created_at| {
                let graph = compile_product_executable_graph(&task, created_at, ids, &resolved)?;
                let analysis = planner
                    .create_plan(ids, objective_preview, "product_golden_path", created_at)?
                    .get("analysis")
                    .cloned()
                    .unwrap_or(json!({}));
                Ok(json!({
                    "schema_version": READ_ONLY_PLAN_SCHEMA_VERSION,
                    "plan_id": ids.plan_id,
                    "plan_sequence": ids.sequence,
                    "created_at": created_at,
                    "updated_at": created_at,
                    "raw_request": objective_preview,
                    "request_source": "product_golden_path",
                    "status": "planned_executable",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": analysis,
                    "graph": graph,
                    "validation": {"valid": true, "errors": []},
                    "execution_order": graph.get("nodes").and_then(Value::as_array).map(|nodes| {
                        nodes.iter().filter_map(|n| n.get("node_id").cloned()).collect::<Vec<_>>()
                    }).unwrap_or_default(),
                    "advisory": {
                        "schema_version": "plan_advisory.v1",
                        "requires_executor": resolved,
                        "product_task_id": task_id,
                        "product_graph_schema_version": PRODUCT_EXECUTABLE_GRAPH_SCHEMA_VERSION,
                    },
                    "boundaries": {
                        "execution_authority": "product_golden_path",
                        "target_repository_writes": "disabled",
                        "runtime_workers": "env_gated_supervised",
                        "sandbox_process_execution": "command_allowlist_in_bound_worktree",
                        "provider_calls": "not_invoked",
                        "approval_execution_authority": "disabled",
                        "resume_execution_authority": "disabled",
                        "cancel_execution_authority": "enabled",
                        "deploy_merge_controls": "not_available",
                        "product_task_id": task_id,
                        "workspace_id": workspace_record_id,
                        "source_revision": binding.get("source_revision"),
                    },
                }))
            },
        )?;
        let plan_id = plan
            .get("plan_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "plan missing plan_id".to_string())?
            .to_string();

        let run =
            self.create_workflow_run_from_plan_scoped(&plan_id, actor, tenant_id, workspace_scope)?;
        let run_id = run
            .get("run_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "run missing run_id".to_string())?
            .to_string();

        // Rebind supervised workspace to the real run so lease injection finds the worktree.
        self.rebind_supervised_workspace_run_id(workspace_record_id, &run_id, actor)?;
        self.bind_product_task_plan_run(task_id, &plan_id, &run_id, actor)?;

        let version = task.get("version").and_then(Value::as_u64).unwrap_or(0);
        self.transition_product_task(
            task_id,
            ProductTaskStatus::GraphReady,
            Some(version),
            actor,
            None,
            None,
            None,
            None,
            Some(&run_id),
        )?;
        let after_graph = self
            .get_product_task(task_id)?
            .ok_or_else(|| "task missing after graph_ready".to_string())?;
        let graph_version = after_graph
            .get("version")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let task = self.transition_product_task(
            task_id,
            ProductTaskStatus::Running,
            Some(graph_version),
            actor,
            None,
            None,
            None,
            None,
            None,
        )?;

        Ok(json!({
            "task": task,
            "plan": plan,
            "run": run,
            "resolved_executor": resolved,
            "reused": false,
            "execution_admitted": true,
            "scheduler_eligible": true,
        }))
    }

    fn bind_product_task_plan_run(
        &self,
        task_id: &str,
        plan_id: &str,
        run_id: &str,
        actor: &str,
    ) -> Result<(), String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE product_tasks SET plan_id = ?1, run_id = ?2, updated_at = ?3 WHERE task_id = ?4",
                    params![plan_id, run_id, now, task_id],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "product_task.bind_plan_run",
                    task_id,
                    &json!({"plan_id": plan_id, "run_id": run_id}),
                )?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE product_tasks SET plan_id = $1, run_id = $2, updated_at = $3 WHERE task_id = $4",
                        &[&plan_id, &run_id, &now, &task_id],
                    )
                    .map_err(|e| e.to_string())?;
                let details = json!({"plan_id": plan_id, "run_id": run_id}).to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, 'product_task.bind_plan_run', $3, $4)",
                        &[&now, &actor, &task_id, &details],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    fn rebind_supervised_workspace_run_id(
        &self,
        workspace_id: &str,
        run_id: &str,
        actor: &str,
    ) -> Result<(), String> {
        let mut workspace = self
            .get_supervised_patch_workspace(workspace_id)?
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let now = self.now();
        if let Some(obj) = workspace.as_object_mut() {
            obj.insert("run_id".to_string(), json!(run_id));
            obj.insert("updated_at".to_string(), json!(now.clone()));
            obj.insert("product_run_rebound".to_string(), json!(true));
        }
        let workspace_json = workspace.to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE supervised_patch_workspaces
                     SET run_id = ?1, updated_at = ?2, workspace_json = ?3
                     WHERE workspace_id = ?4",
                    params![run_id, now, workspace_json, workspace_id],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "supervised_patch.workspace_run_rebind",
                    workspace_id,
                    &json!({"run_id": run_id, "product_golden_path": true}),
                )?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE supervised_patch_workspaces
                         SET run_id = $1, updated_at = $2, workspace_json = $3
                         WHERE workspace_id = $4",
                        &[&run_id, &now, &workspace_json, &workspace_id],
                    )
                    .map_err(|e| e.to_string())?;
                let details = json!({"run_id": run_id, "product_golden_path": true}).to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, 'supervised_patch.workspace_run_rebind', $3, $4)",
                        &[&now, &actor, &workspace_id, &details],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    /// Restart recovery: re-enter prepare for tasks left in workspace_preparing/admitted.
    pub fn recover_product_task_workspace(
        &self,
        task_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let status = task.get("status").and_then(Value::as_str).unwrap_or("");
        if status == ProductTaskStatus::WorkspaceBound.as_str() {
            return Ok(task);
        }
        if status != ProductTaskStatus::WorkspacePreparing.as_str()
            && status != ProductTaskStatus::Admitted.as_str()
        {
            return Err(format!(
                "product task {task_id} is not recoverable for worktree prepare (status={status})"
            ));
        }
        let intake_value = task
            .get("intake")
            .cloned()
            .ok_or_else(|| "product task missing intake payload".to_string())?;
        let intake = reconstruct_intake_from_task(&task, &intake_value)?;
        if status == ProductTaskStatus::Admitted.as_str() {
            let version = task.get("version").and_then(Value::as_u64).unwrap_or(1);
            self.transition_product_task(
                task_id,
                ProductTaskStatus::WorkspacePreparing,
                Some(version),
                actor,
                None,
                None,
                None,
                None,
                None,
            )?;
        }
        match self.prepare_product_task_worktree(task_id, &intake, actor) {
            Ok(task) => Ok(task),
            Err(error) => {
                let _ = self.fail_product_task_and_compensate(
                    task_id,
                    "worktree_recover_failed",
                    &error,
                    actor,
                );
                Err(error)
            }
        }
    }
}

fn map_product_task_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let intake_json: String = row.get("intake_json")?;
    let binding_json: Option<String> = row.get("workspace_binding_json")?;
    let intake: Value = serde_json::from_str(&intake_json).unwrap_or(Value::Null);
    let binding = binding_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or(Value::Null);
    let approval_required: i64 = row.get("approval_required")?;
    let confirm_execution: i64 = row.get("confirm_execution")?;
    let confirm_output: i64 = row.get("confirm_output")?;
    let status: String = row.get("status")?;
    let admits = ProductTaskStatus::parse(&status)
        .map(|s| s.admits_execution())
        .unwrap_or(false);
    Ok(json!({
        "schema_version": row.get::<_, String>("schema_version")?,
        "task_id": row.get::<_, String>("task_id")?,
        "tenant_id": row.get::<_, String>("tenant_id")?,
        "workspace_id": row.get::<_, String>("workspace_id")?,
        "idempotency_key": row.get::<_, String>("idempotency_key")?,
        "status": status,
        "version": row.get::<_, i64>("version")?,
        "objective_fingerprint": row.get::<_, String>("objective_fingerprint")?,
        "target_id": row.get::<_, String>("target_id")?,
        "target_repo_path": row.get::<_, String>("target_repo_path")?,
        "source_revision": row.get::<_, String>("source_revision")?,
        "source_tree_hash": row.get::<_, Option<String>>("source_tree_hash")?,
        "output_intent": row.get::<_, String>("output_intent")?,
        "risk_class": row.get::<_, String>("risk_class")?,
        "approval_required": approval_required != 0,
        "confirm_execution": confirm_execution != 0,
        "confirm_output": confirm_output != 0,
        "intake_contract_sha256": row.get::<_, String>("intake_contract_sha256")?,
        "intake": intake,
        "workspace_binding": binding,
        "plan_id": row.get::<_, Option<String>>("plan_id")?,
        "run_id": row.get::<_, Option<String>>("run_id")?,
        "workspace_record_id": row.get::<_, Option<String>>("workspace_record_id")?,
        "failure_code": row.get::<_, Option<String>>("failure_code")?,
        "failure_detail": row.get::<_, Option<String>>("failure_detail")?,
        "created_at": row.get::<_, String>("created_at")?,
        "updated_at": row.get::<_, String>("updated_at")?,
        "created_by": row.get::<_, String>("created_by")?,
        "execution_admitted": admits,
    }))
}

fn allocate_task_id(now: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stamp = now.replace([':', '-', 'T', 'Z'], "");
    format!("ptask-{stamp}-{nanos:x}")
}

fn reconstruct_intake_from_task(
    task: &Value,
    intake: &Value,
) -> Result<ValidatedProductTaskIntake, String> {
    use crate::product_golden_path::{
        ProductExecutorPolicy, ProductOutputIntent, ProductTaskBudget, ProductVerificationCommand,
        PRODUCT_TASK_INTAKE_SCHEMA_VERSION,
    };

    let allowed_paths = intake
        .get("allowed_paths")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let verification_commands = intake
        .get("verification_commands")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    Some(ProductVerificationCommand {
                        command: v.get("command")?.as_str()?.to_string(),
                        timeout_ms: v.get("timeout_ms")?.as_u64()?,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let executor_policy: ProductExecutorPolicy = serde_json::from_value(
        intake
            .get("executor_policy")
            .cloned()
            .unwrap_or(json!({"allowed_executors":["deterministic"]})),
    )
    .map_err(|e| e.to_string())?;
    let budget: ProductTaskBudget = serde_json::from_value(
        intake
            .get("budget")
            .cloned()
            .unwrap_or_else(|| serde_json::to_value(ProductTaskBudget::default()).unwrap()),
    )
    .unwrap_or_default();
    let output_intent = ProductOutputIntent::parse(
        task.get("output_intent")
            .and_then(Value::as_str)
            .unwrap_or("artifact_only"),
    )?;

    Ok(ValidatedProductTaskIntake {
        schema_version: PRODUCT_TASK_INTAKE_SCHEMA_VERSION.to_string(),
        objective: intake
            .get("objective_preview")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        objective_fingerprint: task
            .get("objective_fingerprint")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        target_id: task
            .get("target_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        target_repo_path: task
            .get("target_repo_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        source_revision: task
            .get("source_revision")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        source_tree_hash: task
            .get("source_tree_hash")
            .and_then(Value::as_str)
            .map(str::to_string),
        allowed_paths,
        verification_commands,
        output_intent,
        executor_policy,
        budget,
        risk_class: task
            .get("risk_class")
            .and_then(Value::as_str)
            .unwrap_or("low")
            .to_string(),
        approval_required: task
            .get("approval_required")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        confirm_execution: true,
        confirm_output: task
            .get("confirm_output")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        idempotency_key: task
            .get("idempotency_key")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        expected_version: None,
        tenant_id: task
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        workspace_id: task
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        workspace_mode: "git_worktree".to_string(),
        intake_contract_sha256: task
            .get("intake_contract_sha256")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

#[cfg(feature = "pg")]
fn product_task_row_to_json_pg(row: &postgres::Row) -> Value {
    let intake_json: String = row.get("intake_json");
    let binding_json: Option<String> = row.get("workspace_binding_json");
    let intake: Value = serde_json::from_str(&intake_json).unwrap_or(Value::Null);
    let binding: Value = binding_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    let approval_required: i32 = row.get("approval_required");
    let confirm_execution: i32 = row.get("confirm_execution");
    let confirm_output: i32 = row.get("confirm_output");
    let status: String = row.get("status");
    let admits = ProductTaskStatus::parse(&status)
        .map(|s| s.admits_execution())
        .unwrap_or(false);
    json!({
        "schema_version": row.get::<_, String>("schema_version"),
        "task_id": row.get::<_, String>("task_id"),
        "tenant_id": row.get::<_, String>("tenant_id"),
        "workspace_id": row.get::<_, String>("workspace_id"),
        "idempotency_key": row.get::<_, String>("idempotency_key"),
        "status": status,
        "version": row.get::<_, i64>("version"),
        "objective_fingerprint": row.get::<_, String>("objective_fingerprint"),
        "target_id": row.get::<_, String>("target_id"),
        "target_repo_path": row.get::<_, String>("target_repo_path"),
        "source_revision": row.get::<_, String>("source_revision"),
        "source_tree_hash": row.get::<_, Option<String>>("source_tree_hash"),
        "output_intent": row.get::<_, String>("output_intent"),
        "risk_class": row.get::<_, String>("risk_class"),
        "approval_required": approval_required != 0,
        "confirm_execution": confirm_execution != 0,
        "confirm_output": confirm_output != 0,
        "intake_contract_sha256": row.get::<_, String>("intake_contract_sha256"),
        "intake": intake,
        "workspace_binding": binding,
        "plan_id": row.get::<_, Option<String>>("plan_id"),
        "run_id": row.get::<_, Option<String>>("run_id"),
        "workspace_record_id": row.get::<_, Option<String>>("workspace_record_id"),
        "failure_code": row.get::<_, Option<String>>("failure_code"),
        "failure_detail": row.get::<_, Option<String>>("failure_detail"),
        "created_at": row.get::<_, String>("created_at"),
        "updated_at": row.get::<_, String>("updated_at"),
        "created_by": row.get::<_, String>("created_by"),
        "execution_admitted": admits,
    })
}
