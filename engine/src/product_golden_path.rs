//! PE7 Product Golden Path — canonical user-task identity and execution contracts.
//!
//! This module owns versioned task intake, worktree binding, graph compilation,
//! and product execution policy. State transitions remain Rust-owned and are
//! advanced only through the existing scheduler and persistence owners.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};

use crate::text::truncate_utf8_bytes;

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
    "managed_deepseek",
    "opencode",
    // Alias retained for readability; resolved to `command` at compile time.
    "deterministic",
];

pub const MAX_OBJECTIVE_BYTES: usize = 8_192;
pub const MAX_PATH_BYTES: usize = 1_024;
pub const MAX_ALLOWED_PATHS: usize = 64;
pub const MAX_VERIFICATION_COMMANDS: usize = 8;
pub const MAX_VERIFICATION_COMMAND_BYTES: usize = 512;
pub const PRODUCT_VERIFICATION_READ_ONLY_COMMANDS: &[&str] = &[
    "echo", "cat", "ls", "head", "tail", "grep", "wc", "true", "false", "test", "python3",
];
/// Legacy accepted smoke verifier (still valid).
pub const MANAGED_DEEPSEEK_LEGACY_DOCS_VERIFIER_COMMAND: &str =
    "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md";
/// Maximum wall-clock for a managed-DeepSeek deterministic verifier node.
/// Raised from the docs-smoke 5s bound so frozen RWE pytest cells can run under
/// the same owner without a second verification system.
const MANAGED_DEEPSEEK_DETERMINISTIC_VERIFIER_MAX_TIMEOUT_MS: u64 = 900_000;

/// Strict shared parser for product/RWE verification commands.
///
/// Admitted shapes:
/// - legacy docs smoke (exact constant)
/// - `PYTHONPATH=<relative> python3 -m pytest <relative paths…> [-q]`
/// - other admitted read-only binaries via existing argv rules (no env prefix)
///
/// Returns (optional env pairs, argv). Env is limited to PYTHONPATH.
#[allow(clippy::type_complexity)]
pub fn parse_strict_product_verification_command(
    command: &str,
) -> Result<(Vec<(String, String)>, Vec<String>), String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("verification command is empty".into());
    }
    if command.starts_with('/') || command.starts_with('.') {
        return Err("verification command must not be an absolute or relative binary path".into());
    }
    if command.contains("..")
        || command.contains(';')
        || command.contains('|')
        || command.contains('&')
        || command.contains('`')
        || command.contains('$')
        || command.contains('\n')
    {
        return Err("verification command contains forbidden shell metacharacters".into());
    }
    if command == MANAGED_DEEPSEEK_LEGACY_DOCS_VERIFIER_COMMAND {
        return Ok((
            Vec::new(),
            command.split_whitespace().map(str::to_string).collect(),
        ));
    }

    let tokens: Vec<&str> = command.split_whitespace().collect();
    let mut env = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        let token = tokens[i];
        if let Some((key, value)) = token.split_once('=') {
            if key.is_empty()
                || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                || key.starts_with('-')
            {
                break;
            }
            if key != "PYTHONPATH" {
                return Err(format!("verification env assignment not admitted: {key}"));
            }
            if value.is_empty() || value.contains("..") || Path::new(value).is_absolute() {
                return Err("PYTHONPATH must be a non-empty repository-relative path".into());
            }
            env.push((key.to_string(), value.to_string()));
            i += 1;
            continue;
        }
        break;
    }
    let argv: Vec<String> = tokens[i..].iter().map(|s| (*s).to_string()).collect();
    if argv.is_empty() {
        return Err("verification command has no executable".into());
    }
    if !PRODUCT_VERIFICATION_READ_ONLY_COMMANDS.contains(&argv[0].as_str()) {
        return Err(format!(
            "verification command must use a read-only admitted binary: {}",
            argv[0]
        ));
    }
    if argv[0] == "python3" || !env.is_empty() {
        if argv.len() < 4 || argv[0] != "python3" || argv[1] != "-m" || argv[2] != "pytest" {
            return Err(
                "python3 verification must be `python3 -m pytest <relative paths…>`".into(),
            );
        }
        for arg in argv.iter().skip(3) {
            if arg.starts_with('-') {
                if arg != "-q" {
                    return Err(format!(
                        "pytest flag not admitted for frozen verifier execution: {arg}"
                    ));
                }
                continue;
            }
            if Path::new(arg).is_absolute()
                || Path::new(arg)
                    .components()
                    .any(|c| matches!(c, Component::ParentDir))
            {
                return Err("pytest paths must be repository-relative".into());
            }
        }
    } else {
        let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        validate_product_verification_command_argv(&argv_refs)?;
    }
    for argument in argv.iter().skip(1) {
        if Path::new(argument).is_absolute()
            || Path::new(argument)
                .components()
                .any(|component| matches!(component, Component::ParentDir))
            || Path::new(argument).components().any(|component| {
                matches!(
                    component,
                    Component::Normal(name)
                        if matches!(name.to_str(), Some(".git" | "target" | "node_modules"))
                )
            })
            || (argument.starts_with('-') && argument.contains('/'))
        {
            return Err(
                "verification command arguments must remain relative to the bound workspace".into(),
            );
        }
    }
    Ok((env, argv))
}

fn grep_short_option_recurses(argument: &str) -> bool {
    let mut flags = argument.trim_start_matches('-').chars().peekable();
    while let Some(flag) = flags.next() {
        if matches!(flag, 'r' | 'R') {
            return true;
        }
        // These GNU grep short options consume the rest of the token as their value.
        // A letter inside that value is not another option (for example `-eerror`).
        if matches!(flag, 'A' | 'B' | 'C' | 'D' | 'd' | 'e' | 'f' | 'm') {
            let value = flags.collect::<String>();
            return flag == 'd' && value == "recurse";
        }
    }
    false
}

fn matches_abbreviated_long_option(argument: &str, canonical: &str) -> bool {
    let name = argument.split_once('=').map_or(argument, |(name, _)| name);
    name.starts_with("--") && name.len() > 2 && canonical.starts_with(name)
}

fn validate_product_verification_command_argv(argv: &[&str]) -> Result<(), String> {
    let binary = argv.first().copied().unwrap_or_default();
    match binary {
        // Recursive traversal composes unsafely with workspace directories deliberately
        // excluded from patch hashing (`target` and `node_modules`). GNU grep also admits
        // recursion through `--directories=recurse` without spelling `-r`.
        "grep" => {
            let mut expect_directories_value = false;
            let mut options = true;
            for argument in argv.iter().skip(1) {
                if expect_directories_value {
                    if *argument == "recurse" {
                        return Err(
                            "verification grep must not recursively traverse the workspace"
                                .to_string(),
                        );
                    }
                    expect_directories_value = false;
                    continue;
                }
                if options && *argument == "--" {
                    options = false;
                    continue;
                }
                if !options {
                    continue;
                }
                let abbreviated_recursive =
                    matches_abbreviated_long_option(argument, "--recursive")
                        || matches_abbreviated_long_option(argument, "--dereference-recursive");
                let directories_recurse = argument.split_once('=').is_some_and(|(name, value)| {
                    matches_abbreviated_long_option(name, "--directories") && value == "recurse"
                });
                if matches!(*argument, "-r" | "-R")
                    || abbreviated_recursive
                    || directories_recurse
                    || (argument.starts_with('-')
                        && !argument.starts_with("--")
                        && grep_short_option_recurses(argument))
                {
                    return Err(
                        "verification grep must not recursively traverse the workspace".to_string(),
                    );
                }
                if *argument == "-d"
                    || (!argument.contains('=')
                        && matches_abbreviated_long_option(argument, "--directories"))
                {
                    expect_directories_value = true;
                } else if argument
                    .strip_prefix("-d")
                    .is_some_and(|value| value == "recurse")
                {
                    return Err(
                        "verification grep must not recursively traverse the workspace".to_string(),
                    );
                }
            }
        }
        // `ls -RL .` can follow symlinks inside excluded directories even though no
        // direct excluded-directory operand appears in argv.
        "ls" => {
            for argument in argv
                .iter()
                .skip(1)
                .take_while(|argument| **argument != "--")
            {
                if *argument == "-R"
                    || matches_abbreviated_long_option(argument, "--recursive")
                    || (argument.starts_with('-')
                        && !argument.starts_with("--")
                        && argument.chars().skip(1).any(|flag| flag == 'R'))
                {
                    return Err(
                        "verification ls must not recursively traverse the workspace".to_string(),
                    );
                }
            }
        }
        // The control file contents are interpreted as path operands by wc, bypassing
        // intake-time path validation entirely.
        "wc" => {
            for argument in argv
                .iter()
                .skip(1)
                .take_while(|argument| **argument != "--")
            {
                if matches_abbreviated_long_option(argument, "--files0-from") {
                    return Err(
                        "verification wc must not load path operands from an indirect file"
                            .to_string(),
                    );
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Resolve the deterministic verifier command for a managed-DeepSeek product graph.
///
/// Accepts exactly one intake-validated verification command whose binary is in
/// the admitted read-only set (including legacy docs smoke and frozen RWE pytest
/// shapes). Caller text never bypasses argv admission.
fn exact_managed_deepseek_verifier_command(
    verification_commands: &Value,
) -> Result<String, String> {
    let commands = verification_commands.as_array().ok_or_else(|| {
        "managed DeepSeek requires exactly one intake-validated verification command".to_string()
    })?;
    let command = commands
        .first()
        .filter(|_| commands.len() == 1)
        .ok_or_else(|| {
            "managed DeepSeek requires exactly one intake-validated verification command"
                .to_string()
        })?;
    let command_text = command
        .get("command")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "managed DeepSeek verification command missing".to_string())?;
    let timeout = command
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .ok_or_else(|| "managed DeepSeek verification timeout_ms missing".to_string())?;
    if timeout == 0 || timeout > MANAGED_DEEPSEEK_DETERMINISTIC_VERIFIER_MAX_TIMEOUT_MS {
        return Err(format!(
            "managed DeepSeek verification timeout_ms must be 1..{MANAGED_DEEPSEEK_DETERMINISTIC_VERIFIER_MAX_TIMEOUT_MS}"
        ));
    }
    // One strict parser: legacy docs smoke always; frozen RWE pytest only when
    // the command is the exact frozen RWE verifier (not arbitrary pytest).
    let (env, argv) = parse_strict_product_verification_command(command_text)?;
    let _ = env;
    if command_text == MANAGED_DEEPSEEK_LEGACY_DOCS_VERIFIER_COMMAND {
        return Ok(command_text.to_string());
    }
    if crate::rwe::frozen_rwe_bindings::is_exact_frozen_rwe_verifier_command(command_text)
        && argv.first().map(String::as_str) == Some("python3")
        && argv.get(1).map(String::as_str) == Some("-m")
        && argv.get(2).map(String::as_str) == Some("pytest")
    {
        return Ok(command_text.to_string());
    }
    Err(
        "managed DeepSeek verifier must be the legacy docs smoke or exact frozen RWE pytest"
            .to_string(),
    )
}

pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;
pub const MAX_TARGET_ID_BYTES: usize = 128;
pub const MAX_EXECUTOR_SET: usize = 16;
pub const MAX_RISK_CLASS_BYTES: usize = 64;

/// Schema identity for the fixture-only deterministic apply helper.
/// This is not a managed coding-executor path and must never be counted as agent evidence.
pub const FIXTURE_DETERMINISTIC_APPLY_SCHEMA: &str = "product_fixture_deterministic_apply.v1";
pub const FIXTURE_DETERMINISTIC_APPLY_FILENAME: &str = ".product_golden_path_apply.py";
pub const FIXTURE_DETERMINISTIC_NOTE_CONTENT: &str = "product golden path fixture note\n";

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
    /// A run exists, but nodes are not yet leased — not "running".
    GraphReady,
    /// G2+: scheduler has leased/advanced at least one node.
    Running,
    /// Post-execution: declared verification commands are executing through the
    /// supervised-patch verification owner.
    Verifying,
    /// Bounded repair is pending or in progress after verification failure.
    RepairPending,
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
    /// External effect (push/PR) outcome could not be determined; must reconcile.
    OutcomeUnknown,
}

/// Runtime-owned authority sampled immediately before and after every Product Golden Path
/// verification command. API callers must populate this from the attached scheduler; the
/// compatibility/manual store path is explicit so it cannot be mistaken for automatic
/// scheduler availability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductVerificationRuntimeAuthority {
    pub scheduler_attached: bool,
    pub scheduler_running: bool,
    pub scheduler_paused: bool,
    pub scheduler_killed: bool,
    pub global_kill_active: bool,
    pub manual_operational_tick: bool,
}

impl ProductVerificationRuntimeAuthority {
    pub fn manual_operational() -> Self {
        Self {
            scheduler_attached: false,
            scheduler_running: false,
            scheduler_paused: false,
            scheduler_killed: false,
            global_kill_active: product_scheduler_kill_active(),
            manual_operational_tick: true,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.global_kill_active {
            return Err("global_kill_active");
        }
        if self.scheduler_killed {
            return Err("scheduler_killed");
        }
        if self.scheduler_paused {
            return Err("scheduler_paused");
        }
        if !self.manual_operational_tick && !self.scheduler_attached {
            return Err("scheduler_not_attached");
        }
        if !self.manual_operational_tick && !self.scheduler_running {
            return Err("scheduler_not_running");
        }
        Ok(())
    }
}

pub fn product_scheduler_kill_active() -> bool {
    std::env::var("ACP_SUPERVISED_WORKERS_KILL_SWITCH")
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
}

impl ProductTaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::WorkspacePreparing => "workspace_preparing",
            Self::WorkspaceBound => "workspace_bound",
            Self::GraphReady => "graph_ready",
            Self::Running => "running",
            Self::Verifying => "verifying",
            Self::RepairPending => "repair_pending",
            Self::AwaitingApproval => "awaiting_approval",
            Self::OutputPending => "output_pending",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Killed => "killed",
            Self::Paused => "paused",
            Self::BudgetExhausted => "budget_exhausted",
            Self::Blocked => "blocked",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "admitted" => Ok(Self::Admitted),
            "workspace_preparing" => Ok(Self::WorkspacePreparing),
            "workspace_bound" => Ok(Self::WorkspaceBound),
            "graph_ready" => Ok(Self::GraphReady),
            "running" => Ok(Self::Running),
            "verifying" => Ok(Self::Verifying),
            "repair_pending" => Ok(Self::RepairPending),
            "awaiting_approval" => Ok(Self::AwaitingApproval),
            "output_pending" => Ok(Self::OutputPending),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "killed" => Ok(Self::Killed),
            "paused" => Ok(Self::Paused),
            "budget_exhausted" => Ok(Self::BudgetExhausted),
            "blocked" => Ok(Self::Blocked),
            "outcome_unknown" => Ok(Self::OutcomeUnknown),
            other => Err(format!("invalid product task status: {other}")),
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed
                | Self::Failed
                | Self::Killed
                | Self::BudgetExhausted
                | Self::Blocked
                | Self::OutcomeUnknown
        )
    }

    /// Scheduler-eligible states. GraphReady means a run exists but may not yet be leased.
    pub fn admits_execution(self) -> bool {
        matches!(
            self,
            Self::GraphReady | Self::Running | Self::Verifying | Self::OutputPending
        )
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
            | (
                GraphReady,
                Running | Verifying | Failed | Killed | Blocked | Paused | BudgetExhausted
            )
            | (
                Running,
                Verifying
                    | RepairPending
                    | AwaitingApproval
                    | OutputPending
                    | Completed
                    | Failed
                    | Killed
                    | Paused
                    | BudgetExhausted
                    | Blocked
                    | OutcomeUnknown
            )
            | (
                Verifying,
                AwaitingApproval
                    | RepairPending
                    | Failed
                    | Killed
                    | Paused
                    | Blocked
                    | BudgetExhausted
                    | OutcomeUnknown
            )
            | (
                RepairPending,
                Verifying | Failed | Killed | Paused | Blocked | BudgetExhausted
            )
            | (
                AwaitingApproval,
                OutputPending | Completed | Failed | Killed | Paused | Blocked | OutcomeUnknown
            )
            | (
                OutputPending,
                Completed | Failed | Killed | Paused | Blocked | AwaitingApproval | OutcomeUnknown
            )
            | (
                OutcomeUnknown,
                OutputPending | Completed | Failed | Killed | Blocked
            )
            | (
                Paused,
                Running | GraphReady | WorkspaceBound | Verifying | Killed | Failed | Blocked
            )
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductOutputIntent {
    ArtifactOnly,
    ExportPatch,
    DraftPr,
    ApplyLocalChanges,
}

impl ProductOutputIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ArtifactOnly => "artifact_only",
            Self::ExportPatch => "export_patch",
            Self::DraftPr => "draft_pr",
            Self::ApplyLocalChanges => "apply_local_changes",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "artifact_only" => Ok(Self::ArtifactOnly),
            "export_patch" => Ok(Self::ExportPatch),
            "draft_pr" => Ok(Self::DraftPr),
            "apply_local_changes" => Ok(Self::ApplyLocalChanges),
            other => Err(format!(
                "output_intent must be artifact_only, export_patch, draft_pr, or apply_local_changes; got {other}"
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
    /// `git_repository` or `local_folder`; defaults from `workspace_mode` for
    /// compatibility with pre-PE7 callers.
    pub source_kind: Option<String>,
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
    pub source_kind: String,
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
    /// Digest-only binding for a local source's configured private-path
    /// exclusions. `None` preserves existing git and historical records.
    #[serde(default)]
    pub local_folder_exclusions_sha256: Option<String>,
    #[serde(default)]
    pub git_origin_remote: Option<String>,
    #[serde(default)]
    pub git_origin_remote_fingerprint: Option<String>,
    #[serde(default)]
    pub git_default_branch: Option<String>,
    #[serde(default)]
    pub git_default_branch_sha: Option<String>,
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
        // One strict parser for admitted verification shapes. Pytest expansion is
        // only accepted for exact frozen RWE authorization (checked after risk_class).
        let (_env, argv) = parse_strict_product_verification_command(command)?;
        if cmd.timeout_ms == 0 || cmd.timeout_ms > 3_600_000 {
            return Err("verification command timeout_ms must be 1..3600000".to_string());
        }
        let is_pytest_shape = argv.first().map(String::as_str) == Some("python3")
            && argv.get(1).map(String::as_str) == Some("-m")
            && argv.get(2).map(String::as_str) == Some("pytest");
        if is_pytest_shape {
            // Scope pytest only to exact frozen RWE — never arbitrary ProductTask policy.
            let risk = request.risk_class.trim();
            if !crate::rwe::frozen_rwe_bindings::is_exact_frozen_rwe_product_intake(
                risk,
                request.source_revision.trim(),
                &request.allowed_paths,
                command,
                cmd.timeout_ms,
            ) {
                return Err(
                    "python3 -m pytest verification is admitted only for exact frozen RWE authorization"
                        .into(),
                );
            }
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
    // Kept in the intake wire contract for compatibility only. Intake-time
    // confirmation never grants output authority; every output intent requires
    // a fresh explicit confirmation after an independent persisted approval.
    let confirm_output = request.confirm_output.unwrap_or(false);

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
    if !matches!(workspace_mode.as_str(), "git_worktree" | "local_folder") {
        return Err("workspace_mode must be git_worktree or local_folder".to_string());
    }
    let source_kind = request
        .source_kind
        .as_deref()
        .unwrap_or(if workspace_mode == "local_folder" {
            "local_folder"
        } else {
            "git_repository"
        })
        .trim()
        .to_string();
    if !matches!(source_kind.as_str(), "git_repository" | "local_folder")
        || (source_kind == "local_folder") != (workspace_mode == "local_folder")
    {
        return Err("source_kind must match workspace_mode".to_string());
    }
    let local_source_manifest = if workspace_mode == "local_folder" {
        if matches!(output_intent, ProductOutputIntent::DraftPr) {
            return Err("local_folder source cannot use draft_pr output".to_string());
        }
        let exclusions = crate::local_folder_source::configured_local_folder_exclusions()?;
        let manifest =
            crate::local_folder_source::capture_local_folder_manifest(target_path, &exclusions)?;
        if let Some(expected) = request
            .source_tree_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if expected != manifest.tree_sha256 {
                return Err(
                    "local-folder source_tree_hash does not match current manifest".to_string(),
                );
            }
        }
        Some(manifest)
    } else {
        None
    };

    let objective_fingerprint = fingerprint_objective(objective);
    let validated = ValidatedProductTaskIntake {
        schema_version: PRODUCT_TASK_INTAKE_SCHEMA_VERSION.to_string(),
        objective: objective.to_string(),
        objective_fingerprint,
        target_id: target_id.to_string(),
        target_repo_path: target_repo_path.to_string(),
        source_kind,
        source_revision: if workspace_mode == "local_folder" {
            local_source_manifest
                .as_ref()
                .expect("local folder manifest established")
                .tree_sha256
                .clone()
        } else {
            source_revision.to_string()
        },
        source_tree_hash: if workspace_mode == "local_folder" {
            Some(
                local_source_manifest
                    .as_ref()
                    .expect("local folder manifest established")
                    .tree_sha256
                    .clone(),
            )
        } else {
            request
                .source_tree_hash
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        },
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
        "source_kind": intake.source_kind,
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
        // Cap at 256 bytes including the three-byte ellipsis if truncated;
        // full body is not a durable evidence corpus.
        "objective_preview": truncate_utf8_bytes(&intake.objective, 256, "…"),
        "target_id": intake.target_id,
        "target_repo_path_fingerprint": fingerprint_private_path(&intake.target_repo_path),
        "source_kind": intake.source_kind,
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

/// Stable evidence identity for a local filesystem path. The operational
/// owner retains the path separately; public/audit projections do not.
pub fn fingerprint_private_path(path: &str) -> String {
    hex::encode(Sha256::digest(path.as_bytes()))
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
    // `compute_manifest` also carries an observational `computed_at` timestamp.  It must not
    // participate in the content identity or an unchanged workspace can appear to mutate when a
    // verification command crosses a wall-clock second boundary.
    let files = manifest
        .get("files")
        .ok_or_else(|| "workspace manifest missing files".to_string())?;
    let payload = serde_json::to_string(files).map_err(|e| e.to_string())?;
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

/// Bind the immutable product-task authority that a managed coding executor may use.
///
/// The raw objective is deliberately excluded from this receipt. Its authoritative
/// fingerprint and the complete intake-contract hash bind the prompt without making
/// terminal/audit evidence a prompt corpus.
pub fn product_apply_binding_sha256(
    workspace_id: &str,
    node_metadata: &Value,
) -> Result<String, String> {
    let required_string = |field: &str| {
        node_metadata
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("product apply binding is missing {field}"))
    };
    let metadata_workspace_id = required_string("workspace_id")?;
    if metadata_workspace_id != workspace_id {
        return Err("product apply workspace identity changed".to_string());
    }
    let allowed_paths = node_metadata
        .get("allowed_paths")
        .and_then(Value::as_array)
        .filter(|paths| {
            !paths.is_empty()
                && paths
                    .iter()
                    .all(|path| path.as_str().is_some_and(|value| !value.trim().is_empty()))
        })
        .ok_or_else(|| "product apply binding is missing allowed_paths".to_string())?;
    let binding_schema_version = node_metadata
        .get("product_apply_binding_schema_version")
        .and_then(Value::as_str)
        .unwrap_or("product_apply_binding.v1");
    if !matches!(
        binding_schema_version,
        "product_apply_binding.v1" | "product_apply_binding.v2"
    ) {
        return Err("product apply binding schema version is unsupported".to_string());
    }
    let mut payload = json!({
        "schema_version": binding_schema_version,
        "workspace_id": metadata_workspace_id,
        "product_task_id": required_string("product_task_id")?,
        "source_revision": required_string("source_revision")?,
        "objective_fingerprint": required_string("objective_fingerprint")?,
        "intake_contract_sha256": required_string("intake_contract_sha256")?,
        "allowed_paths": allowed_paths,
        "executor": required_string("executor")?,
        "executor_class": required_string("executor_class")?,
        "workspace_path": required_string("workspace_path")?,
        "workspace_root": required_string("workspace_root")?,
        "output_intent": required_string("output_intent")?,
    });
    if binding_schema_version == "product_apply_binding.v2" {
        let product_budget = node_metadata
            .get("product_budget")
            .filter(|budget| budget.is_object())
            .ok_or_else(|| "product apply binding is missing product_budget".to_string())?;
        payload["product_budget"] = product_budget.clone();
        payload["managed_executor_identity"] = node_metadata
            .get("managed_executor_identity")
            .cloned()
            .unwrap_or(Value::Null);
    }
    let encoded = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

fn require_rfc3339_utc(field: &str, value: &str) -> Result<String, String> {
    let dt = chrono::DateTime::parse_from_rfc3339(value.trim())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|_| format!("{field} must be canonical RFC3339/UTC"))?;
    Ok(dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
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
    let created_at = require_rfc3339_utc("created_at", created_at)?;
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
    let objective_preview = intake
        .get("objective_preview")
        .cloned()
        .unwrap_or_else(|| json!("bounded ProductTask stage"));
    let budget = intake.get("budget").cloned().unwrap_or(json!({}));
    let effective_total_tokens = budget
        .get("total_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(50_000);
    let mut product_budget = budget.clone();
    product_budget
        .as_object_mut()
        .ok_or_else(|| "product task budget must be an object".to_string())?
        .insert("total_tokens".to_string(), json!(effective_total_tokens));
    let verification_commands = intake
        .get("verification_commands")
        .cloned()
        .unwrap_or(json!([]));

    let task_type = match resolved_executor {
        "command" | "deterministic" => "command",
        "agent_step" => "agent_step",
        "local_runner_validation" => "local_runner_validation",
        "claude_code_cli" => "claude_code_cli",
        "codex_cli" => "codex_cli",
        "managed_deepseek" => "managed_deepseek",
        "opencode" => crate::opencode_runtime::OPENCODE_TASK_TYPE,
        other => {
            return Err(format!(
                "unsupported product executor for graph compile: {other}"
            ))
        }
    };

    // Fixture-only deterministic apply: not a managed coding-executor path.
    // Managed coding executors (CLI/agent_step) receive objective context and must not
    // use this helper. The helper is staged only when executor class is fixture_deterministic.
    let is_fixture_deterministic = matches!(resolved_executor, "command" | "deterministic");
    let command = if is_fixture_deterministic {
        format!("python3 {FIXTURE_DETERMINISTIC_APPLY_FILENAME}")
    } else {
        String::new()
    };
    let executor_class = if is_fixture_deterministic {
        "fixture_deterministic"
    } else {
        "managed_coding"
    };
    let managed_executor_identity = if resolved_executor == "claude_code_cli" {
        let config = crate::cli::CliConfig::from_env();
        let admission = config.claude_code_admission.ok_or_else(|| {
            "Claude Code executor is not exactly admitted at graph compile".to_string()
        })?;
        if effective_total_tokens < admission.max_attempt_tokens
            || budget.get("total_calls").and_then(Value::as_u64) != Some(1)
            || budget.get("max_retries").and_then(Value::as_u64) != Some(0)
            || budget.get("max_concurrency").and_then(Value::as_u64) != Some(1)
        {
            return Err(format!(
                "Claude Code requires total_tokens>={}, total_calls=1, max_retries=0, and max_concurrency=1",
                admission.max_attempt_tokens
            ));
        }
        json!({
            "schema_version": "managed_executor_identity.v1",
            "executor_type": "claude_code_cli",
            "executor_class": "managed_coding",
            "binary_path": admission.binary_path,
            "binary_version": admission.binary_version,
            "binary_sha256": admission.binary_sha256,
            "model": admission.model,
            "model_resolution": admission.model_resolution(),
            "max_turns": admission.max_turns,
            "max_budget_usd": admission.max_budget_usd,
            "max_attempt_tokens": admission.max_attempt_tokens,
            "context_tokens": admission.context_tokens,
            "max_output_tokens": admission.max_output_tokens,
            "input_usd_per_mtok": admission.input_usd_per_mtok,
            "cache_write_5m_usd_per_mtok": admission.cache_write_5m_usd_per_mtok,
            "cache_write_1h_usd_per_mtok": admission.cache_write_1h_usd_per_mtok,
            "cache_read_usd_per_mtok": admission.cache_read_usd_per_mtok,
            "output_usd_per_mtok": admission.output_usd_per_mtok,
            "pricing_source": admission.pricing_source,
            "pricing_verified_at": admission.pricing_verified_at,
        })
    } else if resolved_executor == "codex_cli" {
        let config = crate::cli::CliConfig::from_env();
        let admission = config.codex_admission.ok_or_else(|| {
            "Codex executor requires a current managed coding runtime profile at graph compile"
                .to_string()
        })?;
        let profile = admission.runtime_profile.as_ref().ok_or_else(|| {
            "legacy Codex identity cannot compile a new managed product graph".to_string()
        })?;
        let observed = admission.observed_runtime.as_ref().ok_or_else(|| {
            "Codex runtime profile observation is incomplete at graph compile".to_string()
        })?;
        let profile_sha256 = profile.profile_sha256()?;
        json!({
            "schema_version": "managed_executor_identity.v1",
            "executor_type": "codex_cli",
            "executor_class": "managed_coding",
            "runtime_profile_schema_version": profile.schema_version,
            "runtime_profile_id": profile.profile_id,
            "runtime_profile_sha256": profile_sha256,
            "capability_probe_sha256": observed.capability_probe_sha256,
            "binary_path": observed.canonical_executable_path,
            "binary_version": observed.observed_version,
            "binary_sha256": observed.binary_sha256,
            "executor_kind": profile.executor_kind,
            "protocol_kind": profile.protocol_kind,
            "requested_model": profile.requested_model,
            "resolved_model": profile.resolved_model,
            "model": admission.model,
            "thinking_configuration": profile.thinking_configuration,
            "provider_identity": profile.provider_identity,
            "credential_reference": profile.credential_reference,
            "endpoint_allowlist": profile.endpoint_allowlist,
            "usage_parser_version": profile.usage_parser_version,
            "pricing_source_version": profile.pricing_source_version,
            "admission_classification": profile.admission_classification,
        })
    } else if resolved_executor == "managed_deepseek" {
        json!({
            "schema_version": "managed_executor_identity.v1",
            "executor_type": "managed_deepseek",
            "executor_class": "managed_coding",
            "provider_kind": "deepseek",
            "protocol": "openai_compatible",
            "credential_reference": "DEEPSEEK_API_KEY",
            "planner_model": "deepseek-v4-pro",
            "implementer_model": "deepseek-v4-flash",
            "reviewer_model": "deepseek-v4-pro",
            "route_schema_version": "managed_deepseek_route.v1",
            "usage_parser_version": "protocol_usage.v1",
            "authority_source": "LocalProductStore.managed_acceptance",
            "output_policy": "redacted_digest_only"
        })
    } else {
        Value::Null
    };

    let apply_node_id = format!("{}-apply", plan_ids.workflow_id);
    let mut apply_node = json!({
        "schema_version": "workflow_node.v1",
        "node_id": apply_node_id,
        "workflow_id": plan_ids.workflow_id,
        "task_type": task_type,
        "assigned_agent_id": null,
        "status": "pending",
        "input_refs": [],
        "output_ref": null,
        "budget": effective_total_tokens as f64,
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
        "executor": if is_fixture_deterministic { "command" } else { resolved_executor },
        "executor_class": executor_class,
        "managed_executor_identity": managed_executor_identity,
        "fixture_apply_schema": if is_fixture_deterministic {
            Value::String(FIXTURE_DETERMINISTIC_APPLY_SCHEMA.to_string())
        } else {
            Value::Null
        },
        "objective_fingerprint": task.get("objective_fingerprint"),
        "intake_contract_sha256": task.get("intake_contract_sha256"),
        "verification_commands": verification_commands,
        "output_intent": task.get("output_intent"),
        "product_apply_binding_schema_version": "product_apply_binding.v2",
        "product_budget": product_budget,
        "product_graph_schema_version": PRODUCT_EXECUTABLE_GRAPH_SCHEMA_VERSION,
        "managed_supervised_patch": Value::Null,
        "managed_deepseek": if resolved_executor == "managed_deepseek" {
            json!({
                "schema_version": "managed_deepseek_node.v1",
                "stage": "planning",
                "role": "planner",
                "protocol": "openai_compatible",
                "binding": Value::Null,
                "prompt": objective_preview,
                "authority_binding_source": "LocalProductStore.managed_acceptance"
            })
        } else {
            Value::Null
        },
    });
    let binding_sha256 = product_apply_binding_sha256(workspace_id, &apply_node)?;
    apply_node["managed_supervised_patch"] = json!({
        "schema_version": "managed_supervised_patch.v1",
        "workspace_id": workspace_id,
        "operation": "product_apply",
        "attempt": 1,
        "binding_sha256": binding_sha256,
        "content_excluded": true,
        "product_task_id": task_id,
        "executor_class": executor_class,
    });
    if is_fixture_deterministic {
        apply_node
            .as_object_mut()
            .unwrap()
            .insert("command".to_string(), json!(command));
    }

    if resolved_executor == "managed_deepseek" {
        let verifier_command = exact_managed_deepseek_verifier_command(&verification_commands)?;
        let deferred_binding = task
            .get("managed_deepseek_deferred_binding")
            .and_then(Value::as_bool)
            == Some(true);
        let binding = task
            .get("managed_deepseek_binding")
            .cloned()
            .filter(|value| value.is_object());
        if binding.is_none() && !deferred_binding {
            return Err("managed DeepSeek graph requires a store-issued binding".to_string());
        }
        let stages = [
            (
                "planning",
                "planner",
                "managed_deepseek",
                Vec::<String>::new(),
            ),
            (
                "implementation",
                "implementer",
                "managed_deepseek",
                vec![format!("{}-planning", plan_ids.workflow_id)],
            ),
            (
                "deterministic_verification",
                "deterministic",
                "command",
                vec![format!("{}-implementation", plan_ids.workflow_id)],
            ),
            (
                "review",
                "reviewer",
                "managed_deepseek",
                vec![format!(
                    "{}-deterministic_verification",
                    plan_ids.workflow_id
                )],
            ),
        ];
        let mut route_nodes = Vec::with_capacity(stages.len());
        for (stage, role, executor, input_refs) in &stages {
            let node_id = format!("{}-{stage}", plan_ids.workflow_id);
            let mut node = apply_node.clone();
            node["node_id"] = json!(node_id);
            node["task_type"] = json!(executor);
            node["executor"] = json!(executor);
            node["suggested_executor"] = json!(executor);
            node["input_refs"] = json!(input_refs);
            node["managed_supervised_patch"] = Value::Null;
            if *stage == "deterministic_verification" {
                node["managed_deepseek"] = Value::Null;
                node["command"] = json!(verifier_command);
                node["executor_class"] = json!("deterministic_verifier");
            } else {
                let mut stage_binding = binding.clone().unwrap_or_else(|| json!({}));
                stage_binding["node_id"] = json!(node_id);
                // Prefer a concrete allowed path for prompts (workspace-bound, not caller text).
                let prompt_path = allowed_paths
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(Value::as_str)
                    .unwrap_or("docs/USER_GUIDE.md");
                // The docs Golden Path planner gate admits only the legacy
                // clarify intent for docs/USER_GUIDE.md; frozen RWE cells admit
                // bounded_product_task inside the frozen union. Emit the intent
                // that matches the scope so a real model obeying the prompt is
                // accepted by the executor validator.
                let planning_intent = if prompt_path == "docs/USER_GUIDE.md" {
                    "clarify_doctor_read_only_health_check"
                } else {
                    "bounded_product_task"
                };
                let prompt = match *stage {
                    "planning" => format!(
                        "{}\nReturn exactly one JSON object and no markdown: {{\"schema_version\":\"managed_deepseek_plan.v1\",\"status\":\"planned\",\"path\":\"{prompt_path}\",\"intent\":\"{planning_intent}\"}}. Stay within allowed_paths.",
                        objective_preview
                    ),
                    "implementation" => format!(
                        "{}\nReturn exactly one JSON object and no markdown: {{\"schema_version\":\"managed_workspace_action.v1\",\"action\":\"replace_text\",\"path\":\"{prompt_path}\",\"old_text\":\"...\",\"new_text\":\"...\"}}. Use only an exact allowed path and make one bounded replacement.",
                        objective_preview
                    ),
                    "review" => format!(
                        "{}\nReturn exactly one JSON object and no markdown: {{\"schema_version\":\"managed_deepseek_review.v1\",\"status\":\"accepted\",\"material_objections\":[]}}. Use status rejected when any material objection remains.",
                        objective_preview
                    ),
                    _ => objective_preview.to_string(),
                };
                node["managed_deepseek"] = json!({
                    "schema_version": "managed_deepseek_node.v1",
                    "stage": stage,
                    "role": role,
                    "protocol": "openai_compatible",
                    "binding": stage_binding,
                    "binding_status": if deferred_binding { "deferred" } else { "bound" },
                    "prompt": prompt,
                    "authority_binding_source": "LocalProductStore.managed_acceptance"
                });
            }
            route_nodes.push(node);
        }
        let edges = stages
            .windows(2)
            .map(|window| {
                json!({
                    "edge_id": format!("edge-{}-{}", window[0].0, window[1].0),
                    "from_node_id": format!("{}-{}", plan_ids.workflow_id, window[0].0),
                    "to_node_id": format!("{}-{}", plan_ids.workflow_id, window[1].0),
                    "edge_type": "dependency"
                })
            })
            .collect::<Vec<_>>();
        return Ok(json!({
            "schema_version": "workflow_graph.v1",
            "workflow_id": plan_ids.workflow_id,
            "dispatch_id": plan_ids.dispatch_id,
            "nodes": route_nodes,
            "edges": edges,
            "status": "executable",
            "created_at": created_at,
            "updated_at": created_at,
            "product_task_id": task_id,
            "product_graph_schema_version": PRODUCT_EXECUTABLE_GRAPH_SCHEMA_VERSION,
            "managed_route": "deepseek_pro_flash_verify_pro.v1"
        }));
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
    use std::sync::Mutex;

    fn env_lock() -> &'static Mutex<()> {
        crate::cli::config::cli_env_test_lock()
    }

    fn sample_request() -> ProductTaskIntakeRequest {
        ProductTaskIntakeRequest {
            objective: "Add a README section describing the golden path.".to_string(),
            target_id: "demo-repo".to_string(),
            target_repo_path: "/tmp/demo-repo".to_string(),
            source_kind: None,
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

    fn managed_deepseek_graph_task(verification_commands: Value) -> Value {
        json!({
            "task_id": "product-task-deepseek",
            "status": "workspace_bound",
            "tenant_id": "tenant-1",
            "workspace_id": "workspace-1",
            "objective_fingerprint": "a".repeat(64),
            "intake_contract_sha256": "b".repeat(64),
            "output_intent": "draft_pr",
            "workspace_binding": {
                "workspace_path": "/tmp/product-workspace",
                "workspace_id": "workspace-1",
                "source_revision": "c".repeat(40),
                "allowed_paths": ["docs/USER_GUIDE.md"]
            },
            "managed_deepseek_binding": {
                "product_task_id": "product-task-deepseek",
                "workflow_id": "wf-plan-0001",
                "node_id": "wf-plan-0001-planning",
                "attempt_id": "attempt-1",
                "spend_authorization_id": "spend-1",
                "attempt_lease_id": "lease-1"
            },
            "intake": {
                "objective_preview": "Clarify the read-only doctor health check.",
                "budget": {"total_tokens": 12000},
                "verification_commands": verification_commands
            }
        })
    }

    #[test]
    fn compile_product_executable_graph_rejects_unparseable_created_at() {
        let task = managed_deepseek_graph_task(json!([{
            "command": "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md",
            "timeout_ms": 5_000
        }]));
        let err = compile_product_executable_graph(
            &task,
            "not-a-timestamp",
            &crate::read_only_planner::WorkflowPlanIds::for_sequence(1),
            "managed_deepseek",
        )
        .unwrap_err();
        assert!(
            err.contains("created_at must be canonical RFC3339/UTC"),
            "{err}"
        );
    }

    #[test]
    fn managed_deepseek_executor_compiles_as_an_explicit_product_node() {
        let task = managed_deepseek_graph_task(json!([{
            "command": "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md",
            "timeout_ms": 5_000
        }]));
        let graph = compile_product_executable_graph(
            &task,
            "2026-07-30T00:00:00Z",
            &crate::read_only_planner::WorkflowPlanIds::for_sequence(1),
            "managed_deepseek",
        )
        .unwrap();
        assert_eq!(graph["nodes"].as_array().unwrap().len(), 4);
        let node = &graph["nodes"][0];
        assert_eq!(node["task_type"], "managed_deepseek");
        assert_eq!(node["executor"], "managed_deepseek");
        assert_eq!(node["managed_deepseek"]["stage"], "planning");
        assert_eq!(node["managed_deepseek"]["role"], "planner");
        assert_eq!(
            node["managed_deepseek"]["binding"]["node_id"],
            "wf-plan-0001-planning"
        );
        assert_eq!(graph["nodes"][2]["task_type"], "command");
        assert_eq!(
            graph["nodes"][2]["command"],
            MANAGED_DEEPSEEK_LEGACY_DOCS_VERIFIER_COMMAND
        );
        assert_eq!(graph["nodes"][3]["managed_deepseek"]["role"], "reviewer");
        assert_eq!(
            node["managed_executor_identity"]["planner_model"],
            "deepseek-v4-pro"
        );
        // Regression: the docs Golden Path planning prompt must keep the legacy
        // clarify intent so a real model obeying the prompt passes the executor
        // planner gate (bounded_product_task is only valid inside the frozen
        // RWE union; docs/USER_GUIDE.md is not in it).
        let planning_prompt = graph["nodes"][0]["managed_deepseek"]["prompt"]
            .as_str()
            .unwrap_or("");
        assert!(
            planning_prompt.contains("clarify_doctor_read_only_health_check"),
            "docs GP planning prompt lost the legacy intent: {planning_prompt}"
        );
        assert!(
            !planning_prompt.contains("bounded_product_task"),
            "docs GP planning prompt must not request the RWE intent: {planning_prompt}"
        );
        // And a non-docs allowed path requests the bounded RWE intent while
        // keeping the admitted docs verifier shape.
        let mut rwe_task = managed_deepseek_graph_task(json!([{
            "command": "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md",
            "timeout_ms": 5_000
        }]));
        rwe_task["workspace_binding"]["allowed_paths"] =
            json!(["apps/api/tests/test_alters_persist.py"]);
        let rwe_graph = compile_product_executable_graph(
            &rwe_task,
            "2026-07-30T00:00:00Z",
            &crate::read_only_planner::WorkflowPlanIds::for_sequence(1),
            "managed_deepseek",
        )
        .unwrap();
        let rwe_prompt = rwe_graph["nodes"][0]["managed_deepseek"]["prompt"]
            .as_str()
            .unwrap_or("");
        assert!(
            rwe_prompt.contains("bounded_product_task"),
            "non-docs planning prompt must request the bounded RWE intent: {rwe_prompt}"
        );
    }

    #[test]
    fn managed_deepseek_graph_rejects_damaged_verifier_shapes() {
        let cases = [
            ("missing", None),
            ("malformed_collection", Some(json!({"command": "true"}))),
            ("empty", Some(json!([]))),
            ("malformed_entry", Some(json!(["not-an-object"]))),
            ("missing_command", Some(json!([{"timeout_ms": 5_000}]))),
            (
                "malformed_command",
                Some(json!([{"command": 7, "timeout_ms": 5_000}])),
            ),
            (
                "empty_command",
                Some(json!([{"command": "", "timeout_ms": 5_000}])),
            ),
            (
                "different_command",
                Some(json!([{"command": "true", "timeout_ms": 5_000}])),
            ),
            (
                "missing_timeout",
                Some(json!([{
                    "command": "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md"
                }])),
            ),
            (
                "excessive_timeout",
                Some(json!([{
                    "command": "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md",
                    "timeout_ms": 900_001
                }])),
            ),
            (
                "multiple_commands",
                Some(json!([
                    {
                        "command": "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md",
                        "timeout_ms": 5_000
                    },
                    {"command": "true", "timeout_ms": 1}
                ])),
            ),
        ];
        for (name, verification_commands) in cases {
            let mut task = managed_deepseek_graph_task(json!([]));
            if let Some(verification_commands) = verification_commands {
                task["intake"]["verification_commands"] = verification_commands;
            } else {
                task["intake"]
                    .as_object_mut()
                    .unwrap()
                    .remove("verification_commands");
            }
            let error = compile_product_executable_graph(
                &task,
                "2026-07-30T00:00:00Z",
                &crate::read_only_planner::WorkflowPlanIds::for_sequence(1),
                "managed_deepseek",
            )
            .expect_err(name);
            assert!(
                error.contains("exact bounded docs check")
                    || error.contains("exact frozen RWE pytest")
                    || error.contains("legacy docs smoke")
                    || error.contains("intake-validated verification")
                    || error.contains("verification command")
                    || error.contains("timeout_ms")
                    || error.contains("not admitted")
                    || error.contains("python3"),
                "{name}: {error}"
            );
        }
    }

    #[test]
    fn managed_deepseek_accepts_frozen_rwe_pytest_verifier_shape() {
        let task = managed_deepseek_graph_task(json!([{
            "command": "PYTHONPATH=apps/api/src python3 -m pytest apps/api/tests/ -q",
            "timeout_ms": 900_000
        }]));
        let mut task = task;
        task["workspace_binding"]["allowed_paths"] =
            json!(["apps/api/src", "apps/api/tests", "README.md"]);
        let graph = compile_product_executable_graph(
            &task,
            "2026-07-30T00:00:00Z",
            &crate::read_only_planner::WorkflowPlanIds::for_sequence(2),
            "managed_deepseek",
        )
        .unwrap();
        assert_eq!(
            graph["nodes"][2]["command"],
            "PYTHONPATH=apps/api/src python3 -m pytest apps/api/tests/ -q"
        );
    }

    #[test]
    fn rejects_when_gate_disabled() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::remove_var(PRODUCT_TASK_GATE);
        let err = validate_intake(&sample_request(), "local", "default").unwrap_err();
        assert!(err.contains("disabled"));
    }

    #[test]
    fn accepts_valid_intake_when_gate_enabled() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        let validated = validate_intake(&sample_request(), "local", "default").unwrap();
        assert_eq!(validated.tenant_id, "local");
        assert_eq!(validated.workspace_id, "default");
        assert_eq!(validated.output_intent, ProductOutputIntent::ArtifactOnly);
        assert_eq!(validated.intake_contract_sha256.len(), 64);
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn objective_preview_is_utf8_bounded_and_hash_stable() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        let mut request = sample_request();
        request.objective = format!("前{}🙂", "界".repeat(100));
        let validated = validate_intake(&request, "local", "default").unwrap();

        let first = redacted_intake_json(&validated);
        let second = redacted_intake_json(&validated);
        let preview = first
            .get("objective_preview")
            .and_then(Value::as_str)
            .expect("objective preview");
        assert_eq!(first, second);
        assert!(preview.len() <= 256);
        assert!(preview.ends_with('…'));
        assert!(std::str::from_utf8(preview.as_bytes()).is_ok());
        assert_eq!(
            validated.objective_fingerprint,
            fingerprint_objective(&request.objective)
        );
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn rejects_noop_only_executor_policy() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        let mut req = sample_request();
        req.allowed_paths = vec!["../secret".to_string()];
        assert!(validate_intake(&req, "local", "default").is_err());
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn rejects_absolute_verification_binary() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
    fn rejects_option_attached_absolute_verification_paths() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        for command in [
            "grep -f/etc/shadow README.md",
            "grep --file=/etc/shadow README.md",
            "wc --files0-from=/etc/passwd",
        ] {
            let mut req = sample_request();
            req.verification_commands = vec![ProductVerificationCommand {
                command: command.to_string(),
                timeout_ms: 1000,
            }];
            assert!(
                validate_intake(&req, "local", "default").is_err(),
                "option-attached path must be rejected: {command}"
            );
        }
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn rejects_verification_paths_excluded_from_workspace_observation() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        for command in [
            "cat .git/config",
            "cat target/leak",
            "cat ./target/leak",
            "grep needle node_modules/package/file.js",
        ] {
            let mut req = sample_request();
            req.verification_commands = vec![ProductVerificationCommand {
                command: command.to_string(),
                timeout_ms: 1000,
            }];
            assert!(
                validate_intake(&req, "local", "default").is_err(),
                "unobserved workspace path must be rejected: {command}"
            );
        }
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn rejects_recursive_or_indirect_verification_traversal() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        for command in [
            "grep -R needle .",
            "grep -rn needle .",
            "grep --recursive needle .",
            "grep --rec needle .",
            "grep --dereference-recursive needle .",
            "grep --dereference-rec needle .",
            "grep -d recurse needle .",
            "grep -drecurse needle .",
            "grep --directories=recurse needle .",
            "grep --dir=recurse needle .",
            "ls -R .",
            "ls -LR .",
            "ls --recursive .",
            "ls --rec .",
            "wc --files0-from list",
            "wc --files0-from=list",
            "wc --files0-f=list",
        ] {
            let mut req = sample_request();
            req.verification_commands = vec![ProductVerificationCommand {
                command: command.to_string(),
                timeout_ms: 1000,
            }];
            assert!(
                validate_intake(&req, "local", "default").is_err(),
                "recursive or indirect traversal must be rejected: {command}"
            );
        }
        std::env::remove_var(PRODUCT_TASK_GATE);
    }

    #[test]
    fn accepts_nonrecursive_options_with_recursive_letters_in_values() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::env::set_var(PRODUCT_TASK_GATE, "1");
        for command in [
            "grep -eerror README.md",
            "grep -- -R README.md",
            "ls -- -R",
            "wc -- --files0-from",
        ] {
            let mut req = sample_request();
            req.verification_commands = vec![ProductVerificationCommand {
                command: command.to_string(),
                timeout_ms: 1000,
            }];
            assert!(
                validate_intake(&req, "local", "default").is_ok(),
                "nonrecursive option grammar must remain accepted: {command}"
            );
        }
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

    #[test]
    fn workspace_content_identity_excludes_observation_timestamp() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("README.md"), "stable\n").unwrap();
        let first = workspace_content_hash(workspace.path()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1_100));
        let second = workspace_content_hash(workspace.path()).unwrap();
        assert_eq!(first, second);
    }
}
