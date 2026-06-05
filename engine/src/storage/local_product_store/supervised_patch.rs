use rusqlite::{params, Row};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use super::{append_audit_locked, collect_values, LocalProductStore};

pub const SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION: &str = "supervised_patch_workspace.v1";
pub const SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION: &str = "supervised_patch_artifact.v1";

impl LocalProductStore {
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
        let status = optional_str(request, "status").unwrap_or("requested");
        if !is_valid_workspace_status(status) {
            return Err(format!(
                "invalid supervised patch workspace status: {status}"
            ));
        }

        let boundary = supervised_patch_boundary(target_repo_path, workspace_path)?;
        let target_repo_canonical_path = required_str(&boundary, "target_repo_canonical_path")?;
        let workspace_canonical_path = required_str(&boundary, "workspace_canonical_path")?;

        self.with_conn(|conn| {
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
                "status": status,
                "created_at": created_at,
                "updated_at": created_at,
                "boundary": boundary.clone(),
                "metadata_only": true,
                "execution_authority": "disabled",
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
                    "target_repository_writes": "disabled",
                    "registered_git_worktree": "forbidden",
                    "workspace_directory_creation": "not_performed",
                    "execution_authority": "disabled",
                }),
            )?;
            Ok(workspace)
        })
    }

    pub fn get_supervised_patch_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<Value>, String> {
        self.with_conn(|conn| {
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
        })
    }

    pub fn supervised_patch_workspaces(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
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
        })
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
                "workspace_directory_creation": "not_performed",
                "target_repository_writes": "disabled",
                "registered_git_worktree": "forbidden",
                "git_worktree_add": "forbidden",
                "process_execution": "disabled",
                "provider_calls": "disabled",
                "push_merge_deploy_apply": "disabled",
                "target_repo_canonical_path": target_repo_canonical_path,
                "workspace_canonical_path": workspace_canonical_path,
            })
        });
        validate_imported_workspace_boundary(
            target_repo_path,
            target_repo_canonical_path,
            workspace_path,
            workspace_canonical_path,
            &boundary,
        )?;

        self.with_conn(|conn| {
            let sequence =
                next_sequence(conn, "supervised_patch_workspaces", "workspace_sequence")?;
            let created_at = optional_str(workspace, "created_at")
                .map(str::to_string)
                .unwrap_or_else(|| self.now());
            let updated_at = optional_str(workspace, "updated_at")
                .map(str::to_string)
                .unwrap_or_else(|| created_at.clone());
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
        })
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
        let storage_refs = request
            .get("storage_refs")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let retention_expires_at = optional_str(request, "retention_expires_at");
        let run_id = required_str(&workspace, "run_id")?;
        let plan_id = optional_str(&workspace, "plan_id");
        let target_id = required_str(&workspace, "target_id")?;
        let source_revision = required_str(&workspace, "source_revision")?;

        self.with_conn(|conn| {
            let sequence = next_sequence(conn, "supervised_patch_artifacts", "artifact_sequence")?;
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
                "storage_refs": storage_refs.clone(),
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
                }),
            )?;
            Ok(artifact)
        })
    }

    pub fn get_supervised_patch_artifact(
        &self,
        artifact_id: &str,
    ) -> Result<Option<Value>, String> {
        self.with_conn(|conn| {
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
        })
    }

    pub fn supervised_patch_artifacts(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.with_conn(|conn| {
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
        })
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

        self.with_conn(|conn| {
            let sequence = next_sequence(conn, "supervised_patch_artifacts", "artifact_sequence")?;
            let created_at = optional_str(artifact, "created_at")
                .map(str::to_string)
                .unwrap_or_else(|| self.now());
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
        })
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
        "workspace_directory_creation": "not_performed",
        "target_repository_writes": "disabled",
        "registered_git_worktree": "forbidden",
        "git_worktree_add": "forbidden",
        "process_execution": "disabled",
        "provider_calls": "disabled",
        "push_merge_deploy_apply": "disabled",
        "target_repo_canonical_path": path_string(&target_repo_canonical),
        "workspace_canonical_path": path_string(&workspace_canonical),
    }))
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

fn validate_imported_workspace_boundary(
    target_repo_path: &str,
    target_repo_canonical_path: &str,
    workspace_path: &str,
    workspace_canonical_path: &str,
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
    ensure_optional_string_field(boundary, "workspace_directory_creation", "not_performed")?;
    ensure_optional_string_field(boundary, "target_repository_writes", "disabled")?;
    ensure_optional_string_field(boundary, "registered_git_worktree", "forbidden")?;
    ensure_optional_string_field(boundary, "git_worktree_add", "forbidden")?;
    ensure_optional_string_field(boundary, "process_execution", "disabled")?;
    ensure_optional_string_field(boundary, "provider_calls", "disabled")?;
    ensure_optional_string_field(boundary, "push_merge_deploy_apply", "disabled")
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

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
