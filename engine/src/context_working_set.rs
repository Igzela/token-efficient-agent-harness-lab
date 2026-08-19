//! Pure deterministic working-set projector.
//!
//! Derived, rebuildable projection behind existing context owners. Not a
//! store, evaluator, or EFFECT authority. Implementation disposition for
//! `PE7-CWS-PROJECTOR-CORE-1` is `REIMPLEMENT`: harvest candidates remain
//! `UNKNOWN` or `INELIGIBLE_SOURCE` and are not treated as TRANSPLANT.

use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: &str = "context_working_set.v1";
pub const IMPLEMENTATION_DISPOSITION: &str = "REIMPLEMENT";

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
}
