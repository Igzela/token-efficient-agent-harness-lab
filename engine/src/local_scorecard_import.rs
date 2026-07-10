use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::storage::local_product_store::LocalProductStore;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalScorecardImportSummary {
    pub files_seen: usize,
    pub imported: usize,
    pub unchanged: usize,
    pub artifact_ids: Vec<String>,
    pub errors: Vec<String>,
}

pub fn import_native_scorecard_artifacts(
    store: &LocalProductStore,
    inputs: &[PathBuf],
    actor: &str,
) -> LocalScorecardImportSummary {
    import_scorecard_artifacts(store, inputs, actor)
}

pub fn import_scorecard_artifacts(
    store: &LocalProductStore,
    inputs: &[PathBuf],
    actor: &str,
) -> LocalScorecardImportSummary {
    let mut summary = LocalScorecardImportSummary {
        files_seen: 0,
        imported: 0,
        unchanged: 0,
        artifact_ids: Vec::new(),
        errors: Vec::new(),
    };

    let files = collect_artifact_files(inputs, &mut summary.errors);
    for file in files {
        summary.files_seen += 1;
        match import_one(store, &file, actor) {
            Ok(ImportOutcome::Imported(artifact_id)) => {
                summary.imported += 1;
                summary.artifact_ids.push(artifact_id);
            }
            Ok(ImportOutcome::Unchanged(artifact_id)) => {
                summary.unchanged += 1;
                summary.artifact_ids.push(artifact_id);
            }
            Err(error) => summary.errors.push(format!("{}: {error}", file.display())),
        }
    }

    summary.artifact_ids.sort();
    summary.artifact_ids.dedup();
    summary
}

enum ImportOutcome {
    Imported(String),
    Unchanged(String),
}

fn collect_artifact_files(inputs: &[PathBuf], errors: &mut Vec<String>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for input in inputs {
        if input.is_file() {
            files.push(input.clone());
            continue;
        }
        if input.is_dir() {
            match fs::read_dir(input) {
                Ok(entries) => {
                    for entry in entries {
                        match entry {
                            Ok(entry) => {
                                let path = entry.path();
                                if path.is_file()
                                    && path.extension().and_then(|value| value.to_str())
                                        == Some("json")
                                {
                                    files.push(path);
                                }
                            }
                            Err(error) => {
                                errors.push(format!(
                                    "{}: cannot read directory entry: {error}",
                                    input.display()
                                ));
                            }
                        }
                    }
                }
                Err(error) => {
                    errors.push(format!(
                        "{}: cannot read directory: {error}",
                        input.display()
                    ));
                }
            }
            continue;
        }
        errors.push(format!("{}: input path not found", input.display()));
    }
    files.sort();
    files
}

fn import_one(
    store: &LocalProductStore,
    file: &Path,
    actor: &str,
) -> Result<ImportOutcome, String> {
    let text = fs::read_to_string(file).map_err(|error| format!("cannot read file: {error}"))?;
    let artifact: Value =
        serde_json::from_str(&text).map_err(|error| format!("invalid JSON: {error}"))?;
    let artifact_id = artifact
        .get("artifact_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "artifact_id is required".to_string())?
        .to_string();
    let existed = store.get_native_scorecard_artifact(&artifact_id)?.is_some();
    let stored = store.record_scorecard_artifact(&artifact, actor)?;
    let stored_id = stored
        .get("artifact_id")
        .and_then(Value::as_str)
        .unwrap_or(&artifact_id)
        .to_string();
    if existed {
        Ok(ImportOutcome::Unchanged(stored_id))
    } else {
        Ok(ImportOutcome::Imported(stored_id))
    }
}

/// Validate that a scorecard dict has the required fields for bounded export.
/// This is a lightweight check used by the workflow executor to verify output
/// before recording bounded summary.
pub fn validate_scorecard_for_bounded_export(scorecard: &Value) -> Result<bool, String> {
    let required = [
        "adapter_run_id",
        "runtime_kind",
        "scenario_id",
        "mode",
        "status",
        "input_token_total",
        "output_token_total",
        "step_count",
        "redaction_status",
    ];
    for key in &required {
        if scorecard.get(*key).is_none() {
            return Err(format!("missing required field: {key}"));
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{import_native_scorecard_artifacts, import_scorecard_artifacts};
    use crate::storage::local_product_store::LocalProductStore;
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    fn local_runner_artifact(run_id: &str, mode: &str) -> Value {
        let state_strategy = if mode == "stateful_store" {
            "durable_state"
        } else {
            "full_history"
        };
        let total_tokens = if mode == "stateful_store" { 130 } else { 220 };
        let scorecard = json!({
            "schema_version": "token_efficiency_scorecard.v1",
            "adapter_run_id": run_id,
            "runtime_kind": "native_harness",
            "runtime_version": "provider-gated-real-runner.v1",
            "scenario_id": "provider_gated_remember_dont_reread_runner",
            "mode": mode,
            "state_strategy": state_strategy,
            "status": "pass",
            "pass_fail_reason": "same score threshold met",
            "quality_score": 1.0,
            "quality_method": "rule",
            "input_token_total": total_tokens - 20,
            "output_token_total": 20,
            "context_token_total": total_tokens - 40,
            "repeated_context_token_total": if mode == "stateful_store" { 12 } else { 80 },
            "retrieved_ref_token_total": if mode == "stateful_store" { 10 } else { 0 },
            "tool_call_count": 2,
            "redundant_tool_call_count": 0,
            "retry_count": 0,
            "step_count": 2,
            "duration_ms": 10,
            "estimated_cost_usd": 0.0,
            "raw_trace_artifact_id": format!("bounded-provider-gated-runner-{mode}"),
            "redaction_status": "redacted",
            "derived_metrics": {
                "total_tokens": total_tokens,
                "context_share": if mode == "stateful_store" { 0.692308 } else { 0.818182 },
                "repeated_context_ratio": if mode == "stateful_store" { 0.133333 } else { 0.444444 },
                "tool_redundancy_ratio": 0.0,
                "tokens_per_passing_run": total_tokens,
                "cost_per_passing_run": 0.0,
                "step_retry_ratio": 0.0
            },
            "steps": [
                {
                    "adapter_step_id": format!("{run_id}-iter-00"),
                    "adapter_run_id": run_id,
                    "step_index": 0,
                    "node_name": "real_experiment_iteration_00",
                    "agent_role": "executor",
                    "operation_kind": "model_call",
                    "input_tokens": 50,
                    "output_tokens": 10,
                    "context_tokens": 40,
                    "repeated_context_tokens": 0,
                    "retrieved_refs_count": 0,
                    "retrieved_ref_tokens": 0,
                    "tool_name": null,
                    "tool_call_id": null,
                    "status": "pass",
                    "error_kind": "none",
                    "state_read_bytes": 0,
                    "state_write_bytes": 0
                },
                {
                    "adapter_step_id": format!("{run_id}-iter-01"),
                    "adapter_run_id": run_id,
                    "step_index": 1,
                    "node_name": "real_experiment_iteration_01",
                    "agent_role": "executor",
                    "operation_kind": "model_call",
                    "input_tokens": total_tokens - 70,
                    "output_tokens": 10,
                    "context_tokens": total_tokens - 80,
                    "repeated_context_tokens": if mode == "stateful_store" { 12 } else { 80 },
                    "retrieved_refs_count": if mode == "stateful_store" { 1 } else { 0 },
                    "retrieved_ref_tokens": if mode == "stateful_store" { 10 } else { 0 },
                    "tool_name": null,
                    "tool_call_id": null,
                    "status": "pass",
                    "error_kind": "none",
                    "state_read_bytes": if mode == "stateful_store" { 3 } else { 0 },
                    "state_write_bytes": if mode == "stateful_store" { 96 } else { 0 }
                }
            ]
        });
        let content_sha256 = hex::encode(Sha256::digest(scorecard.to_string().as_bytes()));
        json!({
            "schema_version": "native_scorecard_artifact.v1",
            "artifact_kind": "token_efficiency_scorecard",
            "storage": "app_owned_artifact_json_export",
            "read_only": true,
            "created_at": "1970-01-01T00:00:00Z",
            "artifact_id": format!("scorecard-{run_id}-{mode}"),
            "content_sha256": content_sha256,
            "scorecard_schema_version": "token_efficiency_scorecard.v1",
            "metadata_only": true,
            "target_repository_writes": "disabled",
            "scorecard": scorecard
        })
    }

    #[test]
    fn local_scorecard_import_records_directory_idempotently() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("store.db");
        let artifacts = dir.path().join("artifacts");
        std::fs::create_dir(&artifacts).unwrap();
        let stateful_path = artifacts.join("stateful_store.artifact.json");
        let stateless_path = artifacts.join("stateless_reread.artifact.json");
        std::fs::write(
            &stateful_path,
            local_runner_artifact("real-runner-stateful_store", "stateful_store").to_string(),
        )
        .unwrap();
        std::fs::write(
            &stateless_path,
            local_runner_artifact("real-runner-stateless_reread", "stateless_reread").to_string(),
        )
        .unwrap();

        let store = LocalProductStore::new(&db_path).unwrap();
        let summary = import_scorecard_artifacts(
            &store,
            std::slice::from_ref(&artifacts),
            "local-import-test",
        );
        assert_eq!(summary.files_seen, 2);
        assert_eq!(summary.imported, 2);
        assert_eq!(summary.unchanged, 0);
        assert!(summary.errors.is_empty());
        assert_eq!(
            store
                .native_scorecard_artifacts_by_run("real-runner-stateful_store", 10)
                .unwrap()
                .len(),
            1
        );

        let repeated = import_scorecard_artifacts(&store, &[artifacts], "local-import-test");
        assert_eq!(repeated.files_seen, 2);
        assert_eq!(repeated.imported, 0);
        assert_eq!(repeated.unchanged, 2);
        assert!(repeated.errors.is_empty());
    }

    #[test]
    fn local_scorecard_import_reports_invalid_artifact_without_panic() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new(dir.path().join("store.db")).unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(path.clone(), r#"{"schema_version":"other"}"#).unwrap();

        let summary = import_native_scorecard_artifacts(&store, &[path], "local-import-test");
        assert_eq!(summary.files_seen, 1);
        assert_eq!(summary.imported, 0);
        assert_eq!(summary.unchanged, 0);
        assert_eq!(summary.errors.len(), 1);
        assert!(summary.errors[0].contains("artifact_id is required"));
    }
}
