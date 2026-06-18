use std::path::Path;
use std::process::Command;

use engine::target_repo_output::{
    export_patch, prepare_git_worktree, push_approved_branch, remove_git_worktree,
    stage_and_build_patch, BranchPublishRequest, TargetRepoOutputConfig,
};
use tempfile::tempdir;

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn fixture() -> (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir) {
    let target = tempdir().unwrap();
    let remote = tempdir().unwrap();
    let workspace_root = tempdir().unwrap();

    git(target.path(), &["init", "-b", "main"]);
    git(target.path(), &["config", "user.name", "ACP Test"]);
    git(
        target.path(),
        &["config", "user.email", "acp-test@example.invalid"],
    );
    std::fs::write(target.path().join("README.md"), "base\n").unwrap();
    git(target.path(), &["add", "README.md"]);
    git(target.path(), &["commit", "-m", "base"]);

    git(remote.path(), &["init", "--bare"]);
    git(
        target.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    git(target.path(), &["push", "-u", "origin", "main"]);

    (target, remote, workspace_root)
}

#[test]
fn target_repo_worktree_requires_explicit_gate() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(false, false);

    let error = prepare_git_worktree(&config, target.path(), &workspace, "HEAD").unwrap_err();

    assert!(error.contains("ACP_ENABLE_TARGET_REPO_OUTPUT"));
    assert!(!workspace.exists());
}

#[test]
fn target_repo_worktree_respects_kill_switch() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(true, true);

    let error = prepare_git_worktree(&config, target.path(), &workspace, "HEAD").unwrap_err();

    assert!(error.contains("kill switch"));
    assert!(!workspace.exists());
}

#[test]
fn target_repo_worktree_rejects_option_shaped_revision() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(true, false);

    let error = prepare_git_worktree(&config, target.path(), &workspace, "--help").unwrap_err();

    assert!(error.contains("invalid source revision"));
    assert!(!workspace.exists());
}

#[test]
fn approved_branch_push_preserves_main_and_exports_same_patch() {
    let (target, remote, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(true, false);
    let main_before = git(target.path(), &["rev-parse", "main"]);
    let prepared = prepare_git_worktree(&config, target.path(), &workspace, "main").unwrap();

    std::fs::write(workspace.join("README.md"), "base\napproved change\n").unwrap();
    std::fs::write(workspace.join("src.txt"), "new file\n").unwrap();
    let patch = stage_and_build_patch(&config, &workspace).unwrap();
    let exported = export_patch(&config, &workspace, &prepared.source_revision).unwrap();
    assert_eq!(exported.patch, patch);
    assert_eq!(exported.patch_hash, prepared_hash(&patch));

    let output = push_approved_branch(
        &config,
        BranchPublishRequest {
            target_repo_path: target.path().to_path_buf(),
            workspace_path: workspace.clone(),
            source_revision: prepared.source_revision.clone(),
            expected_patch_hash: exported.patch_hash.clone(),
            branch_name: "acp/patch-artifact-0001".to_string(),
            remote: "origin".to_string(),
            commit_message: "feat: apply approved patch".to_string(),
            pr_title: "Apply approved patch".to_string(),
            pr_body: "Artifact patch-artifact-0001\nTests: passed".to_string(),
        },
    )
    .unwrap();

    assert_eq!(output.branch_name, "acp/patch-artifact-0001");
    assert_eq!(output.patch_hash, exported.patch_hash);
    assert_eq!(git(target.path(), &["rev-parse", "main"]), main_before);
    assert_eq!(
        git(
            remote.path(),
            &["rev-parse", "refs/heads/acp/patch-artifact-0001"]
        ),
        output.commit_sha
    );
    assert!(!output.pr_body.contains("sk-"));

    remove_git_worktree(&config, target.path(), &workspace).unwrap();
    assert!(!workspace.exists());
}

#[test]
fn branch_push_rejects_protected_branch_and_secret_text() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(true, false);
    let prepared = prepare_git_worktree(&config, target.path(), &workspace, "main").unwrap();
    std::fs::write(workspace.join("README.md"), "changed\n").unwrap();
    let patch = stage_and_build_patch(&config, &workspace).unwrap();
    let hash = prepared_hash(&patch);

    let protected = push_approved_branch(
        &config,
        BranchPublishRequest {
            target_repo_path: target.path().to_path_buf(),
            workspace_path: workspace.clone(),
            source_revision: prepared.source_revision.clone(),
            expected_patch_hash: hash.clone(),
            branch_name: "main".to_string(),
            remote: "origin".to_string(),
            commit_message: "change".to_string(),
            pr_title: "change".to_string(),
            pr_body: "safe".to_string(),
        },
    )
    .unwrap_err();
    assert!(protected.contains("acp/"));

    let secret = push_approved_branch(
        &config,
        BranchPublishRequest {
            target_repo_path: target.path().to_path_buf(),
            workspace_path: workspace.clone(),
            source_revision: prepared.source_revision,
            expected_patch_hash: hash,
            branch_name: "acp/safe-branch".to_string(),
            remote: "origin".to_string(),
            commit_message: "change".to_string(),
            pr_title: "change".to_string(),
            pr_body: "api_key=sk-abcdefghijklmnopqrstuvwxyz".to_string(),
        },
    )
    .unwrap_err();
    assert!(secret.contains("sensitive"));

    assert_eq!(
        git(target.path(), &["symbolic-ref", "--short", "HEAD"]),
        "main"
    );
}

#[test]
fn branch_push_rejects_option_shaped_remote() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(true, false);
    let prepared = prepare_git_worktree(&config, target.path(), &workspace, "main").unwrap();
    std::fs::write(workspace.join("README.md"), "changed\n").unwrap();
    let patch = stage_and_build_patch(&config, &workspace).unwrap();

    let error = push_approved_branch(
        &config,
        BranchPublishRequest {
            target_repo_path: target.path().to_path_buf(),
            workspace_path: workspace,
            source_revision: prepared.source_revision,
            expected_patch_hash: prepared_hash(&patch),
            branch_name: "acp/safe-branch".to_string(),
            remote: "--mirror".to_string(),
            commit_message: "change".to_string(),
            pr_title: "change".to_string(),
            pr_body: "safe".to_string(),
        },
    )
    .unwrap_err();

    assert!(error.contains("invalid git remote name"));
}

#[test]
fn target_output_rejects_repository_clean_filters() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(true, false);
    prepare_git_worktree(&config, target.path(), &workspace, "main").unwrap();
    std::fs::write(workspace.join(".gitattributes"), "*.txt filter=unsafe\n").unwrap();
    std::fs::write(workspace.join("change.txt"), "change\n").unwrap();

    let error = stage_and_build_patch(&config, &workspace).unwrap_err();

    assert!(error.contains("clean filters"));
}

#[test]
fn target_output_rejects_binary_changes() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(true, false);
    prepare_git_worktree(&config, target.path(), &workspace, "main").unwrap();
    std::fs::write(workspace.join("secret.bin"), [0_u8, 1, 2, 3]).unwrap();

    let error = stage_and_build_patch(&config, &workspace).unwrap_err();

    assert!(error.contains("binary file"));
}

#[test]
fn target_output_rejects_sensitive_file_names() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let config = TargetRepoOutputConfig::for_test(true, false);
    prepare_git_worktree(&config, target.path(), &workspace, "main").unwrap();
    std::fs::write(
        workspace.join("token=sk-abcdefghijklmnopqrstuvwxyz"),
        "safe content\n",
    )
    .unwrap();

    let error = stage_and_build_patch(&config, &workspace).unwrap_err();

    assert!(error.contains("path contains sensitive content"));
}

#[cfg(unix)]
#[test]
fn target_output_rejects_workspace_symlinks() {
    let (target, _, workspace_root) = fixture();
    let workspace = workspace_root.path().join("workspace");
    let outside = tempdir().unwrap();
    let config = TargetRepoOutputConfig::for_test(true, false);
    prepare_git_worktree(&config, target.path(), &workspace, "main").unwrap();
    std::fs::write(outside.path().join("secret.txt"), "outside\n").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        workspace.join("escape.txt"),
    )
    .unwrap();

    let error = stage_and_build_patch(&config, &workspace).unwrap_err();

    assert!(error.contains("symlink"));
}

fn prepared_hash(patch: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("sha256:{}", hex::encode(Sha256::digest(patch.as_bytes())))
}
