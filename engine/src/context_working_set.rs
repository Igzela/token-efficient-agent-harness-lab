//! Pure deterministic working-set projector.
//!
//! Derived, rebuildable projection behind existing context owners. Not a
//! store, evaluator, or EFFECT authority. Implementation disposition for
//! `PE7-CWS-PROJECTOR-CORE-1` is `REIMPLEMENT`: harvest candidates remain
//! `UNKNOWN` or `INELIGIBLE_SOURCE` and are not treated as TRANSPLANT.

use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: &str = "context_working_set.v1";
pub const IMPLEMENTATION_DISPOSITION: &str = "REIMPLEMENT";
pub const REDUCER_DISPOSITION: &str = "REIMPLEMENT";
pub const CACHE_PARTITION_DISPOSITION: &str = "REIMPLEMENT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Residency {
    Pinned,
    Hot,
    Warm,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeKind {
    GitBlob,
    ArtifactRef,
    DeterministicRerun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Authority,
    Blocker,
    OutcomeUnknown,
    Working,
    ToolResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIdentity {
    pub owner: String,
    pub identity: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceItem {
    pub identity: SourceIdentity,
    pub kind: ItemKind,
    pub residency: Residency,
    pub bytes: Vec<u8>,
    pub supersedes: Option<String>,
    pub source_stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectorBounds {
    pub max_bytes: usize,
    pub max_tokens: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RehydrationHandle {
    pub source_owner: String,
    pub source_identity: String,
    pub content_sha256: String,
    pub recipe_kind: RecipeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedItem {
    pub identity: SourceIdentity,
    pub kind: ItemKind,
    pub residency: Residency,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedWorkingSet {
    pub schema_version: String,
    pub prefix: Vec<ProjectedItem>,
    pub dynamic: Vec<ProjectedItem>,
    pub cold_handles: Vec<RehydrationHandle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectorError {
    pub code: String,
    pub message: String,
}

impl ProjectorError {
    fn pinned_exceeds_bound() -> Self {
        Self {
            code: "pinned_exceeds_bound".to_string(),
            message: "PINNED items exceed projector bounds and cannot be evicted".to_string(),
        }
    }

    fn stale_source() -> Self {
        Self {
            code: "stale_source".to_string(),
            message: "stale PINNED source cannot be projected as live bytes".to_string(),
        }
    }
}

pub fn content_sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn token_weight(bytes: &[u8]) -> usize {
    bytes.len().div_ceil(4)
}

fn recipe_for(kind: ItemKind) -> RecipeKind {
    match kind {
        ItemKind::ToolResult => RecipeKind::ArtifactRef,
        ItemKind::Working => RecipeKind::GitBlob,
        ItemKind::Authority | ItemKind::Blocker | ItemKind::OutcomeUnknown => RecipeKind::GitBlob,
    }
}

fn handle_for(item: &SourceItem) -> RehydrationHandle {
    RehydrationHandle {
        source_owner: item.identity.owner.clone(),
        source_identity: item.identity.identity.clone(),
        content_sha256: item.identity.content_sha256.clone(),
        recipe_kind: recipe_for(item.kind),
    }
}

fn projected(item: &SourceItem) -> ProjectedItem {
    ProjectedItem {
        identity: item.identity.clone(),
        kind: item.kind,
        residency: item.residency,
        bytes: item.bytes.clone(),
    }
}

/// Collapse duplicates and apply supersession. Later items win. Identity is
/// owner + identity string; content hash must match bytes.
fn collapse(items: &[SourceItem]) -> Result<Vec<SourceItem>, ProjectorError> {
    let mut by_key: Vec<SourceItem> = Vec::new();
    for item in items {
        let expected = content_sha256(&item.bytes);
        if item.identity.content_sha256 != expected {
            return Err(ProjectorError {
                code: "integrity_mismatch".to_string(),
                message: format!("content_sha256 mismatch for {}", item.identity.identity),
            });
        }
        if matches!(
            item.kind,
            ItemKind::Authority | ItemKind::Blocker | ItemKind::OutcomeUnknown
        ) && item.residency != Residency::Pinned
        {
            return Err(ProjectorError {
                code: "forbidden_eviction".to_string(),
                message: "authority, blockers, and outcome-unknown must stay PINNED".to_string(),
            });
        }
        if let Some(prev) = item.supersedes.as_ref() {
            by_key.retain(|existing| &existing.identity.identity != prev);
        }
        if let Some(pos) = by_key
            .iter()
            .position(|existing| existing.identity.identity == item.identity.identity)
        {
            by_key[pos] = item.clone();
        } else {
            by_key.push(item.clone());
        }
    }
    Ok(by_key)
}

fn residency_rank(residency: Residency) -> u8 {
    match residency {
        Residency::Pinned => 0,
        Residency::Hot => 1,
        Residency::Warm => 2,
        Residency::Cold => 3,
    }
}

/// Deterministic lexicographic order: residency, then identity.
fn sort_items(items: &mut [SourceItem]) {
    items.sort_by(|a, b| {
        residency_rank(a.residency)
            .cmp(&residency_rank(b.residency))
            .then_with(|| a.identity.identity.cmp(&b.identity.identity))
            .then_with(|| a.identity.owner.cmp(&b.identity.owner))
    });
}

pub fn project(
    items: &[SourceItem],
    bounds: ProjectorBounds,
) -> Result<ProjectedWorkingSet, ProjectorError> {
    let mut collapsed = collapse(items)?;
    sort_items(&mut collapsed);

    let mut prefix = Vec::new();
    let mut dynamic = Vec::new();
    let mut cold_handles = Vec::new();
    let mut used_bytes = 0usize;
    let mut used_tokens = 0usize;

    for item in &collapsed {
        if item.source_stale && item.residency == Residency::Pinned {
            return Err(ProjectorError::stale_source());
        }
        if item.source_stale {
            cold_handles.push(handle_for(item));
            continue;
        }
        if item.residency == Residency::Cold {
            cold_handles.push(handle_for(item));
            continue;
        }

        let add_bytes = item.bytes.len();
        let add_tokens = token_weight(&item.bytes);
        let fits = used_bytes.saturating_add(add_bytes) <= bounds.max_bytes
            && used_tokens.saturating_add(add_tokens) <= bounds.max_tokens;

        if item.residency == Residency::Pinned {
            let next_bytes = used_bytes.saturating_add(add_bytes);
            let next_tokens = used_tokens.saturating_add(add_tokens);
            if next_bytes > bounds.max_bytes || next_tokens > bounds.max_tokens {
                return Err(ProjectorError::pinned_exceeds_bound());
            }
            used_bytes = next_bytes;
            used_tokens = next_tokens;
            prefix.push(projected(item));
            continue;
        }

        if fits {
            used_bytes = used_bytes.saturating_add(add_bytes);
            used_tokens = used_tokens.saturating_add(add_tokens);
            dynamic.push(projected(item));
        } else {
            cold_handles.push(handle_for(item));
        }
    }

    Ok(ProjectedWorkingSet {
        schema_version: SCHEMA_VERSION.to_string(),
        prefix,
        dynamic,
        cold_handles,
    })
}

/// Reconstruct live bytes from a handle only when the offered payload hash
/// matches. Never authorizes EFFECT.
pub fn rehydrate(
    handle: &RehydrationHandle,
    offered_bytes: &[u8],
) -> Result<Vec<u8>, ProjectorError> {
    let digest = content_sha256(offered_bytes);
    if digest != handle.content_sha256 {
        return Err(ProjectorError {
            code: "unavailable".to_string(),
            message: "rehydration hash mismatch".to_string(),
        });
    }
    if handle.recipe_kind == RecipeKind::DeterministicRerun {
        return Err(ProjectorError {
            code: "effect_forbidden".to_string(),
            message: "DETERMINISTIC_RERUN that would be an EFFECT is rejected".to_string(),
        });
    }
    Ok(offered_bytes.to_vec())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepositorySessionMode {
    Fresh,
    Resume,
    Repair,
    Review,
    CiRepair,
}

fn valid_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Project already-authorized canonical sources for a repository-maintenance
/// session. Capsules stay non-authoritative; a changed head cannot be hidden.
pub fn project_repository_session(
    accepted_main_sha: &str,
    head_sha: &str,
    packet_id: &str,
    mode: RepositorySessionMode,
    docs: &[SourceItem],
    bounds: ProjectorBounds,
) -> Result<ProjectedWorkingSet, ProjectorError> {
    if !valid_sha(accepted_main_sha) || !valid_sha(head_sha) {
        return Err(ProjectorError {
            code: "binding_invalid".to_string(),
            message: "accepted_main and head must be 40-hex SHAs".to_string(),
        });
    }
    if packet_id.trim().is_empty() {
        return Err(ProjectorError {
            code: "binding_invalid".to_string(),
            message: "packet_id is required".to_string(),
        });
    }
    if mode == RepositorySessionMode::Fresh && accepted_main_sha != head_sha {
        return Err(ProjectorError {
            code: "changed_head".to_string(),
            message: "fresh session cannot hide a checkout that is not accepted main".to_string(),
        });
    }
    for item in docs {
        if item.kind == ItemKind::Authority && item.residency != Residency::Pinned {
            return Err(ProjectorError {
                code: "forbidden_eviction".to_string(),
                message: "canonical authority must stay PINNED in repository sessions".to_string(),
            });
        }
    }
    let mut binding_items = vec![
        identity_item("git", "accepted_main", accepted_main_sha),
        identity_item("git", "head", head_sha),
        identity_item("packet", "packet_id", packet_id),
        identity_item("session", "mode", format!("{mode:?}").as_str()),
    ];
    binding_items.extend(docs.iter().cloned());
    project(&binding_items, bounds)
}

fn identity_item(owner: &str, identity: &str, body: &str) -> SourceItem {
    let bytes = body.as_bytes().to_vec();
    SourceItem {
        identity: SourceIdentity {
            owner: owner.to_string(),
            identity: identity.to_string(),
            content_sha256: content_sha256(&bytes),
        },
        kind: ItemKind::Authority,
        residency: Residency::Pinned,
        bytes,
        supersedes: None,
        source_stale: false,
    }
}

/// Compose a provider-free runtime prompt from an already-projected working
/// set. The provider never becomes the context owner; cancellation and
/// outcome-unknown text are copied verbatim.
pub fn compose_runtime_prompt(
    task_binding: &str,
    projected: &ProjectedWorkingSet,
    user_prompt: &str,
) -> Result<String, ProjectorError> {
    if task_binding.trim().is_empty() {
        return Err(ProjectorError {
            code: "binding_invalid".to_string(),
            message: "runtime prompt requires a task/authority binding".to_string(),
        });
    }
    let mut out = String::new();
    out.push_str("## CWS runtime projection\n");
    out.push_str(&format!("task_binding: {task_binding}\n"));
    out.push_str("prefix:\n");
    for item in &projected.prefix {
        if item.kind == ItemKind::OutcomeUnknown {
            out.push_str("  outcome-unknown ");
        }
        out.push_str(&format!(
            "  PINNED {} {}\n",
            item.identity.identity,
            std::str::from_utf8(&item.bytes).unwrap_or("[binary]")
        ));
    }
    out.push_str("dynamic:\n");
    for item in &projected.dynamic {
        out.push_str(&format!(
            "  {:?} {} {}\n",
            item.residency,
            item.identity.identity,
            std::str::from_utf8(&item.bytes).unwrap_or("[binary]")
        ));
    }
    out.push_str("cold_handles:\n");
    for handle in &projected.cold_handles {
        out.push_str(&format!(
            "  {} {} sha256 {}\n",
            handle.source_identity, handle.content_sha256, handle.source_owner
        ));
    }
    out.push_str("user:\n");
    out.push_str(user_prompt);
    if user_prompt.to_ascii_uppercase().contains("CANCEL") {
        out.push_str("\n[cancellation preserved]\n");
    }
    Ok(out)
}

/// Optional provider-reported cache usage. Missing fields stay missing; they
/// are never coerced to zero and never enter partition identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheTelemetryObservation {
    pub provider_id: String,
    pub cached_input_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePartition {
    pub stable_prefix_digest: String,
    pub dynamic_digest: String,
    pub telemetry: Option<CacheTelemetryObservation>,
}

fn item_fingerprint(item: &ProjectedItem) -> String {
    format!(
        "{}\0{}\0{}\0{:?}",
        item.identity.owner, item.identity.identity, item.identity.content_sha256, item.kind
    )
}

/// Deterministic stable-prefix vs dynamic partition. Telemetry is attached
/// only as observation and cannot authorize work or change digests.
pub fn partition_working_set(
    projected: &ProjectedWorkingSet,
    telemetry: Option<CacheTelemetryObservation>,
) -> Result<CachePartition, ProjectorError> {
    if projected
        .prefix
        .iter()
        .any(|item| item.residency != Residency::Pinned)
    {
        return Err(ProjectorError {
            code: "prefix_contaminated".to_string(),
            message: "stable prefix may only contain PINNED items".to_string(),
        });
    }
    if projected.dynamic.iter().any(|item| {
        matches!(
            item.kind,
            ItemKind::Authority | ItemKind::Blocker | ItemKind::OutcomeUnknown
        )
    }) {
        return Err(ProjectorError {
            code: "prefix_contaminated".to_string(),
            message: "authority/blocker/unknown must not enter the dynamic partition".to_string(),
        });
    }
    let mut prefix_material = String::new();
    for item in &projected.prefix {
        prefix_material.push_str(&item_fingerprint(item));
        prefix_material.push('\n');
    }
    let mut dynamic_material = String::new();
    for item in &projected.dynamic {
        dynamic_material.push_str(&item_fingerprint(item));
        dynamic_material.push('\n');
    }
    for handle in &projected.cold_handles {
        dynamic_material.push_str(&handle.content_sha256);
        dynamic_material.push('\n');
    }
    Ok(CachePartition {
        stable_prefix_digest: content_sha256(prefix_material.as_bytes()),
        dynamic_digest: content_sha256(dynamic_material.as_bytes()),
        telemetry,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolOutcome {
    Success,
    Failure,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResultAdmission {
    pub outcome: ToolOutcome,
    pub raw: Vec<u8>,
    pub artifact_id: String,
    pub owner: String,
    pub max_visible_bytes: usize,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReducedToolResult {
    pub outcome: ToolOutcome,
    pub visible: Vec<u8>,
    pub handle: RehydrationHandle,
    pub truncated: bool,
    pub redacted: bool,
}

fn is_salient_line(line: &str) -> bool {
    let upper = line.to_ascii_uppercase();
    upper.contains("ERROR")
        || upper.contains("FAIL")
        || upper.contains("PANIC")
        || upper.contains("BLOCKER")
        || upper.contains("UNKNOWN")
}

fn bound_visible(text: &str, max_bytes: usize, must_keep_salient: bool) -> (String, bool) {
    if text.len() <= max_bytes {
        return (text.to_string(), false);
    }
    let mut kept = String::new();
    if must_keep_salient {
        for line in text.lines() {
            if is_salient_line(line) {
                let candidate = if kept.is_empty() {
                    line.to_string()
                } else {
                    format!("{kept}\n{line}")
                };
                if candidate.len() <= max_bytes {
                    kept = candidate;
                }
            }
        }
    }
    if kept.len() >= max_bytes {
        return (kept, true);
    }
    let remaining = max_bytes.saturating_sub(kept.len().saturating_add(1));
    let mut split = remaining.min(text.len());
    while split > 0 && !text.is_char_boundary(split) {
        split -= 1;
    }
    let head = text[..split].to_string();
    if kept.is_empty() {
        (head, true)
    } else if head.is_empty() {
        (kept, true)
    } else {
        (format!("{kept}\n{head}"), true)
    }
}

/// Deterministic admission reduction. Raw bytes stay with the artifact owner;
/// the model sees bounded status, diagnostics, and a rehydration handle.
/// Failure/unknown never become success.
pub fn reduce_tool_result(
    admission: &ToolResultAdmission,
) -> Result<ReducedToolResult, ProjectorError> {
    if admission.stale {
        return Err(ProjectorError {
            code: "unavailable".to_string(),
            message: "stale tool-result artifact cannot be reduced".to_string(),
        });
    }
    if admission.artifact_id.trim().is_empty() {
        return Err(ProjectorError {
            code: "unbound_evidence".to_string(),
            message: "tool result has no artifact identity".to_string(),
        });
    }
    let raw_digest = content_sha256(&admission.raw);
    let lossy = String::from_utf8_lossy(&admission.raw);
    let redacted = crate::provider::redaction::redact_sensitive_patterns(&lossy);
    let redacted_flag = redacted != lossy.as_ref();
    let keep_salient = matches!(
        admission.outcome,
        ToolOutcome::Failure | ToolOutcome::Unknown
    );
    let (visible_text, truncated) =
        bound_visible(&redacted, admission.max_visible_bytes.max(1), keep_salient);
    if keep_salient {
        let had_salient = redacted.lines().any(is_salient_line);
        let kept_salient = visible_text.lines().any(is_salient_line);
        if had_salient && !kept_salient {
            return Err(ProjectorError {
                code: "blocker_dropped".to_string(),
                message: "reduction would drop required failure diagnostics".to_string(),
            });
        }
    }
    Ok(ReducedToolResult {
        outcome: admission.outcome,
        visible: visible_text.into_bytes(),
        handle: RehydrationHandle {
            source_owner: admission.owner.clone(),
            source_identity: admission.artifact_id.clone(),
            content_sha256: raw_digest,
            recipe_kind: RecipeKind::ArtifactRef,
        },
        truncated,
        redacted: redacted_flag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, kind: ItemKind, residency: Residency, body: &str) -> SourceItem {
        let bytes = body.as_bytes().to_vec();
        SourceItem {
            identity: SourceIdentity {
                owner: "docs".to_string(),
                identity: id.to_string(),
                content_sha256: content_sha256(&bytes),
            },
            kind,
            residency,
            bytes,
            supersedes: None,
            source_stale: false,
        }
    }

    #[test]
    fn pins_authority_before_hot_and_is_replay_stable() {
        let items = vec![
            item("b-hot", ItemKind::Working, Residency::Hot, "hot-body"),
            item("a-auth", ItemKind::Authority, Residency::Pinned, "auth"),
        ];
        let bounds = ProjectorBounds {
            max_bytes: 10_000,
            max_tokens: 10_000,
        };
        let first = project(&items, bounds).unwrap();
        let second = project(&items, bounds).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.prefix[0].identity.identity, "a-auth");
        assert_eq!(first.dynamic[0].identity.identity, "b-hot");
        assert_eq!(first.schema_version, SCHEMA_VERSION);
        assert_eq!(IMPLEMENTATION_DISPOSITION, "REIMPLEMENT");
    }

    #[test]
    fn supersession_and_duplicate_identity_keep_latest() {
        let mut first = item("same", ItemKind::Working, Residency::Hot, "old");
        let mut second = item("same", ItemKind::Working, Residency::Hot, "new");
        second.supersedes = Some("obsolete".to_string());
        let obsolete = item("obsolete", ItemKind::Working, Residency::Warm, "gone");
        first.identity.content_sha256 = content_sha256(&first.bytes);
        let out = project(
            &[obsolete, first, second],
            ProjectorBounds {
                max_bytes: 10_000,
                max_tokens: 10_000,
            },
        )
        .unwrap();
        assert_eq!(out.dynamic.len(), 1);
        assert_eq!(out.dynamic[0].bytes, b"new");
        assert!(out
            .dynamic
            .iter()
            .all(|item| item.identity.identity != "obsolete"));
    }

    #[test]
    fn byte_and_token_bounds_demote_hot_not_pinned() {
        let auth = item("auth", ItemKind::Authority, Residency::Pinned, "AA");
        let hot = item("hot", ItemKind::Working, Residency::Hot, "HHHHHHHH");
        let out = project(
            &[auth, hot],
            ProjectorBounds {
                max_bytes: 4,
                max_tokens: 10_000,
            },
        )
        .unwrap();
        assert_eq!(out.prefix.len(), 1);
        assert!(out.dynamic.is_empty());
        assert_eq!(out.cold_handles.len(), 1);
        assert_eq!(out.cold_handles[0].source_identity, "hot");
    }

    #[test]
    fn refuses_to_evict_pinned_when_bound_too_small() {
        let auth = item(
            "auth",
            ItemKind::Authority,
            Residency::Pinned,
            "PINNED-TOO-BIG",
        );
        let err = project(
            &[auth],
            ProjectorBounds {
                max_bytes: 2,
                max_tokens: 1,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "pinned_exceeds_bound");
    }

    #[test]
    fn refuses_non_pinned_authority() {
        let bad = item("auth", ItemKind::Authority, Residency::Hot, "nope");
        let err = project(
            &[bad],
            ProjectorBounds {
                max_bytes: 100,
                max_tokens: 100,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "forbidden_eviction");
    }

    #[test]
    fn stale_non_pinned_becomes_unavailable_handle() {
        let mut hot = item("hot", ItemKind::Working, Residency::Hot, "body");
        hot.source_stale = true;
        let out = project(
            &[hot],
            ProjectorBounds {
                max_bytes: 100,
                max_tokens: 100,
            },
        )
        .unwrap();
        assert!(out.dynamic.is_empty());
        assert_eq!(out.cold_handles[0].content_sha256.len(), 64);
    }

    #[test]
    fn stale_pinned_fails_closed() {
        let mut auth = item("auth", ItemKind::Authority, Residency::Pinned, "auth");
        auth.source_stale = true;
        let err = project(
            &[auth],
            ProjectorBounds {
                max_bytes: 100,
                max_tokens: 100,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "stale_source");
    }

    #[test]
    fn delete_and_rebuild_from_same_items() {
        let items = vec![item("a", ItemKind::Working, Residency::Warm, "w")];
        let bounds = ProjectorBounds {
            max_bytes: 100,
            max_tokens: 100,
        };
        let a = project(&items, bounds).unwrap();
        drop(a.clone());
        let b = project(&items, bounds).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rehydrate_matches_hash_and_rejects_mismatch() {
        let body = b"tool-log";
        let handle = RehydrationHandle {
            source_owner: "artifacts".to_string(),
            source_identity: "art-1".to_string(),
            content_sha256: content_sha256(body),
            recipe_kind: RecipeKind::ArtifactRef,
        };
        assert_eq!(rehydrate(&handle, body).unwrap(), body);
        let err = rehydrate(&handle, b"other").unwrap_err();
        assert_eq!(err.code, "unavailable");
    }

    #[test]
    fn integrity_mismatch_on_input_is_not_success() {
        let mut item = item("x", ItemKind::Working, Residency::Hot, "body");
        item.identity.content_sha256 = "00".repeat(32);
        let err = project(
            &[item],
            ProjectorBounds {
                max_bytes: 100,
                max_tokens: 100,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "integrity_mismatch");
    }

    fn admit(outcome: ToolOutcome, raw: &str, max_visible: usize) -> ToolResultAdmission {
        ToolResultAdmission {
            outcome,
            raw: raw.as_bytes().to_vec(),
            artifact_id: "art-tool-1".to_string(),
            owner: "artifacts".to_string(),
            max_visible_bytes: max_visible,
            stale: false,
        }
    }

    #[test]
    fn failure_log_keeps_error_line_and_never_becomes_success() {
        let raw = "ok start\nerror: compile failed\n".repeat(40);
        let reduced = reduce_tool_result(&admit(ToolOutcome::Failure, &raw, 80)).unwrap();
        assert_eq!(reduced.outcome, ToolOutcome::Failure);
        assert!(reduced.truncated);
        let visible = String::from_utf8(reduced.visible).unwrap();
        assert!(visible.to_ascii_uppercase().contains("ERROR"));
        assert_eq!(REDUCER_DISPOSITION, "REIMPLEMENT");
        assert_eq!(
            rehydrate(&reduced.handle, raw.as_bytes()).unwrap(),
            raw.as_bytes()
        );
    }

    #[test]
    fn success_truncation_does_not_invent_failure() {
        let raw = "a".repeat(200);
        let reduced = reduce_tool_result(&admit(ToolOutcome::Success, &raw, 40)).unwrap();
        assert_eq!(reduced.outcome, ToolOutcome::Success);
        assert!(reduced.truncated);
        assert!(reduced.visible.len() <= 40);
    }

    #[test]
    fn unknown_outcome_stays_unknown() {
        let reduced = reduce_tool_result(&admit(
            ToolOutcome::Unknown,
            "status UNKNOWN: probe incomplete",
            200,
        ))
        .unwrap();
        assert_eq!(reduced.outcome, ToolOutcome::Unknown);
    }

    #[test]
    fn redacts_secret_patterns_without_dropping_failure() {
        let raw = "error: boom\napi_key=sk-abcdefghijklmnopqrstuvwxyz\n";
        let reduced = reduce_tool_result(&admit(ToolOutcome::Failure, raw, 400)).unwrap();
        let visible = String::from_utf8(reduced.visible).unwrap();
        assert!(reduced.redacted);
        assert!(!visible.contains("sk-abcdefghijklmnopqrstuvwxyz"));
        assert!(visible.to_ascii_uppercase().contains("ERROR"));
    }

    #[test]
    fn malformed_bytes_are_lossy_and_hash_binds_raw() {
        let mut raw = b"error: ".to_vec();
        raw.extend_from_slice(&[0xff, 0xfe]);
        let admission = ToolResultAdmission {
            outcome: ToolOutcome::Failure,
            raw: raw.clone(),
            artifact_id: "art-bin".to_string(),
            owner: "artifacts".to_string(),
            max_visible_bytes: 100,
            stale: false,
        };
        let reduced = reduce_tool_result(&admission).unwrap();
        assert_eq!(rehydrate(&reduced.handle, &raw).unwrap(), raw);
    }

    #[test]
    fn stale_or_unbound_tool_result_fails_closed() {
        let mut stale = admit(ToolOutcome::Success, "ok", 10);
        stale.stale = true;
        assert_eq!(reduce_tool_result(&stale).unwrap_err().code, "unavailable");
        let mut unbound = admit(ToolOutcome::Failure, "error: x", 10);
        unbound.artifact_id.clear();
        assert_eq!(
            reduce_tool_result(&unbound).unwrap_err().code,
            "unbound_evidence"
        );
    }

    #[test]
    fn tiny_bound_that_cannot_keep_failure_line_is_blocker_dropped() {
        let err = reduce_tool_result(&admit(
            ToolOutcome::Failure,
            "error: compile failed at src/lib.rs",
            3,
        ))
        .unwrap_err();
        assert_eq!(err.code, "blocker_dropped");
    }

    fn bound() -> ProjectorBounds {
        ProjectorBounds {
            max_bytes: 10_000,
            max_tokens: 10_000,
        }
    }

    #[test]
    fn fresh_session_requires_head_equal_accepted_main() {
        let err = project_repository_session(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "PE7-CWS-REPOSITORY-INTEGRATION-1",
            RepositorySessionMode::Fresh,
            &[],
            bound(),
        )
        .unwrap_err();
        assert_eq!(err.code, "changed_head");
    }

    #[test]
    fn repair_session_keeps_distinct_main_and_head_and_dedupes_docs() {
        let sha = "cccccccccccccccccccccccccccccccccccccccc";
        let head = "dddddddddddddddddddddddddddddddddddddddd";
        let doc = item(
            "docs/CURRENT_STATUS.md",
            ItemKind::Authority,
            Residency::Pinned,
            "status-body",
        );
        let dup = item(
            "docs/CURRENT_STATUS.md",
            ItemKind::Authority,
            Residency::Pinned,
            "status-body",
        );
        let out = project_repository_session(
            sha,
            head,
            "PE7-CWS-REPOSITORY-INTEGRATION-1",
            RepositorySessionMode::Repair,
            &[doc, dup],
            bound(),
        )
        .unwrap();
        let status_hits = out
            .prefix
            .iter()
            .filter(|item| item.identity.identity == "docs/CURRENT_STATUS.md")
            .count();
        assert_eq!(status_hits, 1);
        assert!(out
            .prefix
            .iter()
            .any(|item| item.identity.identity == "accepted_main" && item.bytes == sha.as_bytes()));
        assert!(out
            .prefix
            .iter()
            .any(|item| item.identity.identity == "head" && item.bytes == head.as_bytes()));
        assert!(out
            .prefix
            .iter()
            .any(|item| item.identity.identity == "packet_id"));
    }

    #[test]
    fn review_and_ci_repair_modes_project_and_rehydrate() {
        let main = "1111111111111111111111111111111111111111";
        let head = "2222222222222222222222222222222222222222";
        for mode in [
            RepositorySessionMode::Review,
            RepositorySessionMode::CiRepair,
            RepositorySessionMode::Resume,
        ] {
            let out = project_repository_session(
                main,
                head,
                "PE7-CWS-REPOSITORY-INTEGRATION-1",
                mode,
                &[],
                bound(),
            )
            .unwrap();
            let handle = RehydrationHandle {
                source_owner: "git".to_string(),
                source_identity: "head".to_string(),
                content_sha256: content_sha256(head.as_bytes()),
                recipe_kind: RecipeKind::GitBlob,
            };
            assert_eq!(
                rehydrate(&handle, head.as_bytes()).unwrap(),
                head.as_bytes()
            );
            assert_eq!(out.prefix.len(), 4);
        }
    }

    #[test]
    fn invalid_sha_or_empty_packet_fails_closed() {
        assert_eq!(
            project_repository_session(
                "not-a-sha",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "P",
                RepositorySessionMode::Fresh,
                &[],
                bound(),
            )
            .unwrap_err()
            .code,
            "binding_invalid"
        );
        assert_eq!(
            project_repository_session(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "  ",
                RepositorySessionMode::Fresh,
                &[],
                bound(),
            )
            .unwrap_err()
            .code,
            "binding_invalid"
        );
    }

    #[test]
    fn runtime_prompt_keeps_unknown_and_cancellation() {
        let sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let unknown = item(
            "unknown-receipt",
            ItemKind::OutcomeUnknown,
            Residency::Pinned,
            "outcome-unknown: lease expired",
        );
        let projected = project_repository_session(
            sha,
            sha,
            "PE7-CWS-RUNTIME-INTEGRATION-1",
            RepositorySessionMode::Fresh,
            &[unknown],
            bound(),
        )
        .unwrap();
        let prompt = compose_runtime_prompt("ptask-1", &projected, "CANCEL worker").unwrap();
        assert!(prompt.contains("ptask-1"));
        assert!(prompt.contains("outcome-unknown"));
        assert!(prompt.contains("[cancellation preserved]"));
        assert!(prompt.contains("PINNED"));
    }

    #[test]
    fn empty_task_binding_is_rejected() {
        let empty = ProjectedWorkingSet {
            schema_version: SCHEMA_VERSION.to_string(),
            prefix: vec![],
            dynamic: vec![],
            cold_handles: vec![],
        };
        assert_eq!(
            compose_runtime_prompt("  ", &empty, "x").unwrap_err().code,
            "binding_invalid"
        );
    }

    #[tokio::test]
    async fn stub_provider_hashes_composed_prompt_without_owning_context() {
        use crate::provider::Provider;
        let sha = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let projected = project_repository_session(
            sha,
            sha,
            "PE7-CWS-RUNTIME-INTEGRATION-1",
            RepositorySessionMode::Fresh,
            &[],
            bound(),
        )
        .unwrap();
        let prompt = compose_runtime_prompt("ptask-2", &projected, "run node").unwrap();
        let stub = crate::provider::stub::StubProvider::new("stub-cws");
        let request =
            crate::provider::ProviderRequest::local_stub("stub-cws", "stub-model", &prompt);
        let first = stub.invoke(&request).await.unwrap();
        let second = stub.invoke(&request).await.unwrap();
        assert_eq!(first.output, second.output);
        assert!(first.output.contains("stub-cws"));
    }

    fn projected_sample() -> ProjectedWorkingSet {
        project(
            &[
                item("auth", ItemKind::Authority, Residency::Pinned, "pin"),
                item("hot-work", ItemKind::Working, Residency::Hot, "dyn"),
            ],
            bound(),
        )
        .unwrap()
    }

    #[test]
    fn partition_ignores_cache_telemetry_and_missingness() {
        let projected = projected_sample();
        let none = partition_working_set(&projected, None).unwrap();
        let hit = partition_working_set(
            &projected,
            Some(CacheTelemetryObservation {
                provider_id: "stub".to_string(),
                cached_input_tokens: Some(12),
                cache_write_tokens: None,
            }),
        )
        .unwrap();
        let write = partition_working_set(
            &projected,
            Some(CacheTelemetryObservation {
                provider_id: "stub".to_string(),
                cached_input_tokens: None,
                cache_write_tokens: Some(99),
            }),
        )
        .unwrap();
        assert_eq!(none.stable_prefix_digest, hit.stable_prefix_digest);
        assert_eq!(none.dynamic_digest, hit.dynamic_digest);
        assert_eq!(hit.stable_prefix_digest, write.stable_prefix_digest);
        assert_eq!(hit.dynamic_digest, write.dynamic_digest);
        assert!(none.telemetry.is_none());
        assert!(hit.telemetry.as_ref().unwrap().cache_write_tokens.is_none());
        assert_eq!(CACHE_PARTITION_DISPOSITION, "REIMPLEMENT");
        let replay = partition_working_set(&projected, None).unwrap();
        assert_eq!(replay, none);
    }

    #[test]
    fn mutating_dynamic_invalidates_only_dynamic_digest() {
        let auth = item("auth", ItemKind::Authority, Residency::Pinned, "pin");
        let a = project(
            &[
                auth.clone(),
                item("w1", ItemKind::Working, Residency::Hot, "one"),
            ],
            bound(),
        )
        .unwrap();
        let b = project(
            &[auth, item("w2", ItemKind::Working, Residency::Hot, "two")],
            bound(),
        )
        .unwrap();
        let pa = partition_working_set(&a, None).unwrap();
        let pb = partition_working_set(&b, None).unwrap();
        assert_eq!(pa.stable_prefix_digest, pb.stable_prefix_digest);
        assert_ne!(pa.dynamic_digest, pb.dynamic_digest);
    }

    #[test]
    fn authority_in_dynamic_is_prefix_contamination() {
        let projected = ProjectedWorkingSet {
            schema_version: SCHEMA_VERSION.to_string(),
            prefix: vec![],
            dynamic: vec![ProjectedItem {
                identity: SourceIdentity {
                    owner: "docs".to_string(),
                    identity: "auth".to_string(),
                    content_sha256: content_sha256(b"x"),
                },
                kind: ItemKind::Authority,
                residency: Residency::Hot,
                bytes: b"x".to_vec(),
            }],
            cold_handles: vec![],
        };
        assert_eq!(
            partition_working_set(&projected, None).unwrap_err().code,
            "prefix_contaminated"
        );
    }

    #[test]
    fn unsupported_provider_telemetry_stays_observational() {
        let projected = projected_sample();
        let part = partition_working_set(
            &projected,
            Some(CacheTelemetryObservation {
                provider_id: "unknown-provider".to_string(),
                cached_input_tokens: None,
                cache_write_tokens: None,
            }),
        )
        .unwrap();
        assert!(part
            .telemetry
            .as_ref()
            .unwrap()
            .cached_input_tokens
            .is_none());
        assert_eq!(
            part.stable_prefix_digest,
            partition_working_set(&projected, None)
                .unwrap()
                .stable_prefix_digest
        );
    }
}
