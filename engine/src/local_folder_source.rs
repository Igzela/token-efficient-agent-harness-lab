//! Safe, provider-free local-folder source staging for Product Golden Path.
//!
//! This module intentionally owns no task state, lease, approval, output, or
//! audit record. Callers bind its deterministic manifest/receipts through the
//! existing `LocalProductStore` owners. Public projections should use the tree
//! hash and bounded relative paths rather than `canonical_root`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

pub const LOCAL_FOLDER_MANIFEST_SCHEMA: &str = "local_folder_source_manifest.v1";
pub const LOCAL_FOLDER_APPLY_RECEIPT_SCHEMA: &str = "local_folder_apply_receipt.v1";
/// Comma-separated bounded relative paths kept out of a local-folder source.
/// The values are runtime-local only; task and terminal evidence retain their
/// digest, never the private path names.
pub const LOCAL_FOLDER_EXCLUDED_PATHS_ENV: &str = "ACP_PRODUCT_LOCAL_FOLDER_EXCLUDED_PATHS";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFolderEntry {
    /// Always a bounded slash-separated relative path.
    pub relative_path: String,
    pub sha256: String,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFolderManifest {
    pub schema_version: String,
    /// Internal-only identity; never project it into public terminal evidence.
    pub canonical_root: PathBuf,
    pub entries: Vec<LocalFolderEntry>,
    pub tree_sha256: String,
    /// Binds the manifest to the exact configured exclusion policy without
    /// persisting the excluded private path names in public evidence.
    #[serde(default)]
    pub excluded_paths_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFolderApplyReceipt {
    pub schema_version: String,
    pub source_tree_sha256: String,
    pub staged_tree_sha256: String,
    pub rollback_root: PathBuf,
    pub changed_relative_paths: Vec<String>,
    pub rollback_complete: bool,
}

/// Digest-only description of the bounded difference between the immutable
/// local source preimage and its app-owned staging copy. It is suitable for
/// the existing artifact/approval owners without retaining file content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalFolderChangeSummary {
    pub source_tree_sha256: String,
    pub staged_tree_sha256: String,
    pub changed_relative_paths: Vec<String>,
    pub added_relative_paths: Vec<String>,
    pub modified_relative_paths: Vec<String>,
    pub deleted_relative_paths: Vec<String>,
    pub change_sha256: String,
}

/// Create an app-owned staging copy after rejecting symlinks and unsafe file
/// kinds. `excluded_paths` are exact relative paths or directory prefixes.
pub fn stage_local_folder(
    source_root: &Path,
    staging_root: &Path,
    excluded_paths: &[String],
) -> Result<LocalFolderManifest, String> {
    let source = canonical_local_root(source_root)?;
    let exclusions = normalize_exclusions(excluded_paths)?;
    let manifest = capture_local_folder_manifest_inner(&source, &exclusions)?;
    if staging_root.exists() {
        return Err("local-folder staging root already exists".to_string());
    }
    let parent = staging_root
        .parent()
        .ok_or_else(|| "local-folder staging root has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("local-folder staging parent: {error}"))?;
    fs::create_dir(staging_root).map_err(|error| format!("local-folder staging root: {error}"))?;
    if let Err(error) = copy_manifest_files(&source, staging_root, &manifest.entries) {
        let _ = fs::remove_dir_all(staging_root);
        return Err(error);
    }
    let staged = capture_local_folder_manifest_inner(staging_root, &BTreeSet::new())?;
    if manifest.entries != staged.entries || manifest.tree_sha256 != staged.tree_sha256 {
        let _ = fs::remove_dir_all(staging_root);
        return Err("local-folder staging copy did not preserve source manifest".to_string());
    }
    Ok(manifest)
}

/// Read the local-only exclusion policy. Empty segments are ignored, while
/// every supplied path is normalized and de-duplicated before use.
pub fn configured_local_folder_exclusions() -> Result<Vec<String>, String> {
    let configured = std::env::var(LOCAL_FOLDER_EXCLUDED_PATHS_ENV).unwrap_or_default();
    let raw = configured
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let normalized = normalize_exclusions(&raw)?;
    if normalized.len() > 128 {
        return Err("local-folder exclusion policy exceeds 128 paths".to_string());
    }
    Ok(normalized.into_iter().collect())
}

pub fn capture_local_folder_manifest(
    source_root: &Path,
    excluded_paths: &[String],
) -> Result<LocalFolderManifest, String> {
    let source = canonical_local_root(source_root)?;
    capture_local_folder_manifest_inner(&source, &normalize_exclusions(excluded_paths)?)
}

pub fn verify_local_folder_manifest_current(
    expected: &LocalFolderManifest,
    excluded_paths: &[String],
) -> Result<(), String> {
    let actual = capture_local_folder_manifest(&expected.canonical_root, excluded_paths)?;
    if actual.entries != expected.entries
        || actual.tree_sha256 != expected.tree_sha256
        || actual.excluded_paths_sha256 != expected.excluded_paths_sha256
    {
        return Err("local-folder original source manifest is stale".to_string());
    }
    Ok(())
}

/// Apply only the staged files whose relative paths are within the approved
/// set. The original source must still equal `source_manifest`; before each
/// replacement an app-owned rollback copy is persisted. A failure rolls back
/// every earlier change and reports a bounded error.
pub fn apply_local_folder_changes(
    source_manifest: &LocalFolderManifest,
    staged_root: &Path,
    rollback_root: &Path,
    allowed_paths: &[String],
    excluded_paths: &[String],
    expected_change_sha256: &str,
) -> Result<LocalFolderApplyReceipt, String> {
    let exclusions = normalize_exclusions(excluded_paths)?;
    if exclusion_paths_sha256(&exclusions) != source_manifest.excluded_paths_sha256 {
        return Err("local-folder exclusion policy changed after staging".to_string());
    }
    verify_local_folder_manifest_current(source_manifest, excluded_paths)?;
    let staged_root = canonical_local_root(staged_root)
        .map_err(|_| "local-folder staging root is unsafe".to_string())?;
    let staged = capture_local_folder_manifest_inner(&staged_root, &BTreeSet::new())?;
    let change_summary = summarize_local_folder_manifests(source_manifest, &staged)?;
    if change_summary.change_sha256 != expected_change_sha256 {
        return Err("local-folder staged change identity is stale".to_string());
    }
    let allowed = normalize_exclusions(allowed_paths)?;
    if allowed.is_empty() {
        return Err("local-folder apply requires bounded allowed paths".to_string());
    }
    if rollback_root.exists() {
        return Err("local-folder rollback root already exists".to_string());
    }
    fs::create_dir_all(rollback_root)
        .map_err(|error| format!("local-folder rollback root: {error}"))?;

    let before = entries_by_path(&source_manifest.entries);
    let after = entries_by_path(&staged.entries);
    let all_paths = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut changed = Vec::new();
    for path in all_paths {
        if before.get(&path) != after.get(&path) {
            if exclusions.iter().any(|prefix| path_matches(prefix, &path))
                || !allowed.iter().any(|prefix| path_matches(prefix, &path))
            {
                return Err("local-folder change is outside the approved path scope".to_string());
            }
            changed.push(path);
        }
    }
    if changed.is_empty() {
        return Ok(LocalFolderApplyReceipt {
            schema_version: LOCAL_FOLDER_APPLY_RECEIPT_SCHEMA.to_string(),
            source_tree_sha256: source_manifest.tree_sha256.clone(),
            staged_tree_sha256: staged.tree_sha256,
            rollback_root: rollback_root.to_path_buf(),
            changed_relative_paths: vec![],
            rollback_complete: true,
        });
    }

    let mut applied = Vec::new();
    for relative in &changed {
        if let Err(error) = backup_then_replace(
            &source_manifest.canonical_root,
            &staged_root,
            rollback_root,
            relative,
            before.get(relative),
            after.get(relative),
        ) {
            rollback_local_folder_changes(
                &source_manifest.canonical_root,
                rollback_root,
                &applied,
            )?;
            return Err(error);
        }
        applied.push(relative.clone());
    }
    Ok(LocalFolderApplyReceipt {
        schema_version: LOCAL_FOLDER_APPLY_RECEIPT_SCHEMA.to_string(),
        source_tree_sha256: source_manifest.tree_sha256.clone(),
        staged_tree_sha256: staged.tree_sha256,
        rollback_root: rollback_root.to_path_buf(),
        changed_relative_paths: changed,
        rollback_complete: false,
    })
}

pub fn summarize_local_folder_changes(
    source_manifest: &LocalFolderManifest,
    staged_root: &Path,
    excluded_paths: &[String],
) -> Result<LocalFolderChangeSummary, String> {
    verify_local_folder_manifest_current(source_manifest, excluded_paths)?;
    let exclusions = normalize_exclusions(excluded_paths)?;
    if exclusion_paths_sha256(&exclusions) != source_manifest.excluded_paths_sha256 {
        return Err("local-folder exclusion policy changed after staging".to_string());
    }
    let staged_root = canonical_local_root(staged_root)
        .map_err(|_| "local-folder staging root is unsafe".to_string())?;
    let staged = capture_local_folder_manifest_inner(&staged_root, &BTreeSet::new())?;
    summarize_local_folder_manifests(source_manifest, &staged)
}

fn summarize_local_folder_manifests(
    source_manifest: &LocalFolderManifest,
    staged: &LocalFolderManifest,
) -> Result<LocalFolderChangeSummary, String> {
    let before = entries_by_path(&source_manifest.entries);
    let after = entries_by_path(&staged.entries);
    let paths = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut changed = Vec::new();
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();
    for path in paths {
        match (before.get(&path), after.get(&path)) {
            (None, Some(_)) => {
                changed.push(path.clone());
                added.push(path);
            }
            (Some(_), None) => {
                changed.push(path.clone());
                deleted.push(path);
            }
            (Some(left), Some(right)) if *left != *right => {
                changed.push(path.clone());
                modified.push(path);
            }
            _ => {}
        }
    }
    let digest_input = serde_json::json!({
        "source_tree_sha256": source_manifest.tree_sha256,
        "staged_tree_sha256": staged.tree_sha256,
        "changed_relative_paths": changed,
        "added_relative_paths": added,
        "modified_relative_paths": modified,
        "deleted_relative_paths": deleted,
    });
    let change_sha256 = hex::encode(Sha256::digest(
        serde_json::to_vec(&digest_input).map_err(|error| error.to_string())?,
    ));
    Ok(LocalFolderChangeSummary {
        source_tree_sha256: source_manifest.tree_sha256.clone(),
        staged_tree_sha256: staged.tree_sha256.clone(),
        changed_relative_paths: changed,
        added_relative_paths: added,
        modified_relative_paths: modified,
        deleted_relative_paths: deleted,
        change_sha256,
    })
}

pub fn rollback_local_folder_changes(
    source_root: &Path,
    rollback_root: &Path,
    changed_paths: &[String],
) -> Result<(), String> {
    let source_root = canonical_local_root(source_root)?;
    let source_root_fingerprint = local_root_fingerprint(&source_root);
    for relative in changed_paths.iter().rev() {
        let target = joined_relative(&source_root, relative)?;
        let backup = joined_relative(rollback_root, relative)?;
        let marker_path = backup.with_extension("acp-applied");
        let marker_raw = fs::read(&marker_path)
            .map_err(|_| "local-folder rollback proof is unavailable".to_string())?;
        let marker: serde_json::Value = serde_json::from_slice(&marker_raw)
            .map_err(|_| "local-folder rollback proof is invalid".to_string())?;
        if marker
            .get("schema_version")
            .and_then(serde_json::Value::as_str)
            != Some("local_folder_rollback_preimage.v1")
            || marker
                .get("relative_path")
                .and_then(serde_json::Value::as_str)
                != Some(relative)
            || marker
                .get("source_root_fingerprint")
                .and_then(serde_json::Value::as_str)
                != Some(source_root_fingerprint.as_str())
        {
            return Err("local-folder rollback proof is invalid".to_string());
        }
        if marker
            .get("applied_absent")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            if fs::symlink_metadata(&target).is_ok() {
                return Err("local-folder rollback preimage is stale".to_string());
            }
        } else {
            let expected_sha = marker
                .get("applied_sha256")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "local-folder rollback proof is invalid".to_string())?;
            let expected_executable = marker
                .get("applied_executable")
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| "local-folder rollback proof is invalid".to_string())?;
            let metadata = fs::symlink_metadata(&target)
                .map_err(|_| "local-folder rollback preimage is stale".to_string())?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || file_sha256(&target)? != expected_sha
                || is_executable(&target)? != expected_executable
            {
                return Err("local-folder rollback preimage is stale".to_string());
            }
        }
        let marker = backup.with_extension("acp-present");
        if marker.exists() {
            copy_file_atomic(&backup, &target, is_executable(&backup)?)?;
        } else if target.exists() {
            fs::remove_file(&target)
                .map_err(|error| format!("local-folder rollback remove: {error}"))?;
        }
    }
    Ok(())
}

fn canonical_local_root(root: &Path) -> Result<PathBuf, String> {
    if !root.is_absolute() {
        return Err("local-folder source must be an absolute path".to_string());
    }
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("local-folder source is unavailable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("local-folder source must be a non-symlink directory".to_string());
    }
    let canonical = fs::canonicalize(root)
        .map_err(|error| format!("local-folder source cannot be canonicalized: {error}"))?;
    if canonical != root {
        return Err("local-folder source path must already be canonical".to_string());
    }
    Ok(canonical)
}

fn normalize_exclusions(paths: &[String]) -> Result<BTreeSet<String>, String> {
    paths.iter().map(|path| normalize_relative(path)).collect()
}

fn exclusion_paths_sha256(paths: &BTreeSet<String>) -> String {
    let payload = paths.iter().cloned().collect::<Vec<_>>().join("\n");
    hex::encode(Sha256::digest(payload.as_bytes()))
}

fn normalize_relative(value: &str) -> Result<String, String> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("local-folder path must be a bounded relative path".to_string());
    }
    let normalized = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if normalized.is_empty() || normalized.len() > 1024 {
        return Err("local-folder relative path is invalid".to_string());
    }
    Ok(normalized)
}

fn capture_local_folder_manifest_inner(
    root: &Path,
    exclusions: &BTreeSet<String>,
) -> Result<LocalFolderManifest, String> {
    let mut entries = Vec::new();
    walk_local_folder(root, root, exclusions, &mut entries)?;
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let tree_sha256 = manifest_hash(&entries)?;
    Ok(LocalFolderManifest {
        schema_version: LOCAL_FOLDER_MANIFEST_SCHEMA.to_string(),
        canonical_root: root.to_path_buf(),
        entries,
        tree_sha256,
        excluded_paths_sha256: exclusion_paths_sha256(exclusions),
    })
}

fn walk_local_folder(
    root: &Path,
    directory: &Path,
    exclusions: &BTreeSet<String>,
    entries: &mut Vec<LocalFolderEntry>,
) -> Result<(), String> {
    let mut children = fs::read_dir(directory)
        .map_err(|error| format!("local-folder read directory: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("local-folder read entry: {error}"))?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "local-folder traversal escaped source root".to_string())?;
        let relative = normalize_relative(&relative.to_string_lossy())?;
        if exclusions
            .iter()
            .any(|prefix| path_matches(prefix, &relative))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("local-folder metadata: {error}"))?;
        let kind = metadata.file_type();
        if kind.is_symlink() {
            return Err("local-folder source contains a symlink".to_string());
        }
        if kind.is_dir() {
            walk_local_folder(root, &path, exclusions, entries)?;
        } else if kind.is_file() {
            entries.push(LocalFolderEntry {
                relative_path: relative,
                sha256: file_sha256(&path)?,
                executable: is_executable(&path)?,
            });
        } else {
            return Err("local-folder source contains an unsafe special file".to_string());
        }
    }
    Ok(())
}

fn copy_manifest_files(
    source: &Path,
    destination: &Path,
    entries: &[LocalFolderEntry],
) -> Result<(), String> {
    for entry in entries {
        let from = joined_relative(source, &entry.relative_path)?;
        let to = joined_relative(destination, &entry.relative_path)?;
        copy_file_atomic(&from, &to, entry.executable)?;
    }
    Ok(())
}

fn backup_then_replace(
    source: &Path,
    staged: &Path,
    rollback: &Path,
    relative: &str,
    before: Option<&&LocalFolderEntry>,
    after: Option<&&LocalFolderEntry>,
) -> Result<(), String> {
    let target = joined_relative(source, relative)?;
    let backup = joined_relative(rollback, relative)?;
    ensure_destination_parent_safe(source, relative)?;
    if let Some(expected) = before {
        let metadata = fs::symlink_metadata(&target)
            .map_err(|_| "local-folder source preimage is stale".to_string())?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("local-folder source preimage is unsafe".to_string());
        }
        if file_sha256(&target)? != expected.sha256
            || is_executable(&target)? != expected.executable
        {
            return Err("local-folder source preimage is stale".to_string());
        }
        copy_file_atomic(&target, &backup, is_executable(&target)?)?;
        fs::write(backup.with_extension("acp-present"), b"1")
            .map_err(|error| format!("local-folder rollback marker: {error}"))?;
    } else if fs::symlink_metadata(&target).is_ok() {
        return Err("local-folder source preimage is stale".to_string());
    }
    match after {
        Some(entry) => {
            let staged_input = joined_relative(staged, relative)?;
            let metadata = fs::symlink_metadata(&staged_input)
                .map_err(|_| "local-folder staged input is stale".to_string())?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("local-folder staged input is unsafe".to_string());
            }
            if file_sha256(&staged_input)? != entry.sha256
                || is_executable(&staged_input)? != entry.executable
            {
                return Err("local-folder staged input is stale".to_string());
            }
            copy_file_atomic_existing_parent(&staged_input, &target, entry.executable)?;
        }
        None => fs::remove_file(&target)
            .map_err(|error| format!("local-folder apply delete: {error}"))?,
    }
    let applied_marker = json!({
        "schema_version": "local_folder_rollback_preimage.v1",
        "relative_path": relative,
        "source_root_fingerprint": local_root_fingerprint(source),
        "applied_sha256": after.map(|entry| entry.sha256.as_str()),
        "applied_executable": after.map(|entry| entry.executable),
        "applied_absent": after.is_none(),
    });
    let applied_marker_path = backup.with_extension("acp-applied");
    let marker_parent = applied_marker_path
        .parent()
        .ok_or_else(|| "local-folder rollback marker parent is missing".to_string())?;
    fs::create_dir_all(marker_parent)
        .map_err(|error| format!("local-folder rollback marker directory: {error}"))?;
    fs::write(
        applied_marker_path,
        serde_json::to_vec(&applied_marker).map_err(|error| error.to_string())?,
    )
    .map_err(|error| {
        let compensation = restore_preimage_after_marker_failure(
            &target,
            &backup,
            before.map(|entry| entry.executable),
        );
        match compensation {
            Ok(()) => format!("local-folder rollback apply marker: {error}"),
            Err(compensation_error) => format!(
                "local-folder apply effect is unconfirmed after rollback marker failure: {error}; {compensation_error}"
            ),
        }
    })?;
    Ok(())
}

fn restore_preimage_after_marker_failure(
    target: &Path,
    backup: &Path,
    before_executable: Option<bool>,
) -> Result<(), String> {
    match before_executable {
        Some(executable) => copy_file_atomic_existing_parent(backup, target, executable),
        None => match fs::symlink_metadata(target) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                Err("local-folder marker compensation found an unsafe target".to_string())
            }
            Ok(_) => fs::remove_file(target)
                .map_err(|error| format!("local-folder marker compensation: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("local-folder marker compensation: {error}")),
        },
    }
}

fn local_root_fingerprint(root: &Path) -> String {
    hex::encode(Sha256::digest(root.to_string_lossy().as_bytes()))
}

/// Ensure the source-side parent chain is made only of real directories before
/// a confirmed apply writes below it. This rejects a post-manifest symlink
/// substitution and creates only missing bounded directory components.
fn ensure_destination_parent_safe(source: &Path, relative: &str) -> Result<(), String> {
    let normalized = normalize_relative(relative)?;
    let relative = Path::new(&normalized);
    let Some(parent) = relative.parent() else {
        return Ok(());
    };
    let mut current = source.to_path_buf();
    for component in parent.components() {
        let Component::Normal(name) = component else {
            return Err("local-folder destination parent is invalid".to_string());
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if !metadata.file_type().is_symlink() && metadata.is_dir() => {}
            Ok(_) => return Err("local-folder destination parent is unsafe".to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => {
                        return Err(format!("local-folder destination directory: {error}"));
                    }
                }
                let metadata = fs::symlink_metadata(&current)
                    .map_err(|_| "local-folder destination parent is unavailable".to_string())?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("local-folder destination parent is unsafe".to_string());
                }
            }
            Err(_) => return Err("local-folder destination parent is unavailable".to_string()),
        }
    }
    Ok(())
}

fn entries_by_path(entries: &[LocalFolderEntry]) -> BTreeMap<String, &LocalFolderEntry> {
    entries
        .iter()
        .map(|entry| (entry.relative_path.clone(), entry))
        .collect()
}

fn path_matches(prefix: &str, candidate: &str) -> bool {
    candidate == prefix
        || candidate
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn joined_relative(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = normalize_relative(relative)?;
    let path = root.join(relative);
    if !path.starts_with(root) {
        return Err("local-folder path escaped root".to_string());
    }
    Ok(path)
}

fn copy_file_atomic(from: &Path, to: &Path, executable: bool) -> Result<(), String> {
    let parent = to
        .parent()
        .ok_or_else(|| "local-folder destination parent is missing".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("local-folder create directory: {error}"))?;
    copy_file_atomic_existing_parent(from, to, executable)
}

fn copy_file_atomic_existing_parent(
    from: &Path,
    to: &Path,
    executable: bool,
) -> Result<(), String> {
    let parent = to
        .parent()
        .ok_or_else(|| "local-folder destination parent is missing".to_string())?;
    let metadata = fs::symlink_metadata(parent)
        .map_err(|_| "local-folder destination parent is unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("local-folder destination parent is unsafe".to_string());
    }
    let temporary = parent.join(format!(".acp-stage-{}", uuid::Uuid::new_v4()));
    let mut input =
        fs::File::open(from).map_err(|error| format!("local-folder source read: {error}"))?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| format!("local-folder temporary create: {error}"))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|error| format!("local-folder source read: {error}"))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| format!("local-folder temporary write: {error}"))?;
    }
    output
        .sync_all()
        .map_err(|error| format!("local-folder temporary sync: {error}"))?;
    drop(output);
    set_executable(&temporary, executable)?;
    fs::rename(&temporary, to).map_err(|error| format!("local-folder atomic replace: {error}"))
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|error| format!("local-folder file read: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("local-folder file read: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn manifest_hash(entries: &[LocalFolderEntry]) -> Result<String, String> {
    let encoded = serde_json::to_vec(entries).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(encoded)))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> Result<bool, String> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::metadata(path)
        .map_err(|error| format!("local-folder permissions: {error}"))?
        .permissions()
        .mode()
        & 0o111
        != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> Result<bool, String> {
    Ok(false)
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| format!("local-folder permissions: {error}"))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_and_detects_a_stale_preimage() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), b"before").unwrap();
        let manifest = stage_local_folder(&source, &root.path().join("stage"), &[]).unwrap();
        fs::write(source.join("a.txt"), b"concurrent").unwrap();
        assert!(verify_local_folder_manifest_current(&manifest, &[]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        symlink("/etc/passwd", source.join("escape")).unwrap();
        assert!(capture_local_folder_manifest(&source, &[]).is_err());
    }

    #[test]
    fn rollback_restores_replaced_file() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), b"before").unwrap();
        let stage = root.path().join("stage");
        let manifest = stage_local_folder(&source, &stage, &[]).unwrap();
        fs::write(stage.join("a.txt"), b"after").unwrap();
        let expected = summarize_local_folder_changes(&manifest, &stage, &[])
            .unwrap()
            .change_sha256;
        let receipt = apply_local_folder_changes(
            &manifest,
            &stage,
            &root.path().join("rollback"),
            &["a.txt".to_string()],
            &[],
            &expected,
        )
        .unwrap();
        assert_eq!(fs::read(source.join("a.txt")).unwrap(), b"after");
        rollback_local_folder_changes(
            &source,
            &receipt.rollback_root,
            &receipt.changed_relative_paths,
        )
        .unwrap();
        assert_eq!(fs::read(source.join("a.txt")).unwrap(), b"before");
    }

    #[test]
    fn rollback_refuses_to_overwrite_a_concurrent_source_edit() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), b"before").unwrap();
        let stage = root.path().join("stage");
        let manifest = stage_local_folder(&source, &stage, &[]).unwrap();
        fs::write(stage.join("a.txt"), b"applied").unwrap();
        let expected = summarize_local_folder_changes(&manifest, &stage, &[])
            .unwrap()
            .change_sha256;
        let receipt = apply_local_folder_changes(
            &manifest,
            &stage,
            &root.path().join("rollback"),
            &["a.txt".to_string()],
            &[],
            &expected,
        )
        .unwrap();
        fs::write(source.join("a.txt"), b"operator-edit").unwrap();

        let error = rollback_local_folder_changes(
            &source,
            &receipt.rollback_root,
            &receipt.changed_relative_paths,
        )
        .unwrap_err();
        assert_eq!(error, "local-folder rollback preimage is stale");
        assert_eq!(fs::read(source.join("a.txt")).unwrap(), b"operator-edit");
    }

    #[test]
    fn rollback_refuses_a_different_source_root_even_with_the_same_postimage() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), b"before").unwrap();
        let stage = root.path().join("stage");
        let manifest = stage_local_folder(&source, &stage, &[]).unwrap();
        fs::write(stage.join("a.txt"), b"applied").unwrap();
        let expected = summarize_local_folder_changes(&manifest, &stage, &[])
            .unwrap()
            .change_sha256;
        let receipt = apply_local_folder_changes(
            &manifest,
            &stage,
            &root.path().join("rollback"),
            &["a.txt".to_string()],
            &[],
            &expected,
        )
        .unwrap();
        let unrelated = root.path().join("unrelated");
        fs::create_dir(&unrelated).unwrap();
        fs::write(unrelated.join("a.txt"), b"applied").unwrap();

        let error = rollback_local_folder_changes(
            &unrelated,
            &receipt.rollback_root,
            &receipt.changed_relative_paths,
        )
        .unwrap_err();
        assert_eq!(error, "local-folder rollback proof is invalid");
        assert_eq!(fs::read(unrelated.join("a.txt")).unwrap(), b"applied");
    }

    #[test]
    fn apply_fails_closed_when_exclusion_policy_changes_after_staging() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("public.txt"), b"before").unwrap();
        fs::write(source.join("private.txt"), b"private").unwrap();
        let stage = root.path().join("stage");
        let exclusions = vec!["private.txt".to_string()];
        let manifest = stage_local_folder(&source, &stage, &exclusions).unwrap();
        fs::write(stage.join("public.txt"), b"after").unwrap();

        let error = apply_local_folder_changes(
            &manifest,
            &stage,
            &root.path().join("rollback"),
            &["public.txt".to_string()],
            &[],
            "not-reached",
        )
        .unwrap_err();
        assert_eq!(error, "local-folder exclusion policy changed after staging");
        assert_eq!(fs::read(source.join("public.txt")).unwrap(), b"before");
        assert_eq!(fs::read(source.join("private.txt")).unwrap(), b"private");
    }

    #[test]
    fn change_summary_binds_sorted_add_modify_and_delete_sets() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("delete.txt"), b"before").unwrap();
        fs::write(source.join("modify.txt"), b"before").unwrap();
        let stage = root.path().join("stage");
        let manifest = stage_local_folder(&source, &stage, &[]).unwrap();

        fs::remove_file(stage.join("delete.txt")).unwrap();
        fs::write(stage.join("modify.txt"), b"after").unwrap();
        fs::write(stage.join("add.txt"), b"new").unwrap();

        let first = summarize_local_folder_changes(&manifest, &stage, &[]).unwrap();
        let second = summarize_local_folder_changes(&manifest, &stage, &[]).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.added_relative_paths, vec!["add.txt"]);
        assert_eq!(first.modified_relative_paths, vec!["modify.txt"]);
        assert_eq!(first.deleted_relative_paths, vec!["delete.txt"]);
        assert_eq!(
            first.changed_relative_paths,
            vec!["add.txt", "delete.txt", "modify.txt"]
        );
        assert_eq!(first.change_sha256.len(), 64);
    }

    #[test]
    fn apply_rejects_a_staged_change_that_differs_from_the_approved_digest() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), b"before").unwrap();
        let stage = root.path().join("stage");
        let manifest = stage_local_folder(&source, &stage, &[]).unwrap();
        fs::write(stage.join("a.txt"), b"approved").unwrap();
        let approved = summarize_local_folder_changes(&manifest, &stage, &[])
            .unwrap()
            .change_sha256;
        fs::write(stage.join("a.txt"), b"changed-after-approval").unwrap();

        let error = apply_local_folder_changes(
            &manifest,
            &stage,
            &root.path().join("rollback"),
            &["a.txt".to_string()],
            &[],
            &approved,
        )
        .unwrap_err();
        assert_eq!(error, "local-folder staged change identity is stale");
        assert_eq!(fs::read(source.join("a.txt")).unwrap(), b"before");
    }

    #[test]
    fn replacement_rechecks_the_exact_preimage_immediately_before_write() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("a.txt"), b"before").unwrap();
        let stage = root.path().join("stage");
        let manifest = stage_local_folder(&source, &stage, &[]).unwrap();
        fs::write(stage.join("a.txt"), b"after").unwrap();
        // This simulates a user edit after the manifest check but before this
        // individual replacement receives its write turn.
        fs::write(source.join("a.txt"), b"concurrent").unwrap();
        let before = entries_by_path(&manifest.entries);
        let staged = capture_local_folder_manifest(&stage, &[]).unwrap();
        let after = entries_by_path(&staged.entries);

        let error = backup_then_replace(
            &source,
            &stage,
            &root.path().join("rollback"),
            "a.txt",
            before.get("a.txt"),
            after.get("a.txt"),
        )
        .unwrap_err();
        assert_eq!(error, "local-folder source preimage is stale");
        assert_eq!(fs::read(source.join("a.txt")).unwrap(), b"concurrent");
    }
}
