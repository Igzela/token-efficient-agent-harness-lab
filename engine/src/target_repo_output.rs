use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::provider::redaction::{
    contains_sensitive_patterns, redact_secrets, redact_sensitive_patterns,
};

mod authority;

pub const TARGET_REPO_OUTPUT_SCHEMA_VERSION: &str = "target_repo_output.v1";
pub const GIT_WORKTREE_ADD_OUTCOME_UNKNOWN: &str = "git worktree add outcome is unknown";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_TIMEOUT_MS: u64 = 30_000;
const MAX_GIT_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PATCH_BYTES: usize = 1024 * 1024;
const MAX_CHANGED_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Clone, PartialEq)]
pub struct TargetRepoOutputConfig {
    enabled: bool,
    kill_switch: bool,
    timeout_ms: u64,
    allowed_remotes: HashSet<String>,
    allowed_remote_hosts: HashSet<String>,
    allow_local_remote: bool,
    git_username: String,
    git_token: Option<String>,
}

impl TargetRepoOutputConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: env_enabled("ACP_ENABLE_TARGET_REPO_OUTPUT"),
            kill_switch: env_enabled("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH"),
            timeout_ms: std::env::var("ACP_TARGET_REPO_GIT_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value > 0)
                .map(|value| value.min(MAX_TIMEOUT_MS))
                .unwrap_or(DEFAULT_TIMEOUT_MS),
            allowed_remotes: env_list("ACP_TARGET_REPO_REMOTE_ALLOWLIST", &["origin"]),
            allowed_remote_hosts: env_list("ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST", &[]),
            allow_local_remote: env_enabled("ACP_TARGET_REPO_ALLOW_LOCAL_REMOTE"),
            git_username: std::env::var("ACP_TARGET_REPO_GIT_USERNAME")
                .ok()
                .filter(|value| valid_git_username(value))
                .unwrap_or_else(|| "x-access-token".to_string()),
            git_token: target_repo_git_token(),
        }
    }

    pub fn for_test(enabled: bool, kill_switch: bool) -> Self {
        Self {
            enabled,
            kill_switch,
            timeout_ms: 10_000,
            allowed_remotes: HashSet::from(["origin".to_string()]),
            allowed_remote_hosts: HashSet::new(),
            allow_local_remote: true,
            git_username: "x-access-token".to_string(),
            git_token: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled && !self.kill_switch
    }

    pub fn require_enabled(&self) -> Result<(), String> {
        authority::require_target_output_enabled(self)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GitWorkspaceInfo {
    pub schema_version: String,
    pub workspace_path: String,
    pub source_revision: String,
    pub default_branch: String,
    /// Exact credential-free origin retained only by the internal workspace
    /// owner. Public/audit projections expose the fingerprint below instead.
    pub origin_remote: String,
    /// SHA-256 fingerprint of the credential-free origin. Public and audit
    /// projections retain this fingerprint rather than the internal remote.
    pub origin_remote_fingerprint: String,
    /// Exact current commit of the bound default branch, distinct from a
    /// possibly detached source revision.
    pub default_branch_sha: String,
    pub workspace_mode: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BranchPublishRequest {
    pub target_repo_path: PathBuf,
    pub workspace_path: PathBuf,
    pub source_revision: String,
    pub expected_patch_hash: String,
    pub branch_name: String,
    pub remote: String,
    pub commit_message: String,
    pub pr_title: String,
    pub pr_body: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PatchExport {
    pub schema_version: String,
    pub source_revision: String,
    pub patch_hash: String,
    pub patch: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GitChangeSet {
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
    pub changed_files: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BranchPublishOutput {
    pub schema_version: String,
    pub source_revision: String,
    pub branch_name: String,
    pub remote: String,
    pub commit_sha: String,
    pub patch_hash: String,
    pub pr_title: String,
    pub pr_body: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GitHubRepository {
    pub host: String,
    pub owner: String,
    pub repository: String,
}

#[derive(Clone, PartialEq)]
pub struct GitHubPullRequestConfig {
    enabled: bool,
    api_base: String,
    allowed_repositories: HashSet<String>,
    token: Option<String>,
}

impl GitHubPullRequestConfig {
    pub fn from_env() -> Self {
        let api_base = std::env::var("ACP_GITHUB_API_BASE")
            .ok()
            .unwrap_or_else(|| "https://api.github.com".to_string());
        Self {
            enabled: env_enabled("ACP_ENABLE_GITHUB_PR_OUTPUT"),
            api_base: api_base.trim_end_matches('/').to_string(),
            allowed_repositories: env_list("ACP_GITHUB_REPOSITORY_ALLOWLIST", &[]),
            token: secret_from_named_env("ACP_GITHUB_TOKEN_ENV"),
        }
    }

    pub fn require_enabled(&self) -> Result<(), String> {
        if !self.enabled {
            return Err("GitHub PR output requires ACP_ENABLE_GITHUB_PR_OUTPUT=1".to_string());
        }
        if self.token.is_none() {
            return Err(
                "GitHub PR output requires ACP_GITHUB_TOKEN_ENV to reference a populated token"
                    .to_string(),
            );
        }
        if self.api_base != "https://api.github.com" {
            return Err(
                "GitHub PR output requires the exact https://api.github.com API origin".to_string(),
            );
        }
        Ok(())
    }

    pub fn require_repository(&self, repository: &GitHubRepository) -> Result<(), String> {
        self.require_enabled()?;
        if repository.host != "github.com" {
            return Err("GitHub PR output currently supports github.com remotes only".to_string());
        }
        let identity = format!("{}/{}", repository.owner, repository.repository);
        if !self.allowed_repositories.contains(&identity) {
            return Err(format!(
                "GitHub repository is not explicitly allowlisted: {identity}"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GitHubPullRequestRequest {
    pub repository: GitHubRepository,
    pub head_branch: String,
    pub base_branch: String,
    pub title: String,
    pub body: String,
    pub expected_head_sha: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GitHubPullRequestOutput {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub draft: bool,
    pub reused: bool,
    pub repository: String,
    pub base_branch: String,
    pub head_branch: String,
    pub head_sha: Option<String>,
}

pub fn parse_github_repository_url(url: &str) -> Result<GitHubRepository, String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "GitHub remote must use HTTPS".to_string())?;
    let (authority, path) = rest
        .split_once('/')
        .ok_or_else(|| "GitHub remote path is missing".to_string())?;
    if authority.is_empty() || authority.contains('@') || authority.contains(':') {
        return Err("GitHub remote authority is invalid".to_string());
    }
    let segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    if segments.len() != 2 {
        return Err("GitHub remote must identify exactly owner/repository".to_string());
    }
    let owner = segments[0];
    let repository = segments[1].strip_suffix(".git").unwrap_or(segments[1]);
    if !authority::valid_github_slug(owner) || !authority::valid_github_slug(repository) {
        return Err("GitHub owner or repository is invalid".to_string());
    }
    Ok(GitHubRepository {
        host: authority.to_ascii_lowercase(),
        owner: owner.to_string(),
        repository: repository.to_string(),
    })
}

pub fn github_repository_for_remote(
    config: &TargetRepoOutputConfig,
    workspace: &Path,
    remote: &str,
) -> Result<GitHubRepository, String> {
    validate_remote(config, workspace, remote)?;
    let url = run_git(config, workspace, &["remote", "get-url", remote])?
        .stdout
        .trim()
        .to_string();
    parse_github_repository_url(&url)
}

pub async fn create_or_reuse_github_pull_request(
    config: &GitHubPullRequestConfig,
    request: &GitHubPullRequestRequest,
) -> Result<GitHubPullRequestOutput, String> {
    create_or_reconcile_github_pull_request(config, request, true).await
}

pub async fn reconcile_existing_github_pull_request(
    config: &GitHubPullRequestConfig,
    request: &GitHubPullRequestRequest,
) -> Result<GitHubPullRequestOutput, String> {
    create_or_reconcile_github_pull_request(config, request, false).await
}

async fn create_or_reconcile_github_pull_request(
    config: &GitHubPullRequestConfig,
    request: &GitHubPullRequestRequest,
    allow_create: bool,
) -> Result<GitHubPullRequestOutput, String> {
    config.require_repository(&request.repository)?;
    authority::validate_github_pr_request(request)?;

    let token = config.token.as_deref().unwrap_or_default();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("agent-control-plane")
        .build()
        .map_err(|error| error.to_string())?;
    let endpoint = format!(
        "{}/repos/{}/{}/pulls",
        config.api_base, request.repository.owner, request.repository.repository
    );
    let head = format!("{}:{}", request.repository.owner, request.head_branch);
    let existing = client
        .get(&endpoint)
        .bearer_auth(token)
        .query(&[
            ("state", "open"),
            ("head", head.as_str()),
            ("base", request.base_branch.as_str()),
        ])
        .send()
        .await
        .map_err(|error| format!("github_pr_lookup_failed_known: {error}"))?;
    if !existing.status().is_success() {
        if matches!(existing.status().as_u16(), 403 | 429) {
            return Err(format!(
                "github_pr_rate_limited_or_forbidden_known: status {}",
                existing.status()
            ));
        }
        return Err(format!(
            "github_pr_lookup_failed_known: status {}",
            existing.status()
        ));
    }
    let existing_body: Value = existing
        .json()
        .await
        .map_err(|error| format!("github_pr_lookup_failed_known: invalid response: {error}"))?;
    if let Some(pull_request) = existing_body.as_array().and_then(|items| items.first()) {
        return github_pull_request_output(pull_request, request, true)
            .map_err(|error| format!("github_pr_lookup_failed_known: {error}"));
    }
    require_missing_pr_create_authority(allow_create)?;

    let created = client
        .post(&endpoint)
        .bearer_auth(token)
        .json(&github_draft_pull_request_payload(request))
        .send()
        .await
        .map_err(|error| format!("github_pr_create_outcome_unknown: {error}"))?;
    if !created.status().is_success() {
        if matches!(created.status().as_u16(), 403 | 429) {
            return Err(format!(
                "github_pr_rate_limited_or_forbidden_known: status {}",
                created.status()
            ));
        }
        return Err(format!(
            "github_pr_create_failed_known: status {}",
            created.status()
        ));
    }
    let created_body: Value = created
        .json()
        .await
        .map_err(|error| format!("github_pr_create_outcome_unknown: invalid response: {error}"))?;
    github_pull_request_output(&created_body, request, false)
        .map_err(|error| format!("github_pr_create_outcome_unknown: {error}"))
}

fn require_missing_pr_create_authority(allow_create: bool) -> Result<(), String> {
    if allow_create {
        Ok(())
    } else {
        Err(
            "github_pr_create_outcome_unknown: reconciliation found no matching Draft PR; automatic replay is forbidden"
                .to_string(),
        )
    }
}

fn github_draft_pull_request_payload(request: &GitHubPullRequestRequest) -> Value {
    serde_json::json!({
        "title": request.title,
        "head": request.head_branch,
        "base": request.base_branch,
        "body": request.body,
        "draft": true,
    })
}

fn github_pull_request_output(
    value: &Value,
    request: &GitHubPullRequestRequest,
    reused: bool,
) -> Result<GitHubPullRequestOutput, String> {
    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .ok_or_else(|| "GitHub PR response missing number".to_string())?;
    let url = value
        .get("html_url")
        .and_then(Value::as_str)
        .ok_or_else(|| "GitHub PR response missing html_url".to_string())?;
    let state = value.get("state").and_then(Value::as_str).unwrap_or("open");
    let draft = value.get("draft").and_then(Value::as_bool).unwrap_or(false);
    if state != "open" || !draft {
        return Err("GitHub PR response is not an open Draft PR".to_string());
    }
    let expected_url_prefix = format!(
        "https://github.com/{}/{}/pull/",
        request.repository.owner, request.repository.repository
    );
    if !url.starts_with(&expected_url_prefix) {
        return Err("GitHub PR response URL does not match the admitted repository".to_string());
    }
    let expected_full_name = format!(
        "{}/{}",
        request.repository.owner, request.repository.repository
    );
    if value.pointer("/base/ref").and_then(Value::as_str) != Some(request.base_branch.as_str())
        || value.pointer("/head/ref").and_then(Value::as_str) != Some(request.head_branch.as_str())
        || value
            .pointer("/head/repo/full_name")
            .and_then(Value::as_str)
            != Some(expected_full_name.as_str())
    {
        return Err("GitHub PR response base/head/repository binding changed".to_string());
    }
    if let Some(expected_head_sha) = request.expected_head_sha.as_deref() {
        if value.pointer("/head/sha").and_then(Value::as_str) != Some(expected_head_sha) {
            return Err("GitHub PR response head commit changed".to_string());
        }
    }
    Ok(GitHubPullRequestOutput {
        number,
        url: redact_sensitive_patterns(url),
        state: state.to_string(),
        draft,
        reused,
        repository: expected_full_name,
        base_branch: request.base_branch.clone(),
        head_branch: request.head_branch.clone(),
        head_sha: value
            .pointer("/head/sha")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

pub fn prepare_git_worktree(
    config: &TargetRepoOutputConfig,
    target_repo_path: &Path,
    workspace_path: &Path,
    source_revision: &str,
) -> Result<GitWorkspaceInfo, String> {
    config.require_enabled()?;
    authority::validate_source_revision(source_revision)?;
    let target_repo = canonical_existing_dir(target_repo_path, "target_repo_path")?;
    ensure_absolute_clean(workspace_path, "workspace_path")?;
    if workspace_path.exists() {
        return Err("workspace_path already exists".to_string());
    }

    // Validate every target-side identity before creating even the workspace
    // parent. A target path can be repointed between receipt publication and
    // recovery; pre-effect overlap must fail before any root creation.
    let git_root = run_git(config, &target_repo, &["rev-parse", "--show-toplevel"])?;
    let git_root = canonical_existing_dir(Path::new(git_root.stdout.trim()), "git_root")?;
    if git_root != target_repo {
        return Err("target_repo_path must be the git worktree root".to_string());
    }
    let revision_arg = format!("{source_revision}^{{commit}}");
    let source = run_git(
        config,
        &target_repo,
        &["rev-parse", "--verify", "--end-of-options", &revision_arg],
    )?
    .stdout
    .trim()
    .to_string();
    let (default_branch, origin_remote, default_branch_sha) =
        inspect_git_source_identity(config, &target_repo)?;

    let workspace_parent = workspace_path
        .parent()
        .ok_or_else(|| "workspace_path has no parent".to_string())?;
    let planned_parent = canonicalize_with_missing_tail(workspace_parent, "workspace_parent")?;
    let planned_workspace = planned_parent.join(
        workspace_path
            .file_name()
            .ok_or_else(|| "workspace_path has no file name".to_string())?,
    );
    if paths_overlap(&planned_parent, &target_repo)
        || paths_overlap(&planned_workspace, &target_repo)
    {
        return Err("git worktree must stay outside target repository".to_string());
    }

    std::fs::create_dir_all(&planned_parent).map_err(|error| error.to_string())?;
    let workspace_parent = canonical_existing_dir(&planned_parent, "workspace_parent")?;
    let planned_workspace = workspace_parent.join(
        workspace_path
            .file_name()
            .ok_or_else(|| "workspace_path has no file name".to_string())?,
    );
    if paths_overlap(&workspace_parent, &target_repo)
        || paths_overlap(&planned_workspace, &target_repo)
    {
        return Err("git worktree must stay outside target repository".to_string());
    }
    run_git(
        config,
        &target_repo,
        &[
            "worktree",
            "add",
            "--detach",
            planned_workspace
                .to_str()
                .ok_or_else(|| "workspace_path is not valid UTF-8".to_string())?,
            &source,
        ],
    )
    .map_err(|error| format!("{GIT_WORKTREE_ADD_OUTCOME_UNKNOWN}: {error}"))?;
    let canonical_workspace = canonical_existing_dir(&planned_workspace, "workspace_path")
        .map_err(|error| format!("{GIT_WORKTREE_ADD_OUTCOME_UNKNOWN}: {error}"))?;

    let origin_remote_fingerprint = hex::encode(Sha256::digest(origin_remote.as_bytes()));
    Ok(GitWorkspaceInfo {
        schema_version: TARGET_REPO_OUTPUT_SCHEMA_VERSION.to_string(),
        workspace_path: canonical_workspace.to_string_lossy().into_owned(),
        source_revision: source,
        default_branch,
        origin_remote,
        origin_remote_fingerprint,
        default_branch_sha,
        workspace_mode: "git_worktree".to_string(),
    })
}

/// Inspect an existing app-owned worktree only when the configured target
/// repository still registers its exact canonical path and its checked-out
/// revision is the exact commit resolved from that target.  Recovery must not
/// infer ownership from a matching `HEAD` in an arbitrary directory: another
/// repository can contain the same commit object.
pub fn inspect_registered_git_worktree(
    config: &TargetRepoOutputConfig,
    target_repo_path: &Path,
    workspace_path: &Path,
    source_revision: &str,
) -> Result<GitWorkspaceInfo, String> {
    config.require_enabled()?;
    authority::validate_source_revision(source_revision)?;
    let target_repo = canonical_existing_dir(target_repo_path, "target_repo_path")?;
    ensure_absolute_clean(workspace_path, "workspace_path")?;
    let workspace = canonical_existing_dir(workspace_path, "workspace_path")?;
    if workspace != workspace_path {
        return Err("workspace_path canonical identity differs from its receipt".to_string());
    }
    if paths_overlap(&workspace, &target_repo) {
        return Err("git worktree must stay outside target repository".to_string());
    }

    let git_root = run_git(config, &target_repo, &["rev-parse", "--show-toplevel"])?;
    let git_root = canonical_existing_dir(Path::new(git_root.stdout.trim()), "git_root")?;
    if git_root != target_repo {
        return Err("target_repo_path must be the git worktree root".to_string());
    }
    if !git_worktree_is_registered(config, &target_repo, &workspace)? {
        return Err("workspace_path is not registered by target repository".to_string());
    }

    let workspace_root = run_git(config, &workspace, &["rev-parse", "--show-toplevel"])?;
    let workspace_root = canonical_existing_dir(
        Path::new(workspace_root.stdout.trim()),
        "workspace_git_root",
    )?;
    if workspace_root != workspace {
        return Err("workspace_path is not the registered worktree root".to_string());
    }

    let revision_arg = format!("{source_revision}^{{commit}}");
    let expected_source = run_git(
        config,
        &target_repo,
        &["rev-parse", "--verify", "--end-of-options", &revision_arg],
    )?
    .stdout
    .trim()
    .to_string();
    let actual_source = run_git(
        config,
        &workspace,
        &["rev-parse", "--verify", "--end-of-options", "HEAD^{commit}"],
    )?
    .stdout
    .trim()
    .to_string();
    if actual_source != expected_source {
        return Err("workspace_path source revision does not match target repository".to_string());
    }
    let (default_branch, origin_remote, default_branch_sha) =
        inspect_git_source_identity(config, &target_repo)?;

    let origin_remote_fingerprint = hex::encode(Sha256::digest(origin_remote.as_bytes()));
    Ok(GitWorkspaceInfo {
        schema_version: TARGET_REPO_OUTPUT_SCHEMA_VERSION.to_string(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        source_revision: expected_source,
        default_branch,
        origin_remote,
        origin_remote_fingerprint,
        default_branch_sha,
        workspace_mode: "git_worktree".to_string(),
    })
}

/// Capture the stable target identity used both at workspace admission and
/// immediately before Git output/recovery.
pub(crate) fn inspect_git_source_identity(
    config: &TargetRepoOutputConfig,
    target_repo: &Path,
) -> Result<(String, String, String), String> {
    let default_branch = run_git(config, target_repo, &["symbolic-ref", "--short", "HEAD"])?
        .stdout
        .trim()
        .to_string();
    let origin_remote = run_git(config, target_repo, &["remote", "get-url", "origin"])?
        .stdout
        .trim()
        .to_string();
    if origin_remote.is_empty() || contains_sensitive_patterns(&origin_remote) {
        return Err("target repository origin identity is unavailable or unsafe".to_string());
    }
    let default_branch_sha = run_git(
        config,
        target_repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("refs/heads/{default_branch}^{{commit}}"),
        ],
    )?
    .stdout
    .trim()
    .to_string();
    Ok((default_branch, origin_remote, default_branch_sha))
}

pub fn remove_git_worktree(
    config: &TargetRepoOutputConfig,
    target_repo_path: &Path,
    workspace_path: &Path,
) -> Result<(), String> {
    let target_repo = canonical_existing_dir(target_repo_path, "target_repo_path")?;
    ensure_absolute_clean(workspace_path, "workspace_path")?;
    run_git(
        config,
        &target_repo,
        &[
            "worktree",
            "remove",
            "--force",
            workspace_path
                .to_str()
                .ok_or_else(|| "workspace_path is not valid UTF-8".to_string())?,
        ],
    )?;
    Ok(())
}

/// Remove an exact app-owned worktree only when Git still registers it, then
/// prove both the registration and path are absent. This is intentionally more
/// conservative than a filesystem delete: a timed-out child can leave Git
/// metadata behind after the directory has already disappeared.
pub fn remove_git_worktree_and_verify_absent(
    config: &TargetRepoOutputConfig,
    target_repo_path: &Path,
    workspace_path: &Path,
) -> Result<(), String> {
    config.require_enabled()?;
    let target_repo = canonical_existing_dir(target_repo_path, "target_repo_path")?;
    ensure_absolute_clean(workspace_path, "workspace_path")?;

    if git_worktree_is_registered(config, &target_repo, workspace_path)? {
        remove_git_worktree(config, &target_repo, workspace_path)?;
    }
    if git_worktree_is_registered(config, &target_repo, workspace_path)? {
        return Err("workspace_path remains registered after git worktree removal".to_string());
    }
    match std::fs::symlink_metadata(workspace_path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err("workspace_path exists without a registered Git worktree".to_string()),
        Err(error) => Err(error.to_string()),
    }
}

fn git_worktree_is_registered(
    config: &TargetRepoOutputConfig,
    target_repo_path: &Path,
    workspace_path: &Path,
) -> Result<bool, String> {
    let worktrees = run_git(
        config,
        target_repo_path,
        &["worktree", "list", "--porcelain"],
    )?;
    git_worktree_is_registered_in_porcelain(
        &worktrees.stdout,
        worktrees.stdout_truncated,
        workspace_path,
    )
}

fn git_worktree_is_registered_in_porcelain(
    worktree_list: &str,
    truncated: bool,
    workspace_path: &Path,
) -> Result<bool, String> {
    if truncated {
        return Err("git worktree registration listing is truncated".to_string());
    }
    Ok(worktree_list.lines().any(|line| {
        line.strip_prefix("worktree ")
            .is_some_and(|registered_path| Path::new(registered_path) == workspace_path)
    }))
}

pub fn stage_and_build_patch(
    config: &TargetRepoOutputConfig,
    workspace_path: &Path,
) -> Result<String, String> {
    config.require_enabled()?;
    inspect_git_patch(config, workspace_path)
}

pub fn inspect_git_patch(
    config: &TargetRepoOutputConfig,
    workspace_path: &Path,
) -> Result<String, String> {
    let workspace = canonical_existing_dir(workspace_path, "workspace_path")?;
    reject_unsafe_git_attributes(config, &workspace)?;
    run_git(config, &workspace, &["add", "-A"])?;
    validate_staged_file_content(config, &workspace)?;
    let output = run_git(
        config,
        &workspace,
        &[
            "diff",
            "--cached",
            "--binary",
            "--full-index",
            "--no-ext-diff",
        ],
    )?;
    validate_patch_output(output)
}

fn validate_patch_output(output: GitOutput) -> Result<String, String> {
    if output.stdout_truncated {
        return Err(format!(
            "patch exceeds output limit of {MAX_GIT_OUTPUT_BYTES} bytes"
        ));
    }
    if output.stdout.is_empty() {
        return Err("no git changes available for output".to_string());
    }
    if output.stdout.len() > MAX_PATCH_BYTES {
        return Err(format!(
            "patch exceeds maximum size of {MAX_PATCH_BYTES} bytes"
        ));
    }
    if contains_sensitive_patterns(&output.stdout) {
        return Err("patch contains sensitive content".to_string());
    }
    Ok(output.stdout)
}

/// Build the exact output patch through an isolated temporary index. This observes
/// tracked and untracked workspace changes without changing the worktree's real index,
/// so verification commands see the repository state the caller supplied.
pub(crate) fn inspect_git_patch_read_only(
    config: &TargetRepoOutputConfig,
    workspace_path: &Path,
) -> Result<String, String> {
    let workspace = canonical_existing_dir(workspace_path, "workspace_path")?;
    reject_unsafe_git_attributes(config, &workspace)?;
    let index_dir = tempfile::Builder::new()
        .prefix("acp-readonly-git-index-")
        .tempdir()
        .map_err(|error| format!("temporary git index unavailable: {error}"))?;
    let index_path = index_dir.path().join("index");
    run_git_with_index(config, &workspace, &["read-tree", "HEAD"], &index_path)?;
    run_git_with_index(config, &workspace, &["add", "-A"], &index_path)?;
    validate_staged_file_content_with_index(config, &workspace, &index_path)?;
    let output = run_git_with_index(
        config,
        &workspace,
        &[
            "diff",
            "--cached",
            "--binary",
            "--full-index",
            "--no-ext-diff",
        ],
        &index_path,
    )?;
    validate_patch_output(output)
}

pub fn staged_changed_files(
    config: &TargetRepoOutputConfig,
    workspace_path: &Path,
) -> Result<GitChangeSet, String> {
    config.require_enabled()?;
    let workspace = canonical_existing_dir(workspace_path, "workspace_path")?;
    reject_unsafe_git_attributes(config, &workspace)?;
    run_git(config, &workspace, &["add", "-A"])?;
    let output = run_git(
        config,
        &workspace,
        &["diff", "--cached", "--name-status", "--no-renames", "-z"],
    )?;
    if output.stdout_truncated {
        return Err("git changed-file output exceeded limit".to_string());
    }
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();
    let fields: Vec<&str> = output
        .stdout
        .split('\0')
        .filter(|field| !field.is_empty())
        .collect();
    if !fields.len().is_multiple_of(2) {
        return Err("invalid git changed-file output".to_string());
    }
    for pair in fields.chunks_exact(2) {
        let status = pair[0];
        let path = authority::normalize_git_path(pair[1])?;
        if status != "D" {
            let metadata = std::fs::symlink_metadata(workspace.join(&path))
                .map_err(|error| format!("changed path metadata unavailable: {error}"))?;
            if metadata.file_type().is_symlink() {
                return Err(format!("changed symlink is not allowed: {path}"));
            }
            if metadata.file_type().is_file() {
                validate_changed_file_content(&workspace, &path)?;
            }
        }
        match status {
            "A" => added.push(path),
            "M" | "T" => modified.push(path),
            "D" => deleted.push(path),
            other => return Err(format!("unsupported git change status: {other}")),
        }
    }
    let mut changed_files = Vec::new();
    changed_files.extend(added.iter().map(|path| format!("+{path}")));
    changed_files.extend(modified.iter().map(|path| format!("~{path}")));
    changed_files.extend(deleted.iter().map(|path| format!("-{path}")));
    Ok(GitChangeSet {
        added,
        modified,
        deleted,
        changed_files,
    })
}

pub fn export_patch(
    config: &TargetRepoOutputConfig,
    workspace_path: &Path,
    source_revision: &str,
) -> Result<PatchExport, String> {
    let patch = stage_and_build_patch(config, workspace_path)?;
    Ok(PatchExport {
        schema_version: TARGET_REPO_OUTPUT_SCHEMA_VERSION.to_string(),
        source_revision: source_revision.to_string(),
        patch_hash: patch_hash(&patch),
        patch,
    })
}

pub fn push_approved_branch(
    config: &TargetRepoOutputConfig,
    request: BranchPublishRequest,
) -> Result<BranchPublishOutput, String> {
    config.require_enabled()?;
    validate_branch_name(config, &request.workspace_path, &request.branch_name)?;
    validate_remote(config, &request.workspace_path, &request.remote)?;
    authority::validate_publish_text(&request.commit_message, &request.pr_title, &request.pr_body)?;

    let target_repo = canonical_existing_dir(&request.target_repo_path, "target_repo_path")?;
    let workspace = canonical_existing_dir(&request.workspace_path, "workspace_path")?;
    if workspace.starts_with(&target_repo) {
        return Err("workspace must be outside target repository".to_string());
    }
    let current_source = run_git(config, &workspace, &["rev-parse", "HEAD"])?
        .stdout
        .trim()
        .to_string();
    if current_source != request.source_revision {
        return reuse_published_branch(config, &request, &workspace, current_source);
    }
    let patch = stage_and_build_patch(config, &workspace)?;
    let actual_patch_hash = patch_hash(&patch);
    if actual_patch_hash != request.expected_patch_hash {
        return Err(format!(
            "approved patch hash changed: expected={} actual={actual_patch_hash}",
            request.expected_patch_hash
        ));
    }

    run_git(config, &workspace, &["switch", "-c", &request.branch_name])?;
    run_git_with_identity(
        config,
        &workspace,
        &["commit", "--no-verify", "-m", &request.commit_message],
    )?;
    let commit_sha = run_git(config, &workspace, &["rev-parse", "HEAD"])?
        .stdout
        .trim()
        .to_string();
    let refspec = format!(
        "refs/heads/{}:refs/heads/{}",
        request.branch_name, request.branch_name
    );
    run_git(
        config,
        &workspace,
        &[
            "push",
            "--porcelain",
            "--no-verify",
            "--set-upstream",
            &request.remote,
            &refspec,
        ],
    )
    .map_err(|error| format!("branch_push_outcome_unknown: {error}"))?;

    Ok(BranchPublishOutput {
        schema_version: TARGET_REPO_OUTPUT_SCHEMA_VERSION.to_string(),
        source_revision: request.source_revision,
        branch_name: request.branch_name,
        remote: request.remote,
        commit_sha,
        patch_hash: actual_patch_hash,
        pr_title: redact_sensitive_patterns(&request.pr_title),
        pr_body: redact_sensitive_patterns(&request.pr_body),
    })
}

/// Read the exact checked-out commit through the existing confined git owner.
/// This is used by Product Golden Path verification authority checks; it performs
/// no mutation and never enables network access or credential helpers.
pub(crate) fn current_workspace_revision(
    config: &TargetRepoOutputConfig,
    workspace_path: &Path,
) -> Result<String, String> {
    let workspace = canonical_existing_dir(workspace_path, "workspace_path")?;
    Ok(run_git(config, &workspace, &["rev-parse", "HEAD"])?
        .stdout
        .trim()
        .to_string())
}

fn reuse_published_branch(
    config: &TargetRepoOutputConfig,
    request: &BranchPublishRequest,
    workspace: &Path,
    commit_sha: String,
) -> Result<BranchPublishOutput, String> {
    let branch = run_git(config, workspace, &["symbolic-ref", "--short", "HEAD"])?
        .stdout
        .trim()
        .to_string();
    if branch != request.branch_name {
        return Err(format!(
            "workspace source revision changed: expected={} actual={commit_sha}",
            request.source_revision
        ));
    }
    if !run_git(config, workspace, &["status", "--porcelain"])?
        .stdout
        .trim()
        .is_empty()
    {
        return Err("published workspace has uncommitted changes".to_string());
    }
    let parent = run_git(config, workspace, &["rev-parse", "HEAD^"])?
        .stdout
        .trim()
        .to_string();
    if parent != request.source_revision {
        return Err("published branch parent does not match source revision".to_string());
    }
    let range = format!("{}..HEAD", request.source_revision);
    let patch = run_git(
        config,
        workspace,
        &["diff", "--binary", "--full-index", "--no-ext-diff", &range],
    )?;
    if patch.stdout_truncated || patch.stdout.len() > MAX_PATCH_BYTES {
        return Err("published patch exceeds output limit".to_string());
    }
    if contains_sensitive_patterns(&patch.stdout) {
        return Err("published patch contains sensitive content".to_string());
    }
    let actual_patch_hash = patch_hash(&patch.stdout);
    if actual_patch_hash != request.expected_patch_hash {
        return Err(format!(
            "published patch hash changed: expected={} actual={actual_patch_hash}",
            request.expected_patch_hash
        ));
    }
    let refspec = format!(
        "refs/heads/{}:refs/heads/{}",
        request.branch_name, request.branch_name
    );
    run_git(
        config,
        workspace,
        &[
            "push",
            "--porcelain",
            "--no-verify",
            "--set-upstream",
            &request.remote,
            &refspec,
        ],
    )
    .map_err(|error| format!("branch_push_outcome_unknown: {error}"))?;
    let upstream_name = run_git(
        config,
        workspace,
        &[
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ],
    )?
    .stdout
    .trim()
    .to_string();
    let expected_upstream = format!("{}/{}", request.remote, request.branch_name);
    if upstream_name != expected_upstream {
        return Err(format!(
            "published branch upstream changed: expected={expected_upstream} actual={upstream_name}"
        ));
    }
    let upstream = run_git(config, workspace, &["rev-parse", "@{upstream}"])?
        .stdout
        .trim()
        .to_string();
    if upstream != commit_sha {
        return Err("published branch does not match its upstream".to_string());
    }
    Ok(BranchPublishOutput {
        schema_version: TARGET_REPO_OUTPUT_SCHEMA_VERSION.to_string(),
        source_revision: request.source_revision.clone(),
        branch_name: request.branch_name.clone(),
        remote: request.remote.clone(),
        commit_sha,
        patch_hash: actual_patch_hash,
        pr_title: redact_sensitive_patterns(&request.pr_title),
        pr_body: redact_sensitive_patterns(&request.pr_body),
    })
}

pub fn patch_hash(patch: &str) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(patch.as_bytes())))
}

fn validate_branch_name(
    config: &TargetRepoOutputConfig,
    workspace: &Path,
    branch_name: &str,
) -> Result<(), String> {
    authority::validate_branch_policy(branch_name)?;
    run_git(
        config,
        workspace,
        &["check-ref-format", "--branch", branch_name],
    )?;
    Ok(())
}

fn validate_staged_file_content(
    config: &TargetRepoOutputConfig,
    workspace: &Path,
) -> Result<(), String> {
    let output = run_git(
        config,
        workspace,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMT",
            "-z",
        ],
    )?;
    if output.stdout_truncated {
        return Err("git changed-file output exceeded limit".to_string());
    }
    for path in output.stdout.split('\0').filter(|path| !path.is_empty()) {
        let path = authority::normalize_git_path(path)?;
        validate_changed_file_content(workspace, &path)?;
    }
    Ok(())
}

fn validate_staged_file_content_with_index(
    config: &TargetRepoOutputConfig,
    workspace: &Path,
    index_path: &Path,
) -> Result<(), String> {
    let output = run_git_with_index(
        config,
        workspace,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMT",
            "-z",
        ],
        index_path,
    )?;
    if output.stdout_truncated {
        return Err("git changed-file output exceeded limit".to_string());
    }
    for path in output.stdout.split('\0').filter(|path| !path.is_empty()) {
        let path = authority::normalize_git_path(path)?;
        validate_changed_file_content(workspace, &path)?;
    }
    Ok(())
}

fn validate_changed_file_content(workspace: &Path, path: &str) -> Result<(), String> {
    let full_path = workspace.join(path);
    let metadata = std::fs::symlink_metadata(&full_path)
        .map_err(|error| format!("changed path metadata unavailable: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Err(format!("changed symlink is not allowed: {path}"));
    }
    if metadata.len() > MAX_CHANGED_FILE_BYTES {
        return Err(format!(
            "changed file exceeds maximum size of {MAX_CHANGED_FILE_BYTES} bytes: {path}"
        ));
    }
    let content =
        std::fs::read(&full_path).map_err(|error| format!("changed file unavailable: {error}"))?;
    if std::str::from_utf8(&content).is_err() || content.contains(&0) {
        return Err(format!("changed binary file is not allowed: {path}"));
    }
    Ok(())
}

fn reject_unsafe_git_attributes(
    config: &TargetRepoOutputConfig,
    workspace: &Path,
) -> Result<(), String> {
    let mut stack = vec![workspace.to_path_buf()];
    let mut visited = 0_usize;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_symlink() {
                return Err(format!(
                    "workspace symlink is not allowed for target output: {}",
                    entry.path().display()
                ));
            }
            let path = entry.path();
            if file_type.is_dir() {
                if entry.file_name() != ".git" {
                    stack.push(path);
                }
                continue;
            }
            if file_type.is_file() && entry.file_name() == ".gitattributes" {
                reject_filter_attributes(&path)?;
            }
            visited += 1;
            if visited > 20_000 {
                return Err("git attribute scan file limit exceeded".to_string());
            }
        }
    }
    let common_dir = run_git(config, workspace, &["rev-parse", "--git-common-dir"])?
        .stdout
        .trim()
        .to_string();
    let common_dir = if Path::new(&common_dir).is_absolute() {
        PathBuf::from(common_dir)
    } else {
        workspace.join(common_dir)
    };
    let info_attributes = common_dir.join("info").join("attributes");
    if info_attributes.exists() {
        reject_filter_attributes(&info_attributes)?;
    }
    Ok(())
}

fn reject_filter_attributes(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() > 1024 * 1024 {
        return Err(format!(
            "git attributes file exceeds limit: {}",
            path.display()
        ));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("git attributes must be UTF-8: {error}"))?;
    if content.lines().any(|line| {
        let active = line.split('#').next().unwrap_or("");
        active
            .split_whitespace()
            .any(|token| token.starts_with("filter=") || token == "filter")
    }) {
        return Err(format!(
            "git clean filters are not allowed for target output: {}",
            path.display()
        ));
    }
    Ok(())
}

fn validate_remote(
    config: &TargetRepoOutputConfig,
    workspace: &Path,
    remote: &str,
) -> Result<(), String> {
    authority::validate_remote_name(config, remote)?;
    let url = run_git(config, workspace, &["remote", "get-url", remote])?
        .stdout
        .trim()
        .to_string();
    authority::validate_remote_url_policy(config, &url)
}

fn remote_host(url: &str) -> Option<&str> {
    if let Some(rest) = url.strip_prefix("https://") {
        return rest.split(['/', ':']).next();
    }
    if let Some(rest) = url.strip_prefix("ssh://") {
        let authority = rest.split('/').next()?;
        return authority.rsplit('@').next()?.split(':').next();
    }
    if let Some((user_host, _)) = url.split_once(':') {
        if user_host.contains('@') && !user_host.contains('/') {
            return user_host.rsplit('@').next();
        }
    }
    None
}

fn env_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn env_list(name: &str, defaults: &[&str]) -> HashSet<String> {
    match std::env::var(name) {
        Ok(value) => value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        Err(_) => defaults.iter().map(|item| (*item).to_string()).collect(),
    }
}

fn target_repo_git_token() -> Option<String> {
    secret_from_named_env("ACP_TARGET_REPO_GIT_TOKEN_ENV")
}

fn secret_from_named_env(name: &str) -> Option<String> {
    let variable = std::env::var(name).ok()?;
    if variable.is_empty()
        || variable.len() > 128
        || !variable
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    std::env::var(variable)
        .ok()
        .filter(|token| !token.is_empty())
}

fn valid_git_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn canonical_existing_dir(path: &Path, field: &str) -> Result<PathBuf, String> {
    ensure_absolute_clean(path, field)?;
    let canonical =
        std::fs::canonicalize(path).map_err(|error| format!("{field} must exist: {error}"))?;
    if !canonical.is_dir() {
        return Err(format!("{field} must be a directory"));
    }
    Ok(canonical)
}

fn canonicalize_with_missing_tail(path: &Path, field: &str) -> Result<PathBuf, String> {
    ensure_absolute_clean(path, field)?;
    let mut cursor = path;
    let mut tail = Vec::new();
    while !cursor.exists() {
        let component = cursor
            .file_name()
            .ok_or_else(|| format!("{field} has no existing ancestor"))?;
        tail.push(component.to_os_string());
        cursor = cursor
            .parent()
            .ok_or_else(|| format!("{field} has no existing ancestor"))?;
    }
    let mut canonical = canonical_existing_dir(cursor, field)?;
    for component in tail.into_iter().rev() {
        canonical.push(component);
    }
    Ok(canonical)
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn ensure_absolute_clean(path: &Path, field: &str) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("{field} must be absolute"));
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(format!("{field} must not contain . or .. components"));
    }
    Ok(())
}

struct GitOutput {
    stdout: String,
    stdout_truncated: bool,
}

fn run_git(
    config: &TargetRepoOutputConfig,
    cwd: &Path,
    args: &[&str],
) -> Result<GitOutput, String> {
    run_git_inner(config, cwd, args, false, None)
}

fn run_git_with_identity(
    config: &TargetRepoOutputConfig,
    cwd: &Path,
    args: &[&str],
) -> Result<GitOutput, String> {
    run_git_inner(config, cwd, args, true, None)
}

fn run_git_with_index(
    config: &TargetRepoOutputConfig,
    cwd: &Path,
    args: &[&str],
    index_path: &Path,
) -> Result<GitOutput, String> {
    run_git_inner(config, cwd, args, false, Some(index_path))
}

fn run_git_inner(
    config: &TargetRepoOutputConfig,
    cwd: &Path,
    args: &[&str],
    with_identity: bool,
    index_path: Option<&Path>,
) -> Result<GitOutput, String> {
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string());
    let mut command = Command::new("git");
    command
        .current_dir(cwd)
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(["-c", "core.fsmonitor=false"])
        .args(["-c", "core.attributesFile=/dev/null"])
        .args(["-c", "credential.helper="])
        .args(["-c", "protocol.ext.allow=never"])
        .args(["-c", "http.followRedirects=initial"])
        .args(args)
        .env_clear()
        .env("PATH", path)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(index_path) = index_path {
        command.env("GIT_INDEX_FILE", index_path);
    }
    if let Some(token) = config.git_token.as_deref() {
        let credential = format!("{}:{token}", config.git_username);
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!(
                    "Authorization: Basic {}",
                    base64_encode(credential.as_bytes())
                ),
            );
    }
    if with_identity {
        command
            .env("GIT_AUTHOR_NAME", "Agent Control Plane")
            .env("GIT_AUTHOR_EMAIL", "agent-control-plane@example.invalid")
            .env("GIT_COMMITTER_NAME", "Agent Control Plane")
            .env("GIT_COMMITTER_EMAIL", "agent-control-plane@example.invalid");
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start git {}: {error}", args.join(" ")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "git stdout unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "git stderr unavailable".to_string())?;
    let stdout_reader = thread::spawn(move || read_capped(stdout, MAX_GIT_OUTPUT_BYTES));
    let stderr_reader = thread::spawn(move || read_capped(stderr, MAX_GIT_OUTPUT_BYTES));
    let deadline = Instant::now() + Duration::from_millis(config.timeout_ms);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!(
                    "git {} timed out after {}ms",
                    args.join(" "),
                    config.timeout_ms
                ));
            }
            Err(error) => return Err(format!("failed waiting for git: {error}")),
        }
    };
    let (stdout_bytes, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| "git stdout reader failed".to_string())?
        .map_err(|error| error.to_string())?;
    let (stderr_bytes, _) = stderr_reader
        .join()
        .map_err(|_| "git stderr reader failed".to_string())?
        .map_err(|error| error.to_string())?;
    let secrets = config.git_token.as_deref().into_iter().collect::<Vec<_>>();
    let stdout = redact_secrets(&String::from_utf8_lossy(&stdout_bytes), &secrets);
    let stderr = redact_sensitive_patterns(&redact_secrets(
        &String::from_utf8_lossy(&stderr_bytes),
        &secrets,
    ));
    if !status.success() {
        return Err(format!(
            "git {} failed ({}): {}",
            args.join(" "),
            status,
            stderr.trim()
        ));
    }
    Ok(GitOutput {
        stdout,
        stdout_truncated,
    })
}

fn read_capped(mut reader: impl Read, max_bytes: usize) -> std::io::Result<(Vec<u8>, bool)> {
    let mut kept = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = max_bytes.saturating_sub(kept.len());
        let keep = remaining.min(read);
        kept.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }
    Ok((kept, truncated))
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(a >> 2) as usize] as char);
        output.push(ALPHABET[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(c & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
    }

    #[test]
    fn read_only_patch_snapshot_preserves_real_git_index() {
        let repo = tempfile::tempdir().unwrap();
        test_git(repo.path(), &["init", "-b", "main"]);
        test_git(
            repo.path(),
            &["config", "user.email", "test@example.invalid"],
        );
        test_git(repo.path(), &["config", "user.name", "Test"]);
        std::fs::write(repo.path().join("tracked.txt"), "base\n").unwrap();
        test_git(repo.path(), &["add", "tracked.txt"]);
        test_git(repo.path(), &["commit", "-m", "base"]);
        std::fs::write(repo.path().join("tracked.txt"), "changed\n").unwrap();
        std::fs::write(repo.path().join("untracked.txt"), "new\n").unwrap();
        test_git(repo.path(), &["add", "tracked.txt"]);

        let index_path = repo.path().join(".git/index");
        let index_before = std::fs::read(&index_path).unwrap();
        let status_before = run_git(
            &TargetRepoOutputConfig::for_test(false, false),
            repo.path(),
            &["status", "--porcelain=v1"],
        )
        .unwrap()
        .stdout;
        let patch = inspect_git_patch_read_only(
            &TargetRepoOutputConfig::for_test(false, false),
            repo.path(),
        )
        .unwrap();
        assert!(patch.contains("tracked.txt"));
        assert!(patch.contains("untracked.txt"));
        assert_eq!(std::fs::read(index_path).unwrap(), index_before);
        assert_eq!(
            run_git(
                &TargetRepoOutputConfig::for_test(false, false),
                repo.path(),
                &["status", "--porcelain=v1"],
            )
            .unwrap()
            .stdout,
            status_before
        );
    }

    #[test]
    fn remote_host_supports_https_ssh_and_scp_shapes() {
        assert_eq!(
            remote_host("https://github.com/org/repo.git"),
            Some("github.com")
        );
        assert_eq!(
            remote_host("ssh://git@github.com/org/repo.git"),
            Some("github.com")
        );
        assert_eq!(
            remote_host("git@github.com:org/repo.git"),
            Some("github.com")
        );
        assert_eq!(remote_host("ext::sh -c bad"), None);
    }

    #[test]
    fn basic_auth_encoding_matches_rfc4648_example() {
        assert_eq!(
            base64_encode(b"Aladdin:open sesame"),
            "QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
    }

    #[test]
    fn truncated_worktree_registration_listing_fails_closed() {
        let workspace = Path::new("/tmp/receipt-worktree");
        let error = git_worktree_is_registered_in_porcelain(
            "worktree /tmp/other-worktree\nHEAD deadbeef\n",
            true,
            workspace,
        )
        .expect_err("a truncated listing cannot prove registration absence");
        assert_eq!(error, "git worktree registration listing is truncated");
    }

    fn draft_request() -> GitHubPullRequestRequest {
        GitHubPullRequestRequest {
            repository: GitHubRepository {
                host: "github.com".to_string(),
                owner: "acme".to_string(),
                repository: "widgets".to_string(),
            },
            head_branch: "acp/product-1".to_string(),
            base_branch: "main".to_string(),
            title: "Bounded change".to_string(),
            body: "Approved artifact".to_string(),
            expected_head_sha: Some("a".repeat(40)),
        }
    }

    #[test]
    fn github_create_payload_is_always_draft() {
        let payload = github_draft_pull_request_payload(&draft_request());
        assert_eq!(payload["draft"], true);
        assert_eq!(payload["head"], "acp/product-1");
        assert_eq!(payload["base"], "main");
    }

    #[test]
    fn outcome_unknown_reconciliation_cannot_authorize_another_pr_post() {
        assert!(require_missing_pr_create_authority(true).is_ok());
        let error = require_missing_pr_create_authority(false).unwrap_err();
        assert!(error.contains("automatic replay is forbidden"));
    }

    #[test]
    fn github_pr_config_rejects_noncanonical_api_origin() {
        let config = GitHubPullRequestConfig {
            enabled: true,
            api_base: "https://github.example.invalid/api/v3".to_string(),
            allowed_repositories: HashSet::from(["acme/widgets".to_string()]),
            token: Some("test-only-token".to_string()),
        };
        assert_eq!(
            config.require_enabled().unwrap_err(),
            "GitHub PR output requires the exact https://api.github.com API origin"
        );
    }

    #[test]
    fn github_pr_config_requires_exact_repository_admission() {
        let config = GitHubPullRequestConfig {
            enabled: true,
            api_base: "https://api.github.com".to_string(),
            allowed_repositories: HashSet::from(["acme/widgets".to_string()]),
            token: Some("test-only-token".to_string()),
        };
        config
            .require_repository(&draft_request().repository)
            .unwrap();
        let mut unadmitted = draft_request().repository;
        unadmitted.repository = "other".to_string();
        assert!(config
            .require_repository(&unadmitted)
            .unwrap_err()
            .contains("not explicitly allowlisted"));
    }

    #[test]
    fn github_response_must_be_open_draft_in_admitted_repository() {
        let request = draft_request();
        let valid = serde_json::json!({
            "number": 7,
            "html_url": "https://github.com/acme/widgets/pull/7",
            "state": "open",
            "draft": true,
            "base": {"ref": "main"},
            "head": {
                "ref": "acp/product-1",
                "sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "repo": {"full_name": "acme/widgets"}
            },
        });
        let output = github_pull_request_output(&valid, &request, false).unwrap();
        assert_eq!(output.number, 7);
        assert!(output.draft);
        assert_eq!(output.repository, "acme/widgets");
        assert_eq!(output.base_branch, "main");
        assert_eq!(output.head_branch, "acp/product-1");
        assert_eq!(
            output.head_sha.as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );

        let mut not_draft = valid.clone();
        not_draft["draft"] = serde_json::json!(false);
        assert!(github_pull_request_output(&not_draft, &request, true).is_err());

        let mut wrong_repo = valid.clone();
        wrong_repo["html_url"] = serde_json::json!("https://github.com/acme/other/pull/7");
        assert!(github_pull_request_output(&wrong_repo, &request, false).is_err());

        let mut wrong_base = valid.clone();
        wrong_base["base"]["ref"] = serde_json::json!("release");
        assert!(github_pull_request_output(&wrong_base, &request, false).is_err());

        let mut wrong_head = valid.clone();
        wrong_head["head"]["ref"] = serde_json::json!("acp/other");
        assert!(github_pull_request_output(&wrong_head, &request, false).is_err());

        let mut wrong_sha = valid;
        wrong_sha["head"]["sha"] = serde_json::json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        assert!(github_pull_request_output(&wrong_sha, &request, false).is_err());
    }
}
