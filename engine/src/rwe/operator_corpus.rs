//! Operator-approved real-workload corpus freeze (provider-free).
//!
//! Strictly separated from the fixture corpus: distinct canonical root
//! (`engine/rwe/corpora/<corpus-id>/v2/`), distinct loader entry point, real
//! non-fixture repository identities. The fixture loader and its root
//! (`engine/fixtures/rwe/first_corpus/v1`) are untouched.
//!
//! Versioning: the v1 freeze (Decision B artifacts at main `3c6cd00f`) remains
//! in place under `.../v1/` and is never overwritten. This candidate adds a v2
//! freeze under `.../v2/` for the compatibility delta reported by the operator
//! calibration (output-token envelope 4000 -> 8192); independent evidence,
//! review, and CI acceptance remain required. The v1 failed-attempt live
//! evidence remains valid failure evidence. Both versions are frozen
//! deterministically by the canonical Rust freeze functions.

use std::path::PathBuf;

use super::corpus::{freeze_rwe_corpus_from_root, FirstRweCorpus};
use super::execution_schedule::freeze_operator_execution_schedule_from_root;

pub const OPERATOR_V1_CORPUS_ID: &str = "rwe-minimum-first-corpus-v1";
pub const OPERATOR_V1_CORPUS_RELATIVE_ROOT: &str = "rwe/corpora/rwe-minimum-first-corpus/v1";
pub const OPERATOR_V2_CORPUS_ID: &str = "rwe-minimum-first-corpus-v2";
pub const OPERATOR_V2_CORPUS_RELATIVE_ROOT: &str = "rwe/corpora/rwe-minimum-first-corpus/v2";
pub const OPERATOR_CORPUS_ID: &str = OPERATOR_V2_CORPUS_ID;
pub const OPERATOR_CORPUS_RELATIVE_ROOT: &str = OPERATOR_V2_CORPUS_RELATIVE_ROOT;
pub const OPERATOR_TARGET_REPO: &str = "Igzela/alters-lab";
pub const OPERATOR_ADMITTED_EXECUTOR: &str = "managed_deepseek";
pub const OPERATOR_ADMITTED_MODEL: &str = "deepseek-v4-flash";
/// Planner/reviewer model admitted by the managed_deepseek delegated route
/// (deepseek-v4-pro) for frozen RWE cells; implementer uses OPERATOR_ADMITTED_MODEL.
pub const OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL: &str = "deepseek-v4-pro";
/// The managed_deepseek executor is an in-process adapter compiled into the
/// engine binary (no external codex/cli subprocess). The corpus-level
/// `admitted_codex_version` field therefore binds the engine crate version:
/// the binary identity of the admitted executor, not a model name.
pub const OPERATOR_ADMITTED_BINARY_VERSION: &str = "0.1.0";
pub const OPERATOR_ADMITTED_BINARY_PATH: &str = "in-process:managed_deepseek";
/// Exact harness `main` SHA at which Board A froze the operator corpus, protocol,
/// and schedule. Store-owned production issue/admit derives this binding; callers
/// never supply or override it.
pub const OPERATOR_V1_ARTIFACTS_FROZEN_AT_MAIN_SHA: &str =
    "3c6cd00f68f4db2a9eef99598deebc42f95ab62b";
pub const OPERATOR_V2_ARTIFACTS_FROZEN_AT_MAIN_SHA: &str =
    "ee43eac853644266614da09de764a3bf19f2d281";
pub const OPERATOR_ARTIFACTS_FROZEN_AT_MAIN_SHA: &str = OPERATOR_V2_ARTIFACTS_FROZEN_AT_MAIN_SHA;

pub const OPERATOR_V1_CORPUS_SHA256: &str =
    "81a65fc93fc6b381ce127a7b9b62b0afaa233ec366ed78a5db43f0b53ab2eccc";
pub const OPERATOR_V1_PROTOCOL_SHA256: &str =
    "15efbc60d5edf21ae7c79537f76bfb0b9be6030a57f3b83201e51df6e2e9adb9";
pub const OPERATOR_V1_SCHEDULE_SHA256: &str =
    "2500bb77d15bc1d9c9a1c2db612ff602abfa9e203747159eafe035fc075dc765";
pub const OPERATOR_V2_CORPUS_SHA256: &str =
    "044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20";
pub const OPERATOR_V2_PROTOCOL_SHA256: &str =
    "bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db";
pub const OPERATOR_V2_SCHEDULE_SHA256: &str =
    "6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorCorpusVersion {
    V1,
    V2,
}

impl OperatorCorpusVersion {
    fn corpus_id(self) -> &'static str {
        match self {
            Self::V1 => OPERATOR_V1_CORPUS_ID,
            Self::V2 => OPERATOR_V2_CORPUS_ID,
        }
    }

    fn relative_root(self) -> &'static str {
        match self {
            Self::V1 => OPERATOR_V1_CORPUS_RELATIVE_ROOT,
            Self::V2 => OPERATOR_V2_CORPUS_RELATIVE_ROOT,
        }
    }

    fn freeze_point(self) -> &'static str {
        match self {
            Self::V1 => OPERATOR_V1_ARTIFACTS_FROZEN_AT_MAIN_SHA,
            Self::V2 => OPERATOR_V2_ARTIFACTS_FROZEN_AT_MAIN_SHA,
        }
    }

    fn expected_hashes(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::V1 => (
                OPERATOR_V1_CORPUS_SHA256,
                OPERATOR_V1_PROTOCOL_SHA256,
                OPERATOR_V1_SCHEDULE_SHA256,
            ),
            Self::V2 => (
                OPERATOR_V2_CORPUS_SHA256,
                OPERATOR_V2_PROTOCOL_SHA256,
                OPERATOR_V2_SCHEDULE_SHA256,
            ),
        }
    }
}

fn operator_corpus_notes() -> Vec<String> {
    vec![
        "Operator-approved Minimum First RWE corpus; real tasks on Igzela/alters-lab.".into(),
        "Objective text is hash-bound only in operational evidence.".into(),
        "Live RWE requires a separate store-owned one-use spend authorization.".into(),
        "Not a live baseline until authorized live evidence is sealed.".into(),
    ]
}

pub fn operator_corpus_root() -> PathBuf {
    operator_corpus_root_for_version(OperatorCorpusVersion::V2)
}

pub fn operator_corpus_root_for_version(version: OperatorCorpusVersion) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(version.relative_root())
}

/// The operator-approved frozen contract set: corpus + protocol + schedule +
/// accepted-main SHA. Everything the authorization v2 body must bind.
pub struct OperatorFrozenContractSet {
    pub version: OperatorCorpusVersion,
    pub corpus: FirstRweCorpus,
    pub protocol: crate::rwe::economic_protocol::FrozenEvidenceDocument,
    pub schedule: crate::rwe::execution_schedule::FrozenExecutionSchedule,
    pub accepted_main_sha: String,
    pub corpus_artifact_path: String,
}

/// Freeze the whole operator contract set from accepted-main artifacts.
/// `accepted_main_sha` is the exact harness main SHA the artifacts were frozen
/// at (provided by the freeze tooling from the repository checkout, never
/// operator-typed). Production owners must call
/// [`freeze_current_operator_contract_set`] so the freeze-point SHA is store-owned.
pub fn freeze_operator_contract_set(
    accepted_main_sha: &str,
) -> Result<OperatorFrozenContractSet, String> {
    freeze_operator_contract_set_for_version(OperatorCorpusVersion::V2, accepted_main_sha)
}

pub fn freeze_operator_contract_set_for_version(
    version: OperatorCorpusVersion,
    accepted_main_sha: &str,
) -> Result<OperatorFrozenContractSet, String> {
    if accepted_main_sha != version.freeze_point() {
        return Err("operator contract freeze-point SHA mismatch".into());
    }
    let corpus = freeze_operator_rwe_corpus_for_version(version)?;
    let root = operator_corpus_root_for_version(version);
    let raw = std::fs::read(root.join("protocol/rwe_economic_protocol.v1.json"))
        .map_err(|e| e.to_string())?;
    let body: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let protocol = crate::rwe::economic_protocol::freeze_rwe_economic_protocol(body)?;
    if protocol
        .body
        .get("authority_corpus_sha256")
        .and_then(serde_json::Value::as_str)
        != Some(corpus.corpus_sha256.as_str())
    {
        return Err("frozen protocol authority_corpus_sha256 mismatch".into());
    }
    let schedule = freeze_operator_execution_schedule_from_root(
        &corpus,
        &protocol,
        &root.join("schedule/execution_schedule.v1.json"),
    )?;
    let (_, expected_protocol_sha, expected_schedule_sha) = version.expected_hashes();
    if protocol.body_sha256 != expected_protocol_sha {
        return Err("operator protocol hash does not match approved version lock".into());
    }
    if schedule.schedule_sha256 != expected_schedule_sha {
        return Err("operator schedule hash does not match approved version lock".into());
    }
    Ok(OperatorFrozenContractSet {
        version,
        corpus,
        protocol,
        schedule,
        accepted_main_sha: accepted_main_sha.to_string(),
        corpus_artifact_path: version.relative_root().to_string(),
    })
}

/// Store-owned freeze of the current Board-A operator contract set. Derives the
/// accepted-main freeze-point SHA from the repository owner constant rather than
/// caller-supplied checkout text.
pub fn freeze_current_operator_contract_set() -> Result<OperatorFrozenContractSet, String> {
    freeze_operator_contract_set(OPERATOR_ARTIFACTS_FROZEN_AT_MAIN_SHA)
}

/// Load and freeze the operator-approved corpus from its versioned artifacts on
/// accepted-main. Rejects anything living under the fixture root and any
/// fixture/placeholder repository identity.
pub fn freeze_operator_rwe_corpus() -> Result<FirstRweCorpus, String> {
    freeze_operator_rwe_corpus_for_version(OperatorCorpusVersion::V2)
}

pub fn freeze_operator_rwe_corpus_for_version(
    version: OperatorCorpusVersion,
) -> Result<FirstRweCorpus, String> {
    let root = operator_corpus_root_for_version(version);
    let fixture_root = super::corpus::default_corpus_fixture_root();
    if root.starts_with(&fixture_root) {
        return Err("operator corpus must not live under the fixture root".into());
    }
    let corpus = freeze_rwe_corpus_from_root(
        &root,
        version.corpus_id(),
        OPERATOR_TARGET_REPO,
        OPERATOR_ADMITTED_EXECUTOR,
        OPERATOR_ADMITTED_BINARY_VERSION,
        operator_corpus_notes(),
    )?;
    let mut shared_source_commit: Option<&str> = None;
    for task in &corpus.tasks {
        if !task.source_repository.starts_with("https://")
            || task.source_repository.starts_with("fixture://")
        {
            return Err(format!(
                "operator corpus task {} must bind a real https repository",
                task.task_id
            ));
        }
        if task.definition_path.starts_with("fixtures/") {
            return Err(format!(
                "operator corpus task {} must not live under the fixture root",
                task.task_id
            ));
        }
        if task.source_commit.len() != 40 {
            return Err(format!(
                "operator corpus task {} source_commit must be a 40-hex commit",
                task.task_id
            ));
        }
        match shared_source_commit {
            Some(commit) if commit != task.source_commit => {
                return Err(
                    "operator corpus tasks must share one source_commit (the frozen target main)"
                        .into(),
                )
            }
            None => shared_source_commit = Some(task.source_commit.as_str()),
            _ => {}
        }
    }
    let (expected_corpus_sha, _, _) = version.expected_hashes();
    if corpus.corpus_sha256 != expected_corpus_sha {
        return Err("operator corpus hash does not match approved version lock".into());
    }
    Ok(corpus)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rwe::economic_protocol::freeze_rwe_economic_protocol;
    use crate::rwe::execution_schedule::{
        freeze_operator_execution_schedule, freeze_operator_execution_schedule_from_root,
        EXECUTION_SCHEDULE_SCHEMA,
    };
    use crate::rwe::operator_corpus::{freeze_operator_rwe_corpus, operator_corpus_root};
    use serde_json::Value;

    // Hashes of the candidate v2 artifacts in canonical form. These lock the
    // freeze-point-bound artifacts to their published hashes.
    const EXPECTED_CORPUS_SHA: &str =
        "044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20";
    const EXPECTED_PROTOCOL_SHA: &str =
        "bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db";
    const EXPECTED_SCHEDULE_SHA: &str =
        "6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38";
    // Historical v1 hashes (Decision B freeze at main 3c6cd00f); v1 artifacts
    // must stay untouched after the PE7 v2 refreeze.
    const EXPECTED_V1_CORPUS_SHA: &str =
        "81a65fc93fc6b381ce127a7b9b62b0afaa233ec366ed78a5db43f0b53ab2eccc";
    const EXPECTED_V1_PROTOCOL_SHA: &str =
        "15efbc60d5edf21ae7c79537f76bfb0b9be6030a57f3b83201e51df6e2e9adb9";
    const EXPECTED_V1_SCHEDULE_SHA: &str =
        "2500bb77d15bc1d9c9a1c2db612ff602abfa9e203747159eafe035fc075dc765";

    fn v1_corpus_root() -> std::path::PathBuf {
        operator_corpus_root()
            .parent()
            .expect("corpus root parent")
            .join("v1")
    }

    fn frozen_v1_protocol() -> crate::rwe::economic_protocol::FrozenEvidenceDocument {
        let raw =
            std::fs::read(v1_corpus_root().join("protocol/rwe_economic_protocol.v1.json")).unwrap();
        let v: Value = serde_json::from_slice(&raw).unwrap();
        freeze_rwe_economic_protocol(v).unwrap()
    }

    fn frozen_protocol() -> crate::rwe::economic_protocol::FrozenEvidenceDocument {
        let raw =
            std::fs::read(operator_corpus_root().join("protocol/rwe_economic_protocol.v1.json"))
                .unwrap();
        let v: Value = serde_json::from_slice(&raw).unwrap();
        freeze_rwe_economic_protocol(v).unwrap()
    }

    fn normalized_task(mut value: Value) -> Value {
        let object = value.as_object_mut().unwrap();
        object.remove("per_task_max_output_tokens");
        object.remove("per_task_max_total_tokens");
        value
    }

    fn normalized_corpus(mut value: Value) -> Value {
        let object = value.as_object_mut().unwrap();
        for key in ["corpus_id", "corpus_sha256", "fixture_root"] {
            object.remove(key);
        }
        if let Some(tasks) = object.get_mut("tasks").and_then(Value::as_array_mut) {
            for task in tasks.iter_mut() {
                let normalized = normalized_task(task.take());
                *task = normalized;
                task.as_object_mut().unwrap().remove("definition_sha256");
            }
        }
        value
    }

    fn normalized_protocol(mut value: Value) -> Value {
        let object = value.as_object_mut().unwrap();
        object.remove("protocol_id");
        object.remove("authority_corpus_sha256");
        if let Some(tasks) = object.get_mut("tasks").and_then(Value::as_array_mut) {
            for task in tasks {
                task.as_object_mut()
                    .unwrap()
                    .remove("task_definition_sha256");
            }
        }
        if let Some(budget_points) = object
            .get_mut("budget_points")
            .and_then(Value::as_array_mut)
        {
            for budget_point in budget_points {
                budget_point
                    .as_object_mut()
                    .unwrap()
                    .remove("max_total_tokens");
            }
        }
        value
    }

    fn normalized_schedule(mut value: Value) -> Value {
        let object = value.as_object_mut().unwrap();
        for key in [
            "schedule_id",
            "corpus_id",
            "corpus_sha256",
            "protocol_id",
            "protocol_sha256",
            "schedule_sha256",
        ] {
            object.remove(key);
        }
        if let Some(cells) = object.get_mut("cells").and_then(Value::as_array_mut) {
            for cell in cells {
                let cell = cell.as_object_mut().unwrap();
                cell.remove("max_output_tokens");
                cell.remove("max_total_tokens");
            }
        }
        object
            .get_mut("run_level_budget")
            .and_then(Value::as_object_mut)
            .unwrap()
            .remove("max_total_tokens");
        value
    }

    #[test]
    fn freezes_operator_corpus_deterministically() {
        let corpus = freeze_operator_rwe_corpus().unwrap();
        assert_eq!(corpus.corpus_id, OPERATOR_CORPUS_ID);
        assert_eq!(corpus.corpus_sha256, EXPECTED_CORPUS_SHA);
        assert_eq!(corpus.tasks.len(), 2);
        assert_eq!(corpus.disposable_target_repo, "Igzela/alters-lab");
        assert_eq!(corpus.admitted_executor, "managed_deepseek");
        assert_eq!(
            corpus.admitted_codex_version,
            OPERATOR_ADMITTED_BINARY_VERSION
        );
        assert_eq!(corpus.admitted_codex_version, "0.1.0");
        for task in &corpus.tasks {
            assert!(task.source_repository.starts_with("https://"));
            assert_eq!(task.source_commit.len(), 40);
            assert_eq!(task.source_tree_hash.len(), 64);
            assert_eq!(task.per_task_max_retries, 0);
            assert_eq!(task.per_task_max_provider_requests, 3);
        }
        let again = freeze_operator_rwe_corpus().unwrap();
        assert_eq!(corpus.corpus_sha256, again.corpus_sha256);
        // Fixture corpus is untouched and still deterministic.
        let fixture = crate::rwe::freeze_first_rwe_corpus().unwrap();
        assert_eq!(fixture.tasks.len(), 5);
        let fixture_again = crate::rwe::freeze_first_rwe_corpus().unwrap();
        assert_eq!(fixture.corpus_sha256, fixture_again.corpus_sha256);
        assert_ne!(corpus.corpus_sha256, fixture.corpus_sha256);
    }

    #[test]
    fn current_contract_set_binds_candidate_freeze_point_and_hashes() {
        let contract = freeze_current_operator_contract_set().unwrap();
        assert_eq!(
            contract.accepted_main_sha,
            "ee43eac853644266614da09de764a3bf19f2d281"
        );
        assert_eq!(contract.corpus_artifact_path, OPERATOR_CORPUS_RELATIVE_ROOT);
        assert_eq!(contract.corpus.corpus_sha256, EXPECTED_CORPUS_SHA);
        assert_eq!(contract.protocol.body_sha256, EXPECTED_PROTOCOL_SHA);
        assert_eq!(contract.schedule.schedule_sha256, EXPECTED_SCHEDULE_SHA);

        assert_eq!(contract.corpus.disposable_target_repo, OPERATOR_TARGET_REPO);
        assert!(contract.corpus.target_main_sha_required);
        assert_eq!(
            contract.corpus.admitted_executor,
            OPERATOR_ADMITTED_EXECUTOR
        );
        assert_eq!(
            contract.corpus.admitted_codex_version,
            OPERATOR_ADMITTED_BINARY_VERSION
        );
        assert!(contract.corpus.draft_pr_only);
        assert!(contract.corpus.auto_merge_disabled);
        assert_eq!(
            contract
                .protocol
                .body
                .get("authority_corpus_sha256")
                .and_then(Value::as_str),
            Some(EXPECTED_CORPUS_SHA)
        );
        assert_eq!(
            contract
                .protocol
                .body
                .get("live_execution_authorized")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            contract
                .schedule
                .body
                .get("corpus_sha256")
                .and_then(Value::as_str),
            Some(EXPECTED_CORPUS_SHA)
        );
        assert_eq!(
            contract
                .schedule
                .body
                .get("protocol_sha256")
                .and_then(Value::as_str),
            Some(EXPECTED_PROTOCOL_SHA)
        );
    }

    #[test]
    fn explicit_version_selection_keeps_v1_and_v2_freezes_distinct() {
        let v1 = freeze_operator_contract_set_for_version(
            OperatorCorpusVersion::V1,
            OPERATOR_V1_ARTIFACTS_FROZEN_AT_MAIN_SHA,
        )
        .unwrap();
        let v2 = freeze_operator_contract_set_for_version(
            OperatorCorpusVersion::V2,
            OPERATOR_V2_ARTIFACTS_FROZEN_AT_MAIN_SHA,
        )
        .unwrap();

        assert_eq!(v1.version, OperatorCorpusVersion::V1);
        assert_eq!(v2.version, OperatorCorpusVersion::V2);
        assert_eq!(v1.corpus_artifact_path, OPERATOR_V1_CORPUS_RELATIVE_ROOT);
        assert_eq!(v2.corpus_artifact_path, OPERATOR_V2_CORPUS_RELATIVE_ROOT);
        assert_eq!(v1.corpus.corpus_id, OPERATOR_V1_CORPUS_ID);
        assert_eq!(v2.corpus.corpus_id, OPERATOR_V2_CORPUS_ID);
        assert_eq!(v1.corpus.corpus_sha256, OPERATOR_V1_CORPUS_SHA256);
        assert_eq!(v2.corpus.corpus_sha256, OPERATOR_V2_CORPUS_SHA256);
        assert_ne!(v1.corpus.corpus_sha256, v2.corpus.corpus_sha256);
        assert_eq!(v1.protocol.body_sha256, OPERATOR_V1_PROTOCOL_SHA256);
        assert_eq!(v2.protocol.body_sha256, OPERATOR_V2_PROTOCOL_SHA256);
        assert_eq!(v1.schedule.schedule_sha256, OPERATOR_V1_SCHEDULE_SHA256);
        assert_eq!(v2.schedule.schedule_sha256, OPERATOR_V2_SCHEDULE_SHA256);
    }

    #[test]
    fn freeze_point_mismatch_is_rejected_before_contract_creation() {
        let result = freeze_operator_contract_set(&"0".repeat(40));
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap(),
            "operator contract freeze-point SHA mismatch"
        );
    }

    #[test]
    fn freezes_operator_protocol_from_artifact() {
        let corpus = freeze_operator_rwe_corpus().unwrap();
        let protocol = frozen_protocol();
        assert_eq!(protocol.body_sha256, EXPECTED_PROTOCOL_SHA);
        assert_eq!(
            protocol
                .body
                .get("authority_corpus_sha256")
                .and_then(Value::as_str),
            Some(corpus.corpus_sha256.as_str())
        );
        assert_eq!(
            protocol
                .body
                .get("minimum_repetitions_per_task")
                .and_then(Value::as_u64),
            Some(2)
        );
        assert_eq!(
            protocol.body.get("fixture_only").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            protocol
                .body
                .get("live_execution_authorized")
                .and_then(Value::as_bool),
            Some(false)
        );
        // Protocol task entries bind the exact corpus task-definition hashes.
        for task in &corpus.tasks {
            let protocol_task = protocol
                .body
                .get("tasks")
                .and_then(Value::as_array)
                .unwrap()
                .iter()
                .find(|t| t.get("task_id").and_then(Value::as_str) == Some(task.task_id.as_str()))
                .unwrap();
            assert_eq!(
                protocol_task
                    .get("task_definition_sha256")
                    .and_then(Value::as_str),
                Some(task.definition_sha256.as_str())
            );
        }
    }

    #[test]
    fn freezes_operator_schedule_with_paired_seeds() {
        let corpus = freeze_operator_rwe_corpus().unwrap();
        let protocol = frozen_protocol();
        let schedule = freeze_operator_execution_schedule(&corpus, &protocol).unwrap();
        assert_eq!(schedule.schedule_sha256, EXPECTED_SCHEDULE_SHA);
        assert_eq!(schedule.schema_version, EXECUTION_SCHEDULE_SCHEMA);
        let cells = schedule
            .body
            .get("cells")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(cells.len(), 4); // 2 tasks × 2 repetitions × 1 budget point
        let seeds: Vec<u64> = cells
            .iter()
            .map(|c| c.get("seed").and_then(Value::as_u64).unwrap())
            .collect();
        // Repetition 1 cells share seed 2026080601; repetition 2 share 2026080602.
        assert_eq!(seeds[0], 2026080601);
        assert_eq!(seeds[1], 2026080602);
        assert_eq!(seeds[2], 2026080601);
        assert_eq!(seeds[3], 2026080602);
        for (i, cell) in cells.iter().enumerate() {
            assert_eq!(
                cell.get("sequential_order").and_then(Value::as_u64),
                Some(i as u64 + 1)
            );
        }
        // Cell budgets exactly match corpus per-task limits.
        for cell in cells {
            let task_id = cell.get("task_id").and_then(Value::as_str).unwrap();
            let task = corpus.tasks.iter().find(|t| t.task_id == task_id).unwrap();
            assert_eq!(
                cell.get("max_provider_requests").and_then(Value::as_u64),
                Some(task.per_task_max_provider_requests)
            );
            assert_eq!(
                cell.get("max_total_tokens").and_then(Value::as_u64),
                Some(task.per_task_max_total_tokens)
            );
            assert_eq!(
                cell.get("max_wall_time_ms").and_then(Value::as_u64),
                Some(task.timeout_ms)
            );
        }
        // Replay determinism: re-freeze yields the identical schedule hash.
        let replay = freeze_operator_execution_schedule(&corpus, &protocol).unwrap();
        assert_eq!(schedule.schedule_sha256, replay.schedule_sha256);
    }

    #[test]
    fn operator_schedule_rejects_tampering() {
        let corpus = freeze_operator_rwe_corpus().unwrap();
        let protocol = frozen_protocol();

        // Tampered cell seed breaks the canonical hash and the seed-list binding.
        let mut tampered = serde_json::from_str::<Value>(
            &std::fs::read_to_string(
                operator_corpus_root().join("schedule/execution_schedule.v1.json"),
            )
            .unwrap(),
        )
        .unwrap();
        tampered["cells"][0]["seed"] = Value::from(9999_u64);
        let raw = serde_json::to_vec(&tampered).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("execution_schedule.v1.json");
        std::fs::write(&file, raw).unwrap();
        // The freeze path is bound to the accepted-main artifact root; verify the
        // mutation is detected by the canonical-hash self-check via a helper that
        // loads from an arbitrary root.
        let result = freeze_operator_execution_schedule_from_root(&corpus, &protocol, &file);
        assert!(result.is_err(), "tampered schedule must be rejected");
        assert!(
            result.unwrap_err().contains("schedule_sha256"),
            "canonical tamper detection expected"
        );

        // Corpus binding tampering is also rejected.
        let mut tampered2 = serde_json::from_str::<Value>(
            &std::fs::read_to_string(
                operator_corpus_root().join("schedule/execution_schedule.v1.json"),
            )
            .unwrap(),
        )
        .unwrap();
        tampered2["corpus_sha256"] = Value::String("0".repeat(64));
        let raw2 = serde_json::to_vec(&tampered2).unwrap();
        let file2 = dir.path().join("schedule2.json");
        std::fs::write(&file2, raw2).unwrap();
        let result2 = freeze_operator_execution_schedule_from_root(&corpus, &protocol, &file2);
        assert!(
            result2.unwrap_err().contains("schedule_sha256"),
            "binding tamper must break the canonical hash"
        );
    }

    #[test]
    fn v1_artifacts_remain_untouched_after_refreeze() {
        // The PE7 v2 refreeze must not overwrite or reinterpret v1: its
        // historical hashes must still freeze deterministically from the v1 root.
        let v1_corpus = freeze_rwe_corpus_from_root(
            &v1_corpus_root(),
            "rwe-minimum-first-corpus-v1",
            OPERATOR_TARGET_REPO,
            OPERATOR_ADMITTED_EXECUTOR,
            OPERATOR_ADMITTED_BINARY_VERSION,
            operator_corpus_notes(),
        )
        .unwrap();
        assert_eq!(v1_corpus.corpus_sha256, EXPECTED_V1_CORPUS_SHA);
        let v1_protocol = frozen_v1_protocol();
        assert_eq!(v1_protocol.body_sha256, EXPECTED_V1_PROTOCOL_SHA);
        let v1_schedule = freeze_operator_execution_schedule_from_root(
            &v1_corpus,
            &v1_protocol,
            &v1_corpus_root().join("schedule/execution_schedule.v1.json"),
        )
        .unwrap();
        assert_eq!(v1_schedule.schedule_sha256, EXPECTED_V1_SCHEDULE_SHA);
        // The v2 freeze has distinct hashes and ids.
        let v2_corpus = freeze_operator_rwe_corpus().unwrap();
        let v2_protocol = frozen_protocol();
        let v2_schedule = freeze_operator_execution_schedule(&v2_corpus, &v2_protocol).unwrap();
        assert_ne!(v2_corpus.corpus_sha256, EXPECTED_V1_CORPUS_SHA);
        assert_ne!(v2_protocol.body_sha256, EXPECTED_V1_PROTOCOL_SHA);
        assert_ne!(v2_schedule.schedule_sha256, EXPECTED_V1_SCHEDULE_SHA);
    }

    #[test]
    fn refreeze_changes_only_compatibility_fields() {
        // PE7 v2 contract: the only per-task semantic change is the
        // output-token envelope and its mechanical total; every other binding
        // (objectives, target commit/tree, paths, verifier, model, timeouts,
        // seeds, costs, stop rules) must be byte-identical.
        let v1_corpus = freeze_rwe_corpus_from_root(
            &v1_corpus_root(),
            "rwe-minimum-first-corpus-v1",
            OPERATOR_TARGET_REPO,
            OPERATOR_ADMITTED_EXECUTOR,
            OPERATOR_ADMITTED_BINARY_VERSION,
            operator_corpus_notes(),
        )
        .unwrap();
        let v2_corpus = freeze_operator_rwe_corpus().unwrap();
        assert_eq!(v1_corpus.tasks.len(), v2_corpus.tasks.len());
        for v2_task in &v2_corpus.tasks {
            let v1_task = v1_corpus
                .tasks
                .iter()
                .find(|t| t.task_id == v2_task.task_id)
                .unwrap_or_else(|| panic!("v2 task {} missing from v1", v2_task.task_id));
            assert_eq!(v1_task.per_task_max_output_tokens, 4000);
            assert_eq!(v1_task.per_task_max_total_tokens, 16000);
            assert_eq!(v2_task.per_task_max_output_tokens, 8192);
            assert_eq!(v2_task.per_task_max_total_tokens, 20192);
            assert_eq!(v2_task.per_task_max_input_tokens, 12000);
            assert_eq!(
                v2_task.per_task_max_input_tokens,
                v1_task.per_task_max_input_tokens
            );
            // Every other contract field is unchanged (definition_sha256 is a
            // dependent hash of the refrozen file and is expected to differ).
            assert_eq!(v2_task.task_id, v1_task.task_id);
            assert_eq!(v2_task.class, v1_task.class);
            assert_eq!(v2_task.objective_sha256, v1_task.objective_sha256);
            assert_eq!(v2_task.source_repository, v1_task.source_repository);
            assert_eq!(v2_task.source_commit, v1_task.source_commit);
            assert_eq!(v2_task.source_tree_hash, v1_task.source_tree_hash);
            assert_eq!(v2_task.allowed_mutable_paths, v1_task.allowed_mutable_paths);
            assert_eq!(
                v2_task.expected_verification_commands,
                v1_task.expected_verification_commands
            );
            assert_eq!(
                v2_task.expected_outcome_class,
                v1_task.expected_outcome_class
            );
            assert_eq!(v2_task.patch_max_files, v1_task.patch_max_files);
            assert_eq!(v2_task.patch_max_lines, v1_task.patch_max_lines);
            assert_eq!(v2_task.timeout_ms, v1_task.timeout_ms);
            assert_eq!(v2_task.cancel_behavior, v1_task.cancel_behavior);
            assert_eq!(v2_task.executor_identity, v1_task.executor_identity);
            assert_eq!(v2_task.model_identity, v1_task.model_identity);
            assert_eq!(
                v2_task.per_task_max_provider_requests,
                v1_task.per_task_max_provider_requests
            );
            assert_eq!(v2_task.per_task_max_retries, v1_task.per_task_max_retries);
            assert_eq!(v2_task.deterministic_seed, v1_task.deterministic_seed);
            assert_eq!(v2_task.cleanup_rules, v1_task.cleanup_rules);
        }
        // Versioned ids and mechanically derived run totals.
        assert_eq!(v2_corpus.corpus_id, "rwe-minimum-first-corpus-v2");
        let v2_protocol = frozen_protocol();
        assert_eq!(
            v2_protocol.body.get("protocol_id").and_then(Value::as_str),
            Some("rwe-minimum-first-protocol-v2")
        );
        let v2_schedule = freeze_operator_execution_schedule(&v2_corpus, &v2_protocol).unwrap();
        assert_eq!(
            v2_schedule.body.get("schedule_id").and_then(Value::as_str),
            Some("rwe-minimum-first-schedule-v2")
        );
        let run_tokens = v2_schedule
            .body
            .get("run_level_budget")
            .and_then(|r| r.get("max_total_tokens"))
            .and_then(Value::as_u64)
            .unwrap();
        assert_eq!(run_tokens, 4 * 20192);
        for cell in v2_schedule
            .body
            .get("cells")
            .and_then(Value::as_array)
            .unwrap()
        {
            assert_eq!(
                cell.get("max_output_tokens").and_then(Value::as_u64),
                Some(8192)
            );
            assert_eq!(
                cell.get("max_total_tokens").and_then(Value::as_u64),
                Some(20192)
            );
        }
    }

    #[test]
    fn refreeze_json_semantic_delta_is_whitelisted() {
        let read = |root: &std::path::Path, relative: &str| -> Value {
            serde_json::from_slice(&std::fs::read(root.join(relative)).unwrap()).unwrap()
        };
        for task in [
            "tasks/rwe-minimum-t1-fix_flow_linkage.json",
            "tasks/rwe-minimum-t2-draft_contract_tests.json",
        ] {
            assert_eq!(
                normalized_task(read(&v1_corpus_root(), task)),
                normalized_task(read(&operator_corpus_root(), task)),
                "unexpected semantic drift in {task}"
            );
        }
        assert_eq!(
            normalized_protocol(read(
                &v1_corpus_root(),
                "protocol/rwe_economic_protocol.v1.json",
            )),
            normalized_protocol(read(
                &operator_corpus_root(),
                "protocol/rwe_economic_protocol.v1.json",
            )),
            "unexpected protocol drift outside the compatibility whitelist"
        );
        assert_eq!(
            normalized_schedule(read(
                &v1_corpus_root(),
                "schedule/execution_schedule.v1.json",
            )),
            normalized_schedule(read(
                &operator_corpus_root(),
                "schedule/execution_schedule.v1.json",
            )),
            "unexpected schedule drift outside the compatibility whitelist"
        );

        let v1_contract = freeze_rwe_corpus_from_root(
            &v1_corpus_root(),
            "rwe-minimum-first-corpus-v1",
            OPERATOR_TARGET_REPO,
            OPERATOR_ADMITTED_EXECUTOR,
            OPERATOR_ADMITTED_BINARY_VERSION,
            operator_corpus_notes(),
        )
        .unwrap();
        let v2_contract = freeze_operator_rwe_corpus().unwrap();
        assert_eq!(
            normalized_corpus(v1_contract.to_json()),
            normalized_corpus(v2_contract.to_json()),
            "unexpected corpus drift outside the compatibility whitelist"
        );
    }

    #[test]
    fn v2_task_tamper_breaks_corpus_binding() {
        // A tampered v2 task definition must change the corpus hash, and the
        // protocol/schedule bindings must fail closed against the tampered
        // corpus exactly like the operator freeze path.
        let dir = tempfile::tempdir().unwrap();
        let tampered_root = dir.path().join("v2");
        for rel in [
            "tasks/rwe-minimum-t1-fix_flow_linkage.json",
            "tasks/rwe-minimum-t2-draft_contract_tests.json",
            "protocol/rwe_economic_protocol.v1.json",
            "schedule/execution_schedule.v1.json",
        ] {
            let src = operator_corpus_root().join(rel);
            let dst = tampered_root.join(rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::copy(&src, &dst).unwrap();
        }
        let task_path = tampered_root.join("tasks/rwe-minimum-t1-fix_flow_linkage.json");
        let mut task: Value = serde_json::from_slice(&std::fs::read(&task_path).unwrap()).unwrap();
        task["objective"] =
            Value::String("Tampered objective that is not part of the approved freeze.".into());
        std::fs::write(&task_path, serde_json::to_vec(&task).unwrap()).unwrap();

        let tampered_corpus = freeze_rwe_corpus_from_root(
            &tampered_root,
            OPERATOR_CORPUS_ID,
            OPERATOR_TARGET_REPO,
            OPERATOR_ADMITTED_EXECUTOR,
            OPERATOR_ADMITTED_BINARY_VERSION,
            operator_corpus_notes(),
        )
        .unwrap();
        assert_ne!(
            tampered_corpus.corpus_sha256, EXPECTED_CORPUS_SHA,
            "tampered task must change the corpus hash"
        );
        // The untampered protocol binds the original corpus hash; the binding
        // check mirrors freeze_operator_contract_set and must fail.
        let protocol = freeze_rwe_economic_protocol(
            serde_json::from_slice(
                &std::fs::read(tampered_root.join("protocol/rwe_economic_protocol.v1.json"))
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert_ne!(
            protocol
                .body
                .get("authority_corpus_sha256")
                .and_then(Value::as_str),
            Some(tampered_corpus.corpus_sha256.as_str())
        );
        // The schedule binding check must also fail closed.
        let result = freeze_operator_execution_schedule_from_root(
            &tampered_corpus,
            &protocol,
            &tampered_root.join("schedule/execution_schedule.v1.json"),
        );
        assert!(
            result.unwrap_err().contains("corpus_sha256"),
            "schedule must reject a tampered corpus"
        );
    }
}
