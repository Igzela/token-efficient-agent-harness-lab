use rusqlite::{params, Row};
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

pub const SUPERVISED_PATCH_WORKSPACE_SCHEMA_VERSION: &str = "supervised_patch_workspace.v1";
pub const SUPERVISED_PATCH_ARTIFACT_SCHEMA_VERSION: &str = "supervised_patch_artifact.v1";

const DEFAULT_IGNORE_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".next",
    "dist",
    "build",
    "__pycache__",
    ".pytest_cache",
    ".git",
];

const MAX_FILE_BYTES: u64 = 1_048_576; // 1 MB

fn is_ignored_dir(name: &str) -> bool {
    DEFAULT_IGNORE_DIRS.contains(&name)
}

fn is_binary_content(data: &[u8]) -> bool {
    let check_len = data.len().min(8192);
    data[..check_len].contains(&0)
}

impl LocalProductStore {
    pub fn create_workspace_directory(
        &self,
        workspace_id: &str,
        target_repo_path: &str,
    ) -> Result<String, String> {
        let db_dir = self
            .db_path()
            .parent()
            .ok_or_else(|| "store has no parent directory".to_string())?;
        let workspaces_dir = db_dir.join("workspaces");
        std::fs::create_dir_all(&workspaces_dir).map_err(|e| e.to_string())?;
        let workspace_dir = workspaces_dir.join(workspace_id);
        if workspace_dir.exists() {
            return Ok(workspace_dir.to_string_lossy().into_owned());
        }

        let target_canonical =
            std::fs::canonicalize(target_repo_path).map_err(|e| e.to_string())?;
        if workspaces_dir.starts_with(&target_canonical) {
            return Err("workspace directory must be outside target repository".to_string());
        }

        copy_dir_contents(&target_canonical, &workspace_dir)?;

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
                std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
            }
        }
        self.update_workspace_status(workspace_id, "cleaned", actor)
    }

    pub fn quarantine_workspace(&self, workspace_id: &str, actor: &str) -> Result<Value, String> {
        self.update_workspace_status(workspace_id, "quarantined", actor)
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
        changed_files.extend(added.iter().map(|f| format!("+{f}")));
        changed_files.extend(modified.iter().map(|f| format!("~{f}")));
        changed_files.extend(deleted.iter().map(|f| format!("-{f}")));

        if changed_files.is_empty() {
            return Err("no changes detected against source snapshot".to_string());
        }

        let diff_content = format!(
            "added:{}\nmodified:{}\ndeleted:{}",
            added.join(","),
            modified.join(","),
            deleted.join(",")
        );
        let hash_bytes = sha256_bytes(diff_content.as_bytes());
        let patch_hash = format!("sha256:{}", hex_encode(&hash_bytes));

        let secret_findings = scan_for_secrets(path)?;
        let redaction_status = if secret_findings.is_empty() {
            "redacted"
        } else {
            "failed"
        };

        let review_diff = generate_review_diff(path, &added, &modified, &deleted);

        let artifact_request = json!({
            "workspace_id": workspace_id,
            "patch_hash": patch_hash,
            "changed_files": changed_files,
            "redaction_status": redaction_status,
            "review_diff": review_diff,
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
        if workspace_exists {
            let manifest_path = Path::new(workspace_path).join(".source_manifest.json");
            if manifest_path.exists() {
                let manifest_content = std::fs::read_to_string(&manifest_path).unwrap_or_default();
                let manifest: Value =
                    serde_json::from_str(&manifest_content).unwrap_or(Value::Null);
                if manifest.is_object() {
                    let (added, modified, deleted) =
                        diff_against_manifest(Path::new(workspace_path), &manifest)?;
                    let diff_content = format!(
                        "added:{}\nmodified:{}\ndeleted:{}",
                        added.join(","),
                        modified.join(","),
                        deleted.join(",")
                    );
                    let hash_bytes = sha256_bytes(diff_content.as_bytes());
                    current_hash = format!("sha256:{}", hex_encode(&hash_bytes));
                }
            }
        }
        let hash_unchanged = current_hash.is_empty() || current_hash == patch_hash;
        checks.push(json!({
            "check": "patch_hash_unchanged",
            "passed": hash_unchanged,
            "message": if hash_unchanged { "ok".to_string() } else { format!("hash changed: recorded={} current={}", patch_hash, current_hash) }
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
        let status = optional_str(request, "status").unwrap_or("requested");
        if !is_valid_workspace_status(status) {
            return Err(format!(
                "invalid supervised patch workspace status: {status}"
            ));
        }

        let boundary = supervised_patch_boundary(target_repo_path, workspace_path)?;
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
                    "status": status,
                    "created_at": created_at,
                    "updated_at": created_at,
                    "boundary": boundary.clone(),
                    "metadata_only": true,
                    "execution_authority": "disabled",
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
                    "target_repository_writes": "disabled",
                    "registered_git_worktree": "forbidden",
                    "workspace_directory_creation": "not_performed",
                    "execution_authority": "disabled",
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
        let review_diff = request
            .get("review_diff")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let storage_refs = request
            .get("storage_refs")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let retention_expires_at = optional_str(request, "retention_expires_at");
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
                    "review_diff": review_diff,
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
                    "review_diff": review_diff,
                    "storage_refs": storage_refs.clone(),
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

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn compute_manifest(dir: &Path) -> Result<Value, String> {
    let (files, hashes) = collect_workspace_files(dir)?;
    let entries: Vec<Value> = files
        .iter()
        .zip(hashes.iter())
        .map(|(f, h)| json!({"path": f, "hash": h}))
        .collect();
    Ok(json!({
        "files": entries,
        "computed_at": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    }))
}

#[allow(clippy::type_complexity)]
fn diff_against_manifest(
    workspace_dir: &Path,
    manifest: &Value,
) -> Result<(Vec<String>, Vec<String>, Vec<String>), String> {
    let (current_files, current_hashes) = collect_workspace_files(workspace_dir)?;
    let manifest_entries = manifest
        .get("files")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut source_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for entry in &manifest_entries {
        if let (Some(path), Some(hash)) = (
            entry.get("path").and_then(Value::as_str),
            entry.get("hash").and_then(Value::as_str),
        ) {
            source_map.insert(path.to_string(), hash.to_string());
        }
    }

    let mut current_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for (path, hash) in current_files.iter().zip(current_hashes.iter()) {
        current_map.insert(path.clone(), hash.clone());
    }

    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    for (path, hash) in &current_map {
        match source_map.get(path) {
            Some(source_hash) => {
                if source_hash != hash {
                    modified.push(path.clone());
                }
            }
            None => added.push(path.clone()),
        }
    }
    for path in source_map.keys() {
        if !current_map.contains_key(path) {
            deleted.push(path.clone());
        }
    }

    added.sort();
    modified.sort();
    deleted.sort();
    Ok((added, modified, deleted))
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let entries = std::fs::read_dir(src).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        let target = dst.join(&name);
        if path.is_dir() {
            copy_dir_contents(&path, &target)?;
        } else if path.is_file() {
            std::fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn collect_workspace_files(dir: &Path) -> Result<(Vec<String>, Vec<String>), String> {
    let mut pairs = Vec::new();
    collect_files_recursive(dir, dir, &mut pairs)?;
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let files = pairs.iter().map(|(p, _)| p.clone()).collect();
    let hashes = pairs.iter().map(|(_, h)| h.clone()).collect();
    Ok((files, hashes))
}

fn collect_files_recursive(
    base: &Path,
    dir: &Path,
    pairs: &mut Vec<(String, String)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') || is_ignored_dir(&name) {
                continue;
            }
            collect_files_recursive(base, &path, pairs)?;
        } else if path.is_file() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
            if meta.len() > MAX_FILE_BYTES {
                continue;
            }
            let content = std::fs::read(&path).map_err(|e| e.to_string())?;
            if is_binary_content(&content) {
                continue;
            }
            let relative = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let hash = hex_encode(&sha256_bytes(&content));
            pairs.push((relative, hash));
        }
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn generate_review_diff(
    workspace_dir: &Path,
    added: &[String],
    modified: &[String],
    deleted: &[String],
) -> String {
    let mut diff = String::new();

    for path in added {
        let full = workspace_dir.join(path);
        let content = match std::fs::read_to_string(&full) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let line_count = content.lines().count();
        diff.push_str(&format!(
            "--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{line_count} @@\n"
        ));
        for line in content.lines() {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
    }

    for path in modified {
        let full = workspace_dir.join(path);
        let content = match std::fs::read_to_string(&full) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let line_count = content.lines().count();
        diff.push_str(&format!(
            "--- a/{path}\n+++ b/{path}\n@@ -1,{line_count} +1,{line_count} @@\n"
        ));
        for line in content.lines() {
            diff.push(' ');
            diff.push_str(line);
            diff.push('\n');
        }
    }

    for path in deleted {
        diff.push_str(&format!("--- a/{path}\n+++ /dev/null\n@@ -1,0 +0,0 @@\n"));
        diff.push_str(&format!("(deleted: {path})\n"));
    }

    diff
}

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256Writer::new();
    hasher.write(data);
    hasher.finalize()
}

struct Sha256Writer {
    state: [u32; 8],
    buffer: Vec<u8>,
    total_len: u64,
}

impl Sha256Writer {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: Vec::new(),
            total_len: 0,
        }
    }

    fn write(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.total_len += data.len() as u64;
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.buffer.drain(..64);
            self.process_block(&block);
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_len * 8;
        self.buffer.push(0x80);
        while (self.buffer.len() % 64) != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.buffer.drain(..64);
            self.process_block(&block);
        }
        let mut result = [0u8; 32];
        for (i, &word) in self.state.iter().enumerate() {
            result[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
        }
        result
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

fn scan_for_secrets(dir: &Path) -> Result<Vec<String>, String> {
    let mut findings = Vec::new();
    scan_recursive(dir, &mut findings)?;
    Ok(findings)
}

fn scan_recursive(dir: &Path, findings: &mut Vec<String>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if !name.starts_with('.') {
                scan_recursive(&path, findings)?;
            }
        } else if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines() {
                    let lower = line.to_lowercase();
                    if lower.contains("api_key")
                        || lower.contains("api-key")
                        || lower.contains("secret_key")
                        || lower.contains("password")
                        || lower.contains("bearer ")
                        || lower.contains("private_key")
                    {
                        let relative = path.file_name().unwrap_or_default().to_string_lossy();
                        findings.push(format!("{}: {}", relative, line.trim()));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collect_files_recursive_skips_dotfiles() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("visible.txt"), "content").unwrap();
        fs::write(root.join(".hidden.txt"), "secret").unwrap();
        fs::write(root.join(".source_manifest.json"), "{}").unwrap();

        let mut pairs = Vec::new();
        collect_files_recursive(root, root, &mut pairs).unwrap();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let files: Vec<String> = pairs.iter().map(|(p, _)| p.clone()).collect();

        assert_eq!(files, vec!["visible.txt"]);
        assert!(!files.iter().any(|f| f.contains(".source_manifest")));
        assert!(!files.iter().any(|f| f.starts_with('.')));
    }

    #[test]
    fn collect_files_recursive_skips_dot_directories() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("top.txt"), "content").unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "stuff").unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub/nested.txt"), "data").unwrap();

        let mut pairs = Vec::new();
        collect_files_recursive(root, root, &mut pairs).unwrap();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let files: Vec<String> = pairs.iter().map(|(p, _)| p.clone()).collect();

        assert_eq!(files, vec!["sub/nested.txt", "top.txt"]);
    }

    #[test]
    fn collect_files_recursive_no_dotfiles_in_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("real_patch.rs"), "fn main() {}").unwrap();
        fs::write(root.join(".source_manifest.json"), r#"{"files":[]}"#).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/index"), "binary").unwrap();

        let mut pairs = Vec::new();
        collect_files_recursive(root, root, &mut pairs).unwrap();
        let files: Vec<String> = pairs.iter().map(|(p, _)| p.clone()).collect();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "real_patch.rs");
        assert!(!files.iter().any(|f| f.starts_with('.')));
    }

    #[test]
    fn collect_workspace_files_path_hash_alignment() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Write multiple files with known content so we can verify hash alignment
        fs::write(root.join("aaa.txt"), "alpha").unwrap();
        fs::write(root.join("bbb.txt"), "beta").unwrap();
        fs::write(root.join("ccc.txt"), "gamma").unwrap();

        let (files, hashes) = collect_workspace_files(root).unwrap();

        // Files must be sorted
        assert_eq!(files, vec!["aaa.txt", "bbb.txt", "ccc.txt"]);
        // Each hash must correspond to its file's content
        let expected_aaa = hex_encode(&sha256_bytes(b"alpha"));
        let expected_bbb = hex_encode(&sha256_bytes(b"beta"));
        let expected_ccc = hex_encode(&sha256_bytes(b"gamma"));
        assert_eq!(hashes[0], expected_aaa, "hash mismatch for aaa.txt");
        assert_eq!(hashes[1], expected_bbb, "hash mismatch for bbb.txt");
        assert_eq!(hashes[2], expected_ccc, "hash mismatch for ccc.txt");
    }
}
