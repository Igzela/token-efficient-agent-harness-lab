//! Versioned, hash-bound first RWE corpus identity (pre-convergence baseline).

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub const RWE_CORPUS_SCHEMA: &str = "rwe_first_corpus.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RweTaskClass {
    BoundedSourceEdit,
    FocusedBugRepair,
    SmallTestAddition,
    DocsCodeSync,
    ControlledFailureOrCancel,
}

impl RweTaskClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BoundedSourceEdit => "bounded_source_edit",
            Self::FocusedBugRepair => "focused_bug_repair",
            Self::SmallTestAddition => "small_test_addition",
            Self::DocsCodeSync => "docs_code_sync",
            Self::ControlledFailureOrCancel => "controlled_failure_or_cancel",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RweTaskSpec {
    pub task_class: RweTaskClass,
    /// Hash of task text only — raw task text is not stored in evidence.
    pub task_text_sha256: String,
    pub verification_commands: Vec<String>,
    pub timeout_ms: u64,
    pub allow_cancel_case: bool,
    pub max_provider_requests: u64,
    pub max_retries: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstRweCorpus {
    pub schema_version: String,
    pub corpus_id: String,
    pub corpus_sha256: String,
    pub disposable_target_repo: String,
    pub target_main_sha_required: bool,
    pub admitted_executor: String,
    pub admitted_codex_version: String,
    pub draft_pr_only: bool,
    pub auto_merge_disabled: bool,
    pub tasks: Vec<RweTaskSpec>,
    pub notes: Vec<String>,
}

impl FirstRweCorpus {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "corpus_id": self.corpus_id,
            "corpus_sha256": self.corpus_sha256,
            "disposable_target_repo": self.disposable_target_repo,
            "target_main_sha_required": self.target_main_sha_required,
            "admitted_executor": self.admitted_executor,
            "admitted_codex_version": self.admitted_codex_version,
            "draft_pr_only": self.draft_pr_only,
            "auto_merge_disabled": self.auto_merge_disabled,
            "tasks": self.tasks.iter().map(|t| json!({
                "task_class": t.task_class.as_str(),
                "task_text_sha256": t.task_text_sha256,
                "verification_commands": t.verification_commands,
                "timeout_ms": t.timeout_ms,
                "allow_cancel_case": t.allow_cancel_case,
                "max_provider_requests": t.max_provider_requests,
                "max_retries": t.max_retries,
            })).collect::<Vec<_>>(),
            "notes": self.notes,
            "raw_task_text_stored": false,
            "live_execution_authorized_by_this_corpus": false,
        })
    }
}

fn task_text_hash(label: &str) -> String {
    // Stable fixture labels — not real operator task text.
    hex::encode(Sha256::digest(
        format!("rwe-fixture-task-v1:{label}").as_bytes(),
    ))
}

/// Freeze the first RWE corpus contract (provider-free; no live run).
pub fn freeze_first_rwe_corpus() -> FirstRweCorpus {
    let tasks = vec![
        RweTaskSpec {
            task_class: RweTaskClass::BoundedSourceEdit,
            task_text_sha256: task_text_hash("bounded-source-edit"),
            verification_commands: vec!["cargo test -p engine --lib".into()],
            timeout_ms: 600_000,
            allow_cancel_case: false,
            max_provider_requests: 1,
            max_retries: 0,
        },
        RweTaskSpec {
            task_class: RweTaskClass::FocusedBugRepair,
            task_text_sha256: task_text_hash("focused-bug-repair"),
            verification_commands: vec!["cargo test -p engine --lib".into()],
            timeout_ms: 600_000,
            allow_cancel_case: false,
            max_provider_requests: 1,
            max_retries: 0,
        },
        RweTaskSpec {
            task_class: RweTaskClass::SmallTestAddition,
            task_text_sha256: task_text_hash("small-test-addition"),
            verification_commands: vec!["cargo test -p engine --lib".into()],
            timeout_ms: 600_000,
            allow_cancel_case: false,
            max_provider_requests: 1,
            max_retries: 0,
        },
        RweTaskSpec {
            task_class: RweTaskClass::DocsCodeSync,
            task_text_sha256: task_text_hash("docs-code-sync"),
            verification_commands: vec![
                "uv run --no-project python scripts/check_agent_handoff.py".into(),
            ],
            timeout_ms: 300_000,
            allow_cancel_case: false,
            max_provider_requests: 1,
            max_retries: 0,
        },
        RweTaskSpec {
            task_class: RweTaskClass::ControlledFailureOrCancel,
            task_text_sha256: task_text_hash("controlled-failure-or-cancel"),
            verification_commands: vec![],
            timeout_ms: 120_000,
            allow_cancel_case: true,
            max_provider_requests: 1,
            max_retries: 0,
        },
    ];
    let mut corpus = FirstRweCorpus {
        schema_version: RWE_CORPUS_SCHEMA.to_string(),
        corpus_id: "rwe-first-baseline-2026-07-25".into(),
        corpus_sha256: String::new(),
        disposable_target_repo: "Igzela/pe7-golden-path-acceptance-fixture".into(),
        target_main_sha_required: true,
        admitted_executor: "codex_cli_mediated".into(),
        admitted_codex_version: crate::cli::config::ADMITTED_CODEX_VERSION.to_string(),
        draft_pr_only: true,
        auto_merge_disabled: true,
        tasks,
        notes: vec![
            "Pre-convergence baseline corpus; do not tune from later Architecture Convergence results.".into(),
            "Live RWE requires separate persisted operator spend authorization (not this freeze alone).".into(),
            "Task text is hash-bound only; raw task text is not stored in evidence.".into(),
        ],
    };
    let mut body = corpus.to_json();
    if let Some(obj) = body.as_object_mut() {
        obj.remove("corpus_sha256");
    }
    corpus.corpus_sha256 = hex::encode(Sha256::digest(body.to_string().as_bytes()));
    corpus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_corpus_is_hash_bound_and_provider_free() {
        let c = freeze_first_rwe_corpus();
        assert_eq!(c.schema_version, RWE_CORPUS_SCHEMA);
        assert_eq!(c.tasks.len(), 5);
        assert!(c.draft_pr_only);
        assert!(c.auto_merge_disabled);
        assert_eq!(c.corpus_sha256.len(), 64);
        let j = c.to_json();
        assert_eq!(j["raw_task_text_stored"], false);
        assert_eq!(j["live_execution_authorized_by_this_corpus"], false);
        assert!(!j.to_string().contains("sk-"));
    }
}
