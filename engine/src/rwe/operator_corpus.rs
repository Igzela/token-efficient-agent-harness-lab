//! Operator-approved real-workload corpus freeze (provider-free).
//!
//! Strictly separated from the fixture corpus: distinct canonical root
//! (`engine/rwe/corpora/<corpus-id>/v1/`), distinct loader entry point, real
//! non-fixture repository identities. The fixture loader and its root
//! (`engine/fixtures/rwe/first_corpus/v1`) are untouched.

use std::path::PathBuf;

use super::corpus::{freeze_rwe_corpus_from_root, FirstRweCorpus};

pub const OPERATOR_CORPUS_ID: &str = "rwe-minimum-first-corpus-v1";
pub const OPERATOR_CORPUS_RELATIVE_ROOT: &str = "rwe/corpora/rwe-minimum-first-corpus/v1";
pub const OPERATOR_TARGET_REPO: &str = "Igzela/alters-lab";
pub const OPERATOR_ADMITTED_EXECUTOR: &str = "managed_deepseek";
pub const OPERATOR_ADMITTED_MODEL: &str = "deepseek-v4-flash";
/// The managed_deepseek executor is an in-process adapter compiled into the
/// engine binary (no external codex/cli subprocess). The corpus-level
/// `admitted_codex_version` field therefore binds the engine crate version:
/// the binary identity of the admitted executor, not a model name.
pub const OPERATOR_ADMITTED_BINARY_VERSION: &str = "0.1.0";
pub const OPERATOR_ADMITTED_BINARY_PATH: &str = "in-process:managed_deepseek";

pub fn operator_corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(OPERATOR_CORPUS_RELATIVE_ROOT)
}

/// The operator-approved frozen contract set: corpus + protocol + schedule +
/// accepted-main SHA. Everything the authorization v2 body must bind.
pub struct OperatorFrozenContractSet {
    pub corpus: FirstRweCorpus,
    pub protocol: crate::rwe::economic_protocol::FrozenEvidenceDocument,
    pub schedule: crate::rwe::execution_schedule::FrozenExecutionSchedule,
    pub accepted_main_sha: String,
    pub corpus_artifact_path: String,
}

/// Freeze the whole operator contract set from accepted-main artifacts.
/// `accepted_main_sha` is the exact harness main SHA the artifacts were frozen
/// at (provided by the freeze tooling from the repository checkout, never
/// operator-typed).
pub fn freeze_operator_contract_set(
    accepted_main_sha: &str,
) -> Result<OperatorFrozenContractSet, String> {
    let corpus = freeze_operator_rwe_corpus()?;
    let raw = std::fs::read(operator_corpus_root().join("protocol/rwe_economic_protocol.v1.json"))
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
    let schedule =
        crate::rwe::execution_schedule::freeze_operator_execution_schedule(&corpus, &protocol)?;
    Ok(OperatorFrozenContractSet {
        corpus,
        protocol,
        schedule,
        accepted_main_sha: accepted_main_sha.to_string(),
        corpus_artifact_path: OPERATOR_CORPUS_RELATIVE_ROOT.to_string(),
    })
}

/// Load and freeze the operator-approved corpus from its versioned artifacts on
/// accepted-main. Rejects anything living under the fixture root and any
/// fixture/placeholder repository identity.
pub fn freeze_operator_rwe_corpus() -> Result<FirstRweCorpus, String> {
    let root = operator_corpus_root();
    let fixture_root = super::corpus::default_corpus_fixture_root();
    if root.starts_with(&fixture_root) {
        return Err("operator corpus must not live under the fixture root".into());
    }
    let corpus = freeze_rwe_corpus_from_root(
        &root,
        OPERATOR_CORPUS_ID,
        OPERATOR_TARGET_REPO,
        OPERATOR_ADMITTED_EXECUTOR,
        OPERATOR_ADMITTED_BINARY_VERSION,
        vec![
            "Operator-approved Minimum First RWE corpus; real tasks on Igzela/alters-lab.".into(),
            "Objective text is hash-bound only in operational evidence.".into(),
            "Live RWE requires a separate store-owned one-use spend authorization.".into(),
            "Not a live baseline until authorized live evidence is sealed.".into(),
        ],
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

    // Hashes of the frozen artifacts as generated by the Board-A freeze tooling
    // (scripts/gen_rwe_corpus.py, canonical form). These lock the accepted-main
    // artifacts to their published hashes.
    const EXPECTED_CORPUS_SHA: &str =
        "71daa3512f00a82b7203c6cfb5381f5db9661c46b778e83286dd52cb37a85abb";
    const EXPECTED_PROTOCOL_SHA: &str =
        "da29f2b9107022a0626be840448f348585d5155cba63f9cfb96ebbbde81446de";
    const EXPECTED_SCHEDULE_SHA: &str =
        "f1b6c6fdbca9daca06cf8eee155b809dc0e7f7de49cf741e680e2cf7757cf75c";

    fn frozen_protocol() -> crate::rwe::economic_protocol::FrozenEvidenceDocument {
        let raw =
            std::fs::read(operator_corpus_root().join("protocol/rwe_economic_protocol.v1.json"))
                .unwrap();
        let v: Value = serde_json::from_slice(&raw).unwrap();
        freeze_rwe_economic_protocol(v).unwrap()
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
}
