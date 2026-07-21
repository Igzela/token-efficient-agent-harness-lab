//! PE7 Product Golden Path — canonical user-task identity and intake contracts.
//!
//! G1 owns the versioned root task record, intake validation, and worktree-first
//! binding protocol. Executable graph compilation and scheduler eligibility are
//! later slices; this module must not admit leased execution.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

pub const PRODUCT_TASK_SCHEMA_VERSION: &str = "product_task.v1";
pub const PRODUCT_TASK_INTAKE_SCHEMA_VERSION: &str = "product_task_intake.v1";
pub const PRODUCT_TASK_WORKSPACE_BINDING_SCHEMA_VERSION: &str = "product_task_workspace_binding.v1";
pub const PRODUCT_EXECUTABLE_GRAPH_SCHEMA_VERSION: &str = "product_executable_graph.v1";
pub const PRODUCT_TASK_GATE: &str = "ACP_PRODUCT_GOLDEN_PATH";

/// Executor identifiers that may be admitted by intake policy.
/// Registration in the live executor pool is checked separately at compile time.
pub const ADMITTED_EXECUTOR_IDENTIFIERS: &[&str] = &[
    "command",
    "agent_step",
    "local_runner_validation",
    "claude_code_cli",
    "codex_cli",
    "opencode",
    // Alias retained for readability; resolved to `command` at compile time.
    "deterministic",
];

pub const MAX_OBJECTIVE_BYTES: usize = 8_192;
pub const MAX_PATH_BYTES: usize = 1_024;
pub const MAX_ALLOWED_PATHS: usize = 64;
pub const MAX_VERIFICATION_COMMANDS: usize = 8;
pub const MAX_VERIFICATION_COMMAND_BYTES: usize = 512;
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;
pub const MAX_TARGET_ID_BYTES: usize = 128;
pub const MAX_EXECUTOR_SET: usize = 16;
pub const MAX_RISK_CLASS_BYTES: usize = 64;

/// Canonical product-task lifecycle for G1+.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductTaskStatus {
    /// Intake reserved under idempotency; worktree not yet prepared.
    Admitted,
    /// Controlled worktree creation in progress (restart-safe two-phase).
    WorkspacePreparing,
    /// Workspace verified and bound; no executable run admitted yet (G1 terminal success).
    WorkspaceBound,
    /// G2+: executable graph compiled and run eligible for existing scheduler.
    GraphReady,
    /// G2+: scheduler advancing nodes.
    Running,
    /// G3+: waiting for current approval binding.
    AwaitingApproval,
    /// G3+: approved output pending or in progress.
    OutputPending,
    Completed,
    Failed,
    Killed,
    Paused,
    BudgetExhausted,
    Blocked,
}

impl ProductTaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::WorkspacePreparing => "workspace_preparing",
            Self::WorkspaceBound => "workspace_bound",
            Self::GraphReady => "graph_ready",
            Self::Running => "running",
            Self::AwaitingApproval => "awaiting_approval",
            Self::OutputPending => "output_pending",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Killed => "killed",
            Self::Paused => "paused",
            Self::BudgetExhausted => "budget_exhausted",
            Self::Blocked => "blocked",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "admitted" => Ok(Self::Admitted),
            "workspace_preparing" => Ok(Self::WorkspacePreparing),
            "workspace_bound" => Ok(Self::WorkspaceBound),
            "graph_ready" => Ok(Self::GraphReady),
            "running" => Ok(Self::Running),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "output_pending" => Ok(Self::OutputPending),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "killed" => Ok(Self::Killed),
            "paused" => Ok(Self::Paused),
            "budget_exhausted" => Ok(Self::BudgetExhausted),
            "blocked" => Ok(Self::Blocked),
            other => Err(format!("invalid product task status: {other}")),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Killed | Self::BudgetExhausted | Self::Blocked
        )
    }

    /// G1 never marks a task scheduler-eligible.
    pub fn admits_execution(self) -> bool {
        matches!(self, Self::GraphReady | Self::Running | Self::OutputPending)
    }
}

pub fn is_valid_product_task_transition(from: ProductTaskStatus, to: ProductTaskStatus) -> bool {
    use ProductTaskStatus::*;
    matches!(
        (from, to),
        (Admitted, WorkspacePreparing | Failed | Killed | Blocked)
            | (
                WorkspacePreparing,
                WorkspaceBound | Failed | Killed | Blocked | Admitted
            )
            | (
                WorkspaceBound,
                GraphReady | Failed | Killed | Blocked | Paused
            )
            | (GraphReady, Running | Failed | Killed | Blocked | Paused)
            | (
                Running,
                AwaitingApproval
                    | OutputPending
                    | Completed
                    | Failed
                    | Killed
                    | Paused
                    | BudgetExhausted
                    | Blocked
            )
            | (
                AwaitingApproval,
                OutputPending | Completed | Failed | Killed | Paused | Blocked
            )
            | (
                OutputPending,
                Completed | Failed | Killed | Paused | Blocked | AwaitingApproval
            )
            | (
                Paused,
                Running | GraphReady | WorkspaceBound | Killed | Failed | Blocked
            )
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductOutputIntent {
    ArtifactOnly,
    ExportPatch,
    DraftPr,
}

impl ProductOutputIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ArtifactOnly => "artifact_only",
            Self::ExportPatch => "export_patch",
            Self::DraftPr => "draft_pr",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "artifact_only" => Ok(Self::ArtifactOnly),
            "export_patch" => Ok(Self::ExportPatch),
            "draft_pr" => Ok(Self::DraftPr),
            other => Err(format!(
                "output_intent must be artifact_only, export_patch, or draft_pr; got {other}"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductTaskBudget {
    pub total_tokens: Option<u64>,
    pub total_calls: Option<u64>,
    pub total_elapsed_ms: Option<u64>,
    pub max_retries: Option<u64>,
    pub max_repairs: Option<u64>,
    pub max_concurrency: Option<u64>,
    pub stage_budgets: Option<Value>,
}

impl Default for ProductTaskBudget {
    fn default() -> Self {
        Self {
            total_tokens: Some(50_000),
            total_calls: Some(32),
            total_elapsed_ms: Some(600_000),
            max_retries: Some(2),
            max_repairs: Some(2),
            max_concurrency: Some(1),
            stage_budgets: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductExecutorPolicy {
    /// Explicit admitted executor names (e.g. `deterministic`, `cli`). Never an arbitrary binary path.
    pub allowed_executors: Vec<String>,
    pub prefer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductVerificationCommand {
    pub command: String,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductTaskIntakeRequest {
    pub objective: String,
    pub target_id: String,
    pub target_repo_path: String,
    pub source_revision: String,
    pub source_tree_hash: Option<String>,
    pub allowed_paths: Vec<String>,
    pub verification_commands: Vec<ProductVerificationCommand>,
    pub output_intent: String,
    pub executor_policy: ProductExecutorPolicy,
    pub budget: Option<ProductTaskBudget>,
    pub risk_class: String,
    pub approval_required: bool,
    pub confirm_execution: Option<bool>,
    pub confirm_output: Option<bool>,
    pub idempotency_key: String,
    pub expected_version: Option<u64>,
    pub tenant_id: Option<String>,
    pub workspace_id: Option<String>,
    /// Operator-supplied workspace mode; golden path requires controlled git worktree.
    pub workspace_mode: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidatedProductTaskIntake {
    pub schema_version: String,
    pub objective: String,
    pub objective_fingerprint: String,
    pub target_id: String,
    pub target_repo_path: String,
    pub source_revision: String,
    pub source_tree_hash: Option<String>,
    pub allowed_paths: Vec<String>,
    pub verification_commands: Vec<ProductVerificationCommand>,
    pub output_intent: ProductOutputIntent,
    pub executor_policy: ProductExecutorPolicy,
    pub budget: ProductTaskBudget,
    pub risk_class: String,
    pub approval_required: bool,
    pub confirm_execution: bool,
    pub confirm_output: bool,
    pub idempotency_key: String,
    pub expected_version: Option<u64>,
    pub tenant_id: String,
    pub workspace_id: String,
    pub workspace_mode: String,
    pub intake_contract_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductWorkspaceBinding {
    pub schema_version: String,
    pub workspace_id: String,
    pub workspace_path: String,
    pub workspace_canonical_path: String,
    pub target_repo_canonical_path: String,
    pub source_revision: String,
    pub source_tree_hash: Option<String>,
    pub workspace_content_hash: String,
    pub workspace_mode: String,
    pub provisional_run_id: String,
    pub allowed_paths: Vec<String>,
    pub bound_at: String,
}

pub fn product_gate_enabled() -> bool {
    match std::env::var(PRODUCT_TASK_GATE) {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

pub fn fingerprint_objective(objective: &str) -> String {
    hex::encode(Sha256::digest(objective.as_bytes()))
}

pub fn provisional_run_id_for_task(task_id: &str) -> String {
    format!("product-task:{task_id}")
}

pub fn validate_intake(
    request: &ProductTaskIntakeRequest,
    auth_tenant_id: &str,
    default_workspace_id: &str,
) -> Result<ValidatedProductTaskIntake, String> {
    if !product_gate_enabled() {
        return Err(format!(
            "product golden path intake is disabled; set {PRODUCT_TASK_GATE}=1 to enable"
        ));
    }

    let objective = request.objective.trim();
    if objective.is_empty() || objective.len() > MAX_OBJECTIVE_BYTES {
        return Err(format!(
            "objective must be 1..{MAX_OBJECTIVE_BYTES} bytes after trim"
        ));
    }
    // Do not persist raw secrets from objective; reject obvious patterns.
    reject_sensitive_literal(objective, "objective")?;

    let target_id = request.target_id.trim();
    if target_id.is_empty() || target_id.len() > MAX_TARGET_ID_BYTES {
        return Err(format!(
            "target_id must be 1..{MAX_TARGET_ID_BYTES} bytes after trim"
        ));
    }
    if !target_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("target_id contains forbidden characters".to_string());
    }

    let target_repo_path = request.target_repo_path.trim();
    if target_repo_path.is_empty() || target_repo_path.len() > MAX_PATH_BYTES {
        return Err(format!(
            "target_repo_path must be 1..{MAX_PATH_BYTES} bytes after trim"
        ));
    }
    let target_path = Path::new(target_repo_path);
    if !target_path.is_absolute() {
        return Err("target_repo_path must be an absolute path".to_string());
    }
    if target_path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err("target_repo_path must not contain '..'".to_string());
    }

    let source_revision = request.source_revision.trim();
    if source_revision.is_empty() || source_revision.len() > 128 {
        return Err("source_revision must be 1..128 bytes".to_string());
    }
    if source_revision.chars().any(|c| c.is_whitespace()) {
        return Err("source_revision must not contain whitespace".to_string());
    }

    if let Some(hash) = request.source_tree_hash.as_deref() {
        let hash = hash.trim();
        if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("source_tree_hash must be 64 hex characters when provided".to_string());
        }
    }

    if request.allowed_paths.is_empty() || request.allowed_paths.len() > MAX_ALLOWED_PATHS {
        return Err(format!(
            "allowed_paths must contain 1..{MAX_ALLOWED_PATHS} entries"
        ));
    }
    let mut allowed_paths = Vec::with_capacity(request.allowed_paths.len());
    for path in &request.allowed_paths {
        let normalized = validate_allowed_path(path)?;
        allowed_paths.push(normalized);
    }

    if request.verification_commands.is_empty()
        || request.verification_commands.len() > MAX_VERIFICATION_COMMANDS
    {
        return Err(format!(
            "verification_commands must contain 1..{MAX_VERIFICATION_COMMANDS} entries"
        ));
    }
    let mut verification_commands = Vec::with_capacity(request.verification_commands.len());
    for cmd in &request.verification_commands {
        let command = cmd.command.trim();
        if command.is_empty() || command.len() > MAX_VERIFICATION_COMMAND_BYTES {
            return Err(format!(
                "verification command must be 1..{MAX_VERIFICATION_COMMAND_BYTES} bytes"
            ));
        }
        // Reject absolute executable paths and shell metacharacters that would admit arbitrary binaries.
        if command.starts_with('/') || command.starts_with('.') {
            return Err(
                "verification command must not be an absolute or relative binary path".to_string(),
            );
        }
        if command.contains("..")
            || command.contains(';')
            || command.contains('|')
            || command.contains('&')
            || command.contains('`')
            || command.contains('$')
            || command.contains('\n')
        {
            return Err("verification command contains forbidden shell metacharacters".to_string());
        }
        if cmd.timeout_ms == 0 || cmd.timeout_ms > 3_600_000 {
            return Err("verification command timeout_ms must be 1..3600000".to_string());
        }
        verification_commands.push(ProductVerificationCommand {
            command: command.to_string(),
            timeout_ms: cmd.timeout_ms,
        });
    }

    let output_intent = ProductOutputIntent::parse(request.output_intent.trim())?;
    let executor_policy = validate_executor_policy(&request.executor_policy)?;
    let budget = request.budget.clone().unwrap_or_default();
    validate_budget(&budget)?;

    let risk_class = request.risk_class.trim();
    if risk_class.is_empty() || risk_class.len() > MAX_RISK_CLASS_BYTES {
        return Err(format!(
            "risk_class must be 1..{MAX_RISK_CLASS_BYTES} bytes"
        ));
    }

    let idempotency_key = request.idempotency_key.trim();
    if idempotency_key.is_empty() || idempotency_key.len() > MAX_IDEMPOTENCY_KEY_BYTES {
        return Err(format!(
            "idempotency_key must be 1..{MAX_IDEMPOTENCY_KEY_BYTES} bytes"
        ));
    }
    if !idempotency_key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == ':' || c == '.')
    {
        return Err("idempotency_key contains forbidden characters".to_string());
    }

    let confirm_execution = request.confirm_execution == Some(true);
    if !confirm_execution {
        return Err("confirm_execution=true is required for product task intake".to_string());
    }
    let confirm_output = match output_intent {
        ProductOutputIntent::ArtifactOnly => request.confirm_output.unwrap_or(false),
        ProductOutputIntent::ExportPatch | ProductOutputIntent::DraftPr => {
            if request.confirm_output != Some(true) {
                return Err(
                    "confirm_output=true is required for export_patch and draft_pr intents"
                        .to_string(),
                );
            }
            true
        }
    };

    let tenant_id = request
        .tenant_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(auth_tenant_id)
        .to_string();
    if tenant_id != auth_tenant_id {
        return Err("tenant_id must match authenticated tenant scope".to_string());
    }
    let workspace_id = request
        .workspace_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(default_workspace_id)
        .to_string();
    if workspace_id.is_empty() || workspace_id.len() > 128 {
        return Err("workspace_id must be 1..128 bytes".to_string());
    }

    let workspace_mode = request
        .workspace_mode
        .as_deref()
        .unwrap_or("git_worktree")
        .trim()
        .to_string();
    if workspace_mode != "git_worktree" {
        return Err("product golden path requires workspace_mode=git_worktree".to_string());
    }

    let objective_fingerprint = fingerprint_objective(objective);
    let validated = ValidatedProductTaskIntake {
        schema_version: PRODUCT_TASK_INTAKE_SCHEMA_VERSION.to_string(),
        objective: objective.to_string(),
        objective_fingerprint,
        target_id: target_id.to_string(),
        target_repo_path: target_repo_path.to_string(),
        source_revision: source_revision.to_string(),
        source_tree_hash: request
            .source_tree_hash
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string),
        allowed_paths,
        verification_commands,
        output_intent,
        executor_policy,
        budget,
        risk_class: risk_class.to_string(),
        approval_required: request.approval_required,
        confirm_execution,
        confirm_output,
        idempotency_key: idempotency_key.to_string(),
        expected_version: request.expected_version,
        tenant_id,
        workspace_id,
        workspace_mode,
        intake_contract_sha256: String::new(),
    };
    let mut with_hash = validated;
    with_hash.intake_contract_sha256 = intake_contract_sha256(&with_hash);
    Ok(with_hash)
}

pub fn intake_contract_sha256(intake: &ValidatedProductTaskIntake) -> String {
    // Hash durable contract fields excluding raw objective body (fingerprint only).
    let payload = json!({
        "schema_version": intake.schema_version,
        "objective_fingerprint": intake.objective_fingerprint,
        "target_id": intake.target_id,
        "target_repo_path": intake.target_repo_path,
        "source_revision": intake.source_revision,
        "source_tree_hash": intake.source_tree_hash,
        "allowed_paths": intake.allowed_paths,
        "verification_commands": intake.verification_commands,
        "output_intent": intake.output_intent.as_str(),
        "executor_policy": intake.executor_policy,
        "budget": intake.budget,
        "risk_class": intake.risk_class,
        "approval_required": intake.approval_required,
        "confirm_execution": intake.confirm_execution,
        "confirm_output": intake.confirm_output,
        "idempotency_key": intake.idempotency_key,
        "tenant_id": intake.tenant_id,
        "workspace_id": intake.workspace_id,
        "workspace_mode": intake.workspace_mode,
    });
    hex::encode(Sha256::digest(payload.to_string().as_bytes()))
}

pub fn redacted_intake_json(intake: &ValidatedProductTaskIntake) -> Value {
    json!({
        "schema_version": intake.schema_version,
        "objective_fingerprint": intake.objective_fingerprint,
        // Bounded objective reference only — operators may need short text for review.
        // Cap at 256 chars; full body is not a durable evidence corpus.
        "objective_preview": truncate_preview(&intake.objective, 256),
        "target_id": intake.target_id,
        "target_repo_path": intake.target_repo_path,
        "source_revision": intake.source_revision,
        "source_tree_hash": intake.source_tree_hash,
        "allowed_paths": intake.allowed_paths,
        "verification_commands": intake.verification_commands,
        "output_intent": intake.output_intent.as_str(),
        "executor_policy": intake.executor_policy,
        "budget": intake.budget,
        "risk_class": intake.risk_class,
        "approval_required": intake.approval_required,
        "confirm_execution": intake.confirm_execution,
        "confirm_output": intake.confirm_output,
        "idempotency_key": intake.idempotency_key,
        "expected_version": intake.expected_version,
        "tenant_id": intake.tenant_id,
        "workspace_id": intake.workspace_id,
        "workspace_mode": intake.workspace_mode,
        "intake_contract_sha256": intake.intake_contract_sha256,
    })
}

fn truncate_preview(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}…", &value[..max])
    }
}

fn validate_allowed_path(path: &str) -> Result<String, String> {
    let path = path.trim();
    if path.is_empty() || path.len() > MAX_PATH_BYTES {
        return Err(format!(
            "allowed path must be 1..{MAX_PATH_BYTES} bytes after trim"
        ));
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Err("allowed_paths must be repository-relative".to_string());
    }
    let p = Path::new(path);
    for component in p.components() {
        match component {
            Component::Normal(part) => {
                let s = part.to_string_lossy();
                if s == ".." || s.contains('\0') {
                    return Err("allowed_paths must not escape the repository".to_string());
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err("allowed_paths must not contain '..'".to_string());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err("allowed_paths must be repository-relative".to_string());
            }
        }
    }
    Ok(path.replace('\\', "/"))
}

fn validate_executor_policy(
    policy: &ProductExecutorPolicy,
) -> Result<ProductExecutorPolicy, String> {
    if policy.allowed_executors.is_empty() || policy.allowed_executors.len() > MAX_EXECUTOR_SET {
        return Err(format!(
            "executor_policy.allowed_executors must contain 1..{MAX_EXECUTOR_SET} entries"
        ));
    }
    let mut allowed = Vec::with_capacity(policy.allowed_executors.len());
    for name in &policy.allowed_executors {
        let name = name.trim();
        if name.is_empty() || name.len() > 64 {
            return Err("executor name must be 1..64 bytes".to_string());
        }
        if name.contains('/') || name.contains('\\') || name.contains('.') && name.contains('/') {
            return Err("executor_policy must not admit arbitrary binary paths".to_string());
        }
        if name.starts_with('/') || name.contains("..") {
            return Err("executor_policy must not admit arbitrary binary paths".to_string());
        }
        // Fail closed on non-identifier names that look like paths.
        if !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(
                "executor names must be admitted identifiers, not paths or binaries".to_string(),
            );
        }
        if name == "noop" || name == "stub" || name == "fail" {
            return Err(
                "executor_policy must not admit noop/stub/fail as product coding executors"
                    .to_string(),
            );
        }
        if !ADMITTED_EXECUTOR_IDENTIFIERS.contains(&name) {
            return Err(format!(
                "executor '{name}' is not in the product-admitted identifier set"
            ));
        }
        allowed.push(name.to_string());
    }
    if let Some(prefer) = policy.prefer.as_deref() {
        let prefer = prefer.trim();
        if !allowed.iter().any(|e| e == prefer) {
            return Err("executor_policy.prefer must be in allowed_executors".to_string());
        }
    }
    Ok(ProductExecutorPolicy {
        allowed_executors: allowed,
        prefer: policy
            .prefer
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string),
    })
}

fn validate_budget(budget: &ProductTaskBudget) -> Result<(), String> {
    if let Some(v) = budget.total_tokens {
        if v == 0 {
            return Err("budget.total_tokens must be > 0 when set".to_string());
        }
    }
    if let Some(v) = budget.total_calls {
        if v == 0 {
            return Err("budget.total_calls must be > 0 when set".to_string());
        }
    }
    if let Some(v) = budget.total_elapsed_ms {
        if v == 0 {
            return Err("budget.total_elapsed_ms must be > 0 when set".to_string());
        }
    }
    if let Some(v) = budget.max_concurrency {
        if v == 0 || v > 16 {
            return Err("budget.max_concurrency must be 1..16 when set".to_string());
        }
    }
    Ok(())
}

fn reject_sensitive_literal(value: &str, field: &str) -> Result<(), String> {
    let lower = value.to_ascii_lowercase();
    for pattern in [
        "-----begin ",
        "api_key=",
        "apikey=",
        "secret=",
        "password=",
        "authorization: bearer ",
        "x-api-key",
    ] {
        if lower.contains(pattern) {
            return Err(format!(
                "{field} appears to contain a sensitive literal and is rejected"
            ));
        }
    }
    Ok(())
}

/// Compute a stable content hash for a prepared workspace directory.
///
/// Reuses the supervised-patch workspace file collector so hashing bounds and
/// ignore rules stay consistent with the workspace owner.
pub fn workspace_content_hash(workspace_path: &Path) -> Result<String, String> {
    if !workspace_path.is_absolute() {
        return Err("workspace_path must be absolute".to_string());
    }
    // Symlink containment: refuse if any path component under the workspace is a symlink.
    reject_workspace_symlinks(workspace_path)?;
    let manifest =
        crate::storage::local_product_store::supervised_patch_compute_manifest(workspace_path)?;
    let payload = serde_json::to_string(&manifest).map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(payload.as_bytes())))
}

fn reject_workspace_symlinks(root: &Path) -> Result<(), String> {
    fn walk(dir: &Path, depth: usize) -> Result<(), String> {
        if depth > 64 {
            return Err("workspace directory depth exceeds bound".to_string());
        }
        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() {
                return Err("workspace contains symlink; path containment failed".to_string());
            }
            if meta.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name == ".git" || name == "target" || name == "node_modules" {
                    continue;
                }
                walk(&path, depth + 1)?;
            }
        }
        Ok(())
    }
    walk(root, 0)
}

pub fn validate_source_revision_format(source_revision: &str) -> Result<(), String> {
    let value = source_revision.trim();
    if value.is_empty() || value.len() > 128 {
        return Err("source_revision must be 1..128 bytes".to_string());
    }
    if value.starts_with('-') {
        return Err("source_revision must not look like a git option".to_string());
    }
    if value.chars().any(|c| c.is_whitespace() || c == '\0') {
        return Err("source_revision must not contain whitespace or NUL".to_string());
    }
    Ok(())
}

pub fn planned_workspace_path(
    store_db_path: &Path,
    workspace_fs_id: &str,
) -> Result<PathBuf, String> {
    let db_dir = store_db_path
        .parent()
        .ok_or_else(|| "store has no parent directory".to_string())?;
    Ok(db_dir.join("workspaces").join(workspace_fs_id))
}

/// Resolve intake executor policy to a concrete pool executor type.
pub fn resolve_admitted_executor(policy: &ProductExecutorPolicy) -> Result<String, String> {
    let preferred = policy
        .prefer
        .clone()
        .or_else(|| policy.allowed_executors.first().cloned())
        .ok_or_else(|| "executor_policy has no admitted executor".to_string())?;
    let resolved = match preferred.as_str() {
        "deterministic" => "command".to_string(),
        other => other.to_string(),
    };
    if matches!(resolved.as_str(), "noop" | "stub" | "fail") {
        return Err("product golden path refuses silent noop/stub/fail success".to_string());
    }
    if !policy.allowed_executors.iter().any(|e| {
        e == &preferred
            || (preferred == "deterministic" && (e == "deterministic" || e == "command"))
    }) {
        return Err("resolved executor is not in allowed_executors".to_string());
    }
    Ok(resolved)
}

/// Compile a versioned executable graph for a workspace-bound product task.
///
/// Does not create a second scheduler. Nodes carry exact task/workspace/source
/// bindings so lease-time injection cannot invent authority.
pub fn compile_product_executable_graph(
    task: &Value,
    created_at: &str,
    plan_ids: &crate::read_only_planner::WorkflowPlanIds,
    resolved_executor: &str,
) -> Result<Value, String> {
    let task_id = task
        .get("task_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "product task missing task_id".to_string())?;
    let status =
        ProductTaskStatus::parse(task.get("status").and_then(Value::as_str).unwrap_or(""))?;
    if status != ProductTaskStatus::WorkspaceBound && status != ProductTaskStatus::GraphReady {
        return Err(format!(
            "executable graph requires workspace_bound task; status={}",
            status.as_str()
        ));
    }
    let binding = task
        .get("workspace_binding")
        .cloned()
        .filter(|v| !v.is_null())
        .ok_or_else(|| "product task missing workspace_binding".to_string())?;
    let workspace_path = binding
        .get("workspace_path")
        .and_then(Value::as_str)
        .ok_or_else(|| "workspace_binding missing workspace_path".to_string())?;
    let workspace_id = binding
        .get("workspace_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "workspace_binding missing workspace_id".to_string())?;
    let source_revision = binding
        .get("source_revision")
        .and_then(Value::as_str)
        .ok_or_else(|| "workspace_binding missing source_revision".to_string())?;
    let allowed_paths = binding.get("allowed_paths").cloned().unwrap_or(json!([]));
    let intake = task.get("intake").cloned().unwrap_or(json!({}));
    let budget = intake.get("budget").cloned().unwrap_or(json!({}));
    let verification_commands = intake
        .get("verification_commands")
        .cloned()
        .unwrap_or(json!([]));

    let task_type = match resolved_executor {
        "command" => "command",
        "agent_step" => "agent_step",
        "local_runner_validation" => "local_runner_validation",
        "claude_code_cli" | "codex_cli" | "opencode" => "command",
        other => {
            return Err(format!(
                "unsupported product executor for graph compile: {other}"
            ))
        }
    };

    // Deterministic bounded mutation helper is written into the worktree at compile time
    // (see store). Command stays free of shell metacharacters and uses allowlisted python3.
    let command = if task_type == "command" {
        "python3 .product_golden_path_apply.py"
    } else {
        ""
    };

    let apply_node_id = format!("{}-apply", plan_ids.workflow_id);
    let binding_sha256 = hex::encode(Sha256::digest(
        format!("product_apply:{task_id}:{workspace_id}:{source_revision}").as_bytes(),
    ));
    let mut apply_node = json!({
        "schema_version": "workflow_node.v1",
        "node_id": apply_node_id,
        "workflow_id": plan_ids.workflow_id,
        "task_type": task_type,
        "assigned_agent_id": null,
        "status": "pending",
        "input_refs": [],
        "output_ref": null,
        "budget": budget.get("total_tokens").and_then(Value::as_u64).unwrap_or(50_000) as f64,
        "cost_incurred": 0.0,
        "error": null,
        "created_at": created_at,
        "started_at": null,
        "completed_at": null,
        "product_task_id": task_id,
        "tenant_id": task.get("tenant_id"),
        "workspace_scope_id": task.get("workspace_id"),
        "workspace_path": workspace_path,
        "workspace_root": workspace_path,
        "workspace_id": workspace_id,
        "source_revision": source_revision,
        "allowed_paths": allowed_paths,
        "suggested_executor": resolved_executor,
        "executor": resolved_executor,
        "objective_fingerprint": task.get("objective_fingerprint"),
        "intake_contract_sha256": task.get("intake_contract_sha256"),
        "verification_commands": verification_commands,
        "output_intent": task.get("output_intent"),
        "product_graph_schema_version": PRODUCT_EXECUTABLE_GRAPH_SCHEMA_VERSION,
        "managed_supervised_patch": {
            "schema_version": "managed_supervised_patch.v1",
            "workspace_id": workspace_id,
            "operation": "product_apply",
            "attempt": 1,
            "binding_sha256": binding_sha256,
            "content_excluded": true,
            "product_task_id": task_id,
        },
    });
    if task_type == "command" {
        apply_node
            .as_object_mut()
            .unwrap()
            .insert("command".to_string(), json!(command));
    }

    Ok(json!({
        "schema_version": "workflow_graph.v1",
        "workflow_id": plan_ids.workflow_id,
        "dispatch_id": plan_ids.dispatch_id,
        "nodes": [apply_node],
        "edges": [],
        "status": "executable",
        "created_at": created_at,
        "updated_at": created_at,
        "product_task_id": task_id,
        "product_graph_schema_version": PRODUCT_EXECUTABLE_GRAPH_SCHEMA_VERSION,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn sample_request() -> ProductTaskIntakeRequest {
        ProductTaskIntakeRequest {
            objective: "Add a README section describing the golden path.".to_string(),
            target_id: "demo-repo".to_string(),
            target_repo_path: "/tmp/demo-repo".to_string(),
            source_revision: "abc1234".to_string(),
            source_tree_hash: None,
            allowed_paths: vec!["README.md".to_string()],
            verification_commands: vec![ProductVerificationCommand {
                command: "test -f README.md".to_string(),
                timeout_ms: 5_000,
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
            confirm_output: None,
            idempotency_key: "idem-1".to_string(),
            expected_version: None,
            tenant_id: None,
            workspace_id: None,
            workspace_mode: Some("git_worktree".to_string()),
        }
    }

    #[test]
    fn rejects_when_gate_disabled() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var(PRODUCT_TASK_GATE);
        let err = validate_intake(&sample_request(), "local", "default").unwrap_err();
        assert!(err.contains("disabled"));
    }

    #[test]
    fn accepts_valid_intake_when_gate_enabled() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        let validated = validate_intake(&sample_request(), "local", "default").unwrap();
        assert_eq!(validated.tenant_id, "local");
        assert_eq!(validated.workspace_id, "default");
        assert_eq!(validated.output_intent, ProductOutputIntent::ArtifactOnly);
        assert_eq!(validated.intake_contract_sha256.len(), 64);
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn rejects_noop_only_executor_policy() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        let mut req = sample_request();
        req.executor_policy.allowed_executors = vec!["noop".to_string()];
        req.executor_policy.prefer = None;
        let err = validate_intake(&req, "local", "default").unwrap_err();
        assert!(err.contains("noop") || err.contains("admitted"));
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn rejects_path_escape_in_allowed_paths() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        let mut req = sample_request();
        req.allowed_paths = vec!["../secret".to_string()];
        assert!(validate_intake(&req, "local", "default").is_err());
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn rejects_absolute_verification_binary() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        let mut req = sample_request();
        req.verification_commands = vec![ProductVerificationCommand {
            command: "/usr/bin/evil".to_string(),
            timeout_ms: 1000,
        }];
        assert!(validate_intake(&req, "local", "default").is_err());
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn status_transition_matrix_blocks_execution_from_admitted() {
        assert!(!ProductTaskStatus::Admitted.admits_execution());
        assert!(!ProductTaskStatus::WorkspaceBound.admits_execution());
        assert!(ProductTaskStatus::GraphReady.admits_execution());
        assert!(!is_valid_product_task_transition(
            ProductTaskStatus::Admitted,
            ProductTaskStatus::Running
        ));
        assert!(is_valid_product_task_transition(
            ProductTaskStatus::Admitted,
            ProductTaskStatus::WorkspacePreparing
        ));
    }
}
