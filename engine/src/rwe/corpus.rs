//! Versioned, hash-bound first RWE corpus loaded from real task-definition fixtures.

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

pub const RWE_CORPUS_SCHEMA: &str = "rwe_first_corpus.v1";
pub const RWE_TASK_DEFINITION_SCHEMA: &str = "rwe_task_definition.v1";

/// Default fixture root relative to the engine crate.
pub fn default_corpus_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/rwe/first_corpus/v1")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RweTaskDefinition {
    pub task_id: String,
    pub class: String,
    pub definition_path: String,
    pub definition_sha256: String,
    pub objective_sha256: String,
    pub source_repository: String,
    pub source_commit: String,
    pub source_tree_hash: String,
    pub allowed_mutable_paths: Vec<String>,
    pub expected_verification_commands: Vec<String>,
    pub expected_outcome_class: String,
    pub patch_max_files: u64,
    pub patch_max_lines: u64,
    pub timeout_ms: u64,
    pub cancel_behavior: String,
    pub executor_identity: String,
    pub model_identity: String,
    pub per_task_max_provider_requests: u64,
    pub per_task_max_retries: u64,
    pub per_task_max_input_tokens: u64,
    pub per_task_max_output_tokens: u64,
    pub per_task_max_total_tokens: u64,
    pub deterministic_seed: u64,
    pub cleanup_rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstRweCorpus {
    pub schema_version: String,
    pub corpus_id: String,
    pub corpus_sha256: String,
    pub fixture_root: String,
    pub disposable_target_repo: String,
    pub target_main_sha_required: bool,
    pub admitted_executor: String,
    pub admitted_codex_version: String,
    pub draft_pr_only: bool,
    pub auto_merge_disabled: bool,
    pub tasks: Vec<RweTaskDefinition>,
    pub notes: Vec<String>,
}

impl FirstRweCorpus {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "corpus_id": self.corpus_id,
            "corpus_sha256": self.corpus_sha256,
            "fixture_root": self.fixture_root,
            "disposable_target_repo": self.disposable_target_repo,
            "target_main_sha_required": self.target_main_sha_required,
            "admitted_executor": self.admitted_executor,
            "admitted_codex_version": self.admitted_codex_version,
            "draft_pr_only": self.draft_pr_only,
            "auto_merge_disabled": self.auto_merge_disabled,
            "tasks": self.tasks.iter().map(|t| t.to_json()).collect::<Vec<_>>(),
            "notes": self.notes,
            "raw_task_text_stored_in_evidence": false,
            "live_execution_authorized_by_this_corpus": false,
        })
    }
}

impl RweTaskDefinition {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": RWE_TASK_DEFINITION_SCHEMA,
            "task_id": self.task_id,
            "class": self.class,
            "definition_path": self.definition_path,
            "definition_sha256": self.definition_sha256,
            "objective_sha256": self.objective_sha256,
            "source_repository": self.source_repository,
            "source_commit": self.source_commit,
            "source_tree_hash": self.source_tree_hash,
            "allowed_mutable_paths": self.allowed_mutable_paths,
            "expected_verification_commands": self.expected_verification_commands,
            "expected_outcome_class": self.expected_outcome_class,
            "patch_max_files": self.patch_max_files,
            "patch_max_lines": self.patch_max_lines,
            "timeout_ms": self.timeout_ms,
            "cancel_behavior": self.cancel_behavior,
            "executor_identity": self.executor_identity,
            "model_identity": self.model_identity,
            "per_task_max_provider_requests": self.per_task_max_provider_requests,
            "per_task_max_retries": self.per_task_max_retries,
            "per_task_max_input_tokens": self.per_task_max_input_tokens,
            "per_task_max_output_tokens": self.per_task_max_output_tokens,
            "per_task_max_total_tokens": self.per_task_max_total_tokens,
            "deterministic_seed": self.deterministic_seed,
            "cleanup_rules": self.cleanup_rules,
        })
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                if let Some(v) = map.get(&k) {
                    out.insert(k, sort_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

/// Load and freeze the first RWE corpus from versioned fixture task definitions.
pub fn freeze_first_rwe_corpus() -> Result<FirstRweCorpus, String> {
    freeze_first_rwe_corpus_from_root(&default_corpus_fixture_root())
}

pub fn freeze_first_rwe_corpus_from_root(root: &Path) -> Result<FirstRweCorpus, String> {
    freeze_rwe_corpus_from_root(
        root,
        "rwe-first-corpus-v1",
        "operator-supplied-disposable-only",
        "codex-cli-api-key-mediated",
        "0.145.0",
        vec![
            "Corpus binds real task-definition files under engine/fixtures/rwe/first_corpus/v1."
                .into(),
            "Objective text is hash-bound only in operational evidence.".into(),
            "Live RWE requires separate store-owned RweRunAuthorization.".into(),
            "Not a live baseline until authorized live evidence is sealed.".into(),
        ],
    )
}

/// Generic versioned-task corpus freeze shared by the fixture corpus and the
/// operator-approved real corpus (see `rwe::operator_corpus`). The canonical
/// authority body and its hash are a pure function of the root's task files and
/// the identity constants, so replay after Architecture Convergence re-derives
/// identical hashes from the same accepted-main artifacts.
pub(crate) fn freeze_rwe_corpus_from_root(
    root: &Path,
    corpus_id: &str,
    disposable_target_repo: &str,
    admitted_executor: &str,
    admitted_codex_version: &str,
    notes: Vec<String>,
) -> Result<FirstRweCorpus, String> {
    let tasks_dir = root.join("tasks");
    if !tasks_dir.is_dir() {
        return Err(format!(
            "RWE corpus fixture tasks dir missing: {}",
            tasks_dir.display()
        ));
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&tasks_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err("RWE corpus has no task definition files".into());
    }
    let mut tasks = Vec::new();
    for path in paths {
        let raw = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let definition_sha256 = sha256_hex(&raw);
        let v: Value =
            serde_json::from_slice(&raw).map_err(|e| format!("{}: {e}", path.display()))?;
        if v.get("schema_version").and_then(Value::as_str) != Some(RWE_TASK_DEFINITION_SCHEMA) {
            return Err(format!("{}: unexpected schema_version", path.display()));
        }
        let objective = v
            .get("objective")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{}: objective required", path.display()))?;
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        tasks.push(RweTaskDefinition {
            task_id: required_str(&v, "task_id")?,
            class: required_str(&v, "class")?,
            definition_path: rel,
            definition_sha256,
            objective_sha256: sha256_hex(objective.as_bytes()),
            source_repository: required_str(&v, "source_repository")?,
            source_commit: required_str(&v, "source_commit")?,
            source_tree_hash: required_str(&v, "source_tree_hash")?,
            allowed_mutable_paths: string_array(&v, "allowed_mutable_paths")?,
            expected_verification_commands: string_array(&v, "expected_verification_commands")?,
            expected_outcome_class: required_str(&v, "expected_outcome_class")?,
            patch_max_files: v
                .get("patch_max_files")
                .and_then(Value::as_u64)
                .unwrap_or(3),
            patch_max_lines: v
                .get("patch_max_lines")
                .and_then(Value::as_u64)
                .unwrap_or(80),
            timeout_ms: v
                .get("timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(180_000),
            cancel_behavior: required_str(&v, "cancel_behavior")?,
            executor_identity: required_str(&v, "executor_identity")?,
            model_identity: required_str(&v, "model_identity")?,
            per_task_max_provider_requests: v
                .get("per_task_max_provider_requests")
                .and_then(Value::as_u64)
                .unwrap_or(1),
            per_task_max_retries: v
                .get("per_task_max_retries")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            per_task_max_input_tokens: {
                let val = v
                    .get("per_task_max_input_tokens")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        format!("{}: per_task_max_input_tokens required", path.display())
                    })?;
                if val == 0 {
                    return Err(format!(
                        "{}: per_task_max_input_tokens must be positive",
                        path.display()
                    ));
                }
                val
            },
            per_task_max_output_tokens: {
                let val = v
                    .get("per_task_max_output_tokens")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        format!("{}: per_task_max_output_tokens required", path.display())
                    })?;
                if val == 0 {
                    return Err(format!(
                        "{}: per_task_max_output_tokens must be positive",
                        path.display()
                    ));
                }
                val
            },
            per_task_max_total_tokens: {
                let val = v
                    .get("per_task_max_total_tokens")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        format!("{}: per_task_max_total_tokens required", path.display())
                    })?;
                let input = v
                    .get("per_task_max_input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                let output = v
                    .get("per_task_max_output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                if input.saturating_add(output) != val {
                    return Err(format!(
                        "{}: per_task_max_input_tokens + per_task_max_output_tokens must equal per_task_max_total_tokens",
                        path.display()
                    ));
                }
                val
            },
            deterministic_seed: v
                .get("deterministic_seed")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cleanup_rules: string_array(&v, "cleanup_rules")?,
        });
    }
    let mut authority_body = json!({
        "schema_version": RWE_CORPUS_SCHEMA,
        "corpus_id": corpus_id,
        "tasks": tasks.iter().map(|t| t.to_json()).collect::<Vec<_>>(),
        "disposable_target_repo": disposable_target_repo,
        "target_main_sha_required": true,
        "admitted_executor": admitted_executor,
        "admitted_codex_version": admitted_codex_version,
        "draft_pr_only": true,
        "auto_merge_disabled": true,
    });
    authority_body = sort_value(&authority_body);
    let corpus_sha256 = sha256_hex(authority_body.to_string().as_bytes());
    Ok(FirstRweCorpus {
        schema_version: RWE_CORPUS_SCHEMA.into(),
        corpus_id: corpus_id.into(),
        corpus_sha256,
        fixture_root: root.display().to_string(),
        disposable_target_repo: disposable_target_repo.into(),
        target_main_sha_required: true,
        admitted_executor: admitted_executor.into(),
        admitted_codex_version: admitted_codex_version.into(),
        draft_pr_only: true,
        auto_merge_disabled: true,
        tasks,
        notes,
    })
}

fn required_str(v: &Value, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("{key} required"))
}

fn string_array(v: &Value, key: &str) -> Result<Vec<String>, String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .ok_or_else(|| format!("{key} required array"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freezes_real_fixture_task_definitions() {
        let corpus = freeze_first_rwe_corpus().expect("corpus");
        assert_eq!(corpus.tasks.len(), 5);
        assert_eq!(corpus.corpus_sha256.len(), 64);
        for t in &corpus.tasks {
            assert_eq!(t.definition_sha256.len(), 64);
            assert_eq!(t.objective_sha256.len(), 64);
            assert!(!t.definition_path.is_empty());
            assert_eq!(t.per_task_max_retries, 0);
            assert_eq!(t.per_task_max_provider_requests, 1);
        }
        let again = freeze_first_rwe_corpus().unwrap();
        assert_eq!(corpus.corpus_sha256, again.corpus_sha256);
        // Must not be generic label-only hashes of class names alone.
        assert!(corpus
            .tasks
            .iter()
            .any(|t| t.class == "bounded_source_edit"));
        assert_ne!(
            corpus.tasks[0].definition_sha256,
            sha256_hex(b"rwe-fixture-task-v1:bounded-source-edit")
        );
    }

    #[test]
    fn stage_3_rwe_seal_is_bound() {
        assert_eq!(
            crate::rwe::RESEARCH_MAINLINE_STAGE_3_RWE_SEAL,
            "MISSION-RESEARCH-20260901:stage-3:rwe-evidence-basis.v1",
        );
    }
}
