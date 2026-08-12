//! Canonical product-task persistence and worktree-first intake orchestration (G1).

use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::fs::OpenOptions;
#[cfg(unix)]
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::node_executor::{
    CommandNodeExecutor, NodeExecutionInput, NodeExecutionOutput, NodeExecutor, ProcessOutcome,
};
use crate::product_golden_path::{
    compile_product_executable_graph, fingerprint_objective, is_valid_product_task_transition,
    planned_workspace_path, product_gate_enabled, provisional_run_id_for_task,
    redacted_intake_json, resolve_admitted_executor, validate_source_revision_format,
    workspace_content_hash, ProductExecutorPolicy, ProductTaskStatus,
    ProductVerificationRuntimeAuthority, ProductWorkspaceBinding, ValidatedProductTaskIntake,
    FIXTURE_DETERMINISTIC_APPLY_FILENAME, FIXTURE_DETERMINISTIC_APPLY_SCHEMA,
    FIXTURE_DETERMINISTIC_NOTE_CONTENT, PRODUCT_EXECUTABLE_GRAPH_SCHEMA_VERSION,
    PRODUCT_TASK_SCHEMA_VERSION, PRODUCT_TASK_WORKSPACE_BINDING_SCHEMA_VERSION,
    PRODUCT_VERIFICATION_READ_ONLY_COMMANDS,
};
use crate::read_only_planner::{ReadOnlyPlanner, READ_ONLY_PLAN_SCHEMA_VERSION};
use crate::target_repo_output::{
    current_workspace_revision, inspect_git_patch_read_only, inspect_registered_git_worktree,
    patch_hash as target_patch_hash, prepare_git_worktree, remove_git_worktree_and_verify_absent,
    TargetRepoOutputConfig, GIT_WORKTREE_ADD_OUTCOME_UNKNOWN,
};
use crate::tool_policy_executor::{managed_tool_binding_sha256, ToolPolicyNodeExecutor};

use super::managed_acceptance::{derive_final_execution_manifest, DelegationContract};
use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

pub(crate) type ProductArtifactCommitResult = Result<(Value, Value), String>;

pub(crate) trait ProductArtifactCommitAuthority {
    fn commit(
        &self,
        operation: &mut dyn FnMut() -> ProductArtifactCommitResult,
    ) -> ProductArtifactCommitResult;
}

impl<F> ProductArtifactCommitAuthority for F
where
    F: Fn(&mut dyn FnMut() -> ProductArtifactCommitResult) -> ProductArtifactCommitResult,
{
    fn commit(
        &self,
        operation: &mut dyn FnMut() -> ProductArtifactCommitResult,
    ) -> ProductArtifactCommitResult {
        self(operation)
    }
}

const PRODUCT_TASK_SELECT: &str = "SELECT schema_version, task_id, tenant_id, workspace_id,
    idempotency_key, status, version, objective_fingerprint, target_id, target_repo_path,
    source_revision, source_tree_hash, output_intent, risk_class, approval_required,
    confirm_execution, confirm_output, intake_contract_sha256, intake_json,
    workspace_binding_json, plan_id, run_id, workspace_record_id, failure_code,
    failure_detail, created_at, updated_at, created_by
 FROM product_tasks";

// A duplicate intake must never race the worker that won the durable
// `admitted -> workspace_preparing` transition into the physical git worktree
// operation.  The bounded wait gives the transition owner time to publish its
// terminal workspace state while retaining explicit recovery as the owner of
// interrupted preparation.
const PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_LIMIT: usize = 100;
const PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_DELAY: Duration = Duration::from_millis(20);
const PRODUCT_TASK_WORKSPACE_PREPARATION_ACTIVE: &str =
    "product task workspace preparation is active";
const PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED: &str =
    "product task workspace preparation requires reconciliation";
const PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE: &str =
    "product task workspace preparation precondition is unavailable";
const PRODUCT_TASK_WORKSPACE_PREPARATION_RECEIPT_SCHEMA_VERSION: &str =
    "product_task_workspace_preparation.v1";
const DELEGATED_PRODUCT_PLAN_OWNER_SCHEMA_VERSION: &str = "delegated_product_plan_owner.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct DelegatedProposalTargetIdentity {
    repository: String,
    main_sha: String,
    mutable_paths: Vec<String>,
}

fn normalize_delegated_proposal_target_identity(
    proposal: &Value,
) -> Result<DelegatedProposalTargetIdentity, String> {
    let (repository, main_sha, mutable_paths) =
        match proposal.get("schema_version").and_then(Value::as_str) {
            Some("managed_proposal_manifest.v1") => (
                proposal.get("target_repository"),
                proposal.get("target_main_sha"),
                proposal.get("mutable_paths"),
            ),
            Some("pe7_product_golden_path_live_seal_manifest.v1") => (
                proposal.pointer("/target/repository"),
                proposal.pointer("/target/default_branch_sha"),
                proposal.pointer("/target/allowed_mutable_paths"),
            ),
            _ => return Err("delegated proposal schema is not admitted".into()),
        };
    let repository = repository
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("delegated proposal target repository is missing")?
        .to_string();
    let main_sha = main_sha
        .and_then(Value::as_str)
        .filter(|value| validate_source_revision_format(value).is_ok())
        .ok_or("delegated proposal target main SHA is missing or malformed")?
        .to_string();
    let mutable_paths = mutable_paths
        .and_then(Value::as_array)
        .ok_or("delegated proposal mutable paths are missing")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|path| !path.is_empty())
                .map(str::to_string)
                .ok_or_else(|| "delegated proposal mutable path is malformed".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if mutable_paths.is_empty() {
        return Err("delegated proposal mutable paths are empty".into());
    }
    Ok(DelegatedProposalTargetIdentity {
        repository,
        main_sha,
        mutable_paths,
    })
}

fn delegated_product_plan_owner_id(task_id: &str) -> String {
    hex::encode(Sha256::digest(
        format!("{DELEGATED_PRODUCT_PLAN_OWNER_SCHEMA_VERSION}:{task_id}").as_bytes(),
    ))
}

fn count_unified_diff_changed_lines(review_diff: &str) -> u64 {
    let mut in_hunk = false;
    let mut changed_lines = 0_u64;
    for line in review_diff.lines() {
        if line.starts_with("diff --git ") {
            in_hunk = false;
        } else if line.starts_with("@@ ") {
            in_hunk = true;
        } else if in_hunk && (line.starts_with('+') || line.starts_with('-')) {
            changed_lines = changed_lines.saturating_add(1);
        }
    }
    changed_lines
}

fn product_task_workspace_preparation_reconciliation_error(detail: impl AsRef<str>) -> String {
    format!(
        "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: {}",
        detail.as_ref()
    )
}

/// Remove only the exact receipt-owned Git worktree and prove that both its
/// registration and path are absent. A Git timeout can happen after the child
/// mutates worktree metadata, so an unsuccessful or ambiguous removal is
/// reconciliation rather than terminal compensation.
fn remove_product_task_git_worktree_or_reconcile(
    config: &TargetRepoOutputConfig,
    target_repo_path: &Path,
    workspace_path: &Path,
) -> Result<(), String> {
    remove_git_worktree_and_verify_absent(config, target_repo_path, workspace_path).map_err(|_| {
        product_task_workspace_preparation_reconciliation_error(
            "pinned git worktree removal is unavailable",
        )
    })
}

/// Remove only an exact receipt-owned local-folder staging directory.  Unlike
/// a Git worktree this path has no external metadata to reconcile, but it is
/// still never removed by a broad cleanup: it must be the deterministic child
/// of the receipt root, be a real directory (not a replacement symlink), and
/// be absent after removal.  Anything ambiguous is retained for operator
/// reconciliation rather than risking a path-following delete.
fn remove_product_task_local_folder_stage_or_reconcile(
    receipt: &ProductTaskWorkspacePreparationReceipt,
    task_id: &str,
) -> Result<(), String> {
    let expected = receipt
        .workspace_root
        .join(product_task_workspace_fs_id(task_id));
    if receipt.workspace_path != expected {
        return Err(product_task_workspace_preparation_reconciliation_error(
            "local-folder staging receipt path is invalid",
        ));
    }
    match std::fs::symlink_metadata(&receipt.workspace_path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(product_task_workspace_preparation_reconciliation_error(
                    "local-folder staging path is not an app-owned directory",
                ));
            }
            let canonical = std::fs::canonicalize(&receipt.workspace_path).map_err(|_| {
                product_task_workspace_preparation_reconciliation_error(
                    "local-folder staging identity is unavailable",
                )
            })?;
            if canonical != receipt.workspace_path {
                return Err(product_task_workspace_preparation_reconciliation_error(
                    "local-folder staging path drifted",
                ));
            }
            std::fs::remove_dir_all(&receipt.workspace_path).map_err(|_| {
                product_task_workspace_preparation_reconciliation_error(
                    "local-folder staging cleanup is unavailable",
                )
            })?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(product_task_workspace_preparation_reconciliation_error(
                "local-folder staging identity is unavailable",
            ));
        }
    }
    if receipt.workspace_path.exists() {
        return Err(product_task_workspace_preparation_reconciliation_error(
            "local-folder staging remains after cleanup",
        ));
    }
    Ok(())
}

fn remove_product_task_workspace_or_reconcile(
    intake: &ValidatedProductTaskIntake,
    receipt: &ProductTaskWorkspacePreparationReceipt,
    target_canonical: &Path,
    task_id: &str,
) -> Result<(), String> {
    if intake.workspace_mode == "local_folder" {
        // The local source was captured through the safe manifest boundary
        // above. Its canonical path must remain the exact supervised-workspace
        // target before the staging copy can be cleaned.
        if target_canonical != Path::new(&intake.target_repo_path) {
            return Err(product_task_workspace_preparation_reconciliation_error(
                "local-folder source path drifted",
            ));
        }
        return remove_product_task_local_folder_stage_or_reconcile(receipt, task_id);
    }
    let config = TargetRepoOutputConfig::from_env();
    remove_product_task_git_worktree_or_reconcile(
        &config,
        target_canonical,
        &receipt.workspace_path,
    )
}

fn classify_product_task_git_worktree_prepare_error(error: String) -> String {
    if error.starts_with(GIT_WORKTREE_ADD_OUTCOME_UNKNOWN) {
        product_task_workspace_preparation_reconciliation_error(
            "git worktree creation outcome is unknown",
        )
    } else {
        format!("{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: {error}")
    }
}

/// App-owned, per-worktree exclusion for the physical `git worktree` mutation.
///
/// ProductTask state remains the sole durable authority. This lock only keeps
/// an active admit or explicit recovery from concurrently mutating git's
/// worktree metadata for the same deterministic app-owned path. It is held by
/// the file descriptor and therefore releases if its process exits.
#[cfg(unix)]
struct ProductTaskWorkspacePreparationLock {
    file: std::fs::File,
}

#[cfg(unix)]
impl ProductTaskWorkspacePreparationLock {
    fn acquire<F>(workspace_path: &Path, on_contention: F) -> Result<Self, String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let workspace_root = workspace_path.parent().ok_or_else(|| {
            product_task_workspace_preparation_reconciliation_error(
                "workspace preparation lock root is unavailable",
            )
        })?;
        let workspace_name = workspace_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                product_task_workspace_preparation_reconciliation_error(
                    "workspace preparation lock name is unavailable",
                )
            })?;
        if !workspace_root.is_dir() {
            return Err(product_task_workspace_preparation_reconciliation_error(
                "workspace root is unavailable",
            ));
        }
        let lock_path = workspace_root.join(format!(".{workspace_name}.prepare.lock"));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(lock_path)
            .map_err(|_| {
                product_task_workspace_preparation_reconciliation_error(
                    "workspace preparation lock is unavailable",
                )
            })?;

        loop {
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result == 0 {
                return Ok(Self { file });
            }
            match std::io::Error::last_os_error().kind() {
                std::io::ErrorKind::Interrupted => continue,
                std::io::ErrorKind::WouldBlock => break,
                _ => {
                    return Err(product_task_workspace_preparation_reconciliation_error(
                        "workspace preparation lock is unavailable",
                    ));
                }
            }
        }

        // This event contains only the ProductTask identity and makes an
        // already-observed physical-worktree contention auditable. It does
        // not grant authority or persist a path, command, prompt, or output.
        // Never block an HTTP handler behind an unbounded flock waiter: the
        // caller owns the bounded retry/backoff policy.
        on_contention().map_err(|error| {
            product_task_workspace_preparation_reconciliation_error(format!(
                "workspace preparation contention audit failed: {error}"
            ))
        })?;
        Err(PRODUCT_TASK_WORKSPACE_PREPARATION_ACTIVE.to_string())
    }
}

#[cfg(unix)]
impl Drop for ProductTaskWorkspacePreparationLock {
    fn drop(&mut self) {
        let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// PostgreSQL's best-effort active-owner coordinator. It is intentionally
/// try-only: a contending HTTP request must not consume an unpooled database
/// session while a slow physical operation owns the lock. ProductTask's
/// durable preparation receipt and the pinned local filesystem remain the
/// recovery boundary; this session lock is not a distributed fencing lease.
#[cfg(feature = "pg")]
struct ProductTaskPostgresWorkspacePreparationLock {
    client: postgres::Client,
    key: String,
}

#[cfg(feature = "pg")]
impl ProductTaskPostgresWorkspacePreparationLock {
    fn acquire<F>(
        store: &LocalProductStore,
        task_id: &str,
        mut on_contention: F,
    ) -> Result<Self, String>
    where
        F: FnMut() -> Result<(), String>,
    {
        let key = format!("product_task.workspace_prepare:{task_id}");
        let database_url = store.db_path().to_string_lossy();
        let mut client = postgres::Client::connect(database_url.as_ref(), postgres::NoTls)
            .map_err(|_| {
                product_task_workspace_preparation_reconciliation_error(
                    "product task PostgreSQL preparation lock is unavailable",
                )
            })?;
        let acquired = client
            .query_one(
                "SELECT pg_try_advisory_lock(hashtextextended($1, 0))",
                &[&key],
            )
            .map_err(|_| {
                product_task_workspace_preparation_reconciliation_error(
                    "product task PostgreSQL preparation lock is unavailable",
                )
            })?
            .get::<_, bool>(0);
        if !acquired {
            on_contention().map_err(|error| {
                product_task_workspace_preparation_reconciliation_error(format!(
                    "workspace preparation contention audit failed: {error}"
                ))
            })?;
            return Err(PRODUCT_TASK_WORKSPACE_PREPARATION_ACTIVE.to_string());
        }
        Ok(Self { client, key })
    }
}

#[cfg(feature = "pg")]
impl Drop for ProductTaskPostgresWorkspacePreparationLock {
    fn drop(&mut self) {
        let _ = self.client.execute(
            "SELECT pg_advisory_unlock(hashtextextended($1, 0))",
            &[&self.key],
        );
    }
}

#[cfg(not(unix))]
struct ProductTaskWorkspacePreparationLock;

#[cfg(not(unix))]
impl ProductTaskWorkspacePreparationLock {
    fn acquire<F>(_workspace_path: &Path, _on_contention: F) -> Result<Self, String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        Err(product_task_workspace_preparation_reconciliation_error(
            "workspace preparation lock is unavailable on this platform",
        ))
    }
}

/// Composes the local physical-worktree exclusion with PostgreSQL's active
/// coordinator when applicable. ProductTask remains the sole durable lifecycle
/// owner; both guards are ephemeral synchronization only.
struct ProductTaskWorkspacePreparationGuard {
    // Drop the local physical guard before the PostgreSQL active-owner
    // coordinator. The session guard is not a distributed fencing lease and
    // never proves that independently hosted filesystems share this path.
    _filesystem_lock: ProductTaskWorkspacePreparationLock,
    #[cfg(feature = "pg")]
    _postgres_lock: Option<ProductTaskPostgresWorkspacePreparationLock>,
}

impl ProductTaskWorkspacePreparationGuard {
    fn acquire<F>(
        store: &LocalProductStore,
        task_id: &str,
        workspace_path: &Path,
        mut on_contention: F,
    ) -> Result<Self, String>
    where
        F: FnMut() -> Result<(), String>,
    {
        #[cfg(not(feature = "pg"))]
        let _ = (store, task_id);
        let mut contention_recorded = false;
        #[cfg(feature = "pg")]
        let postgres_lock = match &store.db {
            DatabaseConnection::Pg(_) => Some(
                ProductTaskPostgresWorkspacePreparationLock::acquire(store, task_id, || {
                    if !contention_recorded {
                        on_contention()?;
                        contention_recorded = true;
                    }
                    Ok(())
                })?,
            ),
            DatabaseConnection::Sqlite(_) => None,
        };

        let filesystem_lock = ProductTaskWorkspacePreparationLock::acquire(workspace_path, || {
            if !contention_recorded {
                on_contention()?;
                contention_recorded = true;
            }
            Ok(())
        })?;
        Ok(Self {
            _filesystem_lock: filesystem_lock,
            #[cfg(feature = "pg")]
            _postgres_lock: postgres_lock,
        })
    }
}

fn product_task_has_prepared_workspace(task: &Value) -> bool {
    matches!(
        task.get("status").and_then(Value::as_str).unwrap_or(""),
        "workspace_bound"
            | "graph_ready"
            | "running"
            | "verifying"
            | "repair_pending"
            | "awaiting_approval"
            | "output_pending"
            | "completed"
            | "paused"
    )
}

fn product_task_has_terminal_workspace_prepare_state(task: &Value) -> bool {
    matches!(
        task.get("status").and_then(Value::as_str).unwrap_or(""),
        "failed" | "blocked" | "killed" | "budget_exhausted" | "outcome_unknown"
    )
}

fn is_retryable_product_task_worktree_prepare_error(error: &str) -> bool {
    error == PRODUCT_TASK_WORKSPACE_PREPARATION_ACTIVE
        || error == "product task expected-current update conflict"
        || error.starts_with("stale product task version: current=")
}

fn product_task_workspace_fs_id(task_id: &str) -> String {
    format!(
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
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProductTaskWorkspacePreparationMarkerState {
    Planned,
    MarkerReady,
}

impl ProductTaskWorkspacePreparationMarkerState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::MarkerReady => "marker_ready",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "planned" => Ok(Self::Planned),
            "marker_ready" => Ok(Self::MarkerReady),
            _ => Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt state is invalid"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProductTaskWorkspacePreparationReceipt {
    workspace_root: PathBuf,
    workspace_path: PathBuf,
    marker_sha256: String,
    marker_state: ProductTaskWorkspacePreparationMarkerState,
    receipt_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SqliteWorkspacePreparationObservation {
    Existing(ProductTaskWorkspacePreparationReceipt),
    Admitted { version: i64 },
}

fn observe_sqlite_workspace_preparation(
    tx: &rusqlite::Transaction<'_>,
    task_id: &str,
) -> Result<SqliteWorkspacePreparationObservation, String> {
    let existing = tx
        .query_row(
            "SELECT workspace_root, workspace_path, marker_sha256, marker_state,
                    receipt_sha256
             FROM product_task_workspace_preparations WHERE task_id=?1",
            params![task_id],
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
        .map_err(|error| error.to_string())?;
    if let Some((root, path, marker, state, hash)) = existing {
        return ProductTaskWorkspacePreparationReceipt::from_persisted(
            task_id, root, path, marker, state, hash,
        )
        .map(SqliteWorkspacePreparationObservation::Existing);
    }
    let task = tx
        .query_row(
            "SELECT status, version FROM product_tasks WHERE task_id=?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (status, version): (String, i64) =
        task.ok_or_else(|| "product task missing before workspace preparation".to_string())?;
    if status == ProductTaskStatus::WorkspacePreparing.as_str() {
        return Err(format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: legacy preparing task has no receipt"
        ));
    }
    if status != ProductTaskStatus::Admitted.as_str() {
        return Err("product task is not eligible for workspace preparation".to_string());
    }
    Ok(SqliteWorkspacePreparationObservation::Admitted { version })
}

impl ProductTaskWorkspacePreparationReceipt {
    fn planned(task_id: &str, workspace_root: PathBuf) -> Result<Self, String> {
        let workspace_fs_id = product_task_workspace_fs_id(task_id);
        let workspace_path = workspace_root.join(&workspace_fs_id);
        let marker_sha256 = hex::encode(Sha256::digest(
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECEIPT_SCHEMA_VERSION}:{task_id}:{}",
                uuid::Uuid::new_v4()
            )
            .as_bytes(),
        ));
        let marker_state = ProductTaskWorkspacePreparationMarkerState::Planned;
        let receipt_sha256 = product_task_workspace_preparation_receipt_sha256(
            task_id,
            &workspace_root,
            &workspace_path,
            &marker_sha256,
            marker_state,
        )?;
        Ok(Self {
            workspace_root,
            workspace_path,
            marker_sha256,
            marker_state,
            receipt_sha256,
        })
    }

    fn from_persisted(
        task_id: &str,
        workspace_root: String,
        workspace_path: String,
        marker_sha256: String,
        marker_state: String,
        receipt_sha256: String,
    ) -> Result<Self, String> {
        let workspace_root = PathBuf::from(workspace_root);
        let workspace_path = PathBuf::from(workspace_path);
        let marker_state = ProductTaskWorkspacePreparationMarkerState::parse(&marker_state)?;
        let workspace_fs_id = product_task_workspace_fs_id(task_id);
        if !workspace_root.is_absolute()
            || !workspace_path.is_absolute()
            || workspace_path.parent() != Some(workspace_root.as_path())
            || workspace_path.file_name().and_then(|name| name.to_str()) != Some(&workspace_fs_id)
            || marker_sha256.len() != 64
            || !marker_sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt identity is invalid"
            ));
        }
        let expected_receipt_sha256 = product_task_workspace_preparation_receipt_sha256(
            task_id,
            &workspace_root,
            &workspace_path,
            &marker_sha256,
            marker_state,
        )?;
        if receipt_sha256 != expected_receipt_sha256 {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt hash is invalid"
            ));
        }
        Ok(Self {
            workspace_root,
            workspace_path,
            marker_sha256,
            marker_state,
            receipt_sha256,
        })
    }

    fn with_marker_state(
        &self,
        task_id: &str,
        marker_state: ProductTaskWorkspacePreparationMarkerState,
    ) -> Result<Self, String> {
        Ok(Self {
            workspace_root: self.workspace_root.clone(),
            workspace_path: self.workspace_path.clone(),
            marker_sha256: self.marker_sha256.clone(),
            marker_state,
            receipt_sha256: product_task_workspace_preparation_receipt_sha256(
                task_id,
                &self.workspace_root,
                &self.workspace_path,
                &self.marker_sha256,
                marker_state,
            )?,
        })
    }

    fn marker_path(&self, task_id: &str) -> Result<PathBuf, String> {
        let workspace_fs_id = product_task_workspace_fs_id(task_id);
        if self
            .workspace_path
            .file_name()
            .and_then(|name| name.to_str())
            != Some(workspace_fs_id.as_str())
        {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt workspace identity is invalid"
            ));
        }
        Ok(self
            .workspace_root
            .join(format!(".{workspace_fs_id}.prepare.marker")))
    }
}

fn product_task_workspace_preparation_receipt_sha256(
    task_id: &str,
    workspace_root: &Path,
    workspace_path: &Path,
    marker_sha256: &str,
    marker_state: ProductTaskWorkspacePreparationMarkerState,
) -> Result<String, String> {
    let value = json!({
        "schema_version": PRODUCT_TASK_WORKSPACE_PREPARATION_RECEIPT_SCHEMA_VERSION,
        "task_id": task_id,
        "workspace_root": workspace_root,
        "workspace_path": workspace_path,
        "marker_sha256": marker_sha256,
        "marker_state": marker_state.as_str(),
    });
    let encoded = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProductVerificationNodeAuthority {
    node_id: String,
    attempt_count: u64,
    leased_at: Option<String>,
    result_sha256: String,
}

impl LocalProductStore {
    /// Resolve ProductTask ownership from the durable run owner before a CLI
    /// executor decides whether a Codex process may use the generic path.  The
    /// scheduler node's metadata is not authority: any run owned by a
    /// ProductTask must enter the store-owned managed-Codex boundary.
    pub(crate) fn product_task_id_for_run(&self, run_id: &str) -> Result<Option<String>, String> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return Err("ProductTask run_id is missing".to_string());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut statement = conn
                    .prepare(
                        "SELECT task_id FROM product_tasks WHERE run_id=?1 ORDER BY task_id ASC",
                    )
                    .map_err(|error| error.to_string())?;
                let ids = statement
                    .query_map([run_id], |row| row.get::<_, String>(0))
                    .map_err(|error| error.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?;
                match ids.as_slice() {
                    [] => Ok(None),
                    [task_id] => Ok(Some(task_id.clone())),
                    _ => Err("multiple ProductTasks claim one workflow run".to_string()),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let ids = client
                    .query(
                        "SELECT task_id FROM product_tasks WHERE run_id=$1 ORDER BY task_id ASC",
                        &[&run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .into_iter()
                    .map(|row| row.get::<_, String>(0))
                    .collect::<Vec<_>>();
                match ids.as_slice() {
                    [] => Ok(None),
                    [task_id] => Ok(Some(task_id.clone())),
                    _ => Err("multiple ProductTasks claim one workflow run".to_string()),
                }
            }),
        }
    }

    /// Prove that the authoritative ProductTask receipt owners are readable.
    /// This is deliberately a real read path (not a schema-name or field
    /// existence check), and every backend/read error propagates to the caller.
    pub fn probe_managed_acceptance_product_receipt_owners(&self) -> Result<(), String> {
        const OWNER_TABLES: &[&str] = &[
            "product_task_terminal_evidence",
            "supervised_patch_workspaces",
            "supervised_patch_artifacts",
            "workflow_run_approvals",
            "workflow_runs",
        ];
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                for table in OWNER_TABLES {
                    // Read a result from each owner rather than merely preparing
                    // an empty query. SQLite can defer schema errors until a
                    // statement steps, which would otherwise turn an unreadable
                    // owner into a false positive.
                    let _: i64 = conn
                        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                            row.get(0)
                        })
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                for table in OWNER_TABLES {
                    client
                        .query_one(&format!("SELECT COUNT(*) FROM {table}"), &[])
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            }),
        }
    }

    /// Read the ProductTask run and its exact task node without passing through
    /// the compatibility workflow projection.  That projection intentionally
    /// tolerates historical malformed records for observability; authority
    /// admission must instead reject an unreadable owner at the read boundary.
    fn managed_acceptance_product_run_node_owner(
        &self,
        run_id: &str,
        product_task_id: &str,
    ) -> Result<(Value, Value), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let (workflow_id, run_json, boundaries_json): (String, String, String) = conn
                    .query_row(
                        "SELECT workflow_id, run_json, boundaries_json
                         FROM workflow_runs WHERE run_id=?1",
                        params![run_id],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "ProductTask workflow run owner missing".to_string())?;
                let mut run = managed_acceptance_owner_json_object(&run_json, "workflow run")?;
                let boundaries =
                    managed_acceptance_owner_json_object(&boundaries_json, "workflow boundaries")?;
                let mut statement = conn
                    .prepare(
                        "SELECT node_json FROM workflow_run_nodes
                         WHERE run_id=?1 ORDER BY rowid ASC",
                    )
                    .map_err(|error| error.to_string())?;
                let node_jsons = statement
                    .query_map(params![run_id], |row| row.get::<_, String>(0))
                    .map_err(|error| error.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?;
                let nodes = node_jsons
                    .into_iter()
                    .map(|encoded| managed_acceptance_owner_json_object(&encoded, "workflow node"))
                    .collect::<Result<Vec<_>, _>>()?;
                let matching = nodes
                    .iter()
                    .filter(|node| {
                        node.get("product_task_id").and_then(Value::as_str) == Some(product_task_id)
                    })
                    .collect::<Vec<_>>();
                let node = match matching.as_slice() {
                    [node] => (*node).clone(),
                    [] => return Err("ProductTask workflow node owner missing".to_string()),
                    _ => {
                        return Err(
                            "multiple workflow nodes claim one ProductTask owner".to_string()
                        )
                    }
                };
                let run_object = run
                    .as_object_mut()
                    .expect("managed_acceptance_owner_json_object returns an object");
                run_object.insert("run_id".to_string(), json!(run_id));
                run_object.insert("workflow_id".to_string(), json!(workflow_id));
                run_object.insert("boundaries".to_string(), boundaries);
                run_object.insert("nodes".to_string(), Value::Array(nodes));
                Ok((run, node))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_opt(
                        "SELECT workflow_id, run_json, boundaries_json
                         FROM workflow_runs WHERE run_id=$1",
                        &[&run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "ProductTask workflow run owner missing".to_string())?;
                let workflow_id: String = row.get(0);
                let run_json: String = row.get(1);
                let boundaries_json: String = row.get(2);
                let mut run = managed_acceptance_owner_json_object(&run_json, "workflow run")?;
                let boundaries =
                    managed_acceptance_owner_json_object(&boundaries_json, "workflow boundaries")?;
                let node_rows = client
                    .query(
                        "SELECT node_json FROM workflow_run_nodes
                         WHERE run_id=$1 ORDER BY ctid ASC",
                        &[&run_id],
                    )
                    .map_err(|error| error.to_string())?;
                let nodes = node_rows
                    .iter()
                    .map(|row| {
                        let encoded: String = row.get(0);
                        managed_acceptance_owner_json_object(&encoded, "workflow node")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let matching = nodes
                    .iter()
                    .filter(|node| {
                        node.get("product_task_id").and_then(Value::as_str) == Some(product_task_id)
                    })
                    .collect::<Vec<_>>();
                let node = match matching.as_slice() {
                    [node] => (*node).clone(),
                    [] => return Err("ProductTask workflow node owner missing".to_string()),
                    _ => {
                        return Err(
                            "multiple workflow nodes claim one ProductTask owner".to_string()
                        )
                    }
                };
                let run_object = run
                    .as_object_mut()
                    .expect("managed_acceptance_owner_json_object returns an object");
                run_object.insert("run_id".to_string(), json!(run_id));
                run_object.insert("workflow_id".to_string(), json!(workflow_id));
                run_object.insert("boundaries".to_string(), boundaries);
                run_object.insert("nodes".to_string(), Value::Array(nodes));
                Ok((run, node))
            }),
        }
    }

    /// Strictly reload the workspace evidence owner used by managed Codex
    /// admission.  A malformed workspace or boundary receipt is not converted
    /// into a null compatibility value.
    pub(crate) fn managed_acceptance_workspace_owner(
        &self,
        workspace_id: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT workspace_sequence, workspace_id, plan_id, run_id, target_id,
                            target_repo_path, target_repo_canonical_path, workspace_path,
                            workspace_canonical_path, source_revision, source_tree_hash, status,
                            created_at, updated_at, boundary_json, workspace_json
                     FROM supervised_patch_workspaces
                     WHERE workspace_id=?1",
                    params![workspace_id],
                    managed_acceptance_workspace_row_sqlite,
                )
                .optional()
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "ProductTask workspace owner missing".to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_opt(
                        "SELECT workspace_sequence, workspace_id, plan_id, run_id, target_id,
                                target_repo_path, target_repo_canonical_path, workspace_path,
                                workspace_canonical_path, source_revision, source_tree_hash, status,
                                created_at, updated_at, boundary_json, workspace_json
                         FROM supervised_patch_workspaces
                         WHERE workspace_id=$1",
                        &[&workspace_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "ProductTask workspace owner missing".to_string())?;
                managed_acceptance_workspace_row_pg(&row)
            }),
        }
    }

    /// Validate the ProductTask side of a managed-acceptance attempt against
    /// the actual store owners. Before first live execution only enabled
    /// requirements and owner readability are required. Once the task's state
    /// has crossed a phase boundary, every claimed verification, approval,
    /// output, and terminal receipt must be current and exactly bound.
    pub fn validate_managed_acceptance_product_task_phase(
        &self,
        tenant_id: &str,
        product_task_id: &str,
        spend_target_id: &str,
        spend_target_main_sha: &str,
    ) -> Result<Value, String> {
        self.probe_managed_acceptance_product_receipt_owners()?;
        let task = self
            .get_product_task(product_task_id)?
            .ok_or_else(|| format!("ProductTask owner {product_task_id} required"))?;
        if task.get("tenant_id").and_then(Value::as_str) != Some(tenant_id) {
            return Err("ProductTask tenant mismatch".to_string());
        }
        for requirement in ["approval_required", "confirm_execution", "confirm_output"] {
            if task.get(requirement).and_then(Value::as_bool) != Some(true) {
                return Err(format!(
                    "ProductTask {requirement} must be persisted true for managed acceptance"
                ));
            }
        }
        // `target_id` is the ProductTask's durable logical target identity;
        // target_repo_path is a local filesystem owner and is intentionally not
        // used as a lossy fallback for an external target identity.
        if task.get("target_id").and_then(Value::as_str) != Some(spend_target_id) {
            return Err("ProductTask target_id mismatches spend target_repo".to_string());
        }
        if task.get("source_revision").and_then(Value::as_str) != Some(spend_target_main_sha) {
            return Err("ProductTask source_revision mismatches spend target_main_sha".to_string());
        }
        if task
            .get("target_repo_path")
            .and_then(Value::as_str)
            .is_none_or(|path| path.trim().is_empty())
        {
            return Err("ProductTask target_repo_path owner is missing".to_string());
        }

        let status = ProductTaskStatus::parse(
            task.get("status")
                .and_then(Value::as_str)
                .ok_or_else(|| "ProductTask status missing".to_string())?,
        )?;
        if matches!(
            status,
            ProductTaskStatus::Admitted
                | ProductTaskStatus::WorkspacePreparing
                | ProductTaskStatus::WorkspaceBound
                | ProductTaskStatus::GraphReady
                | ProductTaskStatus::Running
        ) {
            return Ok(json!({
                "stage": "pre_execution_admission",
                "task": task,
                "verification_receipt_required": false,
                "approval_receipt_required": false,
                "output_receipt_required": false,
            }));
        }
        if !matches!(
            status,
            ProductTaskStatus::Verifying
                | ProductTaskStatus::RepairPending
                | ProductTaskStatus::AwaitingApproval
                | ProductTaskStatus::OutputPending
                | ProductTaskStatus::Completed
                | ProductTaskStatus::OutcomeUnknown
        ) {
            return Err(format!(
                "ProductTask status {} is not admissible for managed acceptance",
                status.as_str()
            ));
        }

        let task_version = task
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "ProductTask version missing".to_string())?;
        let run_id = required_product_task_string(&task, "run_id")?;
        let workspace_record_id = required_product_task_string(&task, "workspace_record_id")?;
        let (run, node) =
            self.managed_acceptance_product_run_node_owner(&run_id, product_task_id)?;
        let node_id = required_product_task_string(&node, "node_id")?;
        let workspace = self.managed_acceptance_workspace_owner(&workspace_record_id)?;
        if workspace.get("workspace_id").and_then(Value::as_str) != Some(&workspace_record_id)
            || workspace.get("run_id").and_then(Value::as_str) != Some(&run_id)
            || workspace.get("source_revision").and_then(Value::as_str)
                != task.get("source_revision").and_then(Value::as_str)
        {
            return Err("ProductTask workspace binding is stale".to_string());
        }
        let verification = workspace
            .get("verification")
            .filter(|value| value.is_object())
            .ok_or_else(|| "ProductTask verification receipt missing".to_string())?;
        if verification.get("schema_version").and_then(Value::as_str)
            != Some("workspace_verification.v1")
            || verification.get("status").and_then(Value::as_str) != Some("evidence_recorded")
            || verification.get("result_status").and_then(Value::as_str) != Some("completed")
            || verification.get("trustworthy").and_then(Value::as_bool) != Some(true)
        {
            return Err(
                "ProductTask verification receipt is not accepted and trustworthy".to_string(),
            );
        }
        for (field, expected) in [
            ("product_task_id", product_task_id),
            ("tenant_id", tenant_id),
            ("run_id", run_id.as_str()),
            ("workspace_record_id", workspace_record_id.as_str()),
        ] {
            if verification.get(field).and_then(Value::as_str) != Some(expected) {
                return Err(format!("ProductTask verification receipt {field} mismatch"));
            }
        }
        let verification_version = verification
            .get("expected_task_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "ProductTask verification receipt version missing".to_string())?;
        match status {
            // A verification receipt is written while the ProductTask is in
            // `verifying`; a trustworthy receipt in either of these states
            // must therefore be for this exact version, not merely any older
            // version that happens to be present on the workspace.
            ProductTaskStatus::Verifying | ProductTaskStatus::RepairPending
                if verification_version != task_version =>
            {
                return Err(
                    "ProductTask verification receipt is not bound to the current task version"
                        .to_string(),
                );
            }
            // Capturing the artifact atomically advances the task exactly once
            // from verifying@V to awaiting_approval@(V+1).
            ProductTaskStatus::AwaitingApproval
                if verification_version.checked_add(1) != Some(task_version) =>
            {
                return Err(
                    "ProductTask verification receipt is not bound to the immediately preceding task version"
                        .to_string(),
                );
            }
            _ => {}
        }
        let verification_receipts = verification
            .get("verification_attempts")
            .and_then(Value::as_array)
            .filter(|receipts| !receipts.is_empty())
            .ok_or_else(|| "ProductTask verification receipt set is empty".to_string())?;
        for receipt in verification_receipts {
            if receipt.get("product_task_id").and_then(Value::as_str) != Some(product_task_id)
                || receipt.get("run_id").and_then(Value::as_str) != Some(run_id.as_str())
                || receipt.get("node_id").and_then(Value::as_str) != Some(node_id.as_str())
                || receipt.get("workspace_record_id").and_then(Value::as_str)
                    != Some(workspace_record_id.as_str())
                || receipt.get("expected_task_version").and_then(Value::as_u64)
                    != Some(verification_version)
                || receipt.get("trustworthy").and_then(Value::as_bool) != Some(true)
                || receipt.get("result_status").and_then(Value::as_str) != Some("completed")
            {
                return Err("ProductTask verification attempt binding is stale".to_string());
            }
        }
        let source_revision = required_product_task_string(&task, "source_revision")?;
        let artifact = self.current_product_task_artifact(
            product_task_id,
            &run_id,
            &workspace_record_id,
            &source_revision,
        )?;
        let artifact_id = required_product_task_string(&artifact, "artifact_id")?;
        if artifact.get("product_task_id").and_then(Value::as_str) != Some(product_task_id)
            || artifact.get("run_id").and_then(Value::as_str) != Some(run_id.as_str())
            || artifact.get("workspace_id").and_then(Value::as_str)
                != Some(workspace_record_id.as_str())
            || artifact.get("source_revision").and_then(Value::as_str)
                != Some(source_revision.as_str())
            || artifact
                .get("verification_task_version")
                .and_then(Value::as_u64)
                != Some(verification_version)
            || artifact.get("target_id").and_then(Value::as_str) != Some(spend_target_id)
        {
            return Err("ProductTask artifact target binding is stale".to_string());
        }
        let verification_sha256 = product_json_sha256(verification)?;

        if matches!(
            status,
            ProductTaskStatus::Verifying | ProductTaskStatus::RepairPending
        ) {
            return Ok(json!({
                "stage": "verification",
                "task": task,
                "run": run,
                "workspace": workspace,
                "verification": verification,
                "artifact": artifact,
                "verification_sha256": verification_sha256,
            }));
        }
        if status == ProductTaskStatus::AwaitingApproval {
            return Ok(json!({
                "stage": "awaiting_approval",
                "task": task,
                "run": run,
                "workspace": workspace,
                "verification": verification,
                "artifact": artifact,
                "verification_sha256": verification_sha256,
            }));
        }

        let output_record = if task.get("output_intent").and_then(Value::as_str) == Some("draft_pr")
        {
            artifact
                .get("product_output_operation")
                .ok_or_else(|| "ProductTask output operation missing".to_string())?
        } else {
            artifact
                .get("product_output_receipt")
                .ok_or_else(|| "ProductTask output receipt missing".to_string())?
        };
        if output_record.get("product_task_id").and_then(Value::as_str) != Some(product_task_id)
            || output_record.get("artifact_id").and_then(Value::as_str)
                != Some(artifact_id.as_str())
        {
            return Err("ProductTask output receipt/operation binding is stale".to_string());
        }
        let output_version = output_record
            .get("expected_task_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "ProductTask output receipt/operation version missing".to_string())?;
        let output_request = output_record
            .get("request")
            .ok_or_else(|| "ProductTask output receipt/operation request missing".to_string())?;
        let output_request_sha256 = required_product_task_string(output_record, "request_sha256")?;
        if product_json_sha256(output_request)? != output_request_sha256
            || output_request
                .get("expected_task_version")
                .and_then(Value::as_u64)
                != Some(output_version)
            || output_record.get("source_revision").and_then(Value::as_str)
                != Some(source_revision.as_str())
        {
            return Err(
                "ProductTask output receipt/operation content binding is stale".to_string(),
            );
        }
        match task.get("output_intent").and_then(Value::as_str) {
            Some("draft_pr")
                if output_record.get("schema_version").and_then(Value::as_str)
                    == Some("product_output_operation.v1")
                    && matches!(
                        output_record.get("state").and_then(Value::as_str),
                        Some("active" | "completed")
                    ) => {}
            Some("artifact_only" | "export_patch" | "apply_local_changes")
                if output_record.get("schema_version").and_then(Value::as_str)
                    == Some("product_output_receipt.v1")
                    && output_record.get("state").and_then(Value::as_str) == Some("completed")
                    && output_record.get("output_intent").and_then(Value::as_str)
                        == task.get("output_intent").and_then(Value::as_str) => {}
            _ => {
                return Err("ProductTask output receipt/operation is not current".to_string());
            }
        }
        let approval_id = required_product_task_string(output_record, "approval_id")?;
        let approvals = self.workflow_run_approvals(&run_id, 1_000)?;
        let approval = approvals
            .into_iter()
            .find(|candidate| {
                candidate.get("approval_id").and_then(Value::as_str) == Some(&approval_id)
            })
            .ok_or_else(|| "ProductTask current approval receipt missing".to_string())?;
        validate_current_product_output_approval(
            &approval,
            &task,
            product_task_id,
            &run_id,
            &workspace_record_id,
            task_version,
        )?;
        if approval.get("node_id").and_then(Value::as_str) != Some(node_id.as_str())
            || approval.get("artifact_id").and_then(Value::as_str) != Some(artifact_id.as_str())
            || approval.get("verification_sha256").and_then(Value::as_str)
                != Some(verification_sha256.as_str())
            || approval
                .get("expected_task_version")
                .and_then(Value::as_u64)
                != verification_version.checked_add(1)
            || output_version
                != approval
                    .get("expected_task_version")
                    .and_then(Value::as_u64)
                    .unwrap_or_default()
            || output_version.checked_add(1) != Some(task_version)
        {
            return Err(
                "ProductTask approval receipt is not current for verification/artifact/version"
                    .to_string(),
            );
        }

        if status != ProductTaskStatus::Completed {
            return Ok(json!({
                "stage": "output",
                "task": task,
                "run": run,
                "workspace": workspace,
                "verification": verification,
                "artifact": artifact,
                "approval": approval,
                "output_record": output_record,
            }));
        }

        let terminal_evidence = self.get_product_task_terminal_evidence(product_task_id)?;
        let completed_output = validate_completed_product_output_binding(
            &task,
            &artifact,
            &approval,
            &terminal_evidence,
        )?;
        if terminal_evidence
            .get("product_task_id")
            .and_then(Value::as_str)
            != Some(product_task_id)
            || terminal_evidence.get("tenant_id").and_then(Value::as_str) != Some(tenant_id)
            || terminal_evidence
                .get("task_version")
                .and_then(Value::as_u64)
                != Some(task_version)
            || terminal_evidence
                .get("creation_version")
                .and_then(Value::as_u64)
                != Some(task_version)
            || terminal_evidence.get("run_id").and_then(Value::as_str) != Some(run_id.as_str())
            || terminal_evidence
                .pointer("/node/node_id")
                .and_then(Value::as_str)
                != Some(node_id.as_str())
            || terminal_evidence
                .get("workspace_record_id")
                .and_then(Value::as_str)
                != Some(workspace_record_id.as_str())
            || terminal_evidence
                .pointer("/artifact/artifact_id")
                .and_then(Value::as_str)
                != Some(artifact_id.as_str())
            || terminal_evidence
                .pointer("/approval/approval_id")
                .and_then(Value::as_str)
                != Some(approval_id.as_str())
            || terminal_evidence
                .pointer("/verification/verification_sha256")
                .and_then(Value::as_str)
                != Some(verification_sha256.as_str())
            || (task.get("output_intent").and_then(Value::as_str) == Some("draft_pr")
                && terminal_evidence
                    .pointer("/output/operation_id")
                    .and_then(Value::as_str)
                    != output_record.get("operation_id").and_then(Value::as_str))
            || (matches!(
                task.get("output_intent").and_then(Value::as_str),
                Some("artifact_only" | "export_patch" | "apply_local_changes")
            ) && terminal_evidence
                .pointer("/output/receipt_id")
                .and_then(Value::as_str)
                != output_record.get("receipt_id").and_then(Value::as_str))
        {
            return Err("ProductTask terminal evidence binding is stale".to_string());
        }
        Ok(json!({
            "stage": "terminal",
            "task": task,
            "run": run,
            "workspace": workspace,
            "verification": verification,
            "artifact": artifact,
            "approval": approval,
            "output": completed_output,
            "terminal_evidence": terminal_evidence,
        }))
    }

    /// Authenticated intake: reserve canonical task under idempotency, prepare controlled
    /// worktree, verify bindings, and finalize to `workspace_bound` without admitting execution.
    ///
    /// Concurrent duplicate intake under the same idempotency key collapses to one task and
    /// one worktree effect (restart-safe, expected-current protected).
    pub fn admit_product_task(
        &self,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        validate_source_revision_format(&intake.source_revision)?;

        // Bounded CAS loop for concurrent duplicate intake.  Only the caller
        // that wins `admitted -> workspace_preparing` may create the physical
        // worktree; duplicate callers observe and wait for that durable owner.
        let mut contention_observed = false;
        for attempt in 0..PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_LIMIT {
            let reserved = self.reserve_product_task(intake, actor)?;
            let status = reserved
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("admitted");
            if product_task_has_prepared_workspace(&reserved)
                || product_task_has_terminal_workspace_prepare_state(&reserved)
            {
                let task_id = reserved
                    .get("task_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "reserved product task missing task_id".to_string())?;
                let _ = self
                    .retire_completed_product_task_workspace_preparation(task_id, &reserved, actor);
                return Ok(reserved);
            }

            let task_id = reserved
                .get("task_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "reserved product task missing task_id".to_string())?
                .to_string();
            let owns_workspace_prepare = if status == ProductTaskStatus::Admitted.as_str() {
                // Validate read-only setup before publishing a durable
                // `workspace_preparing` state.  A malformed current
                // environment or target therefore leaves this idempotent task
                // retryable as `admitted`, with no physical worktree effect.
                validate_product_task_workspace_preflight(self, &task_id, intake)?;
                // The later receipt/state transition is atomic and precedes
                // any root, marker, guard, or Git effect. A guard setup
                // failure after that durable boundary is reconciliation, not
                // permission to clean an unproven physical outcome.
                true
            } else if status == ProductTaskStatus::WorkspacePreparing.as_str() {
                // The transition owner prepares the worktree.  An interrupted
                // owner is recovered through recover_product_task_workspace,
                // rather than allowing a duplicate intake to steal the
                // physical operation and race git's worktree metadata.
                false
            } else {
                // All non-terminal states above are handled before this point;
                // retain the existing fail-closed behavior for an unexpected
                // persisted state by letting the preparation boundary validate it.
                true
            };

            if !owns_workspace_prepare {
                if attempt + 1 < PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_LIMIT {
                    std::thread::sleep(PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_DELAY);
                    continue;
                }
                break;
            }

            match self.prepare_product_task_worktree(
                &task_id,
                intake,
                actor,
                "worktree_prepare_failed",
                &mut contention_observed,
            ) {
                Ok(task) => return Ok(task),
                Err(error) if is_retryable_product_task_worktree_prepare_error(&error) => {
                    // Concurrent prepare: re-read and return if winner bound the task.
                    if let Some(current) = self.get_product_task(&task_id)? {
                        if product_task_has_prepared_workspace(&current)
                            || product_task_has_terminal_workspace_prepare_state(&current)
                        {
                            let _ = self.retire_completed_product_task_workspace_preparation(
                                &task_id, &current, actor,
                            );
                            return Ok(current);
                        }
                    }
                    std::thread::sleep(PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_DELAY);
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
        // Final re-read after CAS retries.
        let task = self
            .get_product_task_by_idempotency(
                &intake.tenant_id,
                &intake.workspace_id,
                &intake.idempotency_key,
            )?
            .ok_or_else(|| "product task admit concurrent retry exhausted".to_string())?;
        if product_task_has_prepared_workspace(&task)
            || product_task_has_terminal_workspace_prepare_state(&task)
        {
            let task_id = task
                .get("task_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "reserved product task missing task_id".to_string())?;
            let _ = self.retire_completed_product_task_workspace_preparation(task_id, &task, actor);
            return Ok(task);
        }
        Err(
            "product task admit concurrent retry exhausted while workspace preparation remains in progress"
                .to_string(),
        )
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
                row.map(|r| product_task_row_to_json_pg(&r)).transpose()
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
                row.map(|r| product_task_row_to_json_pg(&r)).transpose()
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
        let intake_json = persisted_product_intake_json(intake).to_string();
        let status = ProductTaskStatus::Admitted.as_str();

        match &self.db {
            DatabaseConnection::Sqlite(_) => {
                self.with_conn(|conn| {
                    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                    let inserted = match tx.execute(
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
                        Ok(_) => 1,
                        Err(rusqlite::Error::SqliteFailure(code, _))
                            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
                        {
                            0
                        }
                        Err(e) => return Err(e.to_string()),
                    };
                    if inserted > 0 {
                        append_audit_locked(
                            &tx,
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
                    }
                    tx.commit().map_err(|e| e.to_string())
                })?;
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| {
                    let mut tx = client.transaction().map_err(|e| e.to_string())?;
                    let n = tx
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
                        });
                        super::workflow_runs::pg_append_audit(
                            &mut tx,
                            &now,
                            actor,
                            "product_task.admit",
                            &task_id,
                            &audit_details,
                        )?;
                    }
                    tx.commit().map_err(|e| e.to_string())
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
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let updated = tx
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
                    &tx,
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
                tx.commit().map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let updated = tx
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
                });
                super::workflow_runs::pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "product_task.transition",
                    task_id,
                    &audit_details,
                )?;
                tx.commit().map_err(|error| error.to_string())
            })?,
        }
        self.get_product_task(task_id)?
            .ok_or_else(|| "product task missing after transition".to_string())
    }

    fn product_task_workspace_preparation_receipt(
        &self,
        task_id: &str,
    ) -> Result<Option<ProductTaskWorkspacePreparationReceipt>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let receipt = conn
                    .query_row(
                        "SELECT workspace_root, workspace_path, marker_sha256, marker_state,
                                receipt_sha256
                         FROM product_task_workspace_preparations WHERE task_id=?1",
                        params![task_id],
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
                    .map_err(|error| error.to_string())?;
                receipt
                    .map(|(root, path, marker, state, hash)| {
                        ProductTaskWorkspacePreparationReceipt::from_persisted(
                            task_id, root, path, marker, state, hash,
                        )
                    })
                    .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let receipt = client
                    .query_opt(
                        "SELECT workspace_root, workspace_path, marker_sha256, marker_state,
                                receipt_sha256
                         FROM product_task_workspace_preparations WHERE task_id=$1",
                        &[&task_id],
                    )
                    .map_err(|_| {
                        format!(
                            "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt is unavailable"
                        )
                    })?;
                receipt
                    .map(|row| {
                        ProductTaskWorkspacePreparationReceipt::from_persisted(
                            task_id,
                            row.get(0),
                            row.get(1),
                            row.get(2),
                            row.get(3),
                            row.get(4),
                        )
                    })
                    .transpose()
            }),
        }
    }

    fn new_product_task_workspace_preparation_receipt(
        &self,
        task_id: &str,
        intake: &ValidatedProductTaskIntake,
    ) -> Result<ProductTaskWorkspacePreparationReceipt, String> {
        let workspace_root = product_task_workspace_root_for_preparation(self, task_id, intake)?;
        // The receipt is deliberately constructed from the read-only,
        // canonicalized configuration view. It is committed with the state
        // transition before any root, marker, or git mutation occurs.
        ProductTaskWorkspacePreparationReceipt::planned(task_id, workspace_root)
    }

    fn ensure_product_task_workspace_preparation_receipt(
        &self,
        task_id: &str,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
    ) -> Result<ProductTaskWorkspacePreparationReceipt, String> {
        self.ensure_product_task_workspace_preparation_receipt_after_sqlite_observation(
            task_id,
            intake,
            actor,
            || {},
        )
    }

    fn ensure_product_task_workspace_preparation_receipt_after_sqlite_observation<F>(
        &self,
        task_id: &str,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
        after_sqlite_initial_observation: F,
    ) -> Result<ProductTaskWorkspacePreparationReceipt, String>
    where
        F: FnOnce(),
    {
        match &self.db {
            DatabaseConnection::Sqlite(_) => {
                let initial = self.with_conn(|conn| {
                    let tx = conn
                        .unchecked_transaction()
                        .map_err(|error| error.to_string())?;
                    let observed = observe_sqlite_workspace_preparation(&tx, task_id)?;
                    tx.commit().map_err(|error| error.to_string())?;
                    Ok(observed)
                })?;
                if let SqliteWorkspacePreparationObservation::Existing(receipt) = initial {
                    return Ok(receipt);
                }

                // Keep path inspection outside the connection lock. The
                // second transaction below is the authoritative observation;
                // the callback is a no-op in production and lets the unit
                // test deterministically publish a concurrent winner here.
                after_sqlite_initial_observation();
                let receipt = self.new_product_task_workspace_preparation_receipt(task_id, intake)?;
                let now = self.now();
                self.with_conn(|conn| {
                    let tx = conn
                        .unchecked_transaction()
                        .map_err(|error| error.to_string())?;
                    let current_version = match observe_sqlite_workspace_preparation(&tx, task_id)? {
                        SqliteWorkspacePreparationObservation::Existing(existing) => {
                            tx.commit().map_err(|error| error.to_string())?;
                            return Ok(existing);
                        }
                        SqliteWorkspacePreparationObservation::Admitted { version } => version,
                    };
                    let next_version = current_version.saturating_add(1);
                    let updated = tx
                        .execute(
                            "UPDATE product_tasks SET status=?1, version=?2, updated_at=?3
                             WHERE task_id=?4 AND status=?5 AND version=?6",
                            params![
                                ProductTaskStatus::WorkspacePreparing.as_str(),
                                next_version,
                                now,
                                task_id,
                                ProductTaskStatus::Admitted.as_str(),
                                current_version,
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                    if updated != 1 {
                        return Err("product task expected-current update conflict".to_string());
                    }
                    tx.execute(
                        "INSERT INTO product_task_workspace_preparations (
                            task_id, workspace_root, workspace_path, marker_sha256, marker_state,
                            receipt_sha256, created_at, updated_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                        params![
                            task_id,
                            receipt.workspace_root.to_string_lossy(),
                            receipt.workspace_path.to_string_lossy(),
                            receipt.marker_sha256,
                            receipt.marker_state.as_str(),
                            receipt.receipt_sha256,
                            now,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                    append_audit_locked(
                        &tx,
                        &now,
                        actor,
                        "product_task.transition",
                        task_id,
                        &json!({
                            "from": ProductTaskStatus::Admitted.as_str(),
                            "to": ProductTaskStatus::WorkspacePreparing.as_str(),
                            "version": next_version,
                            "execution_admitted": false,
                            "failure_code": Value::Null,
                        }),
                    )?;
                    append_audit_locked(
                        &tx,
                        &now,
                        actor,
                        "product_task.workspace_prepare_planned",
                        task_id,
                        &json!({
                            "receipt_sha256": receipt.receipt_sha256,
                            "marker_state": receipt.marker_state.as_str(),
                            "authority_owner": "product_task",
                        }),
                    )?;
                    tx.commit().map_err(|error| error.to_string())?;
                    Ok(receipt.clone())
                })
            }
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                // Share the schema-transition exclusion used by v35 rollback.
                // This keeps the admitted -> workspace_preparing transition
                // and its durable receipt indivisible with respect to a
                // destructive schema rollback: either this transaction
                // publishes both before rollback observes the table, or it
                // sees the retired schema before changing ProductTask state.
                tx.query_one(
                    "SELECT pg_advisory_xact_lock(
                         hashtext(current_database()), hashtext(current_schema())
                     )",
                    &[],
                )
                .map_err(|error| {
                    product_task_workspace_preparation_reconciliation_error(format!(
                        "workspace preparation schema lock is unavailable: {error}"
                    ))
                })?;
                let schema_version: i64 = tx
                    .query_one(
                        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                        &[],
                    )
                    .map_err(|error| {
                        product_task_workspace_preparation_reconciliation_error(format!(
                            "workspace preparation schema version is unavailable: {error}"
                        ))
                    })?
                    .get(0);
                if schema_version != super::schema::CURRENT_POSTGRES_SCHEMA_VERSION {
                    return Err(product_task_workspace_preparation_reconciliation_error(
                        "workspace preparation schema is not current",
                    ));
                }
                let row = tx
                    .query_opt(
                        "SELECT status, version FROM product_tasks WHERE task_id=$1 FOR UPDATE",
                        &[&task_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| {
                        "product task missing before workspace preparation".to_string()
                    })?;
                let current_status: String = row.get(0);
                let current_version: i64 = row.get(1);
                // The task-row lock is the serialization point for a duplicate
                // admission. Query the receipt only after taking it: a prior
                // statement-level PostgreSQL snapshot can otherwise miss the
                // receipt that the lock holder committed atomically with the
                // workspace_preparing transition.
                let existing = tx
                    .query_opt(
                        "SELECT workspace_root, workspace_path, marker_sha256, marker_state,
                                receipt_sha256
                         FROM product_task_workspace_preparations WHERE task_id=$1",
                        &[&task_id],
                    )
                    .map_err(|error| error.to_string())?;
                if let Some(row) = existing {
                    let existing = ProductTaskWorkspacePreparationReceipt::from_persisted(
                        task_id,
                        row.get(0),
                        row.get(1),
                        row.get(2),
                        row.get(3),
                        row.get(4),
                    )?;
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(existing);
                }
                if current_status == ProductTaskStatus::WorkspacePreparing.as_str() {
                    return Err(format!(
                        "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: legacy preparing task has no receipt"
                    ));
                }
                if current_status != ProductTaskStatus::Admitted.as_str() {
                    return Err("product task expected-current update conflict".to_string());
                }
                let receipt = self.new_product_task_workspace_preparation_receipt(task_id, intake)?;
                let now = self.now();
                let next_version = current_version.saturating_add(1);
                let updated = tx
                    .execute(
                        "UPDATE product_tasks SET status=$1, version=$2, updated_at=$3
                         WHERE task_id=$4 AND status=$5 AND version=$6",
                        &[
                            &ProductTaskStatus::WorkspacePreparing.as_str(),
                            &next_version,
                            &now,
                            &task_id,
                            &ProductTaskStatus::Admitted.as_str(),
                            &current_version,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err("product task expected-current update conflict".to_string());
                }
                tx.execute(
                    "INSERT INTO product_task_workspace_preparations (
                        task_id, workspace_root, workspace_path, marker_sha256, marker_state,
                        receipt_sha256, created_at, updated_at
                     ) VALUES ($1, $2, $3, $4, $5, $6, $7, $7)",
                    &[
                        &task_id,
                        &receipt.workspace_root.to_string_lossy(),
                        &receipt.workspace_path.to_string_lossy(),
                        &receipt.marker_sha256,
                        &receipt.marker_state.as_str(),
                        &receipt.receipt_sha256,
                        &now,
                    ],
                )
                .map_err(|error| error.to_string())?;
                super::workflow_runs::pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "product_task.transition",
                    task_id,
                    &json!({
                        "from": ProductTaskStatus::Admitted.as_str(),
                        "to": ProductTaskStatus::WorkspacePreparing.as_str(),
                        "version": next_version,
                        "execution_admitted": false,
                        "failure_code": Value::Null,
                    }),
                )?;
                super::workflow_runs::pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "product_task.workspace_prepare_planned",
                    task_id,
                    &json!({
                        "receipt_sha256": receipt.receipt_sha256,
                        "marker_state": receipt.marker_state.as_str(),
                        "authority_owner": "product_task",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(receipt.clone())
            }),
        }
    }

    fn validate_product_task_workspace_preparation_root(
        &self,
        task_id: &str,
        receipt: &ProductTaskWorkspacePreparationReceipt,
    ) -> Result<(), String> {
        let workspace_fs_id = product_task_workspace_fs_id(task_id);
        let configured = product_workspace_path(self, &workspace_fs_id)?;
        let configured_root = configured.parent().ok_or_else(|| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: configured root is unavailable"
            )
        })?;
        let configured_root = canonicalize_with_missing_tail(configured_root).map_err(|_| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: configured root is unavailable"
            )
        })?;
        if configured_root != receipt.workspace_root
            || receipt.workspace_path != receipt.workspace_root.join(workspace_fs_id)
        {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: configured root does not match the receipt"
            ));
        }
        Ok(())
    }

    /// A receipt pins the app-owned root, but the target path is current
    /// runtime configuration. Reconcile before any root, marker, lock, Git,
    /// or compensation effect when a target path has been repointed into that
    /// receipt root (or vice versa).
    fn validate_product_task_workspace_preparation_target_boundary(
        &self,
        intake: &ValidatedProductTaskIntake,
        receipt: &ProductTaskWorkspacePreparationReceipt,
    ) -> Result<(), String> {
        let target_root = std::fs::canonicalize(&intake.target_repo_path).map_err(|_| {
            product_task_workspace_preparation_reconciliation_error(
                "current target repository is unavailable",
            )
        })?;
        if !target_root.is_dir() {
            return Err(product_task_workspace_preparation_reconciliation_error(
                "current target repository is unavailable",
            ));
        }
        if paths_overlap(&receipt.workspace_root, &target_root) {
            return Err(product_task_workspace_preparation_reconciliation_error(
                "pinned workspace root overlaps current target repository",
            ));
        }
        Ok(())
    }

    fn ensure_product_task_workspace_preparation_root(
        &self,
        task_id: &str,
        receipt: &ProductTaskWorkspacePreparationReceipt,
    ) -> Result<(), String> {
        self.validate_product_task_workspace_preparation_root(task_id, receipt)?;
        std::fs::create_dir_all(&receipt.workspace_root).map_err(|_| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt root is unavailable"
            )
        })?;
        let root = std::fs::canonicalize(&receipt.workspace_root).map_err(|_| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt root is unavailable"
            )
        })?;
        if root != receipt.workspace_root || !root.is_dir() {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt root identity is unavailable"
            ));
        }
        Ok(())
    }

    fn mark_product_task_workspace_preparation_marker_ready(
        &self,
        task_id: &str,
        receipt: &ProductTaskWorkspacePreparationReceipt,
        actor: &str,
    ) -> Result<ProductTaskWorkspacePreparationReceipt, String> {
        if receipt.marker_state == ProductTaskWorkspacePreparationMarkerState::MarkerReady {
            return Ok(receipt.clone());
        }
        let ready = receipt.with_marker_state(
            task_id,
            ProductTaskWorkspacePreparationMarkerState::MarkerReady,
        )?;
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let updated = tx
                    .execute(
                        "UPDATE product_task_workspace_preparations
                         SET marker_state=?1, receipt_sha256=?2, updated_at=?3
                         WHERE task_id=?4 AND marker_state='planned' AND receipt_sha256=?5",
                        params![
                            ready.marker_state.as_str(),
                            ready.receipt_sha256,
                            now,
                            task_id,
                            receipt.receipt_sha256,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err("product task expected-current update conflict".to_string());
                }
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "product_task.workspace_prepare_marker_ready",
                    task_id,
                    &json!({
                        "receipt_sha256": ready.receipt_sha256,
                        "marker_state": ready.marker_state.as_str(),
                        "authority_owner": "product_task",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(ready.clone())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let updated = tx
                    .execute(
                        "UPDATE product_task_workspace_preparations
                         SET marker_state=$1, receipt_sha256=$2, updated_at=$3
                         WHERE task_id=$4 AND marker_state='planned' AND receipt_sha256=$5",
                        &[
                            &ready.marker_state.as_str(),
                            &ready.receipt_sha256,
                            &now,
                            &task_id,
                            &receipt.receipt_sha256,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err("product task expected-current update conflict".to_string());
                }
                super::workflow_runs::pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "product_task.workspace_prepare_marker_ready",
                    task_id,
                    &json!({
                        "receipt_sha256": ready.receipt_sha256,
                        "marker_state": ready.marker_state.as_str(),
                        "authority_owner": "product_task",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(ready.clone())
            }),
        }
    }

    fn ensure_product_task_workspace_preparation_marker(
        &self,
        task_id: &str,
        receipt: &ProductTaskWorkspacePreparationReceipt,
        actor: &str,
    ) -> Result<ProductTaskWorkspacePreparationReceipt, String> {
        validate_product_task_workspace_preparation_marker(task_id, receipt, true)?;
        if receipt.marker_state == ProductTaskWorkspacePreparationMarkerState::Planned {
            self.mark_product_task_workspace_preparation_marker_ready(task_id, receipt, actor)
        } else {
            Ok(receipt.clone())
        }
    }

    fn retire_product_task_workspace_preparation_receipt(
        &self,
        task_id: &str,
        receipt: &ProductTaskWorkspacePreparationReceipt,
        actor: &str,
        retired_after: &str,
    ) -> Result<(), String> {
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let removed = tx
                    .execute(
                        "DELETE FROM product_task_workspace_preparations
                         WHERE task_id=?1 AND receipt_sha256=?2",
                        params![task_id, receipt.receipt_sha256],
                    )
                    .map_err(|error| error.to_string())?;
                if removed == 0 {
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(());
                }
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "product_task.workspace_prepare_retired",
                    task_id,
                    &json!({
                        "receipt_sha256": receipt.receipt_sha256,
                        "retired_after": retired_after,
                        "authority_owner": "product_task",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let removed = tx
                    .execute(
                        "DELETE FROM product_task_workspace_preparations
                         WHERE task_id=$1 AND receipt_sha256=$2",
                        &[&task_id, &receipt.receipt_sha256],
                    )
                    .map_err(|error| error.to_string())?;
                if removed == 0 {
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(());
                }
                super::workflow_runs::pg_append_audit(
                    &mut tx,
                    &now,
                    actor,
                    "product_task.workspace_prepare_retired",
                    task_id,
                    &json!({
                        "receipt_sha256": receipt.receipt_sha256,
                        "retired_after": retired_after,
                        "authority_owner": "product_task",
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())
            }),
        }?;

        #[cfg(unix)]
        // The persisted receipt, not the current configuration, is the only
        // source eligible for marker retirement. If that marker can no longer
        // be proved to be the exact receipt marker, leave it for operator
        // reconciliation rather than removing a replacement path.
        if validate_product_task_workspace_preparation_marker(task_id, receipt, false).is_ok() {
            if let Ok(marker_path) = receipt.marker_path(task_id) {
                let _ = std::fs::remove_file(marker_path);
            }
        }
        Ok(())
    }

    fn retire_completed_product_task_workspace_preparation(
        &self,
        task_id: &str,
        task: &Value,
        actor: &str,
    ) -> Result<(), String> {
        if !product_task_has_prepared_workspace(task)
            && !product_task_has_terminal_workspace_prepare_state(task)
        {
            return Ok(());
        }
        let Some(receipt) = self.product_task_workspace_preparation_receipt(task_id)? else {
            return Ok(());
        };
        let retired_after = if product_task_has_prepared_workspace(task) {
            "workspace_bound"
        } else {
            "compensated_failure"
        };
        self.retire_product_task_workspace_preparation_receipt(
            task_id,
            &receipt,
            actor,
            retired_after,
        )
    }

    fn prepare_product_task_worktree(
        &self,
        task_id: &str,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
        failure_code: &str,
        contention_observed: &mut bool,
    ) -> Result<Value, String> {
        if let Some(existing) = self.get_product_task(task_id)? {
            if product_task_has_prepared_workspace(&existing)
                || product_task_has_terminal_workspace_prepare_state(&existing)
            {
                let _ = self
                    .retire_completed_product_task_workspace_preparation(task_id, &existing, actor);
                return Ok(existing);
            }
        }

        // This read-only boundary must run before any root, marker, lock, or
        // git mutation. In particular, a kill-switched recovery leaves a
        // persisted workspace_preparing task recoverable and untouched.
        validate_product_task_workspace_prerequisites(intake)?;
        let receipt =
            self.ensure_product_task_workspace_preparation_receipt(task_id, intake, actor)?;
        self.validate_product_task_workspace_preparation_root(task_id, &receipt)?;
        self.validate_product_task_workspace_preparation_target_boundary(intake, &receipt)?;
        self.ensure_product_task_workspace_preparation_root(task_id, &receipt)?;
        let _workspace_preparation_lock = ProductTaskWorkspacePreparationGuard::acquire(
            self,
            task_id,
            &receipt.workspace_path,
            || {
                if !*contention_observed {
                    self.record_product_task_workspace_prepare_lock_contention(task_id, actor)?;
                    *contention_observed = true;
                }
                Ok(())
            },
        )?;

        // A concurrent admit or recovery may have completed while this caller
        // retried the try-only guard. A prior owner may instead have
        // terminalized a failed preparation while retaining this lock through
        // compensation; never resurrect that task.
        if let Some(existing) = self.get_product_task(task_id)? {
            if product_task_has_prepared_workspace(&existing)
                || product_task_has_terminal_workspace_prepare_state(&existing)
            {
                let _ = self
                    .retire_completed_product_task_workspace_preparation(task_id, &existing, actor);
                return Ok(existing);
            }
            let status = existing.get("status").and_then(Value::as_str).unwrap_or("");
            if status != ProductTaskStatus::WorkspacePreparing.as_str() {
                return Err("product task is not eligible for workspace preparation".to_string());
            }
        } else {
            return Err("product task missing before workspace preparation".to_string());
        }

        // Re-validate the pinned root after acquiring the guard. The receipt
        // is the only source of the physical path; a changed current root is
        // a recoverable reconciliation condition, never permission to create
        // a second worktree elsewhere.
        self.validate_product_task_workspace_preparation_root(task_id, &receipt)?;
        self.validate_product_task_workspace_preparation_target_boundary(intake, &receipt)?;
        let receipt =
            self.ensure_product_task_workspace_preparation_marker(task_id, &receipt, actor)?;
        let prepared = self.prepare_product_task_worktree_locked(
            task_id,
            intake,
            actor,
            &receipt,
            receipt.workspace_path.clone(),
        );
        match prepared {
            Ok(task) => {
                if product_task_has_prepared_workspace(&task) {
                    // Durable workspace binding has superseded the pre-effect
                    // receipt. Retirement is transactional/audited; a rare
                    // retirement failure leaves only conservative rollback
                    // residue and must not turn a bound task into a false
                    // failed admission result.
                    let _ = self.retire_product_task_workspace_preparation_receipt(
                        task_id,
                        &receipt,
                        actor,
                        "workspace_bound",
                    );
                }
                Ok(task)
            }
            Err(error) if is_retryable_product_task_worktree_prepare_error(&error) => Err(error),
            Err(error)
                if error
                    .starts_with(PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED)
                    || error.starts_with(
                        PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE,
                    ) =>
            {
                // No physical worktree outcome was established. Retain the
                // durable receipt and workspace_preparing state for an exact
                // root/configuration reconciliation rather than terminalizing
                // or deleting an unproven path.
                Err(error)
            }
            Err(error) => {
                // The same lock protects both physical preparation and its
                // compensation/state finalization.  A waiting recovery cannot
                // bind a worktree between this failure and terminal cleanup.
                self.fail_product_task_and_compensate(
                    task_id,
                    intake,
                    failure_code,
                    &error,
                    actor,
                )?;
                Err(error)
            }
        }
    }

    fn prepare_product_task_worktree_locked(
        &self,
        task_id: &str,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
        receipt: &ProductTaskWorkspacePreparationReceipt,
        workspace_path: PathBuf,
    ) -> Result<Value, String> {
        if intake.workspace_mode == "local_folder" {
            return self.prepare_product_task_local_folder_locked(
                task_id,
                intake,
                actor,
                receipt,
                workspace_path,
            );
        }
        // Configuration and target state can change after the initial
        // preflight. Recheck while holding the pinned receipt guard, but keep
        // a pre-effect failure recoverable rather than compensating an
        // unproven worktree outcome.
        validate_product_task_workspace_prerequisites(intake).map_err(|error| {
            format!("{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: {error}")
        })?;
        self.validate_product_task_workspace_preparation_target_boundary(intake, receipt)?;
        let config = TargetRepoOutputConfig::from_env();

        let target_repo = Path::new(&intake.target_repo_path);
        let target_canonical = std::fs::canonicalize(target_repo).map_err(|_| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: target repository is unavailable"
            )
        })?;

        // A concurrent admit or interrupted child can leave an existing path.
        // Reuse is safe only if the configured target repository itself still
        // registers that exact receipt path and resolves it to the exact pinned
        // source.  A matching HEAD in a foreign/stale repository is not proof
        // of target ownership and must remain reconciliation, never cleanup or
        // an alternate worktree creation.
        let prepared = if workspace_path.exists() {
            inspect_registered_git_worktree(
                &config,
                &target_canonical,
                &workspace_path,
                &intake.source_revision,
            )
            .map_err(|_| {
                product_task_workspace_preparation_reconciliation_error(
                    "existing receipt workspace registration or source identity is unproved",
                )
            })?
        } else {
            prepare_git_worktree(
                &config,
                &target_canonical,
                &workspace_path,
                &intake.source_revision,
            )
            .map_err(classify_product_task_git_worktree_prepare_error)?
        };

        let workspaces_root = workspace_path
            .parent()
            .ok_or_else(|| {
                product_task_workspace_preparation_reconciliation_error(
                    "prepared workspace has no app-owned root",
                )
            })?
            .to_path_buf();
        let workspaces_root = std::fs::canonicalize(&workspaces_root).unwrap_or(workspaces_root);
        let ws = std::fs::canonicalize(Path::new(&prepared.workspace_path)).map_err(|_| {
            product_task_workspace_preparation_reconciliation_error(
                "prepared workspace identity is unavailable",
            )
        })?;
        if !ws.starts_with(&workspaces_root) {
            return Err(product_task_workspace_preparation_reconciliation_error(
                "prepared workspace escaped the app-owned workspace root",
            ));
        }

        let content_hash = workspace_content_hash(Path::new(&prepared.workspace_path))?;

        if let Some(expected_tree) = intake.source_tree_hash.as_deref() {
            if expected_tree.len() == 64 && expected_tree != content_hash {
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
                "default_branch_sha": prepared.default_branch_sha,
                "origin_remote": prepared.origin_remote,
                "origin_remote_fingerprint": prepared.origin_remote_fingerprint,
                "source_revision": prepared.source_revision,
            },
            "status": "workspace_created",
            "product_task_id": task_id,
            "allowed_paths": intake.allowed_paths,
        });

        let workspace = match self.record_supervised_patch_workspace(&workspace_request, actor) {
            Ok(ws) => ws,
            Err(e) => return Err(format!("record supervised workspace failed: {e}")),
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
            local_folder_exclusions_sha256: None,
            git_origin_remote: Some(prepared.origin_remote.clone()),
            git_origin_remote_fingerprint: Some(prepared.origin_remote_fingerprint.clone()),
            git_default_branch: Some(prepared.default_branch.clone()),
            git_default_branch_sha: Some(prepared.default_branch_sha.clone()),
            provisional_run_id: provisional_run_id.clone(),
            allowed_paths: intake.allowed_paths.clone(),
            bound_at: self.now(),
        };

        let current = self
            .get_product_task(task_id)?
            .ok_or_else(|| "task missing before finalize".to_string())?;
        if product_task_has_prepared_workspace(&current)
            || product_task_has_terminal_workspace_prepare_state(&current)
        {
            return Ok(current);
        }
        let version = current.get("version").and_then(Value::as_u64).unwrap_or(0);
        match self.transition_product_task(
            task_id,
            ProductTaskStatus::WorkspaceBound,
            Some(version),
            actor,
            Some(&binding),
            Some(&workspace_record_id),
            None,
            None,
            Some(&provisional_run_id),
        ) {
            Ok(task) => Ok(task),
            Err(e) if is_retryable_product_task_worktree_prepare_error(&e) => {
                // Concurrent finisher won; return the bound task if present.
                if let Some(task) = self.get_product_task(task_id)? {
                    if product_task_has_prepared_workspace(&task)
                        || product_task_has_terminal_workspace_prepare_state(&task)
                    {
                        return Ok(task);
                    }
                }
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    fn record_product_task_workspace_prepare_lock_contention(
        &self,
        task_id: &str,
        actor: &str,
    ) -> Result<(), String> {
        let now = self.now();
        let details = json!({
            "synchronization_only": true,
            "authority_owner": "product_task",
        });
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                append_audit_locked(
                    conn,
                    &now,
                    actor,
                    "product_task.workspace_prepare_lock_contended",
                    task_id,
                    &details,
                )?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let details = details.to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, 'product_task.workspace_prepare_lock_contended', $3, $4)",
                        &[&now, &actor, &task_id, &details],
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            }),
        }
    }

    fn fail_product_task_and_compensate(
        &self,
        task_id: &str,
        intake: &ValidatedProductTaskIntake,
        code: &str,
        detail: &str,
        actor: &str,
    ) -> Result<Value, String> {
        let receipt = self
            .product_task_workspace_preparation_receipt(task_id)?
            .ok_or_else(|| {
                format!(
                    "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt is missing before compensation"
                )
        })?;
        self.validate_product_task_workspace_preparation_root(task_id, &receipt)?;
        validate_product_task_workspace_preparation_marker(task_id, &receipt, false)?;
        self.validate_product_task_workspace_preparation_target_boundary(intake, &receipt)?;

        // A receipt-owned path can only be terminalized after the current
        // target boundary is still usable. Git worktrees require both Git
        // metadata and path reconciliation; local-folder staging copies have
        // no Git metadata, but still require exact app-owned path proof.
        validate_product_task_workspace_prerequisites(intake).map_err(|_| {
            product_task_workspace_preparation_reconciliation_error(
                "pinned workspace removal precondition is unavailable",
            )
        })?;
        let target_canonical = std::fs::canonicalize(&intake.target_repo_path).map_err(|_| {
            product_task_workspace_preparation_reconciliation_error(
                "pinned target source is unavailable for workspace removal",
            )
        })?;

        let provisional = provisional_run_id_for_task(task_id);
        let supervised_workspace_count = self
            .supervised_patch_workspace_count_for_run(&provisional)
            .map_err(|_| {
                product_task_workspace_preparation_reconciliation_error(
                    "workspace cleanup is unavailable",
                )
            })?;
        if supervised_workspace_count > 1 {
            return Err(product_task_workspace_preparation_reconciliation_error(
                "multiple supervised workspaces match the provisional run",
            ));
        }
        let supervised_workspace = self
            .get_supervised_patch_workspace_for_run(&provisional)
            .map_err(|_| {
                product_task_workspace_preparation_reconciliation_error(
                    "workspace cleanup is unavailable",
                )
            })?;

        if let Some(ws) = supervised_workspace {
            if ws.get("run_id").and_then(Value::as_str) != Some(provisional.as_str()) {
                return Err(product_task_workspace_preparation_reconciliation_error(
                    "supervised workspace run does not match the receipt",
                ));
            }
            let workspace_id = ws
                .get("workspace_id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    product_task_workspace_preparation_reconciliation_error(
                        "supervised workspace identity is unavailable",
                    )
                })?;
            let workspace_path = ws
                .get("workspace_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    product_task_workspace_preparation_reconciliation_error(
                        "supervised workspace path is unavailable",
                    )
                })?;
            let workspace_canonical_path = ws
                .get("workspace_canonical_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    product_task_workspace_preparation_reconciliation_error(
                        "supervised workspace canonical path is unavailable",
                    )
                })?;
            if Path::new(workspace_path) != receipt.workspace_path
                || Path::new(workspace_canonical_path) != receipt.workspace_path
            {
                return Err(product_task_workspace_preparation_reconciliation_error(
                    "supervised workspace path does not match the receipt",
                ));
            }
            let workspace_target = ws
                .get("target_repo_canonical_path")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    product_task_workspace_preparation_reconciliation_error(
                        "supervised workspace target is unavailable",
                    )
                })?;
            if Path::new(workspace_target) != target_canonical {
                return Err(product_task_workspace_preparation_reconciliation_error(
                    "supervised workspace target does not match the pinned target",
                ));
            }
            remove_product_task_workspace_or_reconcile(
                intake,
                &receipt,
                &target_canonical,
                task_id,
            )?;
            if ws.get("status").and_then(Value::as_str) != Some("cleaned") {
                self.update_workspace_status(workspace_id, "cleaned", actor)
                    .map_err(|_| {
                        product_task_workspace_preparation_reconciliation_error(
                            "supervised workspace cleanup status is unavailable",
                        )
                    })?;
            }
        } else {
            remove_product_task_workspace_or_reconcile(
                intake,
                &receipt,
                &target_canonical,
                task_id,
            )?;
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
            let failed = current.ok_or_else(|| "failed task missing".to_string())?;
            let _ = self.retire_product_task_workspace_preparation_receipt(
                task_id,
                &receipt,
                actor,
                "compensated_failure",
            );
            return Ok(failed);
        }
        let failed = self.transition_product_task(
            task_id,
            ProductTaskStatus::Failed,
            version,
            actor,
            None,
            None,
            Some(code),
            Some(detail),
            None,
        )?;
        let _ = self.retire_product_task_workspace_preparation_receipt(
            task_id,
            &receipt,
            actor,
            "compensated_failure",
        );
        Ok(failed)
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

        // Stage deterministic apply helper inside the bound worktree (app-owned only).
        if resolved == "command" {
            stage_product_apply_helper(Path::new(workspace_path), &task)?;
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
        self.product_execution_objective(
            task_id,
            task.get("objective_fingerprint")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )?;

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
        // Stay at GraphReady: a run is scheduler-eligible, but nodes are not yet leased.
        // Do not mark Running merely because a run was created.
        let task = self.transition_product_task(
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

        Ok(json!({
            "task": task,
            "plan": plan,
            "run": run,
            "resolved_executor": resolved,
            "executor_class": if resolved == "command" || resolved == "deterministic" {
                "fixture_deterministic"
            } else {
                "managed_coding"
            },
            "reused": false,
            "execution_admitted": true,
            "scheduler_eligible": true,
        }))
    }

    /// Prepare the exact, but still non-runnable, managed-DeepSeek plan used by
    /// delegated autonomy. Approval and spend issuance deliberately happen
    /// outside this method after the final manifest is returned.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_delegated_managed_product_task(
        &self,
        task_id: &str,
        actor: &str,
        available_executors: &[String],
        proposal: &Value,
        delegation: &DelegationContract,
        attempt_id: &str,
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".into());
        }
        if attempt_id.trim().is_empty() {
            return Err("delegated attempt_id is required".into());
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let tenant_id = task
            .get("tenant_id")
            .and_then(Value::as_str)
            .ok_or("delegated ProductTask tenant_id is missing")?;
        self.require_delegation_product_task(&delegation.delegation_id, tenant_id, task_id)?;
        self.require_approved_delegated_proposal(&delegation.delegation_id, proposal)?;
        let status =
            ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if status != ProductTaskStatus::WorkspaceBound {
            return Err(format!(
                "delegated prepare requires workspace_bound task; status={}",
                status.as_str()
            ));
        }
        let workspace_binding = task
            .get("workspace_binding")
            .ok_or("workspace_binding missing")?;
        let proposal_target = normalize_delegated_proposal_target_identity(proposal)?;
        if proposal_target.repository != "Igzela/alters-lab"
            || Some(proposal_target.main_sha.as_str())
                != task.get("source_revision").and_then(Value::as_str)
            || Some(&json!(&proposal_target.mutable_paths))
                != workspace_binding.get("allowed_paths")
        {
            return Err("delegated proposal does not match the bound ProductTask target".into());
        }
        let verification_commands = task
            .pointer("/intake/verification_commands")
            .and_then(Value::as_array)
            .ok_or("delegated ProductTask verification commands are missing")?;
        let verifier_cmd = verification_commands
            .first()
            .and_then(|c| c.get("command").and_then(Value::as_str))
            .unwrap_or("");
        let verifier_timeout = verification_commands
            .first()
            .and_then(|c| c.get("timeout_ms").and_then(Value::as_u64))
            .unwrap_or(0);
        let is_docs = verification_commands.len() == 1
            && verifier_cmd
                == "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md"
            && (1..=5_000).contains(&verifier_timeout);
        let is_frozen_rwe = verification_commands.len() == 1
            && crate::rwe::frozen_rwe_bindings::is_exact_frozen_rwe_verifier_command(verifier_cmd)
            && (1..=900_000).contains(&verifier_timeout)
            && {
                let allowed = workspace_binding
                    .get("allowed_paths")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let risk = task.get("risk_class").and_then(Value::as_str).unwrap_or("");
                let rev = task
                    .get("source_revision")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                crate::rwe::frozen_rwe_bindings::is_exact_frozen_rwe_product_intake(
                    risk,
                    rev,
                    &allowed,
                    verifier_cmd,
                    verifier_timeout,
                )
            };
        if !is_docs && !is_frozen_rwe {
            return Err(
                "delegated ProductTask verifier is not the exact bounded docs check or exact frozen RWE pytest"
                    .into(),
            );
        }
        let workspace_path = workspace_binding
            .pointer("/workspace_path")
            .and_then(Value::as_str)
            .ok_or("workspace_path missing")?;
        if !Path::new(workspace_path).is_dir() {
            return Err("bound worktree is missing; zero execution effect".into());
        }
        let workspace_record_id = task
            .get("workspace_record_id")
            .and_then(Value::as_str)
            .ok_or("workspace_record_id missing")?;
        let workspace = self
            .get_supervised_patch_workspace(workspace_record_id)?
            .ok_or("supervised workspace record missing")?;
        if matches!(
            workspace.get("status").and_then(Value::as_str),
            Some("quarantined" | "cleaned" | "rejected")
        ) {
            return Err(
                "workspace status blocks delegated preparation; zero execution effect".into(),
            );
        }
        let executor_policy: ProductExecutorPolicy = serde_json::from_value(
            task.pointer("/intake/executor_policy")
                .cloned()
                .unwrap_or(json!({"allowed_executors":["command"]})),
        )
        .map_err(|e| format!("executor_policy malformed: {e}"))?;
        let resolved = resolve_admitted_executor(&executor_policy)?;
        if resolved != "managed_deepseek"
            || !available_executors.iter().any(|value| value == &resolved)
        {
            return Err("delegated route requires an available managed_deepseek executor".into());
        }
        self.product_execution_objective(
            task_id,
            task.get("objective_fingerprint")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )?;
        let objective_preview = task
            .pointer("/intake/objective_preview")
            .and_then(Value::as_str)
            .unwrap_or("product golden path task");
        let mut deferred_task = task.clone();
        deferred_task["managed_deepseek_deferred_binding"] = json!(true);
        let planner = ReadOnlyPlanner::new();
        let plan_owner_id = delegated_product_plan_owner_id(task_id);
        let plan = self.create_or_recover_delegated_workflow_plan(
            task_id,
            &plan_owner_id,
            objective_preview,
            actor,
            |ids, created_at| {
                let graph = compile_product_executable_graph(&deferred_task, created_at, ids, &resolved)?;
                let analysis = planner.create_plan(ids, objective_preview, "product_golden_path_delegated", created_at)?
                    .get("analysis").cloned().unwrap_or(json!({}));
                Ok(json!({
                    "schema_version": READ_ONLY_PLAN_SCHEMA_VERSION,
                    "plan_id": ids.plan_id,
                    "plan_sequence": ids.sequence,
                    "created_at": created_at,
                    "updated_at": created_at,
                    "raw_request": objective_preview,
                    "request_source": "product_golden_path_delegated",
                    "status": "planned_executable",
                    "workflow_id": ids.workflow_id,
                    "dispatch_id": ids.dispatch_id,
                    "analysis": analysis,
                    "graph": graph,
                    "validation": {"valid": true, "errors": []},
                    "execution_order": graph.get("nodes").and_then(Value::as_array).map(|nodes| nodes.iter().filter_map(|node| node.get("node_id").cloned()).collect::<Vec<_>>()).unwrap_or_default(),
                    "advisory": {"schema_version":"plan_advisory.v1","requires_executor":"managed_deepseek","product_task_id":task_id,"delegated_binding":"required","delegated_plan_owner_id":plan_owner_id},
                    "boundaries": {"execution_authority":"delegated_product_golden_path","provider_calls":"not_invoked","target_repository_writes":"disabled","approval_execution_authority":"separate","product_task_id":task_id,"workspace_id":workspace_record_id,"source_revision":workspace_binding.get("source_revision")}
                }))
            },
        )?;
        validate_deferred_delegated_product_plan(&plan, task_id)?;
        let workflow_id = plan
            .get("workflow_id")
            .and_then(Value::as_str)
            .ok_or("plan missing workflow_id")?;
        let node_ids = plan
            .pointer("/graph/nodes")
            .and_then(Value::as_array)
            .ok_or("delegated plan nodes missing")?
            .iter()
            .filter_map(|node| node.get("node_id").cloned())
            .collect::<Vec<_>>();
        let execution = json!({
            "observed_at": plan.get("created_at"),
            "target_repository": proposal_target.repository,
            "target_main_sha": task.get("source_revision"),
            "tenant_id": tenant_id,
            "product_task_id": task_id,
            "workflow_id": workflow_id,
            "workflow_node_ids": node_ids,
            "attempt_id": attempt_id,
            "verifier": if is_frozen_rwe {
                crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY
            } else {
                crate::rwe::frozen_rwe_bindings::DOCS_GP_VERIFIER_IDENTITY
            },
            "mutable_paths": workspace_binding.get("allowed_paths"),
            "cancellation_identity": format!("product-task:{task_id}:attempt:{attempt_id}:cancel"),
            "rollback_identity": format!("product-task:{task_id}:workspace:{workspace_record_id}:rollback")
        });
        let manifest = derive_final_execution_manifest(proposal, delegation, &execution)?;
        Ok(
            json!({"task": self.get_product_task(task_id)?, "plan": plan, "final_manifest": manifest, "scheduler_eligible": false}),
        )
    }

    /// Consume already-issued delegated receipts, bind the exact graph, then
    /// create the existing scheduler-owned run. This method cannot issue or
    /// renew an approval, spend receipt, or attempt lease.
    pub fn activate_delegated_managed_product_task(
        &self,
        task_id: &str,
        actor: &str,
        manifest: &Value,
        spend_authorization_id: &str,
        attempt_lease_id: &str,
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".into());
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let tenant_id = task
            .get("tenant_id")
            .and_then(Value::as_str)
            .ok_or("delegated ProductTask tenant_id is missing")?;
        let delegation_id = manifest
            .pointer("/delegation/delegation_id")
            .and_then(Value::as_str)
            .ok_or("delegated manifest delegation_id is missing")?;
        self.require_delegation_product_task(delegation_id, tenant_id, task_id)?;
        let task_status =
            ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if !matches!(
            task_status,
            ProductTaskStatus::WorkspaceBound | ProductTaskStatus::GraphReady
        ) {
            return Err(
                "delegated activation requires workspace_bound or exact graph_ready replay".into(),
            );
        }
        let plan_id = task
            .get("plan_id")
            .and_then(Value::as_str)
            .ok_or("delegated plan is missing")?;
        let manifest_sha256 = manifest
            .get("manifest_sha256")
            .and_then(Value::as_str)
            .ok_or("delegated manifest hash is missing")?;
        if manifest_sha256 != super::managed_acceptance::compute_attempt_manifest_sha256(manifest)?
        {
            return Err("delegated manifest hash is stale or malformed".into());
        }
        let execution = manifest
            .get("execution")
            .ok_or("delegated manifest execution is missing")?;
        if execution.get("product_task_id").and_then(Value::as_str) != Some(task_id)
            || execution.get("tenant_id").and_then(Value::as_str) != Some(tenant_id)
            || execution.get("target_main_sha") != task.get("source_revision")
        {
            return Err("delegated manifest task identity is stale or mismatched".into());
        }
        let binding = crate::provider::managed_deepseek::ManagedCallBinding {
            product_task_id: task_id.into(),
            workflow_id: execution
                .get("workflow_id")
                .and_then(Value::as_str)
                .ok_or("delegated workflow_id missing")?
                .into(),
            node_id: format!(
                "{}-planning",
                execution
                    .get("workflow_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            ),
            attempt_id: execution
                .get("attempt_id")
                .and_then(Value::as_str)
                .ok_or("delegated attempt_id missing")?
                .into(),
            spend_authorization_id: spend_authorization_id.into(),
            attempt_lease_id: attempt_lease_id.into(),
        };
        if self
            .current_delegated_provider_authority(&binding)?
            .is_none()
        {
            return Err("delegated activation requires a current persisted authority".into());
        }
        let plan = self.bind_delegated_managed_deepseek_plan(plan_id, &binding, actor)?;
        if task_status == ProductTaskStatus::GraphReady {
            let run_id = task
                .get("run_id")
                .and_then(Value::as_str)
                .ok_or("delegated graph_ready replay is missing its run")?;
            let run = self
                .get_workflow_run(run_id)?
                .ok_or("delegated graph_ready replay run disappeared")?;
            if run.get("plan_id").and_then(Value::as_str) != Some(plan_id)
                || run.get("workflow_id").and_then(Value::as_str)
                    != Some(binding.workflow_id.as_str())
            {
                return Err("delegated graph_ready replay run binding changed".into());
            }
            return Ok(json!({
                "task": task,
                "plan": plan,
                "run": run,
                "scheduler_eligible": true,
                "replayed": true,
            }));
        }
        let tenant_id = task
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("local");
        let workspace_scope = task
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("default");
        let run = self.create_or_reuse_delegated_workflow_run(
            plan_id,
            actor,
            tenant_id,
            workspace_scope,
        )?;
        let run_id = run
            .get("run_id")
            .and_then(Value::as_str)
            .ok_or("run missing run_id")?
            .to_string();
        let workspace_record_id = task
            .get("workspace_record_id")
            .and_then(Value::as_str)
            .ok_or("workspace_record_id missing")?;
        self.rebind_supervised_workspace_run_id(workspace_record_id, &run_id, actor)?;
        self.bind_product_task_plan_run(task_id, plan_id, &run_id, actor)?;
        let transitioned = self.transition_product_task(
            task_id,
            ProductTaskStatus::GraphReady,
            task.get("version").and_then(Value::as_u64),
            actor,
            None,
            None,
            None,
            None,
            Some(&run_id),
        )?;
        Ok(
            json!({"task": transitioned, "plan": plan, "run": run, "scheduler_eligible": true, "replayed": false}),
        )
    }

    pub(super) fn product_execution_objective(
        &self,
        task_id: &str,
        expected_fingerprint: &str,
    ) -> Result<String, String> {
        let intake_json = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT intake_json FROM product_tasks WHERE task_id = ?1",
                    [task_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT intake_json FROM product_tasks WHERE task_id = $1",
                        &[&task_id],
                    )
                    .map_err(|error| error.to_string())
                    .map(|row| row.map(|row| row.get::<_, String>(0)))
            })?,
        }
        .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let intake: Value = serde_json::from_str(&intake_json)
            .map_err(|_| "product task persisted intake is malformed".to_string())?;
        let objective = intake
            .pointer("/_execution_objective_v1/objective")
            .and_then(Value::as_str)
            .or_else(|| intake.get("objective_preview").and_then(Value::as_str))
            .filter(|value| fingerprint_objective(value) == expected_fingerprint)
            .ok_or_else(|| {
                "product task exact execution objective is unavailable or does not match its fingerprint"
                    .to_string()
            })?;
        Ok(objective.to_string())
    }

    /// Observe persisted run state and advance product-task lifecycle without executing nodes.
    ///
    /// This is not a second scheduler. Callers that need node advancement must use the
    /// existing scheduler workers or an explicit operational tick path.
    pub fn sync_product_task_from_run(&self, task_id: &str, actor: &str) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let status =
            ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if status.is_terminal()
            || matches!(
                status,
                ProductTaskStatus::AwaitingApproval
                    | ProductTaskStatus::OutputPending
                    | ProductTaskStatus::Verifying
                    | ProductTaskStatus::RepairPending
            )
        {
            return Ok(task);
        }
        let run_id = match task.get("run_id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => return Ok(task),
        };
        let run = match self.get_workflow_run(&run_id)? {
            Some(run) => run,
            None => return Ok(task),
        };
        let run_status = run.get("status").and_then(Value::as_str).unwrap_or("");
        let nodes = run
            .get("nodes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let any_leased_or_started = nodes.iter().any(|n| {
            matches!(
                n.get("status").and_then(Value::as_str).unwrap_or(""),
                "leased" | "running" | "completed" | "failed" | "blocked"
            )
        });
        let version = task.get("version").and_then(Value::as_u64);
        match run_status {
            "failed" | "cancelled" | "killed" => {
                let (target_status, code, detail) =
                    product_run_failure_transition(&run, run_status);
                self.transition_product_task(
                    task_id,
                    target_status,
                    version,
                    actor,
                    None,
                    None,
                    Some(code),
                    Some(detail),
                    None,
                )
            }
            "completed" => {
                // Execution finished; remain GraphReady/Running until finalize starts verifying.
                if status == ProductTaskStatus::GraphReady && any_leased_or_started {
                    self.transition_product_task(
                        task_id,
                        ProductTaskStatus::Running,
                        version,
                        actor,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                } else {
                    Ok(task)
                }
            }
            "running" | "active" | "leased" => {
                if status == ProductTaskStatus::GraphReady {
                    self.transition_product_task(
                        task_id,
                        ProductTaskStatus::Running,
                        version,
                        actor,
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                } else {
                    Ok(task)
                }
            }
            _ if any_leased_or_started && status == ProductTaskStatus::GraphReady => self
                .transition_product_task(
                    task_id,
                    ProductTaskStatus::Running,
                    version,
                    actor,
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            _ => Ok(task),
        }
    }

    /// Post-terminal processor: observe scheduler-owned run state, execute declared
    /// verification commands through the command verification owner, record evidence via
    /// supervised-patch verification, capture artifacts only on trustworthy pass, then
    /// enter awaiting_approval.
    ///
    /// Does **not** create or drive an executor tick loop. Node execution belongs solely
    /// to the existing scheduler / operational tick path.
    pub fn finalize_product_task_after_execution(
        &self,
        task_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        self.finalize_product_task_after_execution_with_authority(task_id, actor, &|| {
            Ok(ProductVerificationRuntimeAuthority::manual_operational())
        })
    }

    pub fn finalize_product_task_after_execution_with_authority(
        &self,
        task_id: &str,
        actor: &str,
        runtime_authority: &dyn Fn() -> Result<ProductVerificationRuntimeAuthority, String>,
    ) -> Result<Value, String> {
        let commit_authority = |operation: &mut dyn FnMut() -> Result<(Value, Value), String>| {
            runtime_authority()
                .map_err(|error| format!("runtime_authority_unavailable:{error}"))?
                .validate()
                .map_err(|reason| format!("runtime_authority_lost:{reason}"))?;
            operation()
        };
        self.finalize_product_task_after_execution_with_commit_authority(
            task_id,
            actor,
            runtime_authority,
            &commit_authority,
        )
    }

    pub(crate) fn finalize_product_task_after_execution_with_commit_authority(
        &self,
        task_id: &str,
        actor: &str,
        runtime_authority: &dyn Fn() -> Result<ProductVerificationRuntimeAuthority, String>,
        commit_authority: &dyn ProductArtifactCommitAuthority,
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        let task = self.sync_product_task_from_run(task_id, actor)?;
        let status =
            ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if matches!(
            status,
            ProductTaskStatus::AwaitingApproval
                | ProductTaskStatus::OutputPending
                | ProductTaskStatus::Completed
        ) {
            return Ok(json!({"task": task, "reused": true, "phase": status.as_str()}));
        }
        if matches!(
            status,
            ProductTaskStatus::Failed
                | ProductTaskStatus::Killed
                | ProductTaskStatus::Blocked
                | ProductTaskStatus::BudgetExhausted
                | ProductTaskStatus::OutcomeUnknown
        ) {
            let delegated_terminal = self.close_delegated_failure_if_bound(task_id, actor)?;
            return Ok(json!({
                "task": task,
                "reused": true,
                "phase": "terminal_failure",
                "delegated_terminal": delegated_terminal,
            }));
        }
        if !matches!(
            status,
            ProductTaskStatus::GraphReady
                | ProductTaskStatus::Running
                | ProductTaskStatus::Verifying
        ) {
            return Err(format!(
                "finalize requires graph_ready/running/verifying task; status={}",
                status.as_str()
            ));
        }
        let run_id = task
            .get("run_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "product task missing run_id".to_string())?
            .to_string();
        let workspace_record_id = task
            .get("workspace_record_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "product task missing workspace_record_id".to_string())?
            .to_string();
        let workspace_path = task
            .pointer("/workspace_binding/workspace_path")
            .and_then(Value::as_str)
            .ok_or_else(|| "product task missing workspace_path".to_string())?
            .to_string();
        if !Path::new(&workspace_path).is_dir() {
            return Err("worktree missing during finalize; zero output effect".to_string());
        }

        let run = self
            .get_workflow_run(&run_id)?
            .ok_or_else(|| "workflow run missing".to_string())?;
        let run_status = run.get("status").and_then(Value::as_str).unwrap_or("");

        // Scheduler has not finished execution — observe only; do not tick.
        if !matches!(run_status, "completed" | "failed" | "cancelled" | "killed") {
            return Ok(json!({
                "task": task,
                "run": run,
                "phase": "waiting_for_scheduler",
                "reused": false,
                "execution_admitted": true,
                "note": "existing scheduler workers (or operational tick) must advance the run; finalize does not execute nodes",
            }));
        }
        if matches!(run_status, "failed" | "cancelled" | "killed") {
            let version = task.get("version").and_then(Value::as_u64);
            let (target_status, code, detail) = product_run_failure_transition(&run, run_status);
            let failed = self.transition_product_task(
                task_id,
                target_status,
                version,
                actor,
                None,
                None,
                Some(code),
                Some(detail),
                None,
            )?;
            let delegated_terminal = self.close_delegated_failure_if_bound(task_id, actor)?;
            return Ok(json!({
                "task": failed,
                "run": run,
                "phase": "execution_failed",
                "reused": false,
                "delegated_terminal": delegated_terminal,
            }));
        }

        // Enter verifying only after authoritative run completion.
        let version = self
            .get_product_task(task_id)?
            .and_then(|t| t.get("version").and_then(Value::as_u64));
        if status != ProductTaskStatus::Verifying {
            // GraphReady → Running (if needed) → Verifying
            let mut current_status = status;
            if current_status == ProductTaskStatus::GraphReady {
                self.transition_product_task(
                    task_id,
                    ProductTaskStatus::Running,
                    version,
                    actor,
                    None,
                    None,
                    None,
                    None,
                    None,
                )?;
                current_status = ProductTaskStatus::Running;
            }
            if current_status == ProductTaskStatus::Running {
                let v = self
                    .get_product_task(task_id)?
                    .and_then(|t| t.get("version").and_then(Value::as_u64));
                self.transition_product_task(
                    task_id,
                    ProductTaskStatus::Verifying,
                    v,
                    actor,
                    None,
                    None,
                    None,
                    None,
                    None,
                )?;
            }
        }

        let source_revision = task
            .pointer("/workspace_binding/source_revision")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let tenant_id = task
            .get("tenant_id")
            .and_then(Value::as_str)
            .unwrap_or("local")
            .to_string();
        let workspace_scope = task
            .get("workspace_id")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_string();
        let task_version = self
            .get_product_task(task_id)?
            .and_then(|t| t.get("version").and_then(Value::as_u64))
            .unwrap_or(0);

        let mut verification = self.execute_and_record_product_verifications(
            task_id,
            &tenant_id,
            &workspace_scope,
            &run_id,
            &workspace_record_id,
            &workspace_path,
            &source_revision,
            task_version,
            &task,
            actor,
            runtime_authority,
        )?;

        let verification_status = verification
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("verification_failed");
        if verification_status == "authority_lost" {
            let reason = verification
                .get("authority_loss_reason")
                .and_then(Value::as_str)
                .unwrap_or("verification_authority_lost");
            let current = self.get_product_task(task_id)?.ok_or_else(|| {
                "product task disappeared after verification authority loss".to_string()
            })?;
            let current_status = ProductTaskStatus::parse(
                current.get("status").and_then(Value::as_str).unwrap_or(""),
            )?;
            let current_version = current.get("version").and_then(Value::as_u64);
            let target_status = if reason.contains("budget_exhausted") {
                ProductTaskStatus::BudgetExhausted
            } else if reason.contains("kill") {
                ProductTaskStatus::Killed
            } else if reason.contains("pause") {
                ProductTaskStatus::Paused
            } else {
                ProductTaskStatus::Blocked
            };
            let task = if current_status == ProductTaskStatus::Verifying {
                self.transition_product_task(
                    task_id,
                    target_status,
                    current_version,
                    actor,
                    None,
                    None,
                    Some("verification_authority_lost"),
                    Some(reason),
                    None,
                )?
            } else {
                current
            };
            let delegated_terminal = if matches!(
                target_status,
                ProductTaskStatus::Killed
                    | ProductTaskStatus::Blocked
                    | ProductTaskStatus::BudgetExhausted
            ) {
                self.close_delegated_failure_if_bound(task_id, actor)?
            } else {
                None
            };
            return Ok(json!({
                "task": task,
                "run": run,
                "verification": verification,
                "phase": "verification_authority_lost",
                "reused": false,
                "artifact_id": Value::Null,
                "delegated_terminal": delegated_terminal,
            }));
        }
        if verification_status != "evidence_recorded" {
            let version = self
                .get_product_task(task_id)?
                .and_then(|t| t.get("version").and_then(Value::as_u64));
            let outcome_unknown = verification_status == "outcome_unknown";
            let failed = self.transition_product_task(
                task_id,
                if outcome_unknown {
                    ProductTaskStatus::OutcomeUnknown
                } else {
                    ProductTaskStatus::Failed
                },
                version,
                actor,
                None,
                None,
                Some(if outcome_unknown {
                    "verification_outcome_unknown"
                } else {
                    "verification_failed"
                }),
                Some(verification_status),
                None,
            )?;
            let delegated_terminal = self.close_delegated_failure_if_bound(task_id, actor)?;
            return Ok(json!({
                "task": failed,
                "run": run,
                "verification": verification,
                "phase": if outcome_unknown { "verification_outcome_unknown" } else { "verification_failed" },
                "reused": false,
                "artifact_id": Value::Null,
                "delegated_terminal": delegated_terminal,
            }));
        }

        let verified_patch_hash = verification
            .get("verification_attempts")
            .and_then(Value::as_array)
            .and_then(|attempts| attempts.last())
            .and_then(|attempt| attempt.get("workspace_hash_after"))
            .and_then(Value::as_str)
            .ok_or_else(|| "verification is missing its final patch identity".to_string())?;
        let authoritative_node_id = run
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| nodes.first())
            .and_then(|node| node.get("node_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| "authoritative execution node missing before capture".to_string())?;
        let node_authority = product_verification_node_authority(&run, authoritative_node_id)?;
        let current_patch_hash = self.validate_product_verification_authority(
            task_id,
            &run_id,
            &workspace_record_id,
            &workspace_path,
            &source_revision,
            task_version,
            &node_authority,
            runtime_authority,
        )?;
        if current_patch_hash != verified_patch_hash {
            self.record_product_verification_authority_loss(
                task_id,
                &workspace_record_id,
                "late_filesystem_write_before_artifact_capture",
                true,
                actor,
            )?;
            return Err("verified patch identity changed before artifact capture".to_string());
        }

        // Artifact insertion, workspace update, task-version CAS, transition, and both
        // audits commit in one backend transaction under the exact verified patch identity.
        let mut artifact_operation = || {
            self.finalize_product_verification_artifact(
                task_id,
                task_version,
                &workspace_record_id,
                verified_patch_hash,
                actor,
            )
        };
        let artifact_result = commit_authority.commit(&mut artifact_operation);
        let (artifact, task) = match artifact_result {
            Ok(result) => result,
            Err(error)
                if error.starts_with("runtime_authority_lost:")
                    || error.starts_with("runtime_authority_unavailable:")
                    || error.starts_with(
                        "product artifact path is outside product task allowed_paths:",
                    ) =>
            {
                let artifact_scope_violation = error
                    .starts_with("product artifact path is outside product task allowed_paths:");
                let reason = if artifact_scope_violation {
                    format!("workspace_allowed_path_violation:{error}")
                } else {
                    error
                };
                verification["status"] = json!("authority_lost");
                verification["result_status"] = json!("failed");
                verification["trustworthy"] = json!(false);
                verification["authority_loss_reason"] = json!(reason.clone());
                verification["recorded_at"] = json!(self.now());
                self.record_workspace_verification(&workspace_record_id, &verification, actor)?;
                self.record_product_verification_authority_loss(
                    task_id,
                    &workspace_record_id,
                    &reason,
                    artifact_scope_violation,
                    actor,
                )?;
                let current = self.get_product_task(task_id)?.ok_or_else(|| {
                    "product task disappeared after artifact authority loss".to_string()
                })?;
                let current_status = ProductTaskStatus::parse(
                    current.get("status").and_then(Value::as_str).unwrap_or(""),
                )?;
                let target_status = if reason.contains("kill") {
                    ProductTaskStatus::Killed
                } else if reason.contains("pause") {
                    ProductTaskStatus::Paused
                } else {
                    ProductTaskStatus::Blocked
                };
                let task = if current_status == ProductTaskStatus::Verifying {
                    self.transition_product_task(
                        task_id,
                        target_status,
                        current.get("version").and_then(Value::as_u64),
                        actor,
                        None,
                        None,
                        Some("verification_authority_lost"),
                        Some(&reason),
                        None,
                    )?
                } else {
                    current
                };
                let delegated_terminal = if matches!(
                    target_status,
                    ProductTaskStatus::Killed | ProductTaskStatus::Blocked
                ) {
                    self.close_delegated_failure_if_bound(task_id, actor)?
                } else {
                    None
                };
                return Ok(json!({
                    "task": task,
                    "run": run,
                    "verification": verification,
                    "phase": "verification_authority_lost",
                    "reused": false,
                    "artifact_id": Value::Null,
                    "delegated_terminal": delegated_terminal,
                }));
            }
            Err(error) => return Err(error),
        };
        let artifact_id = artifact
            .get("artifact_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        Ok(json!({
            "task": task,
            "run": run,
            "verification": verification,
            "artifact": artifact,
            "artifact_id": artifact_id,
            "phase": "awaiting_approval",
            "reused": false,
        }))
    }

    pub(crate) fn finalize_product_task_after_execution_with_commit_authority_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
        actor: &str,
        runtime_authority: &dyn Fn() -> Result<ProductVerificationRuntimeAuthority, String>,
        commit_authority: &dyn ProductArtifactCommitAuthority,
    ) -> Result<Value, String> {
        self.require_product_task_tenant(task_id, tenant_id)?;
        self.finalize_product_task_after_execution_with_commit_authority(
            task_id,
            actor,
            runtime_authority,
            commit_authority,
        )
    }

    /// Execute every declared verification command via CommandNodeExecutor (same allowlisted
    /// owner used by supervised-patch verification) and persist authoritative receipts.
    /// Never writes `result: pass` before execution. Fail-closed on any non-pass outcome.
    pub fn execute_and_record_product_verifications(
        &self,
        task_id: &str,
        tenant_id: &str,
        workspace_scope: &str,
        run_id: &str,
        workspace_record_id: &str,
        workspace_path: &str,
        source_revision: &str,
        expected_task_version: u64,
        task: &Value,
        actor: &str,
        runtime_authority: &dyn Fn() -> Result<ProductVerificationRuntimeAuthority, String>,
    ) -> Result<Value, String> {
        let commands = task
            .pointer("/intake/verification_commands")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if commands.is_empty() {
            return Err("no verification_commands declared on product task".to_string());
        }

        let initial_run = self
            .get_workflow_run(run_id)?
            .ok_or_else(|| "workflow run missing before verification".to_string())?;
        let node_id = initial_run
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| nodes.first())
            .and_then(|node| node.get("node_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| "authoritative execution node missing before verification".to_string())?
            .to_string();
        let node_authority = product_verification_node_authority(&initial_run, &node_id)?;

        let mut attempts: Vec<Value> = Vec::new();
        let mut all_passed = true;
        let mut final_status = "evidence_recorded";
        let mut authority_loss_reason: Option<String> = None;
        let mut previous_workspace_hash: Option<String> = None;

        for (idx, cmd_val) in commands.iter().enumerate() {
            let command = cmd_val
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("verification command {idx} missing command"))?
                .to_string();
            let declared_timeout_ms = cmd_val
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(30_000)
                .clamp(1, 3_600_000);
            let attempt_number = (idx as u64) + 1;
            let started_at = self.now();

            let pre_command_hash = match self.validate_product_verification_authority(
                task_id,
                run_id,
                workspace_record_id,
                workspace_path,
                source_revision,
                expected_task_version,
                &node_authority,
                runtime_authority,
            ) {
                Ok(hash) => hash,
                Err(reason) => {
                    self.record_product_verification_authority_loss(
                        task_id,
                        workspace_record_id,
                        &reason,
                        false,
                        actor,
                    )?;
                    authority_loss_reason = Some(reason.clone());
                    all_passed = false;
                    final_status = "authority_lost";
                    attempts.push(json!({
                        "schema_version": "product_verification_attempt.v2",
                        "attempt": attempt_number,
                        "command": command,
                        "timeout_ms": declared_timeout_ms,
                        "started_at": started_at,
                        "completed_at": self.now(),
                        "exit_status": null,
                        "process_outcome": null,
                        "result_status": "stale_rejected",
                        "trustworthy": false,
                        "late_result_rejected": false,
                        "error_domain": "verification_authority_lost",
                        "error_message": reason,
                        "product_task_id": task_id,
                        "run_id": run_id,
                        "node_id": node_id,
                        "workspace_record_id": workspace_record_id,
                        "workspace_path": workspace_path,
                        "source_revision": source_revision,
                        "expected_task_version": expected_task_version,
                    }));
                    break;
                }
            };
            if previous_workspace_hash
                .as_ref()
                .is_some_and(|previous| previous != &pre_command_hash)
            {
                let reason = "late_filesystem_write_between_verification_commands".to_string();
                self.record_product_verification_authority_loss(
                    task_id,
                    workspace_record_id,
                    &reason,
                    false,
                    actor,
                )?;
                authority_loss_reason = Some(reason.clone());
                all_passed = false;
                final_status = "authority_lost";
                attempts.push(json!({
                    "schema_version": "product_verification_attempt.v2",
                    "attempt": attempt_number,
                    "command": command,
                    "timeout_ms": declared_timeout_ms,
                    "started_at": started_at,
                    "completed_at": self.now(),
                    "exit_status": null,
                    "process_outcome": null,
                    "result_status": "stale_rejected",
                    "trustworthy": false,
                    "late_result_rejected": true,
                    "error_domain": "verification_authority_lost",
                    "error_message": reason,
                    "product_task_id": task_id,
                    "run_id": run_id,
                    "node_id": node_id,
                    "workspace_record_id": workspace_record_id,
                    "workspace_path": workspace_path,
                    "source_revision": source_revision,
                    "expected_task_version": expected_task_version,
                }));
                break;
            }

            let remaining_elapsed_ms = self.product_task_remaining_elapsed_ms(task_id)?;
            if remaining_elapsed_ms == 0 {
                let reason = "budget_exhausted:total_elapsed_ms reached before command".to_string();
                self.record_product_verification_authority_loss(
                    task_id,
                    workspace_record_id,
                    &reason,
                    false,
                    actor,
                )?;
                authority_loss_reason = Some(reason.clone());
                all_passed = false;
                final_status = "authority_lost";
                attempts.push(json!({
                    "schema_version": "product_verification_attempt.v2",
                    "attempt": attempt_number,
                    "command": command,
                    "declared_timeout_ms": declared_timeout_ms,
                    "effective_timeout_ms": 0,
                    "started_at": started_at,
                    "completed_at": self.now(),
                    "exit_status": null,
                    "process_outcome": null,
                    "result_status": "stale_rejected",
                    "trustworthy": false,
                    "late_result_rejected": false,
                    "error_domain": "verification_authority_lost",
                    "error_message": reason,
                    "product_task_id": task_id,
                    "run_id": run_id,
                    "node_id": node_id,
                    "workspace_record_id": workspace_record_id,
                    "source_revision": source_revision,
                    "expected_task_version": expected_task_version,
                }));
                break;
            }
            let effective_timeout_ms = declared_timeout_ms.min(remaining_elapsed_ms);
            let execution = self.execute_managed_product_verification_command(
                task_id,
                tenant_id,
                workspace_scope,
                workspace_record_id,
                workspace_path,
                source_revision,
                attempt_number,
                &command,
                &pre_command_hash,
                effective_timeout_ms,
                actor,
            );
            let (output, verification_run_id, verification_node_id) = match execution {
                Ok(result) => result,
                Err(error)
                    if error
                        .contains("binding changed for canonical workspace operation attempt") =>
                {
                    let reason = "late_filesystem_write_before_restart_reuse:verification_pre_patch_binding_superseded".to_string();
                    self.record_product_verification_authority_loss(
                        task_id,
                        workspace_record_id,
                        &reason,
                        true,
                        actor,
                    )?;
                    authority_loss_reason = Some(reason.clone());
                    all_passed = false;
                    final_status = "authority_lost";
                    attempts.push(json!({
                        "schema_version": "product_verification_attempt.v2",
                        "attempt": attempt_number,
                        "command": command,
                        "declared_timeout_ms": declared_timeout_ms,
                        "effective_timeout_ms": effective_timeout_ms,
                        "started_at": started_at,
                        "completed_at": self.now(),
                        "exit_status": null,
                        "process_outcome": null,
                        "result_status": "stale_rejected",
                        "trustworthy": false,
                        "late_result_rejected": true,
                        "error_domain": "verification_pre_state_superseded",
                        "error_message": reason,
                        "product_task_id": task_id,
                        "run_id": run_id,
                        "node_id": node_id,
                        "workspace_record_id": workspace_record_id,
                        "source_revision": source_revision,
                        "expected_task_version": expected_task_version,
                    }));
                    break;
                }
                Err(error) => return Err(error),
            };
            let completed_at = self.now();
            let post_authority = self.validate_product_verification_authority(
                task_id,
                run_id,
                workspace_record_id,
                workspace_path,
                source_revision,
                expected_task_version,
                &node_authority,
                runtime_authority,
            );
            let mut lost_after_effect = post_authority.as_ref().err().cloned();
            if let Ok(post_hash) = post_authority.as_ref() {
                if post_hash != &pre_command_hash {
                    lost_after_effect =
                        Some("late_filesystem_write_during_verification".to_string());
                }
                previous_workspace_hash = Some(post_hash.clone());
            }
            if let Some(reason) = lost_after_effect.as_deref() {
                self.record_product_verification_authority_loss(
                    task_id,
                    workspace_record_id,
                    reason,
                    true,
                    actor,
                )?;
                authority_loss_reason = Some(reason.to_string());
                all_passed = false;
                final_status = "authority_lost";
            }
            let passed = lost_after_effect.is_none()
                && output.status == "completed"
                && output
                    .process_outcome
                    .as_ref()
                    .is_some_and(crate::node_executor::ProcessOutcome::successful_exit);
            if !passed {
                all_passed = false;
                if lost_after_effect.is_none() {
                    final_status = product_verification_failure_status(
                        output.error_domain.as_deref(),
                        output.process_outcome.as_ref(),
                    );
                }
            }

            // Digest stdout/stderr for bounded evidence (no raw corpus).
            let output_digest = output.output.as_ref().map(|s| {
                use sha2::{Digest, Sha256};
                let digest = hex::encode(Sha256::digest(s.as_bytes()));
                let preview: String = s.chars().take(256).collect();
                json!({
                    "sha256": digest,
                    "bytes": s.len(),
                    "preview_redacted": crate::provider::redaction::redact_sensitive_patterns(&preview),
                })
            });

            attempts.push(json!({
                "schema_version": "product_verification_attempt.v2",
                "attempt": attempt_number,
                "command": command,
                "command_argv": command.split_whitespace().collect::<Vec<_>>(),
                "declared_timeout_ms": declared_timeout_ms,
                "effective_timeout_ms": effective_timeout_ms,
                "started_at": started_at,
                "completed_at": completed_at,
                "result_status": if lost_after_effect.is_some() { "stale_rejected" } else { output.status.as_str() },
                "trustworthy": passed,
                "late_result_rejected": lost_after_effect.is_some(),
                "authority_loss_reason": lost_after_effect,
                "workspace_hash_before": pre_command_hash,
                "workspace_hash_after": previous_workspace_hash,
                "executor_type": output.executor_type,
                "exit_status": output.process_outcome.as_ref().and_then(|outcome| outcome.exit_code),
                "process_outcome": output.process_outcome,
                "error_domain": output.error_domain,
                "error_message": output.error_message.as_deref().map(|m| {
                    crate::provider::redaction::redact_sensitive_patterns(m)
                }),
                "latency_ms": output.latency_ms,
                "output_digest": output_digest,
                "product_task_id": task_id,
                "tenant_id": tenant_id,
                "workspace_scope_id": workspace_scope,
                "run_id": run_id,
                "node_id": node_id,
                "verification_run_id": verification_run_id,
                "verification_node_id": verification_node_id,
                "workspace_record_id": workspace_record_id,
                "workspace_path": workspace_path,
                "source_revision": source_revision,
                "expected_task_version": expected_task_version,
            }));

            if !passed {
                // Fail closed: do not continue remaining commands after a failure? Spec says
                // run all declared commands. Continue collecting attempts but mark overall fail.
            }
            if authority_loss_reason.is_some() {
                break;
            }
        }

        let verification = json!({
            "schema_version": "workspace_verification.v1",
            "status": if all_passed { "evidence_recorded" } else { final_status },
            "result_status": if all_passed { "completed" } else { "failed" },
            "product_task_id": task_id,
            "tenant_id": tenant_id,
            "workspace_scope_id": workspace_scope,
            "run_id": run_id,
            "workspace_record_id": workspace_record_id,
            "workspace_path": workspace_path,
            "source_revision": source_revision,
            "expected_task_version": expected_task_version,
            "attempt": attempts.len() as u64,
            "verification_attempts": attempts,
            "repair_attempts": [],
            "method": "product_golden_path_managed_tool_policy",
            "trustworthy": all_passed,
            "authority_loss_reason": authority_loss_reason,
            "recorded_at": self.now(),
            "recorded_by": actor,
        });

        self.record_workspace_verification(workspace_record_id, &verification, actor)?;
        Ok(verification)
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_managed_product_verification_command(
        &self,
        task_id: &str,
        tenant_id: &str,
        workspace_scope: &str,
        workspace_record_id: &str,
        workspace_path: &str,
        source_revision: &str,
        attempt: u64,
        command: &str,
        pre_patch_sha256: &str,
        timeout_ms: u64,
        actor: &str,
    ) -> Result<(NodeExecutionOutput, String, String), String> {
        let mut metadata = json!({
            "profile_id": "supervised_patch_verification",
            "command": command,
            "pre_patch_sha256": pre_patch_sha256,
            "workspace_path": workspace_path,
            "workspace_root": workspace_path,
            "executor_timeout_ms": timeout_ms,
            "product_task_id": task_id,
            "tenant_id": tenant_id,
            "workspace_scope_id": workspace_scope,
            "source_revision": source_revision,
            "attempt": attempt,
        });
        let operation = "product_verify";
        let binding_sha256 =
            managed_tool_binding_sha256(workspace_record_id, operation, attempt, &metadata)?;
        metadata
            .as_object_mut()
            .ok_or_else(|| "product verification metadata must be an object".to_string())?
            .insert(
                "managed_supervised_patch".to_string(),
                json!({
                    "schema_version": "managed_supervised_patch.v1",
                    "workspace_id": workspace_record_id,
                    "operation": operation,
                    "attempt": attempt,
                    "binding_sha256": binding_sha256,
                    "content_excluded": true,
                }),
            );
        let managed_run_id = self.ensure_managed_supervised_patch_run(
            workspace_record_id,
            operation,
            attempt,
            &binding_sha256,
            &metadata,
            actor,
        )?;
        let managed_node_id = format!("supervised-{operation}-{attempt}");
        if let Some(output) =
            persisted_product_managed_output(self, &managed_run_id, &managed_node_id)?
        {
            return Ok((output, managed_run_id, managed_node_id));
        }

        let allowed_commands = PRODUCT_VERIFICATION_READ_ONLY_COMMANDS
            .iter()
            .map(|command| (*command).to_string())
            .collect::<Vec<_>>();
        let executor = ToolPolicyNodeExecutor::command_borrowed(
            Arc::new(RedactedProductVerificationExecutor {
                inner: CommandNodeExecutor {
                    timeout_ms,
                    allowed_commands: allowed_commands.clone(),
                    allowed_binaries: allowed_commands,
                    env_vars: Vec::new(),
                },
            }),
            self,
        );
        let tick =
            self.tick_managed_supervised_patch_with_executor(&managed_run_id, actor, &executor)?;
        if tick.get("node_id").and_then(Value::as_str) != Some(managed_node_id.as_str()) {
            if let Some(output) =
                persisted_product_managed_output(self, &managed_run_id, &managed_node_id)?
            {
                return Ok((output, managed_run_id, managed_node_id));
            }
            return Err(
                "product verification canonical managed operation is already in progress"
                    .to_string(),
            );
        }
        let output = tick
            .get("result")
            .ok_or_else(|| "managed product verification result is missing".to_string())
            .and_then(product_node_output_from_value)?;
        Ok((output, managed_run_id, managed_node_id))
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_product_verification_authority(
        &self,
        task_id: &str,
        run_id: &str,
        workspace_record_id: &str,
        workspace_path: &str,
        source_revision: &str,
        expected_task_version: u64,
        expected_node: &ProductVerificationNodeAuthority,
        runtime_authority: &dyn Fn() -> Result<ProductVerificationRuntimeAuthority, String>,
    ) -> Result<String, String> {
        let runtime = runtime_authority()
            .map_err(|error| format!("runtime_authority_unavailable:{error}"))?;
        runtime
            .validate()
            .map_err(|reason| format!("runtime_authority_lost:{reason}"))?;

        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| "task_missing".to_string())?;
        let current_version = task.get("version").and_then(Value::as_u64).unwrap_or(0);
        if current_version != expected_task_version {
            return Err(format!(
                "task_version_superseded:expected={expected_task_version}:actual={current_version}"
            ));
        }
        let status = task.get("status").and_then(Value::as_str).unwrap_or("");
        match status {
            "verifying" => {}
            "paused" => return Err("task_paused".to_string()),
            "killed" => return Err("task_killed".to_string()),
            _ => return Err(format!("task_state_superseded:{status}")),
        }
        if task.get("run_id").and_then(Value::as_str) != Some(run_id)
            || task.get("workspace_record_id").and_then(Value::as_str) != Some(workspace_record_id)
            || task
                .pointer("/workspace_binding/workspace_path")
                .and_then(Value::as_str)
                != Some(workspace_path)
            || task
                .pointer("/workspace_binding/source_revision")
                .and_then(Value::as_str)
                != Some(source_revision)
        {
            return Err("task_operation_binding_superseded".to_string());
        }

        let run = self
            .get_workflow_run(run_id)?
            .ok_or_else(|| "run_missing".to_string())?;
        if run.get("pause_reason").and_then(Value::as_str).is_some() {
            return Err("run_paused".to_string());
        }
        if run.get("status").and_then(Value::as_str) != Some("completed") {
            return Err(format!(
                "run_authority_lost:{}",
                run.get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("missing")
            ));
        }
        let current_node = product_verification_node_authority(&run, &expected_node.node_id)?;
        if &current_node != expected_node {
            return Err("node_attempt_or_lease_superseded".to_string());
        }

        let workspace = self
            .get_supervised_patch_workspace(workspace_record_id)?
            .ok_or_else(|| "workspace_record_missing".to_string())?;
        let workspace_status = workspace
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("");
        if matches!(workspace_status, "quarantined" | "cleaned" | "rejected") {
            return Err(format!("workspace_status_invalid:{workspace_status}"));
        }
        if workspace.get("run_id").and_then(Value::as_str) != Some(run_id)
            || workspace.get("source_revision").and_then(Value::as_str) != Some(source_revision)
            || workspace.get("workspace_path").and_then(Value::as_str) != Some(workspace_path)
        {
            return Err("workspace_binding_superseded".to_string());
        }
        let canonical_path = std::fs::canonicalize(workspace_path)
            .map_err(|_| "workspace_missing_or_replaced".to_string())?;
        if workspace
            .get("workspace_canonical_path")
            .and_then(Value::as_str)
            != canonical_path.to_str()
        {
            return Err("workspace_canonical_path_superseded".to_string());
        }
        if workspace.get("workspace_mode").and_then(Value::as_str) == Some("copy") {
            let source_path = workspace
                .get("target_repo_path")
                .and_then(Value::as_str)
                .ok_or_else(|| "workspace_source_path_unavailable".to_string())?;
            let exclusions = crate::local_folder_source::configured_local_folder_exclusions()
                .map_err(|_| "workspace_exclusion_policy_unavailable".to_string())?;
            let source_manifest = crate::local_folder_source::capture_local_folder_manifest(
                Path::new(source_path),
                &exclusions,
            )
            .map_err(|_| "workspace_source_revision_unavailable".to_string())?;
            if source_manifest.tree_sha256 != source_revision {
                return Err("workspace_source_revision_superseded".to_string());
            }
            let summary = crate::local_folder_source::summarize_local_folder_changes(
                &source_manifest,
                Path::new(workspace_path),
                &exclusions,
            )
            .map_err(|_| "workspace_patch_identity_unavailable".to_string())?;
            return Ok(summary.change_sha256);
        }
        let current_revision = current_workspace_revision(
            &TargetRepoOutputConfig::from_env(),
            Path::new(workspace_path),
        )
        .map_err(|_| "workspace_source_revision_unavailable".to_string())?;
        if current_revision != source_revision {
            return Err("workspace_source_revision_superseded".to_string());
        }
        validate_product_task_elapsed_budget(&task, &self.now())?;
        let patch = inspect_git_patch_read_only(
            &TargetRepoOutputConfig::from_env(),
            Path::new(workspace_path),
        )
        .map_err(|_| "workspace_patch_identity_unavailable".to_string())?;
        Ok(target_patch_hash(&patch))
    }

    fn product_task_remaining_elapsed_ms(&self, task_id: &str) -> Result<u64, String> {
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| "task_missing".to_string())?;
        product_task_remaining_elapsed_ms(&task, &self.now())
    }

    fn record_product_verification_authority_loss(
        &self,
        task_id: &str,
        workspace_record_id: &str,
        reason: &str,
        after_effect: bool,
        actor: &str,
    ) -> Result<(), String> {
        self.append_audit(
            actor,
            "product_task.verification_authority_lost",
            task_id,
            &json!({
                "reason": reason,
                "after_effect": after_effect,
                "late_result_rejected": after_effect,
                "workspace_record_id": workspace_record_id,
                "content_excluded": true,
            }),
        )?;
        if reason.contains("workspace") || reason.contains("late_filesystem_write") {
            self.quarantine_workspace(workspace_record_id, actor)?;
        }
        Ok(())
    }

    /// Record an independent, output-only approval bound to the exact evidence
    /// that will be consumed by a later output operation.
    fn require_product_task_tenant(
        &self,
        task_id: &str,
        expected_tenant_id: &str,
    ) -> Result<(), String> {
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        if task.get("tenant_id").and_then(Value::as_str) != Some(expected_tenant_id) {
            return Err("product task tenant does not match authenticated principal".into());
        }
        Ok(())
    }

    pub(crate) fn get_product_task_for_tenant(
        &self,
        task_id: &str,
        expected_tenant_id: &str,
    ) -> Result<Option<Value>, String> {
        let Some(task) = self.get_product_task(task_id)? else {
            return Ok(None);
        };
        if task.get("tenant_id").and_then(Value::as_str) != Some(expected_tenant_id) {
            return Err("product task tenant does not match authenticated principal".into());
        }
        Ok(Some(task))
    }

    pub(crate) fn compile_and_schedule_product_task_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
        actor: &str,
        available_executors: &[String],
    ) -> Result<Value, String> {
        self.require_product_task_tenant(task_id, tenant_id)?;
        self.compile_and_schedule_product_task(task_id, actor, available_executors)
    }

    /// HTTP/store boundary for approval. The tenant is derived from the
    /// authenticated API context and is checked before the existing mutation
    /// owner is entered; it is never accepted from the request body.
    pub fn approve_product_task_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
        actor: &str,
        expected_task_version: u64,
    ) -> Result<Value, String> {
        self.require_product_task_tenant(task_id, tenant_id)?;
        self.approve_product_task(task_id, actor, expected_task_version)
    }

    pub fn approve_product_task(
        &self,
        task_id: &str,
        actor: &str,
        expected_task_version: u64,
    ) -> Result<Value, String> {
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        if let Some(plan_id) = task.get("plan_id").and_then(Value::as_str) {
            let plan = self
                .get_workflow_plan(plan_id)?
                .ok_or("product output approval plan is missing")?;
            if plan
                .pointer("/advisory/delegated_binding")
                .and_then(Value::as_str)
                == Some("required")
            {
                return Err(
                    "delegated ProductTask requires the independent artifact confirmer".into(),
                );
            }
        }
        let evidence =
            self.product_task_output_approval_evidence(task_id, expected_task_version)?;
        self.record_product_output_approval(
            required_product_task_string(&evidence, "run_id")?.as_str(),
            required_product_task_string(&evidence, "node_id")?.as_str(),
            actor,
            evidence
                .get("binding")
                .ok_or("product output approval binding missing")?,
        )
    }

    /// The delegated artifact confirmer independently binds the exact
    /// ProductTask artifact, deterministic verification, Pro review, realized
    /// cost, target SHA, and delegation before the existing output owner gains
    /// one Draft-PR-only approval.
    fn delegated_provider_request_journal(
        &self,
        delegation_id: &str,
        attempt_id: &str,
    ) -> Result<Vec<Value>, String> {
        let encoded = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE delegation_id=?1 AND attempt_id=?2",
                        params![delegation_id, attempt_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations
                         WHERE delegation_id=$1 AND attempt_id=$2",
                        &[&delegation_id, &attempt_id],
                    )
                    .map(|row| row.map(|row| row.get::<_, String>(0)))
                    .map_err(|error| error.to_string())
            })?,
        }
        .ok_or("delegated provider request journal is missing")?;
        let journal: Vec<Value> = serde_json::from_str(&encoded)
            .map_err(|_| "delegated provider request journal is malformed")?;
        if journal.len() != 3 || journal.iter().any(|entry| !entry.is_object()) {
            return Err(
                "delegated provider request journal must contain exactly three receipts".into(),
            );
        }
        Ok(journal)
    }

    pub fn approve_delegated_product_task(
        &self,
        confirmer: &super::managed_acceptance::AuthenticatedPrincipal,
        task_id: &str,
        actor: &str,
        expected_task_version: u64,
        delegation_id: &str,
        manifest: &Value,
        current_target_main_sha: &str,
    ) -> Result<Value, String> {
        // Both immutable authority owners are tenant-bound before any
        // artifact evidence is read or confirmation state can be written.
        // The task id and delegation id are request inputs, never authority
        // assertions supplied by the caller.
        self.require_product_task_tenant(task_id, confirmer.tenant_id())?;
        self.require_delegation_product_task(delegation_id, confirmer.tenant_id(), task_id)?;
        let evidence =
            self.product_task_output_approval_evidence(task_id, expected_task_version)?;
        let run_id = required_product_task_string(&evidence, "run_id")?;
        let node_id = required_product_task_string(&evidence, "node_id")?;
        let run = evidence
            .get("run")
            .ok_or("delegated output evidence run missing")?;
        let workflow_id = manifest
            .pointer("/execution/workflow_id")
            .and_then(Value::as_str)
            .ok_or("delegated output manifest workflow_id missing")?;
        let attempt_id = manifest
            .pointer("/execution/attempt_id")
            .and_then(Value::as_str)
            .ok_or("delegated output manifest attempt_id missing")?;
        let provider_journal =
            self.delegated_provider_request_journal(delegation_id, attempt_id)?;
        let review = super::managed_acceptance::managed_deepseek_stage_receipt(
            run,
            &format!("{workflow_id}-review"),
            "managed_deepseek_review_receipt.v1",
        )?;
        let provider_nodes = run
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or("delegated output run nodes missing")?
            .iter()
            .filter(|node| node.get("managed_deepseek").is_some_and(Value::is_object))
            .collect::<Vec<_>>();
        if provider_nodes.len() != 3 {
            return Err("delegated output requires exactly three provider requests".into());
        }
        let manifest_node_ids = manifest
            .pointer("/execution/workflow_node_ids")
            .and_then(Value::as_array)
            .ok_or("delegated output manifest route nodes missing")?;
        if manifest_node_ids.len() != 4 {
            return Err("delegated output manifest route is not exact".into());
        }
        let price_profile: crate::provider::managed_deepseek::DeepSeekPriceProfile =
            serde_json::from_value(
                manifest
                    .pointer("/provider/price_profile")
                    .cloned()
                    .ok_or("delegated output price profile missing")?,
            )
            .map_err(|_| "delegated output price profile is malformed")?;
        let mut request_ids = std::collections::HashSet::new();
        let mut request_sha256s = std::collections::HashSet::new();
        let mut provider_requests = Vec::with_capacity(3);
        let mut realized_cost_usd = 0.0;
        let mut cumulative_tokens = 0_u64;
        let mut seen_transport_provenance: Option<String> = None;
        for (index, (stage, role, model)) in [
            ("planning", "planner", "deepseek-v4-pro"),
            ("implementation", "implementer", "deepseek-v4-flash"),
            ("review", "reviewer", "deepseek-v4-pro"),
        ]
        .into_iter()
        .enumerate()
        {
            let node = provider_nodes[index];
            let manifest_index = if index == 2 { 3 } else { index };
            if node.get("node_id") != manifest_node_ids.get(manifest_index)
                || node
                    .pointer("/managed_deepseek/stage")
                    .and_then(Value::as_str)
                    != Some(stage)
            {
                return Err("delegated provider route identity mismatched the manifest".into());
            }
            let result = node
                .get("result")
                .filter(|value| value.is_object())
                .ok_or("delegated provider node result missing")?;
            if node.get("status").and_then(Value::as_str) != Some("completed")
                || result.get("status").and_then(Value::as_str) != Some("completed")
                || result.get("resolved_model").and_then(Value::as_str) != Some(model)
            {
                return Err("delegated provider node did not complete with the exact model".into());
            }
            let output: Value = serde_json::from_str(
                result
                    .get("output")
                    .and_then(Value::as_str)
                    .ok_or("delegated provider output receipt missing")?,
            )
            .map_err(|_| "delegated provider output receipt is malformed")?;
            let request_id = output
                .get("request_id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or("delegated provider request identity missing")?;
            let usage: crate::provider::managed_deepseek::ManagedUsage = serde_json::from_value(
                output
                    .get("usage")
                    .cloned()
                    .ok_or("delegated provider usage receipt missing")?,
            )
            .map_err(|_| "delegated provider usage receipt is malformed")?;
            let observed_cost = result
                .get("estimated_cost")
                .and_then(Value::as_f64)
                .ok_or("delegated provider realized cost missing")?;
            let receipt_cost = output
                .get("estimated_cost_usd")
                .and_then(Value::as_f64)
                .ok_or("delegated provider receipt cost missing")?;
            let recomputed_cost = price_profile.estimate_usd(model, &usage)?;
            let input_tokens = result
                .get("input_tokens")
                .and_then(Value::as_i64)
                .and_then(|value| u64::try_from(value).ok())
                .ok_or("delegated provider input usage missing")?;
            let output_tokens = result
                .get("output_tokens")
                .and_then(Value::as_i64)
                .and_then(|value| u64::try_from(value).ok())
                .ok_or("delegated provider output usage missing")?;
            if output.get("schema_version").and_then(Value::as_str)
                != Some("managed_deepseek_node_output.v1")
                || output.get("provider_kind").and_then(Value::as_str) != Some("deepseek")
                || output.get("protocol").and_then(Value::as_str) != Some("open_ai_compatible")
                || output.get("route_stage").and_then(Value::as_str) != Some(role)
                || output.get("requested_model").and_then(Value::as_str) != Some(model)
                || output.get("resolved_model").and_then(Value::as_str) != Some(model)
                || usage.model != model
                || usage.request_id != request_id
                || input_tokens != usage.input_tokens
                || output_tokens != usage.output_tokens
                || input_tokens
                    > manifest
                        .pointer("/limits/max_input_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(8_000)
                || output_tokens
                    > manifest
                        .pointer("/limits/max_output_tokens")
                        .and_then(Value::as_u64)
                        .unwrap_or(4_000)
                || !request_ids.insert(request_id.to_string())
                || (observed_cost - receipt_cost).abs() > 1e-12
                || (observed_cost - recomputed_cost).abs() > 1e-12
            {
                return Err(
                    "delegated provider request, usage, or cost identity mismatched".into(),
                );
            }
            let journal_entry = provider_journal
                .iter()
                .find(|entry| entry.get("node_id") == node.get("node_id"))
                .ok_or("delegated provider journal lacks the exact node request")?;
            let transport_provenance = journal_entry
                .get("transport_provenance")
                .and_then(Value::as_str)
                .ok_or("delegated provider journal claim lacks transport provenance")?;
            if !matches!(transport_provenance, "external" | "injected") {
                return Err("delegated provider journal transport provenance is invalid".into());
            }
            if seen_transport_provenance
                .as_deref()
                .is_some_and(|seen: &str| seen != transport_provenance)
            {
                return Err(
                    "delegated provider transport provenance is mixed across requests".into(),
                );
            }
            seen_transport_provenance = Some(transport_provenance.to_string());
            let request_sha256 = journal_entry
                .get("request_sha256")
                .and_then(Value::as_str)
                .filter(|value| {
                    value.len() == 64
                        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                        && request_sha256s.insert((*value).to_string())
                })
                .ok_or("delegated provider journal request hash is missing or replayed")?;
            let journal_usage = journal_entry
                .get("usage")
                .ok_or("delegated provider journal usage is missing")?;
            let expected_usage = serde_json::to_value(&usage)
                .map_err(|_| "delegated provider usage cannot be reconciled")?;
            if journal_entry.get("schema_version").and_then(Value::as_str)
                != Some("managed_provider_request_claim.v1")
                || journal_entry.get("ordinal").and_then(Value::as_u64) != Some((index + 1) as u64)
                || journal_entry.get("role").and_then(Value::as_str) != Some(role)
                || journal_entry.get("protocol").and_then(Value::as_str)
                    != Some("open_ai_compatible")
                || journal_entry.get("requested_model").and_then(Value::as_str) != Some(model)
                || journal_entry.get("resolved_model").and_then(Value::as_str) != Some(model)
                || journal_entry.get("status").and_then(Value::as_str) != Some("succeeded")
                || journal_entry.get("request_id").and_then(Value::as_str) != Some(request_id)
                || journal_usage != &expected_usage
                || journal_entry
                    .get("effective_tokens")
                    .and_then(Value::as_u64)
                    != Some(usage.cumulative_tokens)
                || journal_entry
                    .get("effective_cost_usd")
                    .and_then(Value::as_f64)
                    .is_none_or(|cost| (cost - observed_cost).abs() > 1e-12)
                || journal_entry
                    .get("conservative_reserved_cost_usd")
                    .and_then(Value::as_f64)
                    .is_none_or(|reserved| reserved + 1e-12 < observed_cost)
            {
                return Err(
                    "delegated provider node receipt conflicts with the durable journal".into(),
                );
            }
            realized_cost_usd += observed_cost;
            cumulative_tokens = cumulative_tokens
                .checked_add(usage.cumulative_tokens)
                .ok_or("delegated provider cumulative usage overflow")?;
            provider_requests.push(json!({
                "node_id": node.get("node_id"),
                "stage": stage,
                "role": role,
                "protocol": "openai_compatible",
                "requested_model": model,
                "resolved_model": model,
                "request_id": request_id,
                "request_sha256": request_sha256,
                "transport_provenance": transport_provenance,
                "usage": usage,
                "realized_cost_usd": observed_cost,
                "output_sha256": output.get("output_sha256"),
            }));
        }
        let provider_transport_provenance = seen_transport_provenance
            .ok_or("delegated provider journal claims carry no transport provenance")?;
        let max_cumulative_tokens = manifest
            .pointer("/limits/max_cumulative_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(24_000);
        let max_realized_cost = manifest
            .pointer("/limits/max_cost_usd")
            .and_then(Value::as_f64)
            .unwrap_or(0.50);
        if cumulative_tokens > max_cumulative_tokens || realized_cost_usd > max_realized_cost {
            return Err("delegated provider cumulative budget exceeded".into());
        }
        let provider_execution = json!({
            "schema_version": "managed_deepseek_execution_evidence.v1",
            "provider_request_count": provider_requests.len(),
            "transport_provenance": provider_transport_provenance,
            "requests": provider_requests,
            "cumulative_tokens": cumulative_tokens,
            "realized_cost_usd": realized_cost_usd,
        });
        let source_revision = evidence
            .pointer("/binding/source_revision")
            .and_then(Value::as_str)
            .ok_or("delegated output source revision missing")?;
        let task = self
            .get_product_task(task_id)?
            .ok_or("delegated output ProductTask disappeared")?;
        let target_repo_path = task
            .get("target_repo_path")
            .and_then(Value::as_str)
            .ok_or("delegated output target repository path missing")?;
        let (default_branch, origin_remote, observed_main_sha) =
            crate::target_repo_output::inspect_git_source_identity(
                &TargetRepoOutputConfig::from_env(),
                Path::new(target_repo_path),
            )?;
        let origin_fingerprint = hex::encode(Sha256::digest(origin_remote.as_bytes()));
        if default_branch
            != task
                .pointer("/workspace_binding/git_default_branch")
                .and_then(Value::as_str)
                .unwrap_or("")
            || origin_fingerprint
                != task
                    .pointer("/workspace_binding/git_origin_remote_fingerprint")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            || observed_main_sha != source_revision
            || source_revision != current_target_main_sha
        {
            return Err("delegated output target main drifted".into());
        }
        let artifact = evidence
            .get("artifact")
            .ok_or("delegated output artifact missing")?;
        let changed_files = artifact
            .get("changed_files")
            .and_then(Value::as_array)
            .ok_or("delegated output changed files missing")?
            .iter()
            .map(|path| {
                let path = path
                    .as_str()
                    .ok_or("delegated output changed file is not a string")?;
                path.strip_prefix(['+', '~', '-'])
                    .ok_or("delegated output changed file lacks a canonical change prefix")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let review_diff = artifact
            .get("review_diff")
            .and_then(Value::as_str)
            .ok_or("delegated output review diff missing")?;
        let changed_lines = count_unified_diff_changed_lines(review_diff);
        let artifact_sha256 = artifact
            .get("patch_hash")
            .and_then(Value::as_str)
            .and_then(|hash| hash.strip_prefix("sha256:"))
            .ok_or("delegated output artifact lacks canonical sha256 patch hash")?;
        let artifact_summary = json!({
            "artifact_sha256": artifact_sha256,
            "changed_files": changed_files,
            "changed_lines": changed_lines,
        });
        let verification = evidence
            .get("verification")
            .ok_or("delegated output deterministic verification missing")?;
        let verification_summary = json!({
            "status": "succeeded",
            "verification_sha256": product_json_sha256(verification)?,
        });
        let confirmation = self.persist_delegated_artifact_confirmation(
            confirmer,
            delegation_id,
            manifest,
            &artifact_summary,
            &verification_summary,
            &review,
            &provider_execution,
            current_target_main_sha,
            realized_cost_usd,
        )?;
        let confirmation_sha256 =
            required_product_task_string(&confirmation, "artifact_confirmation_sha256")?;
        if let Some(existing) = self
            .workflow_run_approvals(&run_id, 10_000)?
            .into_iter()
            .find(|approval| {
                approval
                    .get("artifact_confirmation_sha256")
                    .and_then(Value::as_str)
                    == Some(confirmation_sha256.as_str())
            })
        {
            return Ok(json!({
                "approval": existing,
                "artifact_confirmation": confirmation,
                "realized_cost_usd": realized_cost_usd,
                "replayed": true,
            }));
        }
        let mut binding = evidence
            .get("binding")
            .cloned()
            .ok_or("delegated output approval binding missing")?;
        let object = binding
            .as_object_mut()
            .ok_or("delegated output approval binding must be an object")?;
        object.insert(
            "artifact_confirmation_sha256".into(),
            json!(confirmation_sha256),
        );
        object.insert(
            "delegation_sha256".into(),
            confirmation
                .get("delegation_sha256")
                .cloned()
                .unwrap_or(Value::Null),
        );
        object.insert("output_mode".into(), json!("draft_pr_only"));
        let approval = self.record_product_output_approval(&run_id, &node_id, actor, &binding)?;
        Ok(json!({
            "approval": approval,
            "artifact_confirmation": confirmation,
            "realized_cost_usd": realized_cost_usd,
            "replayed": false,
        }))
    }

    fn product_task_output_approval_evidence(
        &self,
        task_id: &str,
        expected_task_version: u64,
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        if task.get("version").and_then(Value::as_u64) != Some(expected_task_version) {
            return Err("stale product task version at approval".to_string());
        }
        let status =
            ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if status != ProductTaskStatus::AwaitingApproval {
            return Err(format!(
                "product approval requires awaiting_approval; status={}",
                status.as_str()
            ));
        }
        let run_id = required_product_task_string(&task, "run_id")?;
        let workspace_record_id = required_product_task_string(&task, "workspace_record_id")?;
        let workspace = self
            .get_supervised_patch_workspace(&workspace_record_id)?
            .ok_or_else(|| "workspace missing at approval".to_string())?;
        let verification = workspace
            .get("verification")
            .cloned()
            .ok_or_else(|| "approval blocked: verification missing".to_string())?;
        validate_product_verification_binding(
            &verification,
            task_id,
            &run_id,
            &workspace_record_id,
            expected_task_version,
        )?;
        let verification_sha256 = product_json_sha256(&verification)?;
        let source_revision = task
            .pointer("/workspace_binding/source_revision")
            .and_then(Value::as_str)
            .ok_or_else(|| "product task source revision missing".to_string())?;
        let artifact = self.current_product_task_artifact(
            task_id,
            &run_id,
            &workspace_record_id,
            source_revision,
        )?;
        let artifact_id = required_product_task_string(&artifact, "artifact_id")?;
        let patch_hash = required_product_task_string(&artifact, "patch_hash")?;
        let changed_files = artifact
            .get("changed_files")
            .cloned()
            .or_else(|| artifact.get("changed_files_json").cloned())
            .unwrap_or_else(|| json!([]));
        let run = self
            .get_workflow_run(&run_id)?
            .ok_or_else(|| "workflow run missing at approval".to_string())?;
        let node = run
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| nodes.first())
            .ok_or_else(|| "workflow node missing at approval".to_string())?;
        let node_id = required_product_task_string(node, "node_id")?;
        let output_intent = required_product_task_string(&task, "output_intent")?;
        let binding = json!({
            "schema_version": "product_output_approval.v1",
            "product_task_id": task_id,
            "expected_task_version": expected_task_version,
            "run_id": run_id,
            "node_id": node_id,
            "workspace_record_id": workspace_record_id,
            "workspace_path": task.pointer("/workspace_binding/workspace_path"),
            "source_revision": source_revision,
            "artifact_id": artifact_id,
            "patch_hash": patch_hash,
            "changed_files": changed_files,
            "verification_sha256": verification_sha256,
            "verification_status": verification.get("status"),
            "output_intent": output_intent,
            "output_target": {
                "target_id": task.get("target_id"),
                "target_repo_path": task.get("target_repo_path"),
            },
        });
        Ok(json!({
            "run_id": run_id,
            "node_id": node_id,
            "binding": binding,
            "artifact": artifact,
            "verification": verification,
            "run": run,
        }))
    }

    /// Consume one exact persisted product-output approval after an explicit
    /// output confirmation. Validation completes before any state or audit
    /// mutation, so a missing confirmation has zero side effects.
    pub fn output_product_task_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
        actor: &str,
        expected_task_version: u64,
        approval_id: Option<&str>,
        confirm_output: bool,
    ) -> Result<Value, String> {
        self.require_product_task_tenant(task_id, tenant_id)?;
        self.output_product_task(
            task_id,
            actor,
            expected_task_version,
            approval_id,
            confirm_output,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn output_product_task(
        &self,
        task_id: &str,
        actor: &str,
        expected_task_version: u64,
        approval_id: Option<&str>,
        confirm_output: bool,
    ) -> Result<Value, String> {
        if !confirm_output {
            return Err("confirm_output=true required for product output".to_string());
        }
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let persisted_task_version = task
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "product task version missing at output".to_string())?;
        let status =
            ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
        let completed_idempotent_version = status == ProductTaskStatus::Completed
            && (persisted_task_version == expected_task_version
                || persisted_task_version == expected_task_version.saturating_add(1));
        if persisted_task_version != expected_task_version && !completed_idempotent_version {
            return Err("stale product task version at output".to_string());
        }
        if !matches!(
            status,
            ProductTaskStatus::AwaitingApproval
                | ProductTaskStatus::OutputPending
                | ProductTaskStatus::OutcomeUnknown
                | ProductTaskStatus::Completed
        ) {
            return Err(format!(
                "product output requires awaiting_approval or output_pending; status={}",
                status.as_str()
            ));
        }
        let approval_id = approval_id
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "product output approval_id is required".to_string())?;
        let run_id = required_product_task_string(&task, "run_id")?;
        let workspace_record_id = required_product_task_string(&task, "workspace_record_id")?;
        let approvals = self.workflow_run_approvals(&run_id, 1_000)?;
        let approval = approvals
            .into_iter()
            .find(|candidate| {
                candidate.get("approval_id").and_then(Value::as_str) == Some(approval_id)
            })
            .ok_or_else(|| "product output approval not found".to_string())?;
        validate_current_product_output_approval(
            &approval,
            &task,
            task_id,
            &run_id,
            &workspace_record_id,
            expected_task_version,
        )?;
        let artifact_id = required_product_task_string(&approval, "artifact_id")?;
        let artifact = self
            .get_supervised_patch_artifact(&artifact_id)?
            .ok_or_else(|| "approved artifact is missing".to_string())?;
        let source_revision = required_product_task_string(&approval, "source_revision")?;
        let patch_hash = required_product_task_string(&approval, "patch_hash")?;
        validate_product_artifact_against_approval(&artifact, &approval)?;
        let workspace = self
            .get_supervised_patch_workspace(&workspace_record_id)?
            .ok_or_else(|| "workspace missing at output".to_string())?;
        let verification = workspace
            .get("verification")
            .cloned()
            .ok_or_else(|| "verification missing at output".to_string())?;
        if product_json_sha256(&verification)?
            != required_product_task_string(&approval, "verification_sha256")?
        {
            return Err("stale approval: verification binding changed".to_string());
        }

        if status == ProductTaskStatus::Completed {
            let terminal_evidence = self.get_product_task_terminal_evidence(task_id)?;
            let terminal_output = validate_completed_product_output_binding(
                &task,
                &artifact,
                &approval,
                &terminal_evidence,
            )?;
            return Ok(json!({
                "task": task,
                "approval": approval,
                "artifact": artifact,
                "output": terminal_output.get("output"),
                "output_receipt": terminal_output.get("receipt"),
                "operation": terminal_output.get("operation"),
                "terminal_evidence": terminal_evidence,
                "reused": true,
            }));
        }

        let output_intent = required_product_task_string(&task, "output_intent")?;
        if output_intent == "apply_local_changes" && status == ProductTaskStatus::OutcomeUnknown {
            // A local mutation may already have reached the operator source
            // while its receipt outcome is uncertain. Never retry or infer a
            // second apply; the app-owned rollback bundle must be reconciled
            // through explicit recovery evidence instead.
            return Err(
                "apply_local_changes refuses an outcome-unknown effect; reconciliation required"
                    .to_string(),
            );
        }
        if output_intent == "apply_local_changes" {
            let claim = self.claim_product_local_apply_effect(
                &artifact_id,
                task_id,
                approval_id,
                expected_task_version,
                actor,
            )?;
            if claim.get("claim_action").and_then(Value::as_str) != Some("apply_local_changes") {
                return Err(
                    "apply_local_changes has a prior effect claim; reconciliation required"
                        .to_string(),
                );
            }
        }
        let output_result = if output_intent == "artifact_only" {
            json!({
                "mode": "artifact_only",
                "status": "artifact_only",
                "product_task_id": task_id,
                "artifact_id": artifact_id,
                "approval_id": approval_id,
                "target_mutation": false,
            })
        } else {
            let approval_binding = json!({
                "schema_version": "product_output_approval_binding.v1",
                "approval_id": approval_id,
                "artifact_id": artifact_id,
                "patch_hash": patch_hash,
                "source_revision": source_revision,
                "verification_sha256": approval.get("verification_sha256"),
                "changed_files": approval.get("changed_files"),
                "output_intent": output_intent,
                "export_eligible": true,
            });
            match self.execute_product_task_output(
                task_id,
                &output_intent,
                &run_id,
                &artifact_id,
                &workspace_record_id,
                &source_revision,
                &patch_hash,
                &approval_binding,
                expected_task_version,
                actor,
            ) {
                Ok(output) => output,
                Err(_error) if output_intent == "apply_local_changes" => {
                    let _ = self.mark_product_task_output_outcome_unknown(
                        task_id,
                        actor,
                        "local_apply_execution_unconfirmed",
                    );
                    return Err(
                        "apply_local_changes effect is unconfirmed; reconciliation required"
                            .to_string(),
                    );
                }
                Err(error) => return Err(error),
            }
        };

        let output_status = output_result
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("failed");
        if output_result
            .get("pre_effect_failure")
            .and_then(Value::as_bool)
            == Some(true)
            || output_result
                .get("admission_blocked")
                .and_then(Value::as_bool)
                == Some(true)
            || output_status == "operation_in_progress"
        {
            // A credential failure before any branch/PR effect, or a concurrent
            // durable-operation claimant, is recoverable without advancing
            // ProductTask. A malformed/non-GitHub origin is different: it is an
            // admission block with no durable operation and is not presented as
            // restart recovery. Keep both outcomes at the approval version so
            // their task bindings cannot drift across a retry.
            let current = self
                .get_product_task(task_id)?
                .ok_or_else(|| "product task disappeared during output recovery".to_string())?;
            if current.get("version").and_then(Value::as_u64) != Some(expected_task_version) {
                return Err(
                    "product task version changed during pre-effect output recovery".to_string(),
                );
            }
            return Ok(json!({
                "task": current,
                "approval": approval,
                "artifact": artifact,
                "output": output_result,
                "output_receipt": Value::Null,
                "terminal_evidence": Value::Null,
                "reused": false,
                "recovery_required": output_result
                    .get("recovery_required")
                    .and_then(Value::as_bool)
                    .unwrap_or(output_status == "operation_in_progress"),
            }));
        }
        // Receipt and terminal CAS are separate store transactions. A concurrent
        // winner may commit receipt+terminal (advancing ProductTask version) before
        // this caller rebinds; recover by reconstructing the canonical completed
        // outcome rather than failing solely on the advanced version.
        let output_receipt = if matches!(
            output_intent.as_str(),
            "artifact_only" | "export_patch" | "apply_local_changes"
        ) {
            match self.record_product_nonnetwork_output_receipt(
                &artifact_id,
                task_id,
                approval_id,
                &output_intent,
                expected_task_version,
                &output_result,
                actor,
            ) {
                Ok(receipt) => Some(receipt),
                Err(error) if is_product_output_concurrency_race_error(&error) => {
                    return self.reuse_completed_product_output_response(
                        task_id,
                        &artifact_id,
                        &approval,
                        expected_task_version,
                        &error,
                    );
                }
                Err(_error) if output_intent == "apply_local_changes" => {
                    let _ = self.mark_product_task_output_outcome_unknown(
                        task_id,
                        actor,
                        "local_apply_receipt_completion_unconfirmed",
                    );
                    return Err(
                        "apply_local_changes receipt completion is unconfirmed; reconciliation required"
                            .to_string(),
                    );
                }
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        let terminal_success = matches!(
            (output_intent.as_str(), output_status),
            ("artifact_only", "artifact_only")
                | ("export_patch", "exported")
                | ("apply_local_changes", "applied_local_changes")
                | ("draft_pr", "draft_pr_created")
        ) && (!matches!(
            output_intent.as_str(),
            "artifact_only" | "export_patch" | "apply_local_changes"
        ) || output_receipt.is_some());
        let next_status = if terminal_success {
            ProductTaskStatus::Completed
        } else if output_status == "outcome_unknown" {
            ProductTaskStatus::OutcomeUnknown
        } else if output_status == "failed" {
            ProductTaskStatus::Failed
        } else {
            ProductTaskStatus::OutputPending
        };
        let current = self.get_product_task(task_id)?.unwrap_or(task);
        let current_status =
            ProductTaskStatus::parse(current.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if output_intent == "apply_local_changes"
            && current_status == ProductTaskStatus::OutcomeUnknown
        {
            return Err(
                "apply_local_changes effect became outcome-unknown; reconciliation required"
                    .to_string(),
            );
        }
        if current_status == ProductTaskStatus::Completed {
            return self.reuse_completed_product_output_response(
                task_id,
                &artifact_id,
                &approval,
                expected_task_version,
                "completed product task observed before terminal CAS",
            );
        }
        let state_changed = current_status != next_status;
        let mut terminal_evidence = None;
        let transitioned = if terminal_success {
            let durable_artifact = self
                .get_supervised_patch_artifact(&artifact_id)?
                .ok_or_else(|| "terminal output artifact disappeared".to_string())?;
            let candidate = self.build_product_task_terminal_evidence(
                &current,
                &durable_artifact,
                &approval,
                &output_result,
                expected_task_version.saturating_add(1),
                actor,
            )?;
            match self.complete_product_task_output_authorized(
                task_id,
                &artifact_id,
                approval_id,
                &output_intent,
                expected_task_version,
                &candidate,
                actor,
            ) {
                Ok((task, evidence)) => {
                    terminal_evidence = Some(evidence);
                    task
                }
                Err(error) if is_product_output_concurrency_race_error(&error) => {
                    return self.reuse_completed_product_output_response(
                        task_id,
                        &artifact_id,
                        &approval,
                        expected_task_version,
                        &error,
                    );
                }
                Err(error) => return Err(error),
            }
        } else if state_changed {
            match self.transition_product_task(
                task_id,
                next_status,
                current.get("version").and_then(Value::as_u64),
                actor,
                None,
                None,
                None,
                None,
                None,
            ) {
                Ok(task) => task,
                Err(error) if is_product_output_concurrency_race_error(&error) => {
                    return self.reuse_completed_product_output_response(
                        task_id,
                        &artifact_id,
                        &approval,
                        expected_task_version,
                        &error,
                    );
                }
                Err(error) => return Err(error),
            }
        } else {
            current
        };
        Ok(json!({
            "task": transitioned,
            "approval": approval,
            "artifact": artifact,
            "output": output_result,
            "output_receipt": output_receipt,
            "terminal_evidence": terminal_evidence,
            "reused": !state_changed,
        }))
    }

    /// Reconstruct the already-committed canonical output outcome for a concurrent
    /// or restarted caller. Requires completed@expected+1 (or completed@expected for
    /// exact restart with the post-completion version) plus matching durable receipt
    /// or Draft PR operation. Never converts outcome_unknown into success.
    fn reuse_completed_product_output_response(
        &self,
        task_id: &str,
        artifact_id: &str,
        approval: &Value,
        expected_task_version: u64,
        race_error: &str,
    ) -> Result<Value, String> {
        let completed = self
            .get_product_task(task_id)?
            .ok_or_else(|| "product task disappeared after output race".to_string())?;
        let completed_version = completed
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "completed product task version missing after race".to_string())?;
        if completed.get("status").and_then(Value::as_str)
            != Some(ProductTaskStatus::Completed.as_str())
            || (completed_version != expected_task_version
                && completed_version != expected_task_version.saturating_add(1))
        {
            return Err(race_error.to_string());
        }
        let completed_artifact = self
            .get_supervised_patch_artifact(artifact_id)?
            .ok_or_else(|| "completed output artifact disappeared after race".to_string())?;
        let terminal_evidence = self.get_product_task_terminal_evidence(task_id)?;
        let terminal_output = validate_completed_product_output_binding(
            &completed,
            &completed_artifact,
            approval,
            &terminal_evidence,
        )?;
        Ok(json!({
            "task": completed,
            "approval": approval,
            "artifact": completed_artifact,
            "output": terminal_output.get("output"),
            "output_receipt": terminal_output.get("receipt"),
            "operation": terminal_output.get("operation"),
            "terminal_evidence": terminal_evidence,
            "reused": true,
        }))
    }

    pub fn complete_product_task_draft_pr_output(
        &self,
        task_id: &str,
        artifact_id: &str,
        operation_id: &str,
        expected_operation_version: u64,
        expected_task_version: u64,
        pull_request: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        let approval_id = self
            .get_supervised_patch_artifact(artifact_id)?
            .and_then(|artifact| {
                artifact
                    .pointer("/product_output_operation/approval_id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .ok_or_else(|| "Draft PR completion approval identity missing".to_string())?;
        let operation = self.complete_product_output_draft_pr(
            artifact_id,
            operation_id,
            expected_operation_version,
            task_id,
            &approval_id,
            expected_task_version,
            pull_request,
            actor,
        )?;
        if operation.get("product_task_id").and_then(Value::as_str) != Some(task_id)
            || operation.get("artifact_id").and_then(Value::as_str) != Some(artifact_id)
            || operation.get("state").and_then(Value::as_str) != Some("completed")
            || operation
                .pointer("/branch_push/status")
                .and_then(Value::as_str)
                != Some("completed")
            || operation
                .pointer("/pr_create/status")
                .and_then(Value::as_str)
                != Some("completed")
        {
            return Err("completed Draft PR operation does not match product task".to_string());
        }
        let output = product_draft_pr_output_from_operation(task_id, &operation);
        let approval_id = required_product_task_string(&operation, "approval_id")?;
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| "product task missing before Draft PR completion".to_string())?;
        let run_id = required_product_task_string(&task, "run_id")?;
        let approval = self
            .workflow_run_approvals(&run_id, 1_000)?
            .into_iter()
            .find(|candidate| {
                candidate.get("approval_id").and_then(Value::as_str) == Some(&approval_id)
            })
            .ok_or_else(|| "Draft PR completion approval missing".to_string())?;
        let artifact = self
            .get_supervised_patch_artifact(artifact_id)?
            .ok_or_else(|| "Draft PR completion artifact missing".to_string())?;
        if task.get("status").and_then(Value::as_str) == Some(ProductTaskStatus::Completed.as_str())
        {
            let task_version = task
                .get("version")
                .and_then(Value::as_u64)
                .ok_or_else(|| "completed Draft PR task version missing".to_string())?;
            if task_version != expected_task_version
                && task_version != expected_task_version.saturating_add(1)
            {
                return Err("stale product task version at Draft PR replay".to_string());
            }
            let terminal_evidence = self.get_product_task_terminal_evidence(task_id)?;
            validate_completed_product_output_binding(
                &task,
                &artifact,
                &approval,
                &terminal_evidence,
            )?;
            return Ok(json!({
                "task": task,
                "operation": operation,
                "output": output,
                "terminal_evidence": terminal_evidence,
                "reused": true,
            }));
        }
        let operation = self.bind_product_output_terminal_task_version(
            task_id,
            artifact_id,
            operation_id,
            &approval_id,
            operation
                .get("current_version")
                .and_then(Value::as_u64)
                .ok_or_else(|| "completed Draft PR operation version missing".to_string())?,
            expected_task_version,
            actor,
        )?;
        let output = product_draft_pr_output_from_operation(task_id, &operation);
        let candidate = self.build_product_task_terminal_evidence(
            &task,
            &artifact,
            &approval,
            &output,
            expected_task_version.saturating_add(1),
            actor,
        )?;
        let (transitioned, terminal_evidence) = match self.complete_product_task_output_authorized(
            task_id,
            artifact_id,
            &approval_id,
            "draft_pr",
            expected_task_version,
            &candidate,
            actor,
        ) {
            Ok(completed) => completed,
            Err(error)
                if error.contains("expected-current")
                    || error.contains("stale product task version") =>
            {
                let current = self
                    .get_product_task(task_id)?
                    .ok_or_else(|| "product task disappeared after Draft PR race".to_string())?;
                if current.get("status").and_then(Value::as_str)
                    == Some(ProductTaskStatus::OutputPending.as_str())
                    && current
                        .get("version")
                        .and_then(Value::as_u64)
                        .is_some_and(|version| version > expected_task_version)
                {
                    let current_task_version = current
                        .get("version")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| {
                            "current product task version missing after Draft PR race".to_string()
                        })?;
                    if current_task_version != expected_task_version.saturating_add(1) {
                        return Err(error);
                    }
                    let current_artifact = self
                        .get_supervised_patch_artifact(artifact_id)?
                        .ok_or_else(|| {
                            "current Draft PR artifact disappeared after race".to_string()
                        })?;
                    let current_operation = self.bind_product_output_terminal_task_version(
                        task_id,
                        artifact_id,
                        operation_id,
                        &approval_id,
                        operation
                            .get("current_version")
                            .and_then(Value::as_u64)
                            .ok_or_else(|| {
                                "completed Draft PR operation version missing after race"
                                    .to_string()
                            })?,
                        current_task_version,
                        actor,
                    )?;
                    let current_output =
                        product_draft_pr_output_from_operation(task_id, &current_operation);
                    let current_candidate = self.build_product_task_terminal_evidence(
                        &current,
                        &current_artifact,
                        &approval,
                        &current_output,
                        current_task_version.saturating_add(1),
                        actor,
                    )?;
                    let (current_completed, current_evidence) = self
                        .complete_product_task_output_authorized(
                            task_id,
                            artifact_id,
                            &approval_id,
                            "draft_pr",
                            current_task_version,
                            &current_candidate,
                            actor,
                        )?;
                    return Ok(json!({
                        "task": current_completed,
                        "operation": current_operation,
                        "output": current_output,
                        "terminal_evidence": current_evidence,
                        "reused": false,
                        "recovered_after_task_version_advance": true,
                    }));
                }
                if current.get("status").and_then(Value::as_str)
                    != Some(ProductTaskStatus::Completed.as_str())
                    || current.get("version").and_then(Value::as_u64)
                        != Some(expected_task_version.saturating_add(1))
                {
                    return Err(error);
                }
                let completed_artifact = self
                    .get_supervised_patch_artifact(artifact_id)?
                    .ok_or_else(|| {
                        "completed Draft PR artifact disappeared after race".to_string()
                    })?;
                let evidence = self.get_product_task_terminal_evidence(task_id)?;
                validate_completed_product_output_binding(
                    &current,
                    &completed_artifact,
                    &approval,
                    &evidence,
                )?;
                return Ok(json!({
                    "task": current,
                    "operation": operation,
                    "output": output,
                    "terminal_evidence": evidence,
                    "reused": true,
                }));
            }
            Err(error) => return Err(error),
        };
        Ok(json!({
            "task": transitioned,
            "operation": operation,
            "output": output,
            "terminal_evidence": terminal_evidence,
            "reused": false,
        }))
    }

    pub fn mark_product_task_output_outcome_unknown(
        &self,
        task_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<Value, String> {
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        let status =
            ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
        if status == ProductTaskStatus::OutcomeUnknown {
            return Ok(task);
        }
        if !matches!(
            status,
            ProductTaskStatus::AwaitingApproval | ProductTaskStatus::OutputPending
        ) {
            return Err(format!(
                "output outcome_unknown requires awaiting_approval or output_pending; status={}",
                status.as_str()
            ));
        }
        self.transition_product_task(
            task_id,
            ProductTaskStatus::OutcomeUnknown,
            task.get("version").and_then(Value::as_u64),
            actor,
            None,
            None,
            Some("product_output_outcome_unknown"),
            Some(reason),
            None,
        )
    }

    fn prepare_product_task_local_folder_locked(
        &self,
        task_id: &str,
        intake: &ValidatedProductTaskIntake,
        actor: &str,
        receipt: &ProductTaskWorkspacePreparationReceipt,
        workspace_path: PathBuf,
    ) -> Result<Value, String> {
        if workspace_path != receipt.workspace_path {
            return Err(
                "local-folder staging path does not match the persisted receipt".to_string(),
            );
        }
        let source_root = Path::new(&intake.target_repo_path);
        let exclusions = crate::local_folder_source::configured_local_folder_exclusions()?;
        let source_manifest = crate::local_folder_source::stage_local_folder(
            source_root,
            &workspace_path,
            &exclusions,
        )?;
        if intake.source_revision != source_manifest.tree_sha256
            || intake.source_tree_hash.as_deref() != Some(source_manifest.tree_sha256.as_str())
        {
            let _ = std::fs::remove_dir_all(&workspace_path);
            return Err("local-folder source identity changed before staging".to_string());
        }
        let provisional_run_id = provisional_run_id_for_task(task_id);
        let workspace_request = json!({
            "run_id": provisional_run_id,
            "plan_id": Value::Null,
            "target_id": intake.target_id,
            "target_repo_path": source_manifest.canonical_root,
            "workspace_path": workspace_path,
            "source_revision": source_manifest.tree_sha256,
            "source_tree_hash": source_manifest.tree_sha256,
            "workspace_mode": "copy",
            "git": Value::Null,
            "status": "workspace_created",
            "product_task_id": task_id,
            "allowed_paths": intake.allowed_paths,
        });
        let workspace = self.record_supervised_patch_workspace(&workspace_request, actor)?;
        let workspace_record_id = workspace
            .get("workspace_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "local-folder supervised workspace identity is missing".to_string())?
            .to_string();
        let workspace_canonical_path = workspace
            .get("workspace_canonical_path")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "local-folder supervised workspace path is missing".to_string())?
            .to_string();
        let binding = ProductWorkspaceBinding {
            schema_version: PRODUCT_TASK_WORKSPACE_BINDING_SCHEMA_VERSION.to_string(),
            workspace_id: workspace_record_id.clone(),
            workspace_path: workspace_path.to_string_lossy().into_owned(),
            workspace_canonical_path,
            target_repo_canonical_path: source_manifest
                .canonical_root
                .to_string_lossy()
                .into_owned(),
            source_revision: source_manifest.tree_sha256.clone(),
            source_tree_hash: Some(source_manifest.tree_sha256.clone()),
            workspace_content_hash: source_manifest.tree_sha256,
            workspace_mode: "local_folder".to_string(),
            local_folder_exclusions_sha256: Some(source_manifest.excluded_paths_sha256),
            git_origin_remote: None,
            git_origin_remote_fingerprint: None,
            git_default_branch: None,
            git_default_branch_sha: None,
            provisional_run_id,
            allowed_paths: intake.allowed_paths.clone(),
            bound_at: self.now(),
        };
        let current = self
            .get_product_task(task_id)?
            .ok_or_else(|| "task missing before local-folder finalize".to_string())?;
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
            None,
        )
    }

    /// G3 compatibility wrapper. HTTP exposure requires both approval and
    /// execution scopes; all effects flow through the separated owners.
    pub fn approve_and_output_product_task(
        &self,
        task_id: &str,
        actor: &str,
        confirm_output: bool,
    ) -> Result<Value, String> {
        if !confirm_output {
            return Err("confirm_output=true required for product output".to_string());
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        if task.get("status").and_then(Value::as_str) == Some(ProductTaskStatus::Completed.as_str())
        {
            let run_id = required_product_task_string(&task, "run_id")?;
            let workspace_record_id = required_product_task_string(&task, "workspace_record_id")?;
            let source_revision = task
                .pointer("/workspace_binding/source_revision")
                .and_then(Value::as_str)
                .ok_or_else(|| "product task source revision missing".to_string())?;
            let artifact = self.current_product_task_artifact(
                task_id,
                &run_id,
                &workspace_record_id,
                source_revision,
            )?;
            let approval_id = completed_product_output_approval_id(&task, &artifact)?;
            let version = task
                .get("version")
                .and_then(Value::as_u64)
                .ok_or_else(|| "product task version missing".to_string())?;
            return self.output_product_task(task_id, actor, version, Some(approval_id), true);
        }
        let version = task
            .get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "product task version missing".to_string())?;
        let approval = self.approve_product_task(task_id, actor, version)?;
        self.output_product_task(
            task_id,
            actor,
            version,
            approval.get("approval_id").and_then(Value::as_str),
            true,
        )
    }

    pub fn approve_and_output_product_task_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
        actor: &str,
        confirm_output: bool,
    ) -> Result<Value, String> {
        self.require_product_task_tenant(task_id, tenant_id)?;
        self.approve_and_output_product_task(task_id, actor, confirm_output)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_product_task_terminal_evidence(
        &self,
        task: &Value,
        artifact: &Value,
        approval: &Value,
        output: &Value,
        terminal_task_version: u64,
        actor: &str,
    ) -> Result<Value, String> {
        let task_id = required_product_task_string(task, "task_id")?;
        let plan_id = required_product_task_string(task, "plan_id")?;
        let run_id = required_product_task_string(task, "run_id")?;
        let workspace_record_id = required_product_task_string(task, "workspace_record_id")?;
        let artifact_id = required_product_task_string(artifact, "artifact_id")?;
        let approval_id = required_product_task_string(approval, "approval_id")?;
        let output_intent = required_product_task_string(task, "output_intent")?;
        let source_revision = task
            .pointer("/workspace_binding/source_revision")
            .and_then(Value::as_str)
            .ok_or_else(|| "terminal evidence source revision missing".to_string())?;
        let plan = self
            .get_workflow_plan(&plan_id)?
            .ok_or_else(|| "terminal evidence plan missing".to_string())?;
        let run = self
            .get_workflow_run(&run_id)?
            .ok_or_else(|| "terminal evidence run missing".to_string())?;
        if run.get("status").and_then(Value::as_str) != Some("completed") {
            return Err("terminal evidence requires a completed workflow run".to_string());
        }
        let node_id = required_product_task_string(approval, "node_id")?;
        let node = run
            .get("nodes")
            .and_then(Value::as_array)
            .and_then(|nodes| {
                nodes
                    .iter()
                    .find(|node| node.get("node_id").and_then(Value::as_str) == Some(&node_id))
            })
            .ok_or_else(|| "terminal evidence approved workflow node missing".to_string())?;
        let execution = node
            .get("result")
            .filter(|result| result.is_object())
            .ok_or_else(|| "terminal evidence execution result missing".to_string())?;
        let executor_type = required_product_task_string(execution, "executor_type")?;
        let executor_class = required_product_task_string(node, "executor_class")?;
        let execution_attempt = node
            .get("attempt_count")
            .and_then(Value::as_u64)
            .ok_or_else(|| "terminal evidence execution attempt missing".to_string())?;

        let workspace = self
            .get_supervised_patch_workspace(&workspace_record_id)?
            .ok_or_else(|| "terminal evidence workspace missing".to_string())?;
        let verification = workspace
            .get("verification")
            .filter(|verification| verification.is_object())
            .ok_or_else(|| "terminal evidence verification missing".to_string())?;
        if verification.get("status").and_then(Value::as_str) != Some("evidence_recorded")
            || verification.get("trustworthy").and_then(Value::as_bool) != Some(true)
        {
            return Err("terminal evidence requires trustworthy verification".to_string());
        }
        let verification_sha256 = product_json_sha256(verification)?;
        if approval.get("verification_sha256").and_then(Value::as_str)
            != Some(verification_sha256.as_str())
        {
            return Err("terminal evidence verification approval binding changed".to_string());
        }
        let verification_receipts = verification
            .get("verification_attempts")
            .and_then(Value::as_array)
            .ok_or_else(|| "terminal evidence verification receipt set missing".to_string())?
            .iter()
            .map(|attempt| {
                let process_outcome = attempt
                    .get("process_outcome")
                    .filter(|value| value.is_object())
                    .ok_or_else(|| "verification process outcome missing".to_string())?;
                if attempt.get("result_status").and_then(Value::as_str) != Some("completed")
                    || process_outcome.get("state").and_then(Value::as_str) != Some("exited")
                    || process_outcome.get("exit_code").and_then(Value::as_i64) != Some(0)
                {
                    return Err(
                        "verification receipt lacks a successful OS process outcome".to_string()
                    );
                }
                Ok(json!({
                    "attempt": attempt.get("attempt"),
                    "node_id": attempt.get("node_id"),
                    "executor_type": attempt.get("executor_type"),
                    "result_status": attempt.get("result_status"),
                    "process_outcome": process_outcome,
                    "output_sha256": attempt.pointer("/output_digest/sha256"),
                    "started_at": attempt.get("started_at"),
                    "completed_at": attempt.get("completed_at"),
                }))
            })
            .collect::<Result<Vec<_>, String>>()?;
        if verification_receipts.is_empty() {
            return Err("terminal evidence verification receipt set is empty".to_string());
        }

        let receipt = artifact.get("product_output_receipt");
        let operation = artifact.get("product_output_operation");
        let (output_receipt_id, output_operation_id) = match output_intent.as_str() {
            "artifact_only" | "export_patch" | "apply_local_changes" => (
                Some(required_product_task_string(
                    receipt.ok_or_else(|| "terminal output receipt missing".to_string())?,
                    "receipt_id",
                )?),
                None,
            ),
            "draft_pr" => (
                None,
                Some(required_product_task_string(
                    operation.ok_or_else(|| "terminal output operation missing".to_string())?,
                    "operation_id",
                )?),
            ),
            _ => return Err("terminal evidence output intent is invalid".to_string()),
        };

        let scorecards = self.native_scorecard_artifacts_by_run(&run_id, 100)?;
        let scorecard = if scorecards.is_empty() {
            json!({
                "status": "unavailable",
                "reason": "native scorecard owner has no artifact for the exact run",
                "run_id": run_id,
            })
        } else {
            json!({
                "status": "linked",
                "run_id": run_id,
                "artifact_ids": scorecards.iter().filter_map(|card| card.get("artifact_id").cloned()).collect::<Vec<_>>(),
            })
        };

        let dispatch_id = run.get("dispatch_id").and_then(Value::as_str);
        let (replay_artifact_ids, replay_references_truncated) = match dispatch_id {
            Some(dispatch_id) => self.replay_artifacts_for_dispatch(dispatch_id)?,
            None => (Vec::new(), false),
        };
        let replay = if replay_artifact_ids.is_empty() {
            json!({
                "status": "unavailable",
                "reason": if dispatch_id.is_some() {
                    "replay owner has no exact artifact/binding for the run dispatch"
                } else {
                    "workflow run has no recorder-owned dispatch identity"
                },
                "run_id": run_id,
                "dispatch_id": dispatch_id,
            })
        } else {
            json!({
                "status": "linked",
                "run_id": run_id,
                "dispatch_id": dispatch_id,
                "artifact_ids": replay_artifact_ids,
                "references_complete": !replay_references_truncated,
                "references_truncated_at": replay_references_truncated.then_some(100),
            })
        };

        let fixture = executor_class == "fixture_deterministic";
        let managed_executor_identity = node
            .get("managed_executor_identity")
            .cloned()
            .filter(|identity| identity.is_object());
        let usage = if !fixture
            && (execution
                .get("input_tokens")
                .and_then(Value::as_i64)
                .is_some()
                || execution
                    .get("output_tokens")
                    .and_then(Value::as_i64)
                    .is_some())
        {
            json!({
                "status": "linked",
                "reference": {"run_id": run_id, "node_id": node_id, "attempt": execution_attempt},
                "input_tokens": execution.get("input_tokens"),
                "output_tokens": execution.get("output_tokens"),
                "resolved_model": execution.get("resolved_model").cloned().unwrap_or(Value::Null),
                "provenance": "node_executor_owner_reported",
            })
        } else {
            json!({
                "status": "unavailable",
                "reason": if fixture {
                    "fixture execution is not managed coding-agent usage evidence"
                } else {
                    "executor owner did not report measured token usage"
                },
                "executor_class": executor_class,
            })
        };
        let cost = json!({
            "status": "unavailable",
            "reason": if fixture {
                "fixture execution has no provider pricing or measured usage"
            } else if managed_executor_identity.is_some()
                && execution.get("estimated_cost").and_then(Value::as_f64).is_some()
            {
                "managed CLI cost is a client-side estimate, not authoritative billing evidence"
            } else {
                "no authoritative provider billing receipt is bound to the node result"
            },
        });
        let declared_budget = task
            .pointer("/intake/budget")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let declared_budget_sha256 = product_json_sha256(&declared_budget)?;
        let route_reference = json!({
            "status": "linked",
            "plan_id": plan_id,
            "dispatch_id": plan.get("dispatch_id"),
            "analysis_id": plan.pointer("/analysis/analysis_id"),
        });
        let budget_reference = json!({
            "status": "unavailable",
            "reason": "product task has a declared budget contract but no distinct budget-decision receipt",
            "declared_budget_sha256": declared_budget_sha256,
        });
        let output_result_sha256 = product_json_sha256(output)?;
        let evidence_id = format!(
            "product-terminal-{task_id}-{terminal_task_version}-{}",
            &output_result_sha256[..12]
        );

        Ok(json!({
            "schema_version": "product_task_terminal_evidence.v2",
            "evidence_id": evidence_id,
            "product_task_id": task_id,
            "tenant_id": task.get("tenant_id"),
            "workspace_scope_id": task.get("workspace_id"),
            "task_status": "completed",
            "task_version": terminal_task_version,
            "intake_contract_sha256": task.get("intake_contract_sha256"),
            "route_decision_reference": route_reference,
            "budget_decision_reference": budget_reference,
            "plan_id": plan_id,
            "run_id": run_id,
            "run_status": run.get("status"),
            "node": {
                "node_id": node_id,
                "execution_attempt": execution_attempt,
                "executor_type": executor_type,
                "executor_class": executor_class,
                "managed_executor_identity": managed_executor_identity,
                "process_outcome": execution.get("process_outcome").cloned().unwrap_or_else(|| json!({
                    "schema_version": "process_outcome.v1",
                    "state": "unavailable",
                    "exit_code": null,
                    "signal": null,
                    "unavailable_reason": "node executor does not own an OS process outcome",
                })),
            },
            "workspace_record_id": workspace_record_id,
            "source_revision": source_revision,
            "verification": {
                "verification_sha256": verification_sha256,
                "status": verification.get("status"),
                "trustworthy": verification.get("trustworthy"),
                "receipts": verification_receipts,
            },
            "artifact": {
                "artifact_id": artifact_id,
                "patch_hash": artifact.get("patch_hash"),
            },
            "approval": {
                "approval_id": approval_id,
                "approved_by": approval.get("approved_by"),
                "approval_sha256": product_json_sha256(approval)?,
            },
            "output": {
                "intent": output_intent,
                "result_sha256": output_result_sha256,
                "operation_id": output_operation_id,
                "receipt_id": output_receipt_id,
                "branch": operation.and_then(|value| value.get("head_branch")),
                "pushed_commit": operation.and_then(|value| value.pointer("/branch_push/commit_sha")),
                "draft_pr": operation.and_then(|value| value.get("pr_create")).map(|pr| json!({
                    "number": pr.get("number"),
                    "url": pr.get("url"),
                    "repository": pr.get("repository"),
                    "base_branch": pr.get("base_branch"),
                    "head_branch": pr.get("head_branch"),
                    "head_sha": pr.get("head_sha"),
                    "draft": pr.get("draft"),
                })),
            },
            "replay": replay,
            "scorecard": scorecard,
            "usage": usage,
            "cost": cost,
            "audit_reference": Value::Null,
            "content_sha256": Value::Null,
            "creation_version": terminal_task_version,
            "created_at": self.now(),
            "created_by": actor,
        }))
    }

    /// Compatibility emission is idempotent and can only return the already-committed record.
    pub fn emit_product_task_terminal_evidence(
        &self,
        task_id: &str,
        _actor: &str,
        _output: Option<&Value>,
    ) -> Result<Value, String> {
        self.get_product_task_terminal_evidence(task_id)
    }

    /// Pure read of the canonical record. It never writes audit or evidence rows.
    pub fn get_product_task_terminal_evidence(&self, task_id: &str) -> Result<Value, String> {
        let raw = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT evidence_json FROM product_task_terminal_evidence
                     WHERE product_task_id = ?1 ORDER BY task_version DESC LIMIT 1",
                    params![task_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT evidence_json FROM product_task_terminal_evidence
                         WHERE product_task_id = $1 ORDER BY task_version DESC LIMIT 1",
                        &[&task_id],
                    )
                    .map(|row| row.map(|row| row.get::<_, String>(0)))
                    .map_err(|error| error.to_string())
            })?,
        }
        .ok_or_else(|| "product task terminal evidence is not committed".to_string())?;
        let evidence: Value = serde_json::from_str(&raw)
            .map_err(|error| format!("product task terminal evidence is invalid JSON: {error}"))?;
        validate_product_terminal_evidence_content_hash(&evidence)?;
        Ok(evidence)
    }

    /// Pure read of the store-owned delegated artifact confirmation for an attempt.
    /// Contains `provider_execution` (`managed_deepseek_execution_evidence.v1`) with
    /// `provider_request_count` and `requests` — not product_task_terminal_evidence.v2.
    pub fn get_delegated_artifact_confirmation_for_attempt(
        &self,
        attempt_id: &str,
    ) -> Result<Option<Value>, String> {
        let encoded = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT artifact_confirmation_json FROM managed_acceptance_delegations
                     WHERE attempt_id=?1",
                    params![attempt_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .optional()
                .map_err(|e| e.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT artifact_confirmation_json FROM managed_acceptance_delegations
                         WHERE attempt_id=$1",
                        &[&attempt_id],
                    )
                    .map(|row| row.map(|row| row.get::<_, Option<String>>(0)))
                    .map_err(|e| e.to_string())
            })?,
        };
        match encoded {
            Some(Some(json_s)) if !json_s.is_empty() && json_s != "null" => {
                let v: Value = serde_json::from_str(&json_s)
                    .map_err(|e| format!("artifact confirmation JSON invalid: {e}"))?;
                Ok(Some(v))
            }
            _ => Ok(None),
        }
    }

    /// Canonical RWE cell evidence projection: joins ProductTask terminal evidence.v2
    /// with delegated artifact confirmation's managed provider_execution (when present).
    /// Does not invent alternate field names.
    pub fn project_rwe_cell_store_evidence(
        &self,
        product_task_id: &str,
        delegated_attempt_id: &str,
    ) -> Result<Value, String> {
        let task = self
            .get_product_task(product_task_id)?
            .ok_or("RWE cell ProductTask missing")?;
        let terminal = self
            .get_product_task_terminal_evidence(product_task_id)
            .ok();
        let confirmation = self
            .get_delegated_artifact_confirmation_for_attempt(delegated_attempt_id)?
            .unwrap_or(Value::Null);
        let provider_execution = confirmation
            .get("provider_execution")
            .cloned()
            .unwrap_or(Value::Null);
        let journal = self
            .delegated_provider_request_journal_optional(delegated_attempt_id)
            .unwrap_or_default();
        Ok(Self::sort_projection_value(&json!({
            "schema_version": "rwe_cell_store_evidence_projection.v1",
            "product_task_id": product_task_id,
            "delegated_attempt_id": delegated_attempt_id,
            "product_task": {
                "task_id": task.get("task_id"),
                "status": task.get("status"),
                "run_id": task.get("run_id"),
                "plan_id": task.get("plan_id"),
                "workspace_record_id": task.get("workspace_record_id"),
                "workspace_id": task.get("workspace_id"),
                "idempotency_key": task.get("idempotency_key"),
                "source_revision": task.get("source_revision"),
                "risk_class": task.get("risk_class"),
            },
            "terminal_evidence": terminal,
            "artifact_confirmation": confirmation,
            "provider_execution": provider_execution,
            "provider_request_journal": journal,
        })))
    }

    /// Deterministic key-sorted JSON for the RWE cell evidence projection.
    fn sort_projection_value(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().cloned().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for k in keys {
                    if let Some(v) = map.get(&k) {
                        out.insert(k, Self::sort_projection_value(v));
                    }
                }
                Value::Object(out)
            }
            Value::Array(items) => {
                Value::Array(items.iter().map(Self::sort_projection_value).collect())
            }
            other => other.clone(),
        }
    }

    fn delegated_provider_request_journal_optional(
        &self,
        attempt_id: &str,
    ) -> Result<Vec<Value>, String> {
        let encoded = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|connection| {
                connection
                    .query_row(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=?1",
                        params![attempt_id],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT provider_request_journal_json
                         FROM managed_acceptance_delegations WHERE attempt_id=$1",
                        &[&attempt_id],
                    )
                    .map(|row| row.map(|row| row.get::<_, String>(0)))
                    .map_err(|error| error.to_string())
            })?,
        };
        match encoded {
            Some(s) => serde_json::from_str(&s)
                .map_err(|_| "delegated provider request journal is malformed".into()),
            None => Ok(Vec::new()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_product_task_output(
        &self,
        task_id: &str,
        output_intent: &str,
        _run_id: &str,
        artifact_id: &str,
        workspace_record_id: &str,
        source_revision: &str,
        patch_hash: &str,
        approval_binding: &Value,
        expected_task_version: u64,
        actor: &str,
    ) -> Result<Value, String> {
        let workspace = self
            .get_supervised_patch_workspace(workspace_record_id)?
            .ok_or_else(|| "workspace missing for output".to_string())?;
        let workspace_path = workspace
            .get("workspace_path")
            .and_then(Value::as_str)
            .ok_or_else(|| "workspace_path missing for output".to_string())?;
        let target_repo_path = workspace
            .get("target_repo_path")
            .and_then(Value::as_str)
            .unwrap_or("");
        let config = TargetRepoOutputConfig::from_env();
        if workspace.get("workspace_mode").and_then(Value::as_str) != Some("copy") {
            let task = self
                .get_product_task(task_id)?
                .ok_or_else(|| "product task missing before git output".to_string())?;
            validate_bound_git_output_source(
                &config,
                &task,
                Path::new(target_repo_path),
                Path::new(workspace_path),
                source_revision,
            )?;
        }

        if output_intent == "export_patch" {
            if workspace.get("workspace_mode").and_then(Value::as_str) == Some("copy") {
                let exclusions = crate::local_folder_source::configured_local_folder_exclusions()?;
                let source_manifest = crate::local_folder_source::capture_local_folder_manifest(
                    Path::new(target_repo_path),
                    &exclusions,
                )?;
                if source_manifest.tree_sha256 != source_revision {
                    return Err("local-folder source changed before export".to_string());
                }
                let summary = crate::local_folder_source::summarize_local_folder_changes(
                    &source_manifest,
                    Path::new(workspace_path),
                    &exclusions,
                )?;
                if summary.change_sha256 != patch_hash {
                    return Err(
                        "local-folder change bundle identity changed before export".to_string()
                    );
                }
                let export_dir = product_export_root(self)?.join(task_id);
                std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;
                let export_path =
                    export_dir.join(format!("{artifact_id}.local-folder-change-bundle.json"));
                let bundle = json!({
                    "schema_version": "local_folder_change_bundle.v1",
                    "source_tree_sha256": summary.source_tree_sha256,
                    "staged_tree_sha256": summary.staged_tree_sha256,
                    "change_sha256": summary.change_sha256,
                    "changed_relative_paths": summary.changed_relative_paths,
                    "added_relative_paths": summary.added_relative_paths,
                    "modified_relative_paths": summary.modified_relative_paths,
                    "deleted_relative_paths": summary.deleted_relative_paths,
                });
                std::fs::write(
                    &export_path,
                    serde_json::to_vec(&bundle).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                return Ok(json!({
                    "mode": "export_patch",
                    "status": "exported",
                    "artifact_id": artifact_id,
                    "patch_hash": patch_hash,
                    "change_bundle_sha256": product_json_sha256(&bundle)?,
                    "approval_id": approval_binding.get("approval_id"),
                    "approval_binding": approval_binding,
                    "product_task_id": task_id,
                }));
            }
            let exported = crate::target_repo_output::export_patch(
                &config,
                Path::new(workspace_path),
                source_revision,
            )?;
            if exported.patch_hash != patch_hash {
                return Err(format!(
                    "export patch hash mismatch: expected={patch_hash} actual={}",
                    exported.patch_hash
                ));
            }
            // Persist patch under app-owned store path (not target main).
            let export_dir = product_export_root(self)?.join(task_id);
            std::fs::create_dir_all(&export_dir).map_err(|e| e.to_string())?;
            let export_path = export_dir.join(format!("{artifact_id}.patch"));
            std::fs::write(&export_path, &exported.patch).map_err(|e| e.to_string())?;
            return Ok(json!({
                "mode": "export_patch",
                "status": "exported",
                "artifact_id": artifact_id,
                "patch_hash": exported.patch_hash,
                "export_path": export_path.to_string_lossy(),
                "approval_id": approval_binding.get("approval_id"),
                "approval_binding": approval_binding,
                "product_task_id": task_id,
            }));
        }

        if output_intent == "apply_local_changes" {
            if workspace.get("workspace_mode").and_then(Value::as_str) != Some("copy") {
                return Err(
                    "apply_local_changes requires a staged local-folder workspace".to_string(),
                );
            }
            let source = Path::new(target_repo_path);
            let exclusions = crate::local_folder_source::configured_local_folder_exclusions()?;
            let current =
                crate::local_folder_source::capture_local_folder_manifest(source, &exclusions)?;
            if current.tree_sha256 != source_revision {
                return Err("local-folder source changed before confirmed output".to_string());
            }
            let task = self
                .get_product_task(task_id)?
                .ok_or_else(|| "local-folder ProductTask disappeared before output".to_string())?;
            let allowed_paths = task
                .pointer("/intake/allowed_paths")
                .and_then(Value::as_array)
                .ok_or_else(|| "local-folder workspace allowed paths are missing".to_string())?
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>();
            let expected_exclusion_hash = task
                .pointer("/workspace_binding/local_folder_exclusions_sha256")
                .and_then(Value::as_str)
                .ok_or_else(|| "local-folder exclusion policy binding is missing".to_string())?;
            if current.excluded_paths_sha256 != expected_exclusion_hash {
                return Err(
                    "local-folder exclusion policy changed before confirmed output".to_string(),
                );
            }
            let rollback_root = product_export_root(self)?
                .join("local-folder-rollbacks")
                .join(task_id)
                .join(artifact_id);
            let receipt = crate::local_folder_source::apply_local_folder_changes(
                &current,
                Path::new(workspace_path),
                &rollback_root,
                &allowed_paths,
                &exclusions,
                patch_hash,
            )?;
            return Ok(json!({
                "mode": "apply_local_changes",
                "status": "applied_local_changes",
                "artifact_id": artifact_id,
                "patch_hash": patch_hash,
                "approval_id": approval_binding.get("approval_id"),
                "approval_binding": approval_binding,
                "product_task_id": task_id,
                "changed_relative_paths": receipt.changed_relative_paths,
                "source_tree_sha256": receipt.source_tree_sha256,
                "staged_tree_sha256": receipt.staged_tree_sha256,
                "rollback_bundle_present": true,
                "rollback_root_fingerprint": hex::encode(Sha256::digest(rollback_root.to_string_lossy().as_bytes())),
            }));
        }

        // draft_pr: always persist the exact hash-bound output plan first.
        // The explicit network gate controls only execution of that same
        // operation; planning cannot push a branch or create a pull request.
        let allow_network = std::env::var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);

        self.prepare_product_draft_pr_output(
            task_id,
            artifact_id,
            workspace_path,
            target_repo_path,
            source_revision,
            patch_hash,
            approval_binding,
            expected_task_version,
            actor,
            &config,
            allow_network,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_product_draft_pr_output(
        &self,
        task_id: &str,
        artifact_id: &str,
        workspace_path: &str,
        target_repo_path: &str,
        source_revision: &str,
        patch_hash: &str,
        approval_binding: &Value,
        expected_task_version: u64,
        actor: &str,
        config: &TargetRepoOutputConfig,
        allow_network: bool,
    ) -> Result<Value, String> {
        let branch_name = format!("acp/product-{task_id}");
        let artifact = self
            .get_supervised_patch_artifact(artifact_id)?
            .ok_or_else(|| "artifact missing for draft_pr".to_string())?;
        let artifact_workspace = self
            .get_supervised_patch_workspace(
                artifact
                    .get("workspace_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "artifact workspace binding missing".to_string())?,
            )?
            .ok_or_else(|| "artifact workspace is missing for draft_pr".to_string())?;
        let bound_origin = artifact_workspace
            .pointer("/git/origin_remote")
            .and_then(Value::as_str)
            .ok_or_else(|| "workspace is missing its credential-free bound origin".to_string())?;
        // Provider-free planning does not require GitHub admission. The
        // repository/credential boundary is enforced only when a network
        // effect is explicitly enabled.
        let repository = match crate::target_repo_output::parse_github_repository_url(bound_origin)
        {
            Ok(repository) => repository,
            Err(error) => {
                if !allow_network {
                    return Ok(json!({
                        "mode": "draft_pr",
                        "status": "blocked",
                        "admission_blocked": true,
                        "recovery_required": false,
                        "reason": error,
                        "product_task_id": task_id,
                        "artifact_id": artifact_id,
                    }));
                }
                return Ok(json!({
                    "mode": "draft_pr",
                    "status": "blocked",
                    "admission_blocked": true,
                    "recovery_required": false,
                    "reason": error,
                    "product_task_id": task_id,
                    "artifact_id": artifact_id,
                }));
            }
        };
        let target_repository = format!("{}/{}", repository.owner, repository.repository);
        let base_branch = artifact_workspace
            .pointer("/git/default_branch")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| "workspace is missing the bound target default branch".to_string())?;
        let request = json!({
            "schema_version": "product_draft_pr_output_request.v1",
            "product_task_id": task_id,
            "artifact_id": artifact_id,
            "approval_id": approval_binding.get("approval_id"),
            "output_intent": "draft_pr",
            "expected_task_version": expected_task_version,
            "workspace_id": artifact.get("workspace_id"),
            "run_id": artifact.get("run_id"),
            "target_id": artifact.get("target_id"),
            "patch_hash": artifact.get("patch_hash"),
            "source_revision": artifact.get("source_revision"),
            "target_repository": target_repository,
            "repository_host": repository.host,
            "base_branch": base_branch,
            "head_branch": branch_name,
            "remote": "origin",
            "commit_message": format!("feat: product golden path {task_id}"),
            "pr_title": format!("Draft: product task {task_id}"),
            "pr_body": format!(
                "Product golden path Draft PR for task `{task_id}`.\n\nDo not merge automatically."
            ),
        });
        let request_sha256 = product_json_sha256(&request)?;
        let operation = self.claim_product_output_operation(
            artifact_id,
            &request,
            &request_sha256,
            expected_task_version,
            actor,
        )?;
        let action = operation
            .get("claim_action")
            .and_then(Value::as_str)
            .unwrap_or("reconciliation_required");
        if action == "reused" {
            return Ok(product_draft_pr_output_from_operation(task_id, &operation));
        }
        if matches!(action, "create_or_reconcile_pr" | "reconcile_pr_only") {
            return Ok(product_draft_pr_pending_output(task_id, &operation));
        }
        if action == "operation_in_progress" {
            return Ok(json!({
                "mode": "draft_pr",
                "status": "operation_in_progress",
                "operation": operation,
                "product_task_id": task_id,
            }));
        }
        if !matches!(action, "push_branch" | "push_or_reconcile_branch") {
            return Ok(json!({
                "mode": "draft_pr",
                "status": "outcome_unknown",
                "reconciliation_required": true,
                "operation": operation,
                "product_task_id": task_id,
            }));
        }
        if !allow_network {
            return Ok(json!({
                "mode": "draft_pr",
                "status": "planned",
                "network_effect": false,
                "request_sha256": request_sha256,
                "operation": operation,
                "target_repository": target_repository,
                "base_branch": base_branch,
                "head_branch": branch_name,
                "approval_binding": approval_binding,
                "product_task_id": task_id,
            }));
        }
        if let Err(error) = crate::target_repo_output::GitHubPullRequestConfig::from_env()
            .require_repository(&repository)
        {
            let operation_id = operation
                .get("operation_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "product output operation identity missing".to_string())?;
            let operation_version = operation
                .get("current_version")
                .and_then(Value::as_u64)
                .ok_or_else(|| "product output operation version missing".to_string())?;
            let operation = self.mark_product_output_branch_failed_known(
                artifact_id,
                operation_id,
                operation_version,
                task_id,
                approval_binding
                    .get("approval_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "product output approval identity missing".to_string())?,
                expected_task_version,
                actor,
                &error,
            )?;
            return Ok(json!({
                "mode": "draft_pr",
                "status": "blocked",
                "pre_effect_failure": true,
                "recovery_required": true,
                "reason": error,
                "operation": operation,
                "product_task_id": task_id,
                "artifact_id": artifact_id,
            }));
        }
        let publish = crate::target_repo_output::BranchPublishRequest {
            target_repo_path: Path::new(target_repo_path).to_path_buf(),
            workspace_path: Path::new(workspace_path).to_path_buf(),
            source_revision: source_revision.to_string(),
            expected_patch_hash: patch_hash.to_string(),
            branch_name,
            remote: "origin".to_string(),
            commit_message: format!("feat: product golden path {task_id}"),
            pr_title: format!("Draft: product task {task_id}"),
            pr_body: format!(
                "Product golden path Draft PR for task `{task_id}`.\n\nDo not merge automatically."
            ),
        };
        match crate::target_repo_output::push_approved_branch(config, publish) {
            Ok(published) => {
                self.record_product_output_branch_pushed(
                    artifact_id,
                    operation
                        .get("operation_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "product output operation identity missing".to_string())?,
                    operation
                        .get("current_version")
                        .and_then(Value::as_u64)
                        .ok_or_else(|| "product output operation version missing".to_string())?,
                    task_id,
                    approval_binding
                        .get("approval_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| "product output approval identity missing".to_string())?,
                    expected_task_version,
                    &published.commit_sha,
                    actor,
                )?;
                let pr_operation = self.claim_product_output_operation(
                    artifact_id,
                    &request,
                    &request_sha256,
                    expected_task_version,
                    actor,
                )?;
                if pr_operation.get("claim_action").and_then(Value::as_str)
                    != Some("create_or_reconcile_pr")
                {
                    return Ok(json!({
                        "mode": "draft_pr",
                        "status": "operation_in_progress",
                        "operation": pr_operation,
                        "product_task_id": task_id,
                    }));
                }
                Ok(product_draft_pr_pending_output(task_id, &pr_operation))
            }
            Err(error) => {
                let operation_id = operation
                    .get("operation_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "product output operation identity missing".to_string())?;
                let operation_version = operation
                    .get("current_version")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "product output operation version missing".to_string())?;
                let outcome_unknown = error.starts_with("branch_push_outcome_unknown:");
                let operation = if outcome_unknown {
                    self.mark_product_output_branch_outcome_unknown(
                        artifact_id,
                        operation_id,
                        operation_version,
                        task_id,
                        approval_binding
                            .get("approval_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                "product output approval identity missing".to_string()
                            })?,
                        expected_task_version,
                        actor,
                        &error,
                    )?
                } else {
                    self.mark_product_output_branch_failed_known(
                        artifact_id,
                        operation_id,
                        operation_version,
                        task_id,
                        approval_binding
                            .get("approval_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                "product output approval identity missing".to_string()
                            })?,
                        expected_task_version,
                        actor,
                        &error,
                    )?
                };
                Ok(json!({
                    "mode": "draft_pr",
                    "status": if outcome_unknown { "outcome_unknown" } else { "failed" },
                    "reason": error,
                    "operation": operation,
                    "product_task_id": task_id,
                }))
            }
        }
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

    pub(crate) fn current_product_task_artifact(
        &self,
        task_id: &str,
        run_id: &str,
        workspace_record_id: &str,
        source_revision: &str,
    ) -> Result<Value, String> {
        let artifacts = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut statement = conn
                    .prepare(
                        "SELECT artifact_sequence, artifact_id, workspace_id, run_id, plan_id,
                                target_id, source_revision, artifact_type, patch_hash,
                                changed_files_json, redaction_status, created_at, artifact_json
                         FROM supervised_patch_artifacts
                         WHERE run_id=?1 AND workspace_id=?2 AND source_revision=?3
                         ORDER BY artifact_sequence DESC",
                    )
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map(
                        params![run_id, workspace_record_id, source_revision],
                        managed_acceptance_artifact_row_sqlite,
                    )
                    .map_err(|error| error.to_string())?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT artifact_sequence, artifact_id, workspace_id, run_id, plan_id,
                                target_id, source_revision, artifact_type, patch_hash,
                                changed_files_json, redaction_status, created_at, artifact_json
                         FROM supervised_patch_artifacts
                         WHERE run_id=$1 AND workspace_id=$2 AND source_revision=$3
                         ORDER BY artifact_sequence DESC",
                        &[&run_id, &workspace_record_id, &source_revision],
                    )
                    .map_err(|error| error.to_string())?;
                rows.iter()
                    .map(managed_acceptance_artifact_row_pg)
                    .collect::<Result<Vec<_>, _>>()
            })?,
        };
        let matches = artifacts
            .into_iter()
            .filter(|artifact| {
                artifact.get("product_task_id").and_then(Value::as_str) == Some(task_id)
                    && artifact.get("run_id").and_then(Value::as_str) == Some(run_id)
                    && artifact.get("workspace_id").and_then(Value::as_str)
                        == Some(workspace_record_id)
                    && artifact.get("source_revision").and_then(Value::as_str)
                        == Some(source_revision)
            })
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [artifact] => Ok(artifact.clone()),
            [] => Err("no exact artifact found for product task".to_string()),
            _ => Err("multiple artifacts match current product task binding".to_string()),
        }
    }

    /// Restart recovery: re-enter prepare for tasks left in workspace_preparing/admitted.
    pub fn recover_product_task_workspace(
        &self,
        task_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        let mut contention_observed = false;
        for attempt in 0..PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_LIMIT {
            let task = self
                .get_product_task(task_id)?
                .ok_or_else(|| format!("product task not found: {task_id}"))?;
            if product_task_has_prepared_workspace(&task)
                || product_task_has_terminal_workspace_prepare_state(&task)
            {
                let _ =
                    self.retire_completed_product_task_workspace_preparation(task_id, &task, actor);
                return Ok(task);
            }
            let status = task.get("status").and_then(Value::as_str).unwrap_or("");
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
            // This is intentionally before the receipt/guard path for both
            // admitted and preparing rows: a disabled or invalid target-output
            // boundary must not create a root, marker, or lock file.
            validate_product_task_workspace_prerequisites(&intake)?;
            if status == ProductTaskStatus::Admitted.as_str() {
                validate_product_task_workspace_preflight(self, task_id, &intake)?;
            }
            match self.prepare_product_task_worktree(
                task_id,
                &intake,
                actor,
                "worktree_recover_failed",
                &mut contention_observed,
            ) {
                Ok(task) => return Ok(task),
                Err(error)
                    if error == PRODUCT_TASK_WORKSPACE_PREPARATION_ACTIVE
                        && attempt + 1 < PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_LIMIT =>
                {
                    std::thread::sleep(PRODUCT_TASK_CONCURRENT_ADMIT_RETRY_DELAY);
                }
                Err(error) => return Err(error),
            }
        }
        Err(
            "product task workspace recovery retry exhausted while preparation remains active"
                .to_string(),
        )
    }

    pub(crate) fn recover_product_task_workspace_for_tenant(
        &self,
        tenant_id: &str,
        task_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        self.require_product_task_tenant(task_id, tenant_id)?;
        self.recover_product_task_workspace(task_id, actor)
    }

    /// Execute the receipt-bound compensation path for a completed local
    /// apply. The rollback locator never crosses a public projection; only the
    /// artifact receipt may supply it. A crash after the claim is deliberately
    /// reconciliation-only rather than a second filesystem mutation.
    pub fn rollback_product_task_local_folder_apply(
        &self,
        task_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        if !product_gate_enabled() {
            return Err("product golden path intake is disabled".to_string());
        }
        let task = self
            .get_product_task(task_id)?
            .ok_or_else(|| format!("product task not found: {task_id}"))?;
        if task.get("status").and_then(Value::as_str) != Some(ProductTaskStatus::Completed.as_str())
            || task.get("output_intent").and_then(Value::as_str) != Some("apply_local_changes")
        {
            return Err(
                "local-folder rollback requires a completed apply_local_changes task".to_string(),
            );
        }
        let run_id = required_product_task_string(&task, "run_id")?;
        let workspace_record_id = required_product_task_string(&task, "workspace_record_id")?;
        let source_revision = task
            .pointer("/workspace_binding/source_revision")
            .and_then(Value::as_str)
            .ok_or_else(|| "local-folder rollback source identity is missing".to_string())?;
        let artifact = self.current_product_task_artifact(
            task_id,
            &run_id,
            &workspace_record_id,
            source_revision,
        )?;
        let artifact_id = required_product_task_string(&artifact, "artifact_id")?;
        let output = artifact
            .pointer("/product_output_receipt/output")
            .ok_or_else(|| "local-folder rollback output receipt is missing".to_string())?;
        let changed_paths = output
            .get("changed_relative_paths")
            .and_then(Value::as_array)
            .ok_or_else(|| "local-folder rollback paths are missing".to_string())?
            .iter()
            .map(|path| {
                path.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "local-folder rollback path is invalid".to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let source = task
            .get("target_repo_path")
            .and_then(Value::as_str)
            .ok_or_else(|| "local-folder rollback source is missing".to_string())?;
        let expected_rollback_root = product_export_root(self)?
            .join("local-folder-rollbacks")
            .join(task_id)
            .join(&artifact_id);
        let expected_rollback_root_fingerprint = hex::encode(Sha256::digest(
            expected_rollback_root.to_string_lossy().as_bytes(),
        ));
        if output
            .get("rollback_root_fingerprint")
            .and_then(Value::as_str)
            != Some(expected_rollback_root_fingerprint.as_str())
        {
            return Err(
                "local-folder rollback proof does not match its artifact binding".to_string(),
            );
        }
        let claim = self.claim_product_local_apply_rollback(&artifact_id, task_id, actor)?;
        if claim.get("reused").and_then(Value::as_bool) == Some(true) {
            return Ok(json!({"task": task, "rollback": claim.get("rollback"), "reused": true}));
        }
        if let Err(_error) = crate::local_folder_source::rollback_local_folder_changes(
            Path::new(source),
            &expected_rollback_root,
            &changed_paths,
        ) {
            let _ = self.mark_product_local_apply_rollback_outcome_unknown(
                &artifact_id,
                task_id,
                actor,
            );
            return Err(
                "local-folder rollback effect is unconfirmed; reconciliation required".to_string(),
            );
        }
        let rollback = match self.complete_product_local_apply_rollback(
            &artifact_id,
            task_id,
            actor,
        ) {
            Ok(result) => result,
            Err(_error) => {
                let _ = self.mark_product_local_apply_rollback_outcome_unknown(
                    &artifact_id,
                    task_id,
                    actor,
                );
                return Err("local-folder rollback receipt completion is unconfirmed; reconciliation required".to_string());
            }
        };
        Ok(json!({"task": task, "rollback": rollback.get("rollback"), "reused": false}))
    }
}

/// Git output is allowed only while the repository identity captured at
/// worktree binding is unchanged.  The source commit alone is insufficient:
/// an operator could otherwise retarget `origin` or advance the default branch
/// after review but before export/publish.
fn validate_bound_git_output_source(
    config: &TargetRepoOutputConfig,
    task: &Value,
    target_repo_path: &Path,
    workspace_path: &Path,
    source_revision: &str,
) -> Result<(), String> {
    let binding = task
        .get("workspace_binding")
        .ok_or_else(|| "git output workspace binding is missing".to_string())?;
    let expected_remote = binding
        .get("git_origin_remote")
        .and_then(Value::as_str)
        .ok_or_else(|| "git output origin identity is missing".to_string())?;
    let expected_fingerprint = binding
        .get("git_origin_remote_fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| "git output origin fingerprint is missing".to_string())?;
    let expected_branch = binding
        .get("git_default_branch")
        .and_then(Value::as_str)
        .ok_or_else(|| "git output default branch identity is missing".to_string())?;
    let expected_branch_sha = binding
        .get("git_default_branch_sha")
        .and_then(Value::as_str)
        .ok_or_else(|| "git output default branch SHA is missing".to_string())?;
    let observed =
        inspect_registered_git_worktree(config, target_repo_path, workspace_path, source_revision)?;
    if observed.origin_remote != expected_remote
        || observed.origin_remote_fingerprint != expected_fingerprint
        || observed.default_branch != expected_branch
        || observed.default_branch_sha != expected_branch_sha
    {
        return Err("git source identity changed before output".to_string());
    }
    Ok(())
}

#[cfg(unix)]
fn validate_product_task_workspace_preparation_marker(
    task_id: &str,
    receipt: &ProductTaskWorkspacePreparationReceipt,
    create_if_planned: bool,
) -> Result<(), String> {
    let current_root = std::fs::canonicalize(&receipt.workspace_root).map_err(|_| {
        format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt root is unavailable"
        )
    })?;
    if current_root != receipt.workspace_root || !current_root.is_dir() {
        return Err(format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt root identity is unavailable"
        ));
    }
    let marker_path = receipt.marker_path(task_id)?;
    let expected_contents = format!("{}\n", receipt.marker_sha256);
    let read_marker = || -> Result<(), String> {
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&marker_path)
            .map_err(|_| {
                format!(
                    "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is unavailable"
                )
            })?;
        let metadata = file.metadata().map_err(|_| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is unavailable"
            )
        })?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is not a regular file"
            ));
        }
        let mode = metadata.permissions().mode();
        if mode & 0o077 != 0 {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker permissions are invalid"
            ));
        }
        if metadata.len() != expected_contents.len() as u64 {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker length is invalid"
            ));
        }
        let mut contents = [0_u8; 65];
        file.read_exact(&mut contents).map_err(|_| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is unavailable"
            )
        })?;
        if contents.as_slice() != expected_contents.as_bytes() {
            return Err(format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker identity is invalid"
            ));
        }
        Ok(())
    };

    if receipt.marker_state == ProductTaskWorkspacePreparationMarkerState::MarkerReady
        || !create_if_planned
    {
        return read_marker();
    }

    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&marker_path)
    {
        Ok(mut file) => {
            file.write_all(expected_contents.as_bytes()).map_err(|_| {
                format!(
                    "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is unavailable"
                )
            })?;
            file.sync_all().map_err(|_| {
                format!(
                    "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is unavailable"
                )
            })?;
            let mode = file
                .metadata()
                .map_err(|_| {
                    format!(
                        "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is unavailable"
                    )
                })?
                .permissions()
                .mode();
            if mode & 0o077 != 0 {
                return Err(format!(
                    "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker permissions are invalid"
                ));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read_marker(),
        Err(_) => Err(format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is unavailable"
        )),
    }
}

#[cfg(not(unix))]
fn validate_product_task_workspace_preparation_marker(
    _task_id: &str,
    _receipt: &ProductTaskWorkspacePreparationReceipt,
    _create_if_planned: bool,
) -> Result<(), String> {
    Err(format!(
        "{PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED}: receipt marker is unsupported on this platform"
    ))
}

fn product_workspace_path(
    store: &LocalProductStore,
    workspace_fs_id: &str,
) -> Result<PathBuf, String> {
    if let Ok(configured) = std::env::var("ACP_PRODUCT_WORKSPACE_ROOT") {
        let root = PathBuf::from(configured);
        if !root.is_absolute() {
            return Err("ACP_PRODUCT_WORKSPACE_ROOT must be absolute".to_string());
        }
        return Ok(root.join(workspace_fs_id));
    }
    if store.is_postgres() {
        return Err(
            "PostgreSQL product workspaces require absolute ACP_PRODUCT_WORKSPACE_ROOT".to_string(),
        );
    }
    planned_workspace_path(store.db_path(), workspace_fs_id)
}

fn canonicalize_with_missing_tail(path: &Path) -> Result<PathBuf, String> {
    if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::CurDir | std::path::Component::ParentDir
        )
    }) {
        return Err(format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: workspace root contains relative traversal"
        ));
    }
    let mut cursor = path;
    let mut tail = Vec::new();
    while !cursor.exists() {
        let component = cursor.file_name().ok_or_else(|| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: workspace root is unavailable"
            )
        })?;
        tail.push(component.to_os_string());
        cursor = cursor.parent().ok_or_else(|| {
            format!(
                "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: workspace root is unavailable"
            )
        })?;
    }
    if !cursor.is_dir() {
        return Err(format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: workspace root is unavailable"
        ));
    }
    let mut canonical = std::fs::canonicalize(cursor).map_err(|_| {
        format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: workspace root is unavailable"
        )
    })?;
    for component in tail.into_iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn product_task_workspace_root_for_preparation(
    store: &LocalProductStore,
    task_id: &str,
    intake: &ValidatedProductTaskIntake,
) -> Result<PathBuf, String> {
    validate_product_task_workspace_prerequisites(intake)?;
    let workspace_fs_id = product_task_workspace_fs_id(task_id);
    let workspace_path = product_workspace_path(store, &workspace_fs_id)?;
    let workspace_root = workspace_path.parent().ok_or_else(|| {
        format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: workspace root is unavailable"
        )
    })?;
    let target_root = std::fs::canonicalize(&intake.target_repo_path).map_err(|_| {
        format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: target repository is unavailable"
        )
    })?;
    let canonical_workspace_root = canonicalize_with_missing_tail(workspace_root)?;
    if paths_overlap(&canonical_workspace_root, &target_root) {
        return Err(format!(
            "{PRODUCT_TASK_WORKSPACE_PREPARATION_PRECONDITION_UNAVAILABLE}: workspace root overlaps the target repository"
        ));
    }
    Ok(canonical_workspace_root)
}

/// Validate only the read-only target-output and target-repository boundary.
/// Both fresh admission and restart recovery run this before they can create a
/// lock/marker or mutate a worktree.
fn validate_product_task_workspace_prerequisites(
    intake: &ValidatedProductTaskIntake,
) -> Result<(), String> {
    if intake.workspace_mode == "local_folder" {
        let exclusions = crate::local_folder_source::configured_local_folder_exclusions()?;
        crate::local_folder_source::capture_local_folder_manifest(
            Path::new(&intake.target_repo_path),
            &exclusions,
        )?;
        return Ok(());
    }
    let config = TargetRepoOutputConfig::from_env();
    config.require_enabled()?;
    let target_repo = Path::new(&intake.target_repo_path);
    if !target_repo.is_absolute() {
        return Err("target_repo_path must be absolute".to_string());
    }
    if !target_repo.is_dir() {
        return Err("target_repo_path is not a directory".to_string());
    }
    std::fs::canonicalize(target_repo).map_err(|error| error.to_string())?;
    Ok(())
}

/// Validate every read-only prerequisite before a fresh intake publishes
/// `workspace_preparing`. Physical preparation repeats the generic checks
/// while holding its exclusion; a persisted receipt supplies the root on
/// recovery so a changed current root is never silently adopted.
fn validate_product_task_workspace_preflight(
    store: &LocalProductStore,
    task_id: &str,
    intake: &ValidatedProductTaskIntake,
) -> Result<(), String> {
    product_task_workspace_root_for_preparation(store, task_id, intake).map(|_| ())
}

fn product_export_root(store: &LocalProductStore) -> Result<PathBuf, String> {
    if let Ok(configured) = std::env::var("ACP_PRODUCT_WORKSPACE_ROOT") {
        let root = PathBuf::from(configured);
        if !root.is_absolute() {
            return Err("ACP_PRODUCT_WORKSPACE_ROOT must be absolute".to_string());
        }
        return Ok(root.join("exports"));
    }
    if store.is_postgres() {
        return Err(
            "PostgreSQL product exports require absolute ACP_PRODUCT_WORKSPACE_ROOT".to_string(),
        );
    }
    store
        .db_path()
        .parent()
        .map(|parent| parent.join("exports"))
        .ok_or_else(|| "store path has no parent".to_string())
}

fn product_verification_node_authority(
    run: &Value,
    node_id: &str,
) -> Result<ProductVerificationNodeAuthority, String> {
    let node = run
        .get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node.get("node_id").and_then(Value::as_str) == Some(node_id))
        })
        .ok_or_else(|| "authoritative execution node missing".to_string())?;
    if node.get("db_status").and_then(Value::as_str) != Some("completed") {
        return Err(format!(
            "execution node authority is not completed: {}",
            node.get("db_status")
                .and_then(Value::as_str)
                .unwrap_or("missing")
        ));
    }
    let attempt_count = node
        .get("attempt_count")
        .and_then(Value::as_u64)
        .filter(|attempt| *attempt > 0)
        .ok_or_else(|| "execution node attempt authority missing".to_string())?;
    let result = node
        .get("result")
        .filter(|result| !result.is_null())
        .ok_or_else(|| "execution node result authority missing".to_string())?;
    Ok(ProductVerificationNodeAuthority {
        node_id: node_id.to_string(),
        attempt_count,
        leased_at: node
            .get("leased_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        result_sha256: product_json_sha256(result)?,
    })
}

fn persisted_product_managed_output(
    store: &LocalProductStore,
    run_id: &str,
    node_id: &str,
) -> Result<Option<NodeExecutionOutput>, String> {
    let run = store
        .get_workflow_run(run_id)?
        .ok_or_else(|| "managed product verification run disappeared".to_string())?;
    let node = run
        .get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| nodes.first())
        .filter(|node| node.get("node_id").and_then(Value::as_str) == Some(node_id))
        .ok_or_else(|| "managed product verification node disappeared".to_string())?;
    match node.get("status").and_then(Value::as_str) {
        Some("completed" | "failed" | "awaiting_approval") => node
            .get("result")
            .ok_or_else(|| "managed product verification persisted result is missing".to_string())
            .and_then(product_node_output_from_value)
            .map(Some),
        Some("pending") => Ok(None),
        Some("running") => Err(
            "product verification canonical managed operation is already in progress".to_string(),
        ),
        Some(status) => Err(format!(
            "managed product verification has unsupported status: {status}"
        )),
        None => Err("managed product verification status is missing".to_string()),
    }
}

fn product_node_output_from_value(value: &Value) -> Result<NodeExecutionOutput, String> {
    let required = |field: &str| {
        value
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("managed product verification result missing {field}"))
    };
    Ok(NodeExecutionOutput {
        status: required("status")?,
        executor_type: required("executor_type")?,
        output: value
            .get("output")
            .and_then(Value::as_str)
            .map(str::to_string),
        error_domain: value
            .get("error_domain")
            .and_then(Value::as_str)
            .map(str::to_string),
        error_message: value
            .get("error_message")
            .and_then(Value::as_str)
            .map(str::to_string),
        input_tokens: value.get("input_tokens").and_then(Value::as_i64),
        output_tokens: value.get("output_tokens").and_then(Value::as_i64),
        estimated_cost: value.get("estimated_cost").and_then(Value::as_f64),
        latency_ms: value.get("latency_ms").and_then(Value::as_i64),
        process_outcome: value
            .get("process_outcome")
            .cloned()
            .filter(|outcome| !outcome.is_null())
            .map(serde_json::from_value::<ProcessOutcome>)
            .transpose()
            .map_err(|error| format!("invalid managed process_outcome: {error}"))?,
        resolved_model: value
            .get("resolved_model")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn validate_product_task_elapsed_budget(task: &Value, now: &str) -> Result<(), String> {
    let remaining = product_task_remaining_elapsed_ms(task, now)?;
    if remaining == 0 {
        let limit_ms = task
            .pointer("/intake/budget/total_elapsed_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        return Err(format!(
            "budget_exhausted:total_elapsed_ms={limit_ms}:remaining_ms=0"
        ));
    }
    Ok(())
}

fn product_run_token_budget_exhausted(run: &Value) -> bool {
    run.get("nodes")
        .and_then(Value::as_array)
        .is_some_and(|nodes| {
            nodes.iter().any(|node| {
                matches!(
                    node.pointer("/result/error_domain").and_then(Value::as_str),
                    Some("product_token_budget_exhausted" | "product_call_budget_exhausted")
                )
            })
        })
}

fn product_run_failure_transition<'a>(
    run: &Value,
    run_status: &'a str,
) -> (ProductTaskStatus, &'static str, &'a str) {
    if product_run_token_budget_exhausted(run) {
        (
            ProductTaskStatus::BudgetExhausted,
            "execution_budget_exhausted",
            "budget_exhausted:product_execution",
        )
    } else if run_status == "killed" {
        (ProductTaskStatus::Killed, "execution_killed", run_status)
    } else {
        (ProductTaskStatus::Failed, "execution_failed", run_status)
    }
}

fn validate_deferred_delegated_product_plan(plan: &Value, task_id: &str) -> Result<(), String> {
    let expected_plan_owner_id = delegated_product_plan_owner_id(task_id);
    if plan.get("request_source").and_then(Value::as_str) != Some("product_golden_path_delegated")
        || plan
            .pointer("/advisory/product_task_id")
            .and_then(Value::as_str)
            != Some(task_id)
        || plan
            .pointer("/advisory/requires_executor")
            .and_then(Value::as_str)
            != Some("managed_deepseek")
        || plan
            .pointer("/advisory/delegated_plan_owner_id")
            .and_then(Value::as_str)
            != Some(expected_plan_owner_id.as_str())
    {
        return Err("persisted delegated plan is stale or owned by another task".into());
    }
    let nodes = plan
        .pointer("/graph/nodes")
        .and_then(Value::as_array)
        .ok_or("persisted delegated plan nodes are missing")?;
    if nodes.len() != 4 {
        return Err("persisted delegated plan must contain exactly four nodes".into());
    }
    let expected = [
        ("planner", "planning"),
        ("implementer", "implementation"),
        ("reviewer", "review"),
    ];
    for (role, stage) in expected {
        let node = nodes
            .iter()
            .find(|node| {
                node.pointer("/managed_deepseek/role")
                    .and_then(Value::as_str)
                    == Some(role)
            })
            .ok_or("persisted delegated plan is missing a managed stage")?;
        if node
            .pointer("/managed_deepseek/stage")
            .and_then(Value::as_str)
            != Some(stage)
            || node
                .pointer("/managed_deepseek/binding_status")
                .and_then(Value::as_str)
                != Some("deferred")
        {
            return Err("persisted delegated plan is no longer an exact deferred route".into());
        }
    }
    if nodes
        .iter()
        .filter(|node| node.get("managed_deepseek").is_some_and(Value::is_object))
        .count()
        != 3
    {
        return Err("persisted delegated plan has unexpected managed nodes".into());
    }
    Ok(())
}

fn product_task_remaining_elapsed_ms(task: &Value, now: &str) -> Result<u64, String> {
    let Some(limit_ms) = task
        .pointer("/intake/budget/total_elapsed_ms")
        .and_then(Value::as_u64)
    else {
        return Ok(u64::MAX);
    };
    let created_at = task
        .get("created_at")
        .and_then(Value::as_str)
        .ok_or_else(|| "task_elapsed_budget_created_at_missing".to_string())?;
    let created = chrono::DateTime::parse_from_rfc3339(created_at)
        .map_err(|_| "task_elapsed_budget_created_at_invalid".to_string())?;
    let current = chrono::DateTime::parse_from_rfc3339(now)
        .map_err(|_| "task_elapsed_budget_clock_invalid".to_string())?;
    let elapsed_ms = current
        .signed_duration_since(created)
        .num_milliseconds()
        .max(0) as u64;
    Ok(limit_ms.saturating_sub(elapsed_ms))
}

fn product_verification_failure_status(
    error_domain: Option<&str>,
    process_outcome: Option<&ProcessOutcome>,
) -> &'static str {
    if matches!(
        error_domain,
        Some(
            "tool_execution_outcome_unknown"
                | "tool_effect_outcome_unknown"
                | "tool_execution_receipt_error"
        )
    ) && process_outcome.is_none()
    {
        "outcome_unknown"
    } else {
        "verification_failed"
    }
}

/// Product verification may inspect repository files, but workflow-node persistence must
/// never retain their raw contents or command stderr. Preserve the authoritative process
/// outcome and replace both text channels with content hashes before the generic workflow
/// owner serializes the result.
struct RedactedProductVerificationExecutor {
    inner: CommandNodeExecutor,
}

impl NodeExecutor for RedactedProductVerificationExecutor {
    fn executor_type_name(&self) -> &str {
        self.inner.executor_type_name()
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let mut output = self.inner.execute_node(input);
        output.output = output.output.as_deref().map(|value| {
            format!(
                "redacted_command_output_sha256:{}",
                hex::encode(Sha256::digest(value.as_bytes()))
            )
        });
        output.error_message = output.error_message.as_deref().map(|value| {
            format!(
                "redacted_command_error_sha256:{}",
                hex::encode(Sha256::digest(value.as_bytes()))
            )
        });
        output
    }
}

fn required_product_task_string(value: &Value, field: &str) -> Result<String, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|candidate| !candidate.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing {field}"))
}

fn product_draft_pr_pending_output(task_id: &str, operation: &Value) -> Value {
    json!({
        "mode": "draft_pr",
        "status": "pr_create_pending",
        "product_task_id": task_id,
        "operation_id": operation.get("operation_id"),
        "artifact_id": operation.get("artifact_id"),
        "target_repository": operation.get("target_repository"),
        "base_branch": operation.get("base_branch"),
        "head_branch": operation.get("head_branch"),
        "commit_sha": operation.pointer("/branch_push/commit_sha"),
        "branch_push_status": operation.pointer("/branch_push/status"),
        "pr_create_status": operation.pointer("/pr_create/status"),
        "operation": operation,
    })
}

pub(super) fn product_draft_pr_output_from_operation(task_id: &str, operation: &Value) -> Value {
    json!({
        "mode": "draft_pr",
        "status": "draft_pr_created",
        "product_task_id": task_id,
        "operation_id": operation.get("operation_id"),
        "artifact_id": operation.get("artifact_id"),
        "target_repository": operation.get("target_repository"),
        "base_branch": operation.get("base_branch"),
        "head_branch": operation.get("head_branch"),
        "commit_sha": operation.pointer("/branch_push/commit_sha"),
        "branch_push_status": operation.pointer("/branch_push/status"),
        "pr_create_status": operation.pointer("/pr_create/status"),
        "pull_request": operation.get("pr_create"),
        "operation": operation,
    })
}

fn product_json_sha256(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(super) fn validate_product_terminal_evidence_content_hash(
    evidence: &Value,
) -> Result<(), String> {
    if evidence.get("schema_version").and_then(Value::as_str)
        != Some("product_task_terminal_evidence.v2")
    {
        return Err("product terminal evidence schema version is invalid".to_string());
    }
    let expected = required_product_task_string(evidence, "content_sha256")?;
    let mut hash_input = evidence.clone();
    hash_input
        .as_object_mut()
        .ok_or_else(|| "product terminal evidence must be an object".to_string())?
        .insert("content_sha256".to_string(), Value::Null);
    let actual = product_json_sha256(&hash_input)?;
    if actual != expected {
        return Err("product terminal evidence content hash mismatch".to_string());
    }
    Ok(())
}

fn validate_product_verification_binding(
    verification: &Value,
    task_id: &str,
    run_id: &str,
    workspace_record_id: &str,
    expected_task_version: u64,
) -> Result<(), String> {
    if verification.get("status").and_then(Value::as_str) != Some("evidence_recorded")
        || verification.get("trustworthy").and_then(Value::as_bool) != Some(true)
    {
        return Err("approval blocked: verification is not trustworthy".to_string());
    }
    for (field, expected) in [
        ("product_task_id", task_id),
        ("run_id", run_id),
        ("workspace_record_id", workspace_record_id),
    ] {
        if verification.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!("approval blocked: verification {field} mismatch"));
        }
    }
    if verification
        .get("expected_task_version")
        .and_then(Value::as_u64)
        != expected_task_version.checked_sub(1)
    {
        return Err(
            "approval blocked: verification does not bind the immediately preceding task version"
                .to_string(),
        );
    }
    Ok(())
}

fn is_product_output_concurrency_race_error(error: &str) -> bool {
    error.contains("stale product task version at output authority boundary")
        || error.contains("stale product task version at output")
        || error.contains("product output terminal expected-current update conflict")
        || error.contains("product task expected-current update conflict")
        || error.contains("expected-current")
        || error.contains("stale product task version")
}

fn validate_current_product_output_approval(
    approval: &Value,
    task: &Value,
    task_id: &str,
    run_id: &str,
    workspace_record_id: &str,
    expected_task_version: u64,
) -> Result<(), String> {
    if approval.get("schema_version").and_then(Value::as_str) != Some("product_output_approval.v1")
        || approval.get("approval_kind").and_then(Value::as_str) != Some("product_output")
        || approval.get("decision").and_then(Value::as_str) != Some("approved")
        || approval.get("output_authority").and_then(Value::as_str) != Some("product_output")
        || approval.get("execution_authority").and_then(Value::as_str) != Some("disabled")
    {
        return Err("approval does not grant product output authority".to_string());
    }
    for (field, expected) in [
        ("product_task_id", task_id),
        ("run_id", run_id),
        ("workspace_record_id", workspace_record_id),
    ] {
        if approval.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(format!("stale approval: {field} mismatch"));
        }
    }
    if approval
        .get("expected_task_version")
        .and_then(Value::as_u64)
        .is_none_or(|approved_version| approved_version > expected_task_version)
    {
        return Err("stale approval: task version mismatch".to_string());
    }
    if approval.get("source_revision").and_then(Value::as_str)
        != task
            .pointer("/workspace_binding/source_revision")
            .and_then(Value::as_str)
    {
        return Err("stale approval: source revision mismatch".to_string());
    }
    if approval.get("output_intent").and_then(Value::as_str)
        != task.get("output_intent").and_then(Value::as_str)
    {
        return Err("stale approval: output intent mismatch".to_string());
    }
    let target = approval
        .get("output_target")
        .ok_or_else(|| "approval output target missing".to_string())?;
    if target.get("target_id") != task.get("target_id")
        || target.get("target_repo_path") != task.get("target_repo_path")
    {
        return Err("stale approval: output target mismatch".to_string());
    }
    Ok(())
}

fn validate_product_artifact_against_approval(
    artifact: &Value,
    approval: &Value,
) -> Result<(), String> {
    for field in [
        "product_task_id",
        "artifact_id",
        "run_id",
        "source_revision",
        "patch_hash",
    ] {
        if artifact.get(field) != approval.get(field) {
            return Err(format!("stale approval: artifact {field} mismatch"));
        }
    }
    if artifact
        .get("verification_task_version")
        .and_then(Value::as_u64)
        .and_then(|version| version.checked_add(1))
        != approval
            .get("expected_task_version")
            .and_then(Value::as_u64)
    {
        return Err("stale approval: artifact verification version mismatch".to_string());
    }
    if artifact.get("workspace_id") != approval.get("workspace_record_id") {
        return Err("stale approval: artifact workspace mismatch".to_string());
    }
    let artifact_files = artifact
        .get("changed_files")
        .or_else(|| artifact.get("changed_files_json"));
    if artifact_files != approval.get("changed_files") {
        return Err("stale approval: artifact changed files mismatch".to_string());
    }
    Ok(())
}

fn completed_product_output_approval_id<'a>(
    task: &Value,
    artifact: &'a Value,
) -> Result<&'a str, String> {
    let intent = required_product_task_string(task, "output_intent")?;
    let record = if intent == "draft_pr" {
        artifact
            .get("product_output_operation")
            .ok_or_else(|| "completed task is missing its Draft PR operation".to_string())?
    } else {
        artifact
            .get("product_output_receipt")
            .ok_or_else(|| "completed task is missing its output receipt".to_string())?
    };
    record
        .get("approval_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "completed output is missing its approval binding".to_string())
}

fn validate_completed_product_output_binding(
    task: &Value,
    artifact: &Value,
    approval: &Value,
    terminal_evidence: &Value,
) -> Result<Value, String> {
    let task_id = required_product_task_string(task, "task_id")?;
    let artifact_id = required_product_task_string(artifact, "artifact_id")?;
    let approval_id = required_product_task_string(approval, "approval_id")?;
    let output_intent = required_product_task_string(task, "output_intent")?;
    let source_revision = required_product_task_string(artifact, "source_revision")?;
    let patch_hash = required_product_task_string(artifact, "patch_hash")?;
    let task_version = task
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "completed task version missing".to_string())?;
    if output_intent == "draft_pr" {
        let operation = artifact
            .get("product_output_operation")
            .ok_or_else(|| "completed task is missing its Draft PR operation".to_string())?;
        validate_product_terminal_evidence_content_hash(terminal_evidence)?;
        let expected_output = product_draft_pr_output_from_operation(&task_id, operation);
        let expected_output_sha256 = product_json_sha256(&expected_output)?;
        if operation.get("state").and_then(Value::as_str) != Some("completed")
            || operation.get("product_task_id").and_then(Value::as_str) != Some(task_id.as_str())
            || operation.get("artifact_id").and_then(Value::as_str) != Some(artifact_id.as_str())
            || operation.get("approval_id").and_then(Value::as_str) != Some(approval_id.as_str())
            || operation.get("source_revision").and_then(Value::as_str)
                != Some(source_revision.as_str())
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
            || operation
                .get("terminal_task_version")
                .and_then(Value::as_u64)
                .and_then(|version| version.checked_add(1))
                != Some(task_version)
            || operation.pointer("/request/expected_task_version")
                != operation.get("expected_task_version")
            || terminal_evidence.get("task_status").and_then(Value::as_str) != Some("completed")
            || terminal_evidence
                .get("task_version")
                .and_then(Value::as_u64)
                != Some(task_version)
            || terminal_evidence
                .get("product_task_id")
                .and_then(Value::as_str)
                != Some(task_id.as_str())
            || terminal_evidence
                .pointer("/output/operation_id")
                .and_then(Value::as_str)
                != operation.get("operation_id").and_then(Value::as_str)
            || terminal_evidence
                .pointer("/output/result_sha256")
                .and_then(Value::as_str)
                != Some(expected_output_sha256.as_str())
        {
            return Err("completed Draft PR operation binding is stale".to_string());
        }
        let request = operation
            .get("request")
            .ok_or_else(|| "completed Draft PR request missing".to_string())?;
        if product_json_sha256(request)?
            != required_product_task_string(operation, "request_sha256")?
        {
            return Err("completed Draft PR request hash changed".to_string());
        }
        return Ok(json!({
            "operation": operation,
            "output": product_draft_pr_output_from_operation(&task_id, operation),
        }));
    }
    let receipt = artifact
        .get("product_output_receipt")
        .ok_or_else(|| "completed task is missing its output receipt".to_string())?;
    if receipt.get("schema_version").and_then(Value::as_str) != Some("product_output_receipt.v1")
        || receipt.get("state").and_then(Value::as_str) != Some("completed")
        || receipt.get("product_task_id").and_then(Value::as_str) != Some(task_id.as_str())
        || receipt.get("artifact_id").and_then(Value::as_str) != Some(artifact_id.as_str())
        || receipt.get("approval_id").and_then(Value::as_str) != Some(approval_id.as_str())
        || receipt.get("output_intent").and_then(Value::as_str) != Some(output_intent.as_str())
        || receipt.get("source_revision").and_then(Value::as_str) != Some(source_revision.as_str())
        || receipt.get("patch_hash").and_then(Value::as_str) != Some(patch_hash.as_str())
        || receipt
            .get("expected_task_version")
            .and_then(Value::as_u64)
            .and_then(|version| version.checked_add(1))
            != Some(task_version)
    {
        return Err("completed nonnetwork output receipt binding is stale".to_string());
    }
    let request = receipt
        .get("request")
        .ok_or_else(|| "completed output receipt request missing".to_string())?;
    let output = receipt
        .get("output")
        .ok_or_else(|| "completed output receipt result missing".to_string())?;
    if request.get("expected_task_version") != receipt.get("expected_task_version")
        || product_json_sha256(request)? != required_product_task_string(receipt, "request_sha256")?
        || product_json_sha256(output)? != required_product_task_string(receipt, "output_sha256")?
    {
        return Err("completed output receipt content hash changed".to_string());
    }
    if (output_intent == "artifact_only"
        && (output.get("status").and_then(Value::as_str) != Some("artifact_only")
            || output.get("target_mutation").and_then(Value::as_bool) != Some(false)))
        || (output_intent == "export_patch"
            && (output.get("status").and_then(Value::as_str) != Some("exported")
                || output.get("patch_hash") != Some(&Value::String(patch_hash.clone()))))
        || (output_intent == "apply_local_changes"
            && (output.get("status").and_then(Value::as_str) != Some("applied_local_changes")
                || output.get("patch_hash") != Some(&Value::String(patch_hash.clone()))
                || output
                    .get("rollback_bundle_present")
                    .and_then(Value::as_bool)
                    != Some(true)))
    {
        return Err("completed output receipt result does not match its output intent".to_string());
    }
    Ok(json!({"receipt": receipt, "output": output}))
}

fn persisted_product_intake_json(intake: &ValidatedProductTaskIntake) -> Value {
    let mut persisted = redacted_intake_json(intake);
    persisted["_execution_objective_v1"] = json!({
        "schema_version": "product_execution_objective.v1",
        "objective": intake.objective,
        "objective_fingerprint": intake.objective_fingerprint,
        "content_excluded_from_public_task_and_terminal_evidence": true,
    });
    persisted
}

fn public_product_intake_json(encoded: &str) -> Result<Value, String> {
    let mut intake: Value = serde_json::from_str(encoded)
        .map_err(|error| format!("ProductTask intake_json is invalid JSON: {error}"))?;
    if let Some(object) = intake.as_object_mut() {
        object.remove("_execution_objective_v1");
    }
    Ok(intake)
}

/// Convert an internal ProductTask record into an API/evidence-safe
/// projection. Local filesystem paths remain available only to the owning
/// store methods; callers outside that boundary receive stable fingerprints.
pub(crate) fn public_product_task_projection(task: &Value) -> Value {
    use crate::product_golden_path::fingerprint_private_path;

    let mut public = task.clone();
    let redact_path = |object: &mut serde_json::Map<String, Value>, field: &str| {
        if let Some(path) = object
            .remove(field)
            .and_then(|value| value.as_str().map(str::to_string))
        {
            object.insert(
                format!("{field}_fingerprint"),
                Value::String(fingerprint_private_path(&path)),
            );
        }
    };
    if let Some(object) = public.as_object_mut() {
        redact_path(object, "target_repo_path");
        if let Some(intake) = object.get_mut("intake").and_then(Value::as_object_mut) {
            // New records already store only the fingerprint, while the
            // adapter prevents historical redacted-intake fixtures leaking a
            // legacy raw field.
            redact_path(intake, "target_repo_path");
        }
        if let Some(binding) = object
            .get_mut("workspace_binding")
            .and_then(Value::as_object_mut)
        {
            for field in [
                "workspace_path",
                "workspace_canonical_path",
                "target_repo_canonical_path",
            ] {
                redact_path(binding, field);
            }
        }
    }
    public
}

/// Redact every ProductTask response crossing the HTTP/evidence boundary.
/// Internal store records retain the minimum local path identity needed for
/// reconciliation; public responses use stable fingerprints only.
pub(crate) fn public_product_task_result_projection(result: &Value) -> Value {
    use crate::product_golden_path::fingerprint_private_path;

    fn redact(value: &mut Value) {
        const PRIVATE_PATH_FIELDS: &[&str] = &[
            "target_repo_path",
            "workspace_path",
            "workspace_canonical_path",
            "target_repo_canonical_path",
            "origin_remote",
            "git_origin_remote",
            "canonical_root",
            "rollback_root",
            "export_path",
        ];
        match value {
            Value::Object(object) => {
                for field in PRIVATE_PATH_FIELDS {
                    if let Some(path) = object
                        .remove(*field)
                        .and_then(|value| value.as_str().map(str::to_string))
                    {
                        object.insert(
                            format!("{field}_fingerprint"),
                            Value::String(fingerprint_private_path(&path)),
                        );
                    }
                }
                for nested in object.values_mut() {
                    redact(nested);
                }
            }
            Value::Array(values) => {
                for nested in values {
                    redact(nested);
                }
            }
            _ => {}
        }
    }

    let mut public = result.clone();
    redact(&mut public);
    public
}

fn map_product_task_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let intake_json: String = row.get("intake_json")?;
    let binding_json: Option<String> = row.get("workspace_binding_json")?;
    let intake =
        public_product_intake_json(&intake_json).map_err(product_task_sqlite_read_error)?;
    let binding = match binding_json.as_deref() {
        Some(encoded) => serde_json::from_str::<Value>(encoded).map_err(|error| {
            product_task_sqlite_read_error(format!(
                "ProductTask workspace_binding_json is invalid JSON: {error}"
            ))
        })?,
        None => Value::Null,
    };
    let approval_required: i64 = row.get("approval_required")?;
    let confirm_execution: i64 = row.get("confirm_execution")?;
    let confirm_output: i64 = row.get("confirm_output")?;
    let status: String = row.get("status")?;
    let admits = ProductTaskStatus::parse(&status)
        .map_err(product_task_sqlite_read_error)?
        .admits_execution();
    let approval_required =
        strict_product_task_bool_sqlite(approval_required, "approval_required")?;
    let confirm_execution =
        strict_product_task_bool_sqlite(confirm_execution, "confirm_execution")?;
    let confirm_output = strict_product_task_bool_sqlite(confirm_output, "confirm_output")?;
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
        "approval_required": approval_required,
        "confirm_execution": confirm_execution,
        "confirm_output": confirm_output,
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

fn stage_product_apply_helper(workspace_path: &Path, task: &Value) -> Result<(), String> {
    let allowed = task
        .pointer("/workspace_binding/allowed_paths")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // Prefer a path that does not already exist in a typical disposable repo (not README.md).
    let target_rel = allowed
        .iter()
        .filter_map(|v| v.as_str())
        .find(|p| {
            *p != "README.md" && !p.starts_with('.') && (p.ends_with(".md") || p.contains('/'))
        })
        .or_else(|| {
            allowed
                .iter()
                .filter_map(|v| v.as_str())
                .find(|p| *p != "README.md")
        })
        .or_else(|| allowed.iter().filter_map(|v| v.as_str()).next())
        .unwrap_or("docs/product_golden_path_fixture.md");
    // Fixture-only deterministic helper. Not a managed coding-executor path.
    let helper = workspace_path.join(FIXTURE_DETERMINISTIC_APPLY_FILENAME);
    let content = FIXTURE_DETERMINISTIC_NOTE_CONTENT;
    let script = format!(
        r#"# {schema}
# Fixture-only deterministic apply for product golden path acceptance.
# Not a managed coding agent. Mutates only the declared relative path.
from pathlib import Path
# The helper is control-plane scaffolding, not product output. Remove it before
# creating the declared change so verification and artifact capture can observe
# only repository paths admitted by the task.
Path(__file__).unlink(missing_ok=True)
target = Path({target_rel:?})
if ".." in target.parts:
    raise SystemExit("path escape rejected")
if str(target.parent) not in ("", "."):
    target.parent.mkdir(parents=True, exist_ok=True)
expected = {content:?}
target.write_text(expected, encoding="utf-8")
if target.read_text(encoding="utf-8") != expected:
    raise SystemExit("fixture write verification failed")
print("fixture_applied", target)
"#,
        schema = FIXTURE_DETERMINISTIC_APPLY_SCHEMA,
        target_rel = target_rel,
        content = content,
    );
    std::fs::write(&helper, script).map_err(|e| e.to_string())?;
    Ok(())
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
        source_kind: intake
            .get("source_kind")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                if intake.get("workspace_mode").and_then(Value::as_str) == Some("local_folder") {
                    "local_folder"
                } else {
                    "git_repository"
                }
            })
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
        workspace_mode: intake
            .get("workspace_mode")
            .and_then(Value::as_str)
            .filter(|mode| matches!(*mode, "git_worktree" | "local_folder"))
            .unwrap_or("git_worktree")
            .to_string(),
        intake_contract_sha256: task
            .get("intake_contract_sha256")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

fn product_task_sqlite_read_error(message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn managed_acceptance_owner_json_object(encoded: &str, owner: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(encoded)
        .map_err(|error| format!("managed acceptance {owner} owner is invalid JSON: {error}"))?;
    if !value.is_object() {
        return Err(format!(
            "managed acceptance {owner} owner must be a JSON object"
        ));
    }
    Ok(value)
}

fn managed_acceptance_owner_json_array(encoded: &str, owner: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(encoded)
        .map_err(|error| format!("managed acceptance {owner} owner is invalid JSON: {error}"))?;
    if !value.is_array() {
        return Err(format!(
            "managed acceptance {owner} owner must be a JSON array"
        ));
    }
    Ok(value)
}

fn managed_acceptance_workspace_row_sqlite(row: &Row<'_>) -> rusqlite::Result<Value> {
    let boundary_text: String = row.get(14)?;
    let workspace_text: String = row.get(15)?;
    let boundary = managed_acceptance_owner_json_object(&boundary_text, "workspace boundary")
        .map_err(product_task_sqlite_read_error)?;
    let mut workspace = managed_acceptance_owner_json_object(&workspace_text, "workspace")
        .map_err(product_task_sqlite_read_error)?;
    let object = workspace
        .as_object_mut()
        .expect("managed_acceptance_owner_json_object returns an object");
    object.insert(
        "workspace_sequence".to_string(),
        json!(row.get::<_, i64>(0)?),
    );
    object.insert("workspace_id".to_string(), json!(row.get::<_, String>(1)?));
    object.insert(
        "plan_id".to_string(),
        json!(row.get::<_, Option<String>>(2)?),
    );
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
        json!(row.get::<_, Option<String>>(10)?),
    );
    object.insert("status".to_string(), json!(row.get::<_, String>(11)?));
    object.insert("created_at".to_string(), json!(row.get::<_, String>(12)?));
    object.insert("updated_at".to_string(), json!(row.get::<_, String>(13)?));
    object.insert("boundary".to_string(), boundary);
    object.insert("metadata_only".to_string(), json!(true));
    object.insert("execution_authority".to_string(), json!("disabled"));
    Ok(workspace)
}

#[cfg(feature = "pg")]
fn managed_acceptance_workspace_row_pg(row: &postgres::Row) -> Result<Value, String> {
    let boundary_text: String = row.get(14);
    let workspace_text: String = row.get(15);
    let boundary = managed_acceptance_owner_json_object(&boundary_text, "workspace boundary")?;
    let mut workspace = managed_acceptance_owner_json_object(&workspace_text, "workspace")?;
    let object = workspace
        .as_object_mut()
        .expect("managed_acceptance_owner_json_object returns an object");
    object.insert(
        "workspace_sequence".to_string(),
        json!(row.get::<_, i64>(0)),
    );
    object.insert("workspace_id".to_string(), json!(row.get::<_, String>(1)));
    object.insert(
        "plan_id".to_string(),
        json!(row.get::<_, Option<String>>(2)),
    );
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
        json!(row.get::<_, Option<String>>(10)),
    );
    object.insert("status".to_string(), json!(row.get::<_, String>(11)));
    object.insert("created_at".to_string(), json!(row.get::<_, String>(12)));
    object.insert("updated_at".to_string(), json!(row.get::<_, String>(13)));
    object.insert("boundary".to_string(), boundary);
    object.insert("metadata_only".to_string(), json!(true));
    object.insert("execution_authority".to_string(), json!("disabled"));
    Ok(workspace)
}

fn managed_acceptance_artifact_row_sqlite(row: &Row<'_>) -> rusqlite::Result<Value> {
    let changed_files_text: String = row.get(9)?;
    let artifact_text: String = row.get(12)?;
    let changed_files =
        managed_acceptance_owner_json_array(&changed_files_text, "artifact changed-files")
            .map_err(product_task_sqlite_read_error)?;
    let mut artifact = managed_acceptance_owner_json_object(&artifact_text, "artifact")
        .map_err(product_task_sqlite_read_error)?;
    let object = artifact
        .as_object_mut()
        .expect("managed_acceptance_owner_json_object returns an object");
    object.insert(
        "artifact_sequence".to_string(),
        json!(row.get::<_, i64>(0)?),
    );
    object.insert("artifact_id".to_string(), json!(row.get::<_, String>(1)?));
    object.insert("workspace_id".to_string(), json!(row.get::<_, String>(2)?));
    object.insert("run_id".to_string(), json!(row.get::<_, String>(3)?));
    object.insert(
        "plan_id".to_string(),
        json!(row.get::<_, Option<String>>(4)?),
    );
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
    Ok(artifact)
}

#[cfg(feature = "pg")]
fn managed_acceptance_artifact_row_pg(row: &postgres::Row) -> Result<Value, String> {
    let changed_files_text: String = row.get(9);
    let artifact_text: String = row.get(12);
    let changed_files =
        managed_acceptance_owner_json_array(&changed_files_text, "artifact changed-files")?;
    let mut artifact = managed_acceptance_owner_json_object(&artifact_text, "artifact")?;
    let object = artifact
        .as_object_mut()
        .expect("managed_acceptance_owner_json_object returns an object");
    object.insert("artifact_sequence".to_string(), json!(row.get::<_, i64>(0)));
    object.insert("artifact_id".to_string(), json!(row.get::<_, String>(1)));
    object.insert("workspace_id".to_string(), json!(row.get::<_, String>(2)));
    object.insert("run_id".to_string(), json!(row.get::<_, String>(3)));
    object.insert(
        "plan_id".to_string(),
        json!(row.get::<_, Option<String>>(4)),
    );
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
    Ok(artifact)
}

fn strict_product_task_bool_sqlite(value: i64, field: &str) -> rusqlite::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(product_task_sqlite_read_error(format!(
            "ProductTask {field} is not a persisted boolean (expected 0 or 1, got {other})"
        ))),
    }
}

#[cfg(feature = "pg")]
fn strict_product_task_bool_pg(value: i32, field: &str) -> Result<bool, String> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(format!(
            "ProductTask {field} is not a persisted boolean (expected 0 or 1, got {other})"
        )),
    }
}

#[cfg(feature = "pg")]
fn product_task_row_to_json_pg(row: &postgres::Row) -> Result<Value, String> {
    let intake_json: String = row.get("intake_json");
    let binding_json: Option<String> = row.get("workspace_binding_json");
    let intake = public_product_intake_json(&intake_json)?;
    let binding: Value = match binding_json.as_deref() {
        Some(encoded) => serde_json::from_str(encoded).map_err(|error| {
            format!("ProductTask workspace_binding_json is invalid JSON: {error}")
        })?,
        None => Value::Null,
    };
    let approval_required: i32 = row.get("approval_required");
    let confirm_execution: i32 = row.get("confirm_execution");
    let confirm_output: i32 = row.get("confirm_output");
    let status: String = row.get("status");
    let admits = ProductTaskStatus::parse(&status)?.admits_execution();
    let approval_required = strict_product_task_bool_pg(approval_required, "approval_required")?;
    let confirm_execution = strict_product_task_bool_pg(confirm_execution, "confirm_execution")?;
    let confirm_output = strict_product_task_bool_pg(confirm_output, "confirm_output")?;
    Ok(json!({
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
        "approval_required": approval_required,
        "confirm_execution": confirm_execution,
        "confirm_output": confirm_output,
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
    }))
}

#[cfg(test)]
mod delegated_product_task_contract_tests {
    use super::{count_unified_diff_changed_lines, normalize_delegated_proposal_target_identity};
    use serde_json::json;

    #[test]
    fn legacy_live_seal_proposal_normalizes_the_nested_target_identity() {
        let proposal = json!({
            "schema_version": "pe7_product_golden_path_live_seal_manifest.v1",
            "target": {
                "repository": "Igzela/alters-lab",
                "default_branch": "main",
                "default_branch_sha": "6240768506320a324d68787b9eaa86971c8c930c",
                "allowed_mutable_paths": ["docs/USER_GUIDE.md"],
                "target_main_must_remain_unchanged": true,
                "output": "one_unmerged_acp_draft_pr_only"
            },
            "provider": {
                "protocol": "openai_compatible"
            },
            "role_models": {
                "planner": "deepseek-v4-pro",
                "implementer": "deepseek-v4-flash",
                "reviewer": "deepseek-v4-pro"
            },
            "limits": {
                "max_cost_usd": null
            }
        });

        let normalized = normalize_delegated_proposal_target_identity(&proposal).unwrap();
        assert_eq!(normalized.repository, "Igzela/alters-lab");
        assert_eq!(
            normalized.main_sha,
            "6240768506320a324d68787b9eaa86971c8c930c"
        );
        assert_eq!(normalized.mutable_paths, vec!["docs/USER_GUIDE.md"]);

        let current = json!({
            "schema_version": "managed_proposal_manifest.v1",
            "target_repository": "Igzela/alters-lab",
            "target_main_sha": "6240768506320a324d68787b9eaa86971c8c930c",
            "mutable_paths": ["docs/USER_GUIDE.md"]
        });
        assert_eq!(
            normalize_delegated_proposal_target_identity(&current).unwrap(),
            normalized
        );
    }

    #[test]
    fn unified_diff_line_count_does_not_confuse_hunk_content_with_file_headers() {
        let diff = "\
diff --git a/docs/USER_GUIDE.md b/docs/USER_GUIDE.md
--- a/docs/USER_GUIDE.md
+++ b/docs/USER_GUIDE.md
@@ -1,3 +1,3 @@
 context
---deleted content beginning with two minuses
+++added content beginning with two pluses
";
        assert_eq!(count_unified_diff_changed_lines(diff), 2);
    }
}

#[cfg(test)]
mod product_verification_failure_tests {
    use super::{product_verification_failure_status, RedactedProductVerificationExecutor};
    use crate::node_executor::{
        CommandNodeExecutor, NodeExecutionInput, NodeExecutor, ProcessOutcome,
    };
    use serde_json::json;

    #[test]
    fn consumed_or_uncertain_tool_effect_remains_outcome_unknown() {
        for domain in [
            "tool_execution_outcome_unknown",
            "tool_effect_outcome_unknown",
            "tool_execution_receipt_error",
        ] {
            assert_eq!(
                product_verification_failure_status(Some(domain), None),
                "outcome_unknown"
            );
        }
        assert_eq!(
            product_verification_failure_status(Some("command_not_allowed"), None),
            "verification_failed"
        );
        assert_eq!(
            product_verification_failure_status(
                Some("tool_effect_outcome_unknown"),
                Some(&ProcessOutcome::failure(
                    "exited",
                    Some(7),
                    "known non-zero exit",
                )),
            ),
            "verification_failed"
        );
    }

    #[test]
    fn product_verification_redacts_command_content_before_workflow_persistence() {
        let workspace = tempfile::tempdir().unwrap();
        let secret_content = "repository-content-that-must-not-be-persisted";
        std::fs::write(workspace.path().join("README.md"), secret_content).unwrap();
        let executor = RedactedProductVerificationExecutor {
            inner: CommandNodeExecutor {
                timeout_ms: 5_000,
                allowed_commands: vec!["cat".to_string()],
                allowed_binaries: vec!["cat".to_string()],
                env_vars: Vec::new(),
            },
        };
        let output = executor.execute_node(&NodeExecutionInput {
            node_id: "verify-node".to_string(),
            task_type: "command".to_string(),
            run_id: "verify-run".to_string(),
            workflow_id: "verify-workflow".to_string(),
            node_metadata: json!({
                "command": "cat README.md",
                "workspace_path": workspace.path(),
                "workspace_root": workspace.path(),
            }),
        });
        assert_eq!(output.status, "completed");
        let persisted = output.to_value().to_string();
        assert!(!persisted.contains(secret_content));
        assert!(persisted.contains("redacted_command_output_sha256"));
        assert_eq!(
            output.process_outcome.and_then(|outcome| outcome.exit_code),
            Some(0)
        );
    }
}

#[cfg(test)]
mod local_folder_product_task_tests {
    use super::*;
    use crate::product_golden_path::{
        validate_intake, ProductExecutorPolicy, ProductTaskIntakeRequest,
        ProductVerificationCommand, PRODUCT_TASK_GATE,
    };

    struct ProductGateGuard {
        previous: Option<std::ffi::OsString>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl ProductGateGuard {
        fn enable() -> Self {
            let lock = crate::cli::config::cli_env_test_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let previous = std::env::var_os(PRODUCT_TASK_GATE);
            std::env::set_var(PRODUCT_TASK_GATE, "1");
            Self {
                previous,
                _lock: lock,
            }
        }
    }

    impl Drop for ProductGateGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                std::env::set_var(PRODUCT_TASK_GATE, value);
            } else {
                std::env::remove_var(PRODUCT_TASK_GATE);
            }
        }
    }

    #[test]
    fn sqlite_workspace_preparation_transaction_reuses_receipt_committed_after_fast_miss() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("operator-folder");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("README.md"), b"original").unwrap();
        let _gate_guard = ProductGateGuard::enable();
        let source = std::fs::canonicalize(&source).unwrap();
        let request = ProductTaskIntakeRequest {
            objective: "Stage one bounded local document".to_string(),
            target_id: "sqlite-receipt-race-fixture".to_string(),
            target_repo_path: source.to_string_lossy().into_owned(),
            source_kind: Some("local_folder".to_string()),
            source_revision: "operator-supplied-placeholder".to_string(),
            source_tree_hash: None,
            allowed_paths: vec!["README.md".to_string()],
            verification_commands: vec![ProductVerificationCommand {
                command: "test -f README.md".to_string(),
                timeout_ms: 1_000,
            }],
            output_intent: "artifact_only".to_string(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["deterministic".to_string()],
                prefer: Some("deterministic".to_string()),
            },
            budget: None,
            risk_class: "low".to_string(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(false),
            idempotency_key: "sqlite-receipt-race".to_string(),
            expected_version: None,
            tenant_id: Some("tenant-local".to_string()),
            workspace_id: Some("workspace-local".to_string()),
            workspace_mode: Some("local_folder".to_string()),
        };
        let intake = validate_intake(&request, "tenant-local", "workspace-local").unwrap();
        let store = LocalProductStore::new(root.path().join("store.db")).unwrap();
        let task = store.admit_product_task(&intake, "operator").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        store
            .with_conn(|conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                tx.execute(
                    "DELETE FROM product_task_workspace_preparations WHERE task_id=?1",
                    rusqlite::params![task_id],
                )
                .map_err(|error| error.to_string())?;
                tx.execute(
                    "UPDATE product_tasks SET status=?1 WHERE task_id=?2",
                    rusqlite::params![ProductTaskStatus::Admitted.as_str(), task_id],
                )
                .map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())
            })
            .unwrap();
        assert!(store
            .product_task_workspace_preparation_receipt(task_id)
            .unwrap()
            .is_none());

        let (loser_receipt, winner_receipt) = std::thread::scope(|scope| {
            let (start_winner_tx, start_winner_rx) = std::sync::mpsc::sync_channel(0);
            let (winner_committed_tx, winner_committed_rx) = std::sync::mpsc::sync_channel(0);
            let winner_store = &store;
            let winner_intake = &intake;
            let winner = scope.spawn(move || {
                start_winner_rx.recv().unwrap();
                let receipt = winner_store
                    .ensure_product_task_workspace_preparation_receipt(
                        task_id,
                        winner_intake,
                        "winner",
                    )
                    .unwrap();
                winner_committed_tx.send(()).unwrap();
                receipt
            });
            let loser_receipt = store
                .ensure_product_task_workspace_preparation_receipt_after_sqlite_observation(
                    task_id,
                    &intake,
                    "loser",
                    || {
                        start_winner_tx.send(()).unwrap();
                        winner_committed_rx.recv().unwrap();
                    },
                )
                .unwrap();
            (loser_receipt, winner.join().unwrap())
        });
        assert_eq!(loser_receipt, winner_receipt);
        assert_eq!(loser_receipt.receipt_sha256, winner_receipt.receipt_sha256);
        assert_eq!(
            store
                .product_task_workspace_preparation_receipt(task_id)
                .unwrap()
                .unwrap(),
            winner_receipt
        );

        store
            .with_conn(|conn| {
                conn.execute(
                    "DELETE FROM product_task_workspace_preparations WHERE task_id=?1",
                    rusqlite::params![task_id],
                )
                .map(|_| ())
                .map_err(|error| error.to_string())
            })
            .unwrap();
        let error = store
            .ensure_product_task_workspace_preparation_receipt(task_id, &intake, "legacy")
            .unwrap_err();
        assert!(error.starts_with(PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED));
        assert!(error.contains("legacy preparing task has no receipt"));
    }

    #[test]
    fn local_folder_intake_stages_a_non_git_source_without_mutating_original() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("operator-folder");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("README.md"), b"original").unwrap();
        let _gate_guard = ProductGateGuard::enable();
        let source = std::fs::canonicalize(&source).unwrap();
        let request = ProductTaskIntakeRequest {
            objective: "Update a bounded local document".to_string(),
            target_id: "local-folder-fixture".to_string(),
            target_repo_path: source.to_string_lossy().into_owned(),
            source_kind: Some("local_folder".to_string()),
            source_revision: "operator-supplied-placeholder".to_string(),
            source_tree_hash: None,
            allowed_paths: vec!["README.md".to_string()],
            verification_commands: vec![ProductVerificationCommand {
                command: "test -f README.md".to_string(),
                timeout_ms: 1_000,
            }],
            output_intent: "artifact_only".to_string(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["deterministic".to_string()],
                prefer: Some("deterministic".to_string()),
            },
            budget: None,
            risk_class: "low".to_string(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(false),
            idempotency_key: "local-folder-staging".to_string(),
            expected_version: None,
            tenant_id: Some("tenant-local".to_string()),
            workspace_id: Some("workspace-local".to_string()),
            workspace_mode: Some("local_folder".to_string()),
        };
        let intake = validate_intake(&request, "tenant-local", "workspace-local").unwrap();
        let store = LocalProductStore::new(root.path().join("store.db")).unwrap();
        let task = store.admit_product_task(&intake, "operator").unwrap();
        assert_eq!(task["status"], "workspace_bound");
        assert_eq!(task["workspace_binding"]["workspace_mode"], "local_folder");
        let staged = task["workspace_binding"]["workspace_canonical_path"]
            .as_str()
            .unwrap();
        assert_eq!(
            std::fs::read(Path::new(staged).join("README.md")).unwrap(),
            b"original"
        );
        assert_eq!(
            std::fs::read(source.join("README.md")).unwrap(),
            b"original"
        );
        let public = public_product_task_projection(&task);
        let encoded = public.to_string();
        assert!(!encoded.contains(source.to_string_lossy().as_ref()));
        assert!(public["target_repo_path_fingerprint"].is_string());
        assert!(public["workspace_binding"]["workspace_path_fingerprint"].is_string());
        assert!(public["workspace_binding"]["target_repo_canonical_path_fingerprint"].is_string());
    }

    #[test]
    fn public_result_projection_removes_nested_private_paths() {
        let private_path = "/operator/private/source";
        let private_remote = "https://git.example.invalid/operator/private-repository";
        let result = json!({
            "task": {"target_repo_path": private_path, "git_origin_remote": private_remote},
            "approval": {"output_target": {"target_repo_path": private_path}},
            "output": {"rollback_root": private_path, "export_path": private_path},
        });
        let public = public_product_task_result_projection(&result);
        assert!(!public.to_string().contains(private_path));
        assert!(!public.to_string().contains(private_remote));
        assert!(public
            .pointer("/task/target_repo_path_fingerprint")
            .is_some());
        assert!(public
            .pointer("/task/git_origin_remote_fingerprint")
            .is_some());
        assert!(public
            .pointer("/approval/output_target/target_repo_path_fingerprint")
            .is_some());
        assert!(public
            .pointer("/output/rollback_root_fingerprint")
            .is_some());
        assert!(public.pointer("/output/export_path_fingerprint").is_some());
    }

    fn prepare_local_folder_apply_task(
        root: &tempfile::TempDir,
        idempotency_key: &str,
    ) -> (
        std::path::PathBuf,
        LocalProductStore,
        String,
        u64,
        String,
        String,
    ) {
        let source = root.path().join("operator-folder");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("README.md"), b"original").unwrap();
        let source = std::fs::canonicalize(&source).unwrap();
        let request = ProductTaskIntakeRequest {
            objective: "Create the approved bounded local document".to_string(),
            target_id: "local-folder-apply-fixture".to_string(),
            target_repo_path: source.to_string_lossy().into_owned(),
            source_kind: Some("local_folder".to_string()),
            source_revision: "operator-supplied-placeholder".to_string(),
            source_tree_hash: None,
            allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
            verification_commands: vec![ProductVerificationCommand {
                command: "test -f docs/product_golden_path_fixture.md".to_string(),
                timeout_ms: 1_000,
            }],
            output_intent: "apply_local_changes".to_string(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["command".to_string()],
                prefer: Some("command".to_string()),
            },
            budget: None,
            risk_class: "low".to_string(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(false),
            idempotency_key: idempotency_key.to_string(),
            expected_version: None,
            tenant_id: Some("tenant-local".to_string()),
            workspace_id: Some("workspace-local".to_string()),
            workspace_mode: Some("local_folder".to_string()),
        };
        let intake = validate_intake(&request, "tenant-local", "workspace-local").unwrap();
        let store = LocalProductStore::new(root.path().join("store.db")).unwrap();
        let task = store.admit_product_task(&intake, "operator").unwrap();
        let task_id = task["task_id"].as_str().unwrap().to_string();
        let compiled = store
            .compile_and_schedule_product_task(&task_id, "operator", &["command".to_string()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        let executor = crate::node_executor::CommandNodeExecutor::default();
        for _ in 0..8 {
            let tick = store
                .tick_with_executor(run_id, "scheduler", 1, &executor)
                .unwrap();
            if tick.pointer("/run/status").and_then(Value::as_str) == Some("completed") {
                break;
            }
            assert_ne!(
                tick.pointer("/run/status").and_then(Value::as_str),
                Some("failed")
            );
        }
        assert_eq!(
            store.get_workflow_run(run_id).unwrap().unwrap()["status"],
            "completed"
        );
        let finalized = store
            .finalize_product_task_after_execution(&task_id, "scheduler")
            .expect("local folder finalization must succeed");
        let version = finalized["task"]["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(&task_id, "independent-operator", version)
            .expect("local folder approval must succeed");
        let approval_id = approval["approval_id"].as_str().unwrap().to_string();
        let artifact_id = approval["artifact_id"].as_str().unwrap().to_string();
        (source, store, task_id, version, approval_id, artifact_id)
    }

    #[test]
    fn local_folder_confirmed_output_applies_once_through_existing_receipt_owner() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("operator-folder");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("README.md"), b"original").unwrap();
        let _gate_guard = ProductGateGuard::enable();
        let source = std::fs::canonicalize(&source).unwrap();
        let request = ProductTaskIntakeRequest {
            objective: "Create the approved bounded local document".to_string(),
            target_id: "local-folder-apply-fixture".to_string(),
            target_repo_path: source.to_string_lossy().into_owned(),
            source_kind: Some("local_folder".to_string()),
            source_revision: "operator-supplied-placeholder".to_string(),
            source_tree_hash: None,
            allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
            verification_commands: vec![ProductVerificationCommand {
                command: "test -f docs/product_golden_path_fixture.md".to_string(),
                timeout_ms: 1_000,
            }],
            output_intent: "apply_local_changes".to_string(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["command".to_string()],
                prefer: Some("command".to_string()),
            },
            budget: None,
            risk_class: "low".to_string(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(false),
            idempotency_key: "local-folder-apply".to_string(),
            expected_version: None,
            tenant_id: Some("tenant-local".to_string()),
            workspace_id: Some("workspace-local".to_string()),
            workspace_mode: Some("local_folder".to_string()),
        };
        let intake = validate_intake(&request, "tenant-local", "workspace-local").unwrap();
        let store = LocalProductStore::new(root.path().join("store.db")).unwrap();
        let task = store.admit_product_task(&intake, "operator").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "operator", &["command".to_string()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        let executor = crate::node_executor::CommandNodeExecutor::default();
        for _ in 0..8 {
            let tick = store
                .tick_with_executor(run_id, "scheduler", 1, &executor)
                .unwrap();
            if tick.pointer("/run/status").and_then(Value::as_str) == Some("completed") {
                break;
            }
            assert_ne!(
                tick.pointer("/run/status").and_then(Value::as_str),
                Some("failed")
            );
        }
        assert_eq!(
            store.get_workflow_run(run_id).unwrap().unwrap()["status"],
            "completed"
        );
        let finalized = store
            .finalize_product_task_after_execution(task_id, "scheduler")
            .expect("local folder finalization must succeed");
        let version = finalized["task"]["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", version)
            .expect("local folder approval must succeed");
        let completed = store
            .output_product_task(
                task_id,
                "output-operator",
                version,
                approval["approval_id"].as_str(),
                true,
            )
            .unwrap();
        assert_eq!(completed["task"]["status"], "completed");
        assert_eq!(
            std::fs::read_to_string(source.join("docs/product_golden_path_fixture.md")).unwrap(),
            FIXTURE_DETERMINISTIC_NOTE_CONTENT
        );
        let repeated = store
            .output_product_task(
                task_id,
                "output-operator",
                completed["task"]["version"].as_u64().unwrap(),
                approval["approval_id"].as_str(),
                true,
            )
            .unwrap();
        assert_eq!(repeated["reused"], true);
        let rolled_back = store
            .rollback_product_task_local_folder_apply(task_id, "rollback-operator")
            .unwrap();
        assert_eq!(rolled_back["rollback"]["state"], "completed");
        assert!(!source.join("docs/product_golden_path_fixture.md").exists());
        let rollback_repeated = store
            .rollback_product_task_local_folder_apply(task_id, "rollback-operator")
            .unwrap();
        assert_eq!(rollback_repeated["reused"], true);
    }

    #[test]
    fn local_folder_apply_claim_survives_restart_and_refuses_retry() {
        let root = tempfile::tempdir().unwrap();
        let _gate_guard = ProductGateGuard::enable();
        let (source, store, task_id, version, approval_id, artifact_id) =
            prepare_local_folder_apply_task(&root, "local-folder-apply-restart");
        let claim = store
            .claim_product_local_apply_effect(
                &artifact_id,
                &task_id,
                &approval_id,
                version,
                "output-operator",
            )
            .unwrap();
        assert_eq!(claim["state"], "effect_started");
        assert_eq!(claim["claim_action"], "apply_local_changes");
        drop(store);

        let restarted = LocalProductStore::new(root.path().join("store.db")).unwrap();
        let error = restarted
            .output_product_task(
                &task_id,
                "output-operator",
                version,
                Some(&approval_id),
                true,
            )
            .unwrap_err();
        assert!(error.contains("prior effect claim"));
        assert_eq!(
            restarted.get_product_task(&task_id).unwrap().unwrap()["status"],
            "awaiting_approval"
        );
        assert!(!source.join("docs/product_golden_path_fixture.md").exists());
    }

    #[test]
    fn local_folder_export_emits_a_redacted_change_bundle_without_mutating_source() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("operator-folder");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("README.md"), b"original").unwrap();
        let _gate_guard = ProductGateGuard::enable();
        let source = std::fs::canonicalize(&source).unwrap();
        let request = ProductTaskIntakeRequest {
            objective: "Create the approved bounded local document".to_string(),
            target_id: "local-folder-export-fixture".to_string(),
            target_repo_path: source.to_string_lossy().into_owned(),
            source_kind: Some("local_folder".to_string()),
            source_revision: "operator-supplied-placeholder".to_string(),
            source_tree_hash: None,
            allowed_paths: vec!["docs/product_golden_path_fixture.md".to_string()],
            verification_commands: vec![ProductVerificationCommand {
                command: "test -f docs/product_golden_path_fixture.md".to_string(),
                timeout_ms: 1_000,
            }],
            output_intent: "export_patch".to_string(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["command".to_string()],
                prefer: Some("command".to_string()),
            },
            budget: None,
            risk_class: "low".to_string(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(false),
            idempotency_key: "local-folder-export".to_string(),
            expected_version: None,
            tenant_id: Some("tenant-local".to_string()),
            workspace_id: Some("workspace-local".to_string()),
            workspace_mode: Some("local_folder".to_string()),
        };
        let intake = validate_intake(&request, "tenant-local", "workspace-local").unwrap();
        let store = LocalProductStore::new(root.path().join("store.db")).unwrap();
        let task = store.admit_product_task(&intake, "operator").unwrap();
        let task_id = task["task_id"].as_str().unwrap();
        let compiled = store
            .compile_and_schedule_product_task(task_id, "operator", &["command".to_string()])
            .unwrap();
        let run_id = compiled["task"]["run_id"].as_str().unwrap();
        let executor = crate::node_executor::CommandNodeExecutor::default();
        for _ in 0..8 {
            let tick = store
                .tick_with_executor(run_id, "scheduler", 1, &executor)
                .unwrap();
            if tick.pointer("/run/status").and_then(Value::as_str) == Some("completed") {
                break;
            }
            assert_ne!(
                tick.pointer("/run/status").and_then(Value::as_str),
                Some("failed")
            );
        }
        let finalized = store
            .finalize_product_task_after_execution(task_id, "scheduler")
            .expect("local folder finalization must succeed");
        let version = finalized["task"]["version"].as_u64().unwrap();
        let approval = store
            .approve_product_task(task_id, "independent-operator", version)
            .expect("local folder approval must succeed");
        let completed = store
            .output_product_task(
                task_id,
                "output-operator",
                version,
                approval["approval_id"].as_str(),
                true,
            )
            .unwrap();
        assert_eq!(completed["task"]["status"], "completed");
        assert_eq!(completed["output"]["status"], "exported");
        assert!(completed["output"]["change_bundle_sha256"].is_string());
        assert!(completed["output"].get("export_path").is_none());
        assert!(!source.join("docs/product_golden_path_fixture.md").exists());
    }

    #[test]
    fn local_folder_compensation_removes_only_the_receipt_owned_staging_copy() {
        let root = tempfile::tempdir().unwrap();
        let workspace_root = std::fs::canonicalize(root.path()).unwrap();
        let task_id = "ptask-local-cleanup";
        let workspace_path = workspace_root.join(product_task_workspace_fs_id(task_id));
        std::fs::create_dir(&workspace_path).unwrap();
        std::fs::write(workspace_path.join("staged.txt"), b"staged").unwrap();
        let unrelated = workspace_root.join("unrelated");
        std::fs::create_dir(&unrelated).unwrap();

        let receipt =
            ProductTaskWorkspacePreparationReceipt::planned(task_id, workspace_root.clone())
                .unwrap();
        assert_eq!(receipt.workspace_path, workspace_path);
        remove_product_task_local_folder_stage_or_reconcile(&receipt, task_id).unwrap();
        assert!(!workspace_path.exists());
        assert!(unrelated.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn local_folder_compensation_rejects_a_replaced_staging_symlink() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let workspace_root = std::fs::canonicalize(root.path()).unwrap();
        let task_id = "ptask-local-symlink";
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("must-remain"), b"safe").unwrap();
        let receipt =
            ProductTaskWorkspacePreparationReceipt::planned(task_id, workspace_root).unwrap();
        symlink(outside.path(), &receipt.workspace_path).unwrap();

        let error =
            remove_product_task_local_folder_stage_or_reconcile(&receipt, task_id).unwrap_err();
        assert!(error.starts_with(PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED));
        assert!(receipt.workspace_path.is_symlink());
        assert_eq!(
            std::fs::read(outside.path().join("must-remain")).unwrap(),
            b"safe"
        );
    }
}

#[cfg(all(test, unix))]
mod product_task_workspace_preparation_marker_tests {
    use super::*;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    fn marker_ready_receipt(
        task_id: &str,
        workspace_root: PathBuf,
    ) -> ProductTaskWorkspacePreparationReceipt {
        let workspace_path = workspace_root.join(product_task_workspace_fs_id(task_id));
        let marker_sha256 = "a".repeat(64);
        let marker_state = ProductTaskWorkspacePreparationMarkerState::MarkerReady;
        let receipt_sha256 = product_task_workspace_preparation_receipt_sha256(
            task_id,
            &workspace_root,
            &workspace_path,
            &marker_sha256,
            marker_state,
        )
        .unwrap();
        ProductTaskWorkspacePreparationReceipt {
            workspace_root,
            workspace_path,
            marker_sha256,
            marker_state,
            receipt_sha256,
        }
    }

    #[test]
    fn marker_reader_rejects_a_fifo_without_blocking_or_reading_it() {
        let root = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();
        let task_id = "marker-fifo";
        let receipt = marker_ready_receipt(task_id, root);
        let marker_path = receipt.marker_path(task_id).unwrap();
        let marker_path = CString::new(marker_path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(marker_path.as_ptr(), 0o600) }, 0);

        let error = validate_product_task_workspace_preparation_marker(task_id, &receipt, false)
            .expect_err("a FIFO marker must be reconciliation, never a blocking read");
        assert!(error.starts_with(PRODUCT_TASK_WORKSPACE_PREPARATION_RECONCILIATION_REQUIRED));
        assert!(error.contains("not a regular file"));
    }
}
