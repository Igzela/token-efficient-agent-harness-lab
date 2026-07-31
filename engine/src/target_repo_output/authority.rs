use std::path::{Component, Path};

use crate::provider::redaction::contains_sensitive_patterns;

use super::{GitHubPullRequestRequest, TargetRepoOutputConfig};

pub(super) fn require_target_output_enabled(config: &TargetRepoOutputConfig) -> Result<(), String> {
    if config.kill_switch {
        return Err("target repo output kill switch is active".to_string());
    }
    if !config.enabled {
        return Err("target repo output requires ACP_ENABLE_TARGET_REPO_OUTPUT=1".to_string());
    }
    Ok(())
}

pub(super) fn validate_source_revision(source_revision: &str) -> Result<(), String> {
    if source_revision.is_empty()
        || source_revision.len() > 200
        || source_revision.starts_with('-')
        || source_revision.chars().any(char::is_whitespace)
        || source_revision.chars().any(char::is_control)
    {
        return Err("invalid source revision".to_string());
    }
    Ok(())
}

pub(super) fn validate_branch_policy(branch_name: &str) -> Result<(), String> {
    if !branch_name.starts_with("acp/") || branch_name.len() <= 4 {
        return Err("output branch must use the acp/ prefix".to_string());
    }
    if branch_name.len() > 200
        || branch_name.contains("..")
        || branch_name.chars().any(char::is_whitespace)
    {
        return Err("invalid output branch name".to_string());
    }
    Ok(())
}

pub(super) fn validate_remote_name(
    config: &TargetRepoOutputConfig,
    remote: &str,
) -> Result<(), String> {
    if remote.is_empty()
        || remote.len() > 128
        || remote.starts_with('-')
        || !remote.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err("invalid git remote name".to_string());
    }
    if !config.allowed_remotes.contains(remote) {
        return Err(format!("git remote is not allowlisted: {remote}"));
    }
    Ok(())
}

pub(super) fn validate_remote_url_policy(
    config: &TargetRepoOutputConfig,
    url: &str,
) -> Result<(), String> {
    if url.starts_with("ext::") || url.contains('\n') || url.contains('\r') {
        return Err("unsafe git remote URL".to_string());
    }
    let path = Path::new(url);
    if path.is_absolute() {
        if config.allow_local_remote {
            return Ok(());
        }
        return Err("local git remotes require ACP_TARGET_REPO_ALLOW_LOCAL_REMOTE=1".to_string());
    }
    if !url.starts_with("https://") {
        return Err("network git remotes must use HTTPS".to_string());
    }
    let authority = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    if authority.contains('@') {
        return Err("git remote URL must not contain embedded credentials".to_string());
    }
    let host = super::remote_host(url).ok_or_else(|| "unsupported git remote URL".to_string())?;
    if !config.allowed_remote_hosts.contains(host) {
        return Err(format!("git remote host is not allowlisted: {host}"));
    }
    if config.git_token.is_none() {
        return Err(
            "HTTPS git push requires ACP_TARGET_REPO_GIT_TOKEN_ENV to reference a populated token"
                .to_string(),
        );
    }
    Ok(())
}

pub(super) fn validate_publish_text(
    commit_message: &str,
    pr_title: &str,
    pr_body: &str,
) -> Result<(), String> {
    for (field, value) in [
        ("commit_message", commit_message),
        ("pr_title", pr_title),
        ("pr_body", pr_body),
    ] {
        if value.trim().is_empty() {
            return Err(format!("{field} must not be empty"));
        }
        if contains_sensitive_patterns(value) {
            return Err(format!("{field} contains sensitive content"));
        }
    }
    if commit_message.len() > 500 || commit_message.chars().any(|character| character == '\0') {
        return Err("commit_message exceeds safety limits".to_string());
    }
    if pr_title.len() > 200
        || pr_title
            .chars()
            .any(|character| matches!(character, '\0' | '\n' | '\r'))
    {
        return Err("pr_title exceeds safety limits".to_string());
    }
    if pr_body.len() > 64 * 1024 || pr_body.contains('\0') {
        return Err("pr_body exceeds safety limits".to_string());
    }
    Ok(())
}

pub(super) fn validate_github_pr_request(request: &GitHubPullRequestRequest) -> Result<(), String> {
    if request.repository.host != "github.com" {
        return Err("GitHub PR output currently supports github.com remotes only".to_string());
    }
    for (field, value, max_len) in [
        ("head_branch", request.head_branch.as_str(), 200_usize),
        ("base_branch", request.base_branch.as_str(), 200_usize),
        ("title", request.title.as_str(), 200_usize),
        ("body", request.body.as_str(), 64 * 1024_usize),
    ] {
        if value.trim().is_empty() || value.len() > max_len || contains_sensitive_patterns(value) {
            return Err(format!("{field} exceeds safety limits"));
        }
    }
    for (field, sha) in [
        ("expected_head_sha", request.expected_head_sha.as_deref()),
        ("expected_base_sha", request.expected_base_sha.as_deref()),
    ] {
        if sha
            .is_some_and(|sha| sha.len() != 40 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(format!("{field} is invalid"));
        }
    }
    Ok(())
}

pub(super) fn valid_github_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

pub(super) fn normalize_git_path(path: &str) -> Result<String, String> {
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path.contains('\\')
        || path.contains('\n')
        || path.contains('\r')
    {
        return Err("git changed path is not normalized".to_string());
    }
    if contains_sensitive_patterns(path) {
        return Err("git changed path contains sensitive content".to_string());
    }
    Ok(path.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn config() -> TargetRepoOutputConfig {
        TargetRepoOutputConfig {
            enabled: true,
            kill_switch: false,
            timeout_ms: 10_000,
            allowed_remotes: HashSet::from(["origin".to_string()]),
            allowed_remote_hosts: HashSet::from(["github.com".to_string()]),
            allow_local_remote: false,
            git_username: "x-access-token".to_string(),
            git_token: Some("token".to_string()),
        }
    }

    #[test]
    fn target_output_gate_fails_closed() {
        let mut cfg = config();
        cfg.enabled = false;
        assert!(require_target_output_enabled(&cfg)
            .unwrap_err()
            .contains("ACP_ENABLE"));

        cfg.enabled = true;
        cfg.kill_switch = true;
        assert!(require_target_output_enabled(&cfg)
            .unwrap_err()
            .contains("kill switch"));
    }

    #[test]
    fn branch_remote_and_path_policy_reject_unsafe_shapes() {
        assert!(validate_branch_policy("acp/safe").is_ok());
        assert!(validate_branch_policy("main").is_err());
        assert!(validate_remote_name(&config(), "origin").is_ok());
        assert!(validate_remote_name(&config(), "--mirror").is_err());
        assert!(normalize_git_path("src/lib.rs").is_ok());
        assert!(normalize_git_path("../secret").is_err());
    }

    #[test]
    fn remote_url_policy_requires_https_allowlist_and_token() {
        let cfg = config();
        assert!(validate_remote_url_policy(&cfg, "https://github.com/acme/widgets.git").is_ok());
        assert!(validate_remote_url_policy(&cfg, "git@github.com:acme/widgets.git").is_err());
        assert!(
            validate_remote_url_policy(&cfg, "https://user@github.com/acme/widgets.git").is_err()
        );
    }

    #[test]
    fn publish_text_policy_rejects_secret_and_control_text() {
        assert!(validate_publish_text("commit", "title", "body").is_ok());
        assert!(validate_publish_text("commit", "bad\ntitle", "body").is_err());
        assert!(
            validate_publish_text("commit", "title", "api_key=sk-abcdefghijklmnopqrstuvwxyz")
                .is_err()
        );
    }
}
