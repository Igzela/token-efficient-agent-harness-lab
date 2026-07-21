//! PE7 Harness Evolution laboratory — B1 evidence foundation (default-off).
//!
//! Owns versioned candidate/proposal/lineage identity and validation only.
//! Persistence is through `LocalProductStore`. Evaluation (B2) and PR_READY
//! finalization (B3) are separate packets. The active Harness, evaluator,
//! permissions, budgets, audit, target-output, merge, release, and rollback
//! owners remain immutable to candidates.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

pub const EVOLUTION_LAB_SCHEMA_VERSION: &str = "harness_evolution_lab.v1";
pub const CANDIDATE_SCHEMA_VERSION: &str = "harness_evolution_candidate.v1";
pub const PROPOSAL_SCHEMA_VERSION: &str = "harness_evolution_proposal.v1";
pub const LINEAGE_SCHEMA_VERSION: &str = "harness_evolution_lineage.v1";
pub const RECEIPT_SCHEMA_VERSION: &str = "harness_evolution_receipt.v1";
pub const WORKSPACE_SCHEMA_VERSION: &str = "harness_evolution_workspace.v1";
pub const ACTIVE_VERSION_SCHEMA: &str = "harness_active_version.v1";
pub const MUTABLE_SURFACE_SCHEMA: &str = "harness_mutable_surface.v1";

pub const ENABLE_ENV: &str = "ACP_ENABLE_HARNESS_EVOLUTION_LAB";
pub const KILL_SWITCH_ENV: &str = "ACP_HARNESS_EVOLUTION_KILL_SWITCH";
/// Canonical app-owned root for evolution candidate workspaces (must be set for real workspace ops).
pub const WORKSPACE_ROOT_ENV: &str = "ACP_HARNESS_EVOLUTION_WORKSPACE_ROOT";

/// Serializes process-wide evolution lab env mutations across unit tests.
#[cfg(test)]
pub(crate) static EVOLUTION_LAB_TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub const MAX_PROPOSAL_BYTES: usize = 16 * 1024;
pub const MAX_EVIDENCE_HASHES: usize = 32;
pub const MAX_MUTABLE_SURFACES: usize = 8;
pub const MAX_SCOPE_PATHS: usize = 32;
pub const MAX_WORKSPACE_REL_DEPTH: usize = 8;
pub const MAX_WORKSPACE_FILES: usize = 64;
pub const MAX_WORKSPACE_FILE_BYTES: usize = 64 * 1024;

/// Documented component-level mutable surfaces for the initial laboratory.
pub const ADMITTED_MUTABLE_SURFACES: &[&str] = &[
    "prompts_and_bounded_rules",
    "context_selection_and_summarization",
    "tool_descriptions_and_selection_policy",
    "retry_and_stop_policy",
    "model_routing_within_admitted_set",
    "recursive_decomposition_policy",
];

/// Surfaces that must never be declared mutable by a candidate/evolver.
pub const FORBIDDEN_MUTABLE_SURFACES: &[&str] = &[
    "evaluator",
    "sealed_labels",
    "permissions",
    "credentials",
    "budgets",
    "audit",
    "promotion_thresholds",
    "target_output",
    "merge",
    "release",
    "deployment",
    "rollback",
    "active_harness_source",
    "scheduler",
    "auth",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateTerminalReason {
    Admitted,
    RejectedStaleParent,
    RejectedChangedActiveVersion,
    RejectedDuplicate,
    RejectedTamper,
    RejectedWorkspaceEscape,
    RejectedMalformed,
    RejectedKillSwitch,
    RejectedPaused,
    RejectedLateWrite,
    RejectedForbiddenSurface,
    WorkspaceDiscarded,
}

impl CandidateTerminalReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::RejectedStaleParent => "rejected_stale_parent",
            Self::RejectedChangedActiveVersion => "rejected_changed_active_version",
            Self::RejectedDuplicate => "rejected_duplicate",
            Self::RejectedTamper => "rejected_tamper",
            Self::RejectedWorkspaceEscape => "rejected_workspace_escape",
            Self::RejectedMalformed => "rejected_malformed",
            Self::RejectedKillSwitch => "rejected_kill_switch",
            Self::RejectedPaused => "rejected_paused",
            Self::RejectedLateWrite => "rejected_late_write",
            Self::RejectedForbiddenSurface => "rejected_forbidden_surface",
            Self::WorkspaceDiscarded => "workspace_discarded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Proposed,
    Admitted,
    Rejected,
    Discarded,
}

impl CandidateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Admitted => "admitted",
            Self::Rejected => "rejected",
            Self::Discarded => "discarded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveHarnessIdentity {
    pub schema_version: String,
    pub active_version_id: String,
    pub active_version_hash: String,
    pub evaluator_identity_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutableSurfaceDeclaration {
    pub schema_version: String,
    pub surfaces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionProposal {
    pub schema_version: String,
    pub proposal_id: String,
    pub parent_candidate_id: Option<String>,
    pub active_version_id: String,
    pub active_version_hash: String,
    pub evaluator_identity_hash: String,
    pub mutable_surface: MutableSurfaceDeclaration,
    /// Hash of redacted structured proposal body only — never raw prompts/outputs.
    pub proposal_body_sha256: String,
    pub evidence_hashes: Vec<String>,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateWorkspace {
    pub schema_version: String,
    pub workspace_id: String,
    /// Relative path under the app-owned evolution root (never absolute).
    pub relative_path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionCandidate {
    pub schema_version: String,
    pub candidate_id: String,
    pub lineage_id: String,
    pub parent_candidate_id: Option<String>,
    pub proposal_id: String,
    pub active_version_id: String,
    pub active_version_hash: String,
    pub evaluator_identity_hash: String,
    pub mutable_surface: MutableSurfaceDeclaration,
    pub workspace: CandidateWorkspace,
    pub content_hash: String,
    pub status: CandidateStatus,
    pub terminal_reason: CandidateTerminalReason,
    pub seed: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionReceipt {
    pub schema_version: String,
    pub receipt_id: String,
    pub candidate_id: String,
    pub proposal_id: String,
    pub lineage_id: String,
    pub active_version_id: String,
    pub terminal_reason: CandidateTerminalReason,
    pub content_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionAdmissionError {
    pub code: String,
    pub message: String,
}

impl EvolutionAdmissionError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

pub fn lab_enabled() -> bool {
    std::env::var(ENABLE_ENV).as_deref() == Ok("1")
}

pub fn kill_switch_active() -> bool {
    std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1")
}

pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub fn canonical_json_sha256(value: &Value) -> Result<String, String> {
    let canonical =
        crate::event_schema::canonical_event_json(value).map_err(|error| error.to_string())?;
    Ok(sha256_hex(&canonical))
}

pub fn derive_candidate_id(proposal_id: &str, content_hash: &str, seed: u64) -> String {
    let material = format!("candidate.v1|{proposal_id}|{content_hash}|{seed}");
    format!("hevc-{}", &sha256_hex(&material)[..32])
}

pub fn derive_lineage_id(parent_candidate_id: Option<&str>, proposal_id: &str) -> String {
    let parent = parent_candidate_id.unwrap_or("root");
    let material = format!("lineage.v1|{parent}|{proposal_id}");
    format!("heln-{}", &sha256_hex(&material)[..32])
}

pub fn derive_proposal_id(
    active_version_id: &str,
    proposal_body_sha256: &str,
    seed: u64,
) -> String {
    let material = format!("proposal.v1|{active_version_id}|{proposal_body_sha256}|{seed}");
    format!("hepr-{}", &sha256_hex(&material)[..32])
}

pub fn derive_receipt_id(candidate_id: &str, terminal_reason: CandidateTerminalReason) -> String {
    let material = format!("receipt.v1|{candidate_id}|{}", terminal_reason.as_str());
    format!("herc-{}", &sha256_hex(&material)[..32])
}

pub fn validate_sha256_hex(value: &str) -> Result<(), EvolutionAdmissionError> {
    if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(EvolutionAdmissionError::new(
            "evolution_hash_invalid",
            "expected 64-char lowercase hex sha256",
        ));
    }
    if value.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(EvolutionAdmissionError::new(
            "evolution_hash_invalid",
            "sha256 must be lowercase hex",
        ));
    }
    Ok(())
}

pub fn validate_mutable_surface(
    surface: &MutableSurfaceDeclaration,
) -> Result<(), EvolutionAdmissionError> {
    if surface.schema_version != MUTABLE_SURFACE_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "evolution_mutable_surface_schema",
            "mutable surface schema_version mismatch",
        ));
    }
    if surface.surfaces.is_empty() || surface.surfaces.len() > MAX_MUTABLE_SURFACES {
        return Err(EvolutionAdmissionError::new(
            "evolution_mutable_surface_bound",
            "mutable surface count out of bound",
        ));
    }
    let mut seen = BTreeSet::new();
    for name in &surface.surfaces {
        if !seen.insert(name.clone()) {
            return Err(EvolutionAdmissionError::new(
                "evolution_mutable_surface_duplicate",
                format!("duplicate mutable surface: {name}"),
            ));
        }
        if FORBIDDEN_MUTABLE_SURFACES.contains(&name.as_str()) {
            return Err(EvolutionAdmissionError::new(
                "evolution_forbidden_surface",
                format!("forbidden mutable surface: {name}"),
            ));
        }
        if !ADMITTED_MUTABLE_SURFACES.contains(&name.as_str()) {
            return Err(EvolutionAdmissionError::new(
                "evolution_unknown_surface",
                format!("mutable surface not in admitted set: {name}"),
            ));
        }
    }
    Ok(())
}

pub fn validate_workspace_relative_path(path: &str) -> Result<(), EvolutionAdmissionError> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_escape",
            "workspace path must be relative and non-empty",
        ));
    }
    if path.contains('\0') || path.contains("..") {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_escape",
            "workspace path traversal is forbidden",
        ));
    }
    let pb = PathBuf::from(path);
    let mut depth = 0usize;
    for component in pb.components() {
        match component {
            Component::Normal(_) => depth = depth.saturating_add(1),
            Component::CurDir => {}
            _ => {
                return Err(EvolutionAdmissionError::new(
                    "evolution_workspace_escape",
                    "workspace path contains forbidden component",
                ));
            }
        }
    }
    if depth == 0 || depth > MAX_WORKSPACE_REL_DEPTH {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_escape",
            "workspace path depth out of bound",
        ));
    }
    Ok(())
}

/// Resolve the configured app-owned evolution workspace root (canonicalized).
pub fn configured_workspace_root() -> Result<PathBuf, EvolutionAdmissionError> {
    let raw = std::env::var(WORKSPACE_ROOT_ENV).map_err(|_| {
        EvolutionAdmissionError::new(
            "evolution_workspace_root_unset",
            "ACP_HARNESS_EVOLUTION_WORKSPACE_ROOT must be set to an app-owned directory",
        )
    })?;
    if raw.trim().is_empty() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_root_unset",
            "ACP_HARNESS_EVOLUTION_WORKSPACE_ROOT must not be empty",
        ));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_root",
            "workspace root must be an absolute path",
        ));
    }
    if !path.exists() || !path.is_dir() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_root",
            "workspace root must be an existing directory",
        ));
    }
    path.canonicalize()
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_root", e.to_string()))
}

/// Resolve a candidate workspace under an app-owned root; refuse escape and symlink ownership.
pub fn resolve_workspace_under_root(
    root: &Path,
    relative: &str,
) -> Result<PathBuf, EvolutionAdmissionError> {
    validate_workspace_relative_path(relative)?;
    let root_canon = root
        .canonicalize()
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_root", e.to_string()))?;
    let mut cursor = root_canon.clone();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(name) => {
                cursor = cursor.join(name);
                if cursor.exists() {
                    let canon = cursor.canonicalize().map_err(|e| {
                        EvolutionAdmissionError::new("evolution_workspace_escape", e.to_string())
                    })?;
                    if !canon.starts_with(&root_canon) {
                        return Err(EvolutionAdmissionError::new(
                            "evolution_workspace_escape",
                            "workspace escapes app-owned root",
                        ));
                    }
                    cursor = canon;
                }
            }
            _ => {
                return Err(EvolutionAdmissionError::new(
                    "evolution_workspace_escape",
                    "workspace path contains forbidden component",
                ));
            }
        }
    }
    Ok(cursor)
}

/// Deterministic content hash of the bounded workspace surface (sorted relative paths + contents).
pub fn hash_workspace_directory(workspace_dir: &Path) -> Result<String, EvolutionAdmissionError> {
    if !workspace_dir.is_dir() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_missing",
            "candidate workspace directory is missing",
        ));
    }
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    collect_workspace_files(workspace_dir, workspace_dir, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.len() > MAX_WORKSPACE_FILES {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_bound",
            "workspace file count exceeds bound",
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"harness_evolution_workspace_surface.v1\n");
    for (rel, bytes) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
        hasher.update(b"\n");
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect_workspace_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), EvolutionAdmissionError> {
    let read = std::fs::read_dir(current)
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_read", e.to_string()))?;
    for entry in read {
        let entry = entry
            .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_read", e.to_string()))?;
        let path = entry.path();
        let meta = entry
            .metadata()
            .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_read", e.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(EvolutionAdmissionError::new(
                "evolution_workspace_escape",
                "symlinks are forbidden inside candidate workspaces",
            ));
        }
        if meta.is_dir() {
            collect_workspace_files(root, &path, out)?;
            continue;
        }
        if !meta.is_file() {
            return Err(EvolutionAdmissionError::new(
                "evolution_workspace_escape",
                "non-file workspace entries are forbidden",
            ));
        }
        if meta.len() as usize > MAX_WORKSPACE_FILE_BYTES {
            return Err(EvolutionAdmissionError::new(
                "evolution_workspace_bound",
                "workspace file exceeds size bound",
            ));
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| {
                EvolutionAdmissionError::new(
                    "evolution_workspace_escape",
                    "workspace file escaped root during collection",
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        validate_workspace_relative_path(&rel)?;
        let bytes = std::fs::read(&path)
            .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_read", e.to_string()))?;
        out.push((rel, bytes));
    }
    Ok(())
}

/// Materialize bounded fixture files under the app-owned root and return a workspace descriptor.
pub fn materialize_candidate_workspace(
    root: &Path,
    workspace_id: &str,
    files: &[(String, Vec<u8>)],
) -> Result<CandidateWorkspace, EvolutionAdmissionError> {
    if workspace_id.is_empty()
        || workspace_id.contains('/')
        || workspace_id.contains('\\')
        || workspace_id.contains("..")
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_id",
            "workspace_id must be a single path segment",
        ));
    }
    if files.is_empty() || files.len() > MAX_WORKSPACE_FILES {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_bound",
            "workspace file count out of bound",
        ));
    }
    let relative_path = format!("candidates/{workspace_id}");
    validate_workspace_relative_path(&relative_path)?;
    let dir = resolve_workspace_under_root(root, &relative_path)?;
    if dir.exists() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_exists",
            "workspace directory already exists",
        ));
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_create", e.to_string()))?;
    for (rel, bytes) in files {
        validate_workspace_relative_path(rel)?;
        if bytes.len() > MAX_WORKSPACE_FILE_BYTES {
            return Err(EvolutionAdmissionError::new(
                "evolution_workspace_bound",
                "workspace file exceeds size bound",
            ));
        }
        let target = resolve_workspace_under_root(&dir, rel)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                EvolutionAdmissionError::new("evolution_workspace_create", e.to_string())
            })?;
        }
        std::fs::write(&target, bytes).map_err(|e| {
            EvolutionAdmissionError::new("evolution_workspace_write", e.to_string())
        })?;
    }
    let content_hash = hash_workspace_directory(&dir)?;
    Ok(CandidateWorkspace {
        schema_version: WORKSPACE_SCHEMA_VERSION.to_string(),
        workspace_id: workspace_id.to_string(),
        relative_path,
        content_hash,
    })
}

/// Recompute the workspace surface hash and refuse if it no longer matches the admitted hash.
pub fn revalidate_workspace_content(
    root: &Path,
    workspace: &CandidateWorkspace,
) -> Result<String, EvolutionAdmissionError> {
    let dir = resolve_workspace_under_root(root, &workspace.relative_path)?;
    let actual = hash_workspace_directory(&dir)?;
    if actual != workspace.content_hash {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_tamper",
            "workspace content hash no longer matches admitted surface",
        ));
    }
    Ok(actual)
}

/// Discard an unpromoted candidate workspace directory without touching active main.
pub fn discard_candidate_workspace(
    root: &Path,
    workspace: &CandidateWorkspace,
) -> Result<(), EvolutionAdmissionError> {
    let dir = resolve_workspace_under_root(root, &workspace.relative_path)?;
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir)
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_discard", e.to_string()))?;
    Ok(())
}

/// Derive a stable workspace_id from proposal and seed material.
pub fn derive_workspace_id(proposal_id: &str, seed: u64) -> String {
    let material = format!("workspace.v1|{proposal_id}|{seed}");
    format!("hews-{}", &sha256_hex(&material)[..16])
}

pub fn validate_proposal(proposal: &EvolutionProposal) -> Result<(), EvolutionAdmissionError> {
    if proposal.schema_version != PROPOSAL_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_proposal_schema",
            "proposal schema_version mismatch",
        ));
    }
    if proposal.proposal_id.is_empty()
        || proposal.active_version_id.is_empty()
        || proposal.active_version_hash.is_empty()
        || proposal.evaluator_identity_hash.is_empty()
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_proposal_identity",
            "proposal identity fields required",
        ));
    }
    validate_sha256_hex(&proposal.active_version_hash)?;
    validate_sha256_hex(&proposal.evaluator_identity_hash)?;
    validate_sha256_hex(&proposal.proposal_body_sha256)?;
    if proposal.evidence_hashes.len() > MAX_EVIDENCE_HASHES {
        return Err(EvolutionAdmissionError::new(
            "evolution_evidence_bound",
            "too many evidence hashes",
        ));
    }
    for hash in &proposal.evidence_hashes {
        validate_sha256_hex(hash)?;
    }
    validate_mutable_surface(&proposal.mutable_surface)?;
    let expected = derive_proposal_id(
        &proposal.active_version_id,
        &proposal.proposal_body_sha256,
        proposal.seed,
    );
    if proposal.proposal_id != expected {
        return Err(EvolutionAdmissionError::new(
            "evolution_proposal_id_mismatch",
            "proposal_id is not deterministically derived",
        ));
    }
    Ok(())
}

pub fn validate_candidate_for_admission(
    candidate: &EvolutionCandidate,
    current_active: &ActiveHarnessIdentity,
    parent_still_valid: bool,
) -> Result<(), EvolutionAdmissionError> {
    if !lab_enabled() {
        return Err(EvolutionAdmissionError::new(
            "evolution_lab_disabled",
            "Harness evolution laboratory is default-off",
        ));
    }
    if kill_switch_active() {
        return Err(EvolutionAdmissionError::new(
            "evolution_kill_switch",
            "Harness evolution kill switch is active",
        ));
    }
    if candidate.schema_version != CANDIDATE_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_candidate_schema",
            "candidate schema_version mismatch",
        ));
    }
    if current_active.schema_version != ACTIVE_VERSION_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "evolution_active_version_schema",
            "active version schema mismatch",
        ));
    }
    validate_sha256_hex(&candidate.active_version_hash)?;
    validate_sha256_hex(&candidate.evaluator_identity_hash)?;
    validate_sha256_hex(&candidate.content_hash)?;
    validate_mutable_surface(&candidate.mutable_surface)?;
    validate_workspace_relative_path(&candidate.workspace.relative_path)?;
    if candidate.workspace.schema_version != WORKSPACE_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_schema",
            "workspace schema_version mismatch",
        ));
    }
    validate_sha256_hex(&candidate.workspace.content_hash)?;
    if candidate.content_hash != candidate.workspace.content_hash {
        return Err(EvolutionAdmissionError::new(
            "evolution_content_workspace_mismatch",
            "candidate content_hash must equal workspace surface hash",
        ));
    }

    if candidate.active_version_id != current_active.active_version_id
        || candidate.active_version_hash != current_active.active_version_hash
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_changed_active_version",
            "candidate active-version binding does not match immutable current active Harness",
        ));
    }
    if candidate.evaluator_identity_hash != current_active.evaluator_identity_hash {
        return Err(EvolutionAdmissionError::new(
            "evolution_evaluator_immutable",
            "evaluator identity must remain immutable to the candidate",
        ));
    }
    if candidate.parent_candidate_id.is_some() && !parent_still_valid {
        return Err(EvolutionAdmissionError::new(
            "evolution_stale_parent",
            "parent candidate is stale or missing",
        ));
    }
    let expected_id = derive_candidate_id(
        &candidate.proposal_id,
        &candidate.content_hash,
        candidate.seed,
    );
    if candidate.candidate_id != expected_id {
        return Err(EvolutionAdmissionError::new(
            "evolution_candidate_id_mismatch",
            "candidate_id is not deterministically derived",
        ));
    }
    let expected_lineage = derive_lineage_id(
        candidate.parent_candidate_id.as_deref(),
        &candidate.proposal_id,
    );
    if candidate.lineage_id != expected_lineage {
        return Err(EvolutionAdmissionError::new(
            "evolution_lineage_id_mismatch",
            "lineage_id is not deterministically derived",
        ));
    }
    // Fail closed on forbidden fields that must never appear in durable evidence.
    let as_json = serde_json::to_value(candidate)
        .map_err(|e| EvolutionAdmissionError::new("evolution_candidate_encode", e.to_string()))?;
    refuse_sensitive_payload_fields(&as_json)?;
    Ok(())
}

fn refuse_sensitive_payload_fields(value: &Value) -> Result<(), EvolutionAdmissionError> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let lower = key.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "raw_prompt"
                        | "prompt_text"
                        | "model_output"
                        | "transcript"
                        | "repository_contents"
                        | "secret"
                        | "credential"
                        | "private_path"
                        | "api_key"
                        | "authorization"
                ) {
                    return Err(EvolutionAdmissionError::new(
                        "evolution_sensitive_payload",
                        format!("forbidden durable evidence field: {key}"),
                    ));
                }
                refuse_sensitive_payload_fields(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                refuse_sensitive_payload_fields(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn build_admission_receipt(
    candidate: &EvolutionCandidate,
    created_at: impl Into<String>,
) -> EvolutionReceipt {
    let created_at = created_at.into();
    EvolutionReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION.to_string(),
        receipt_id: derive_receipt_id(&candidate.candidate_id, candidate.terminal_reason),
        candidate_id: candidate.candidate_id.clone(),
        proposal_id: candidate.proposal_id.clone(),
        lineage_id: candidate.lineage_id.clone(),
        active_version_id: candidate.active_version_id.clone(),
        terminal_reason: candidate.terminal_reason,
        content_hash: candidate.content_hash.clone(),
        created_at,
    }
}

pub fn sample_active_identity() -> ActiveHarnessIdentity {
    ActiveHarnessIdentity {
        schema_version: ACTIVE_VERSION_SCHEMA.to_string(),
        active_version_id: "active-harness-v0".to_string(),
        active_version_hash: sha256_hex("active-harness-fixture-body"),
        evaluator_identity_hash: sha256_hex("evaluator-fixture-identity"),
    }
}

pub fn proposal_from_body(
    active: &ActiveHarnessIdentity,
    parent_candidate_id: Option<String>,
    mutable_surfaces: &[&str],
    body: &Value,
    evidence_hashes: Vec<String>,
    seed: u64,
) -> Result<EvolutionProposal, EvolutionAdmissionError> {
    let proposal_body_sha256 = canonical_json_sha256(body)
        .map_err(|e| EvolutionAdmissionError::new("evolution_proposal_body", e))?;
    let proposal_id = derive_proposal_id(&active.active_version_id, &proposal_body_sha256, seed);
    let proposal = EvolutionProposal {
        schema_version: PROPOSAL_SCHEMA_VERSION.to_string(),
        proposal_id,
        parent_candidate_id,
        active_version_id: active.active_version_id.clone(),
        active_version_hash: active.active_version_hash.clone(),
        evaluator_identity_hash: active.evaluator_identity_hash.clone(),
        mutable_surface: MutableSurfaceDeclaration {
            schema_version: MUTABLE_SURFACE_SCHEMA.to_string(),
            surfaces: mutable_surfaces.iter().map(|s| (*s).to_string()).collect(),
        },
        proposal_body_sha256,
        evidence_hashes,
        seed,
    };
    validate_proposal(&proposal)?;
    Ok(proposal)
}

/// Build a candidate bound to an already-materialized app-owned workspace.
///
/// `content_hash` must equal the workspace surface hash (no independent caller authority).
pub fn candidate_from_proposal(
    proposal: &EvolutionProposal,
    workspace: &CandidateWorkspace,
    created_at: impl Into<String>,
) -> Result<EvolutionCandidate, EvolutionAdmissionError> {
    validate_sha256_hex(&workspace.content_hash)?;
    validate_workspace_relative_path(&workspace.relative_path)?;
    if workspace.schema_version != WORKSPACE_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_schema",
            "workspace schema_version mismatch",
        ));
    }
    let content_hash = workspace.content_hash.clone();
    let candidate_id = derive_candidate_id(&proposal.proposal_id, &content_hash, proposal.seed);
    let lineage_id = derive_lineage_id(
        proposal.parent_candidate_id.as_deref(),
        &proposal.proposal_id,
    );
    let candidate = EvolutionCandidate {
        schema_version: CANDIDATE_SCHEMA_VERSION.to_string(),
        candidate_id,
        lineage_id,
        parent_candidate_id: proposal.parent_candidate_id.clone(),
        proposal_id: proposal.proposal_id.clone(),
        active_version_id: proposal.active_version_id.clone(),
        active_version_hash: proposal.active_version_hash.clone(),
        evaluator_identity_hash: proposal.evaluator_identity_hash.clone(),
        mutable_surface: proposal.mutable_surface.clone(),
        workspace: workspace.clone(),
        content_hash,
        status: CandidateStatus::Proposed,
        terminal_reason: CandidateTerminalReason::Admitted,
        seed: proposal.seed,
        created_at: created_at.into(),
    };
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn derives_stable_identities() {
        let active = sample_active_identity();
        let body = json!({"kind":"prompt_tweak","digest":"abc"});
        let p1 = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &body,
            vec![sha256_hex("evidence-1")],
            7,
        )
        .unwrap();
        let p2 = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &body,
            vec![sha256_hex("evidence-1")],
            7,
        )
        .unwrap();
        assert_eq!(p1.proposal_id, p2.proposal_id);
        let ws = CandidateWorkspace {
            schema_version: WORKSPACE_SCHEMA_VERSION.to_string(),
            workspace_id: derive_workspace_id(&p1.proposal_id, p1.seed),
            relative_path: "candidates/c1".to_string(),
            content_hash: sha256_hex("content"),
        };
        let c = candidate_from_proposal(&p1, &ws, "2026-07-21T00:00:00Z").unwrap();
        assert!(c.candidate_id.starts_with("hevc-"));
        assert!(c.lineage_id.starts_with("heln-"));
        assert_eq!(c.content_hash, ws.content_hash);
    }

    #[test]
    fn materializes_and_revalidates_workspace_surface() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let files = vec![("manifest.json".to_string(), b"{\"k\":1}".to_vec())];
        let ws = materialize_candidate_workspace(root, "ws-unit-1", &files).unwrap();
        assert!(ws.relative_path.starts_with("candidates/"));
        revalidate_workspace_content(root, &ws).unwrap();
        // Tamper after materialization is detected.
        let path = resolve_workspace_under_root(root, &ws.relative_path).unwrap();
        std::fs::write(path.join("extra.txt"), b"tamper").unwrap();
        let err = revalidate_workspace_content(root, &ws).unwrap_err();
        assert_eq!(err.code, "evolution_workspace_tamper");
        discard_candidate_workspace(root, &ws).unwrap();
        assert!(!resolve_workspace_under_root(root, &ws.relative_path)
            .unwrap()
            .exists());
    }

    #[test]
    fn rejects_forbidden_mutable_surface() {
        let active = sample_active_identity();
        let err = proposal_from_body(&active, None, &["evaluator"], &json!({"x":1}), vec![], 1)
            .unwrap_err();
        assert_eq!(err.code, "evolution_forbidden_surface");
    }

    #[test]
    fn rejects_workspace_escape() {
        assert!(validate_workspace_relative_path("../etc").is_err());
        assert!(validate_workspace_relative_path("/abs").is_err());
        assert!(validate_workspace_relative_path("ok/path").is_ok());
    }

    struct UnitLabEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_enable: Option<String>,
        prev_kill: Option<String>,
    }

    impl UnitLabEnvGuard {
        fn set(enable: bool, kill: bool) -> Self {
            let lock = EVOLUTION_LAB_TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let prev_enable = std::env::var(ENABLE_ENV).ok();
            let prev_kill = std::env::var(KILL_SWITCH_ENV).ok();
            if enable {
                std::env::set_var(ENABLE_ENV, "1");
            } else {
                std::env::remove_var(ENABLE_ENV);
            }
            if kill {
                std::env::set_var(KILL_SWITCH_ENV, "1");
            } else {
                std::env::remove_var(KILL_SWITCH_ENV);
            }
            Self {
                _lock: lock,
                prev_enable,
                prev_kill,
            }
        }
    }

    impl Drop for UnitLabEnvGuard {
        fn drop(&mut self) {
            match &self.prev_enable {
                Some(v) => std::env::set_var(ENABLE_ENV, v),
                None => std::env::remove_var(ENABLE_ENV),
            }
            match &self.prev_kill {
                Some(v) => std::env::set_var(KILL_SWITCH_ENV, v),
                None => std::env::remove_var(KILL_SWITCH_ENV),
            }
        }
    }

    #[test]
    fn rejects_changed_active_version() {
        let _env = UnitLabEnvGuard::set(true, false);
        let active = sample_active_identity();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"k":"v"}),
            vec![],
            3,
        )
        .unwrap();
        let ws = CandidateWorkspace {
            schema_version: WORKSPACE_SCHEMA_VERSION.to_string(),
            workspace_id: "hews-c2".to_string(),
            relative_path: "candidates/c2".to_string(),
            content_hash: sha256_hex("content"),
        };
        let mut candidate =
            candidate_from_proposal(&proposal, &ws, "2026-07-21T00:00:00Z").unwrap();
        candidate.active_version_hash = sha256_hex("different");
        let err = validate_candidate_for_admission(&candidate, &active, true).unwrap_err();
        assert_eq!(err.code, "evolution_changed_active_version");
    }

    #[test]
    fn rejects_sensitive_payload_fields() {
        let _env = UnitLabEnvGuard::set(true, false);
        let poisoned = json!({"raw_prompt":"secret text","ok":true});
        let err = refuse_sensitive_payload_fields(&poisoned).unwrap_err();
        assert_eq!(err.code, "evolution_sensitive_payload");
    }

    #[test]
    fn kill_switch_fails_closed() {
        let _env = UnitLabEnvGuard::set(true, true);
        let active = sample_active_identity();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"k":"v"}),
            vec![],
            4,
        )
        .unwrap();
        let ws = CandidateWorkspace {
            schema_version: WORKSPACE_SCHEMA_VERSION.to_string(),
            workspace_id: "hews-c4".to_string(),
            relative_path: "candidates/c4".to_string(),
            content_hash: sha256_hex("content"),
        };
        let candidate = candidate_from_proposal(&proposal, &ws, "2026-07-21T00:00:00Z").unwrap();
        let err = validate_candidate_for_admission(&candidate, &active, true).unwrap_err();
        assert_eq!(err.code, "evolution_kill_switch");
    }
}
