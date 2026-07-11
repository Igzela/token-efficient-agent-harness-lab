use engine::event_schema::canonical_event_json;
use engine::storage::local_product_store::LocalProductStore;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn hash_payload(value: &Value) -> String {
    hex::encode(Sha256::digest(
        canonical_event_json(value).unwrap().as_bytes(),
    ))
}

fn sample_report(scenario_id: &str, outcome: &str) -> Value {
    let mut report = json!({
        "schema_version": "token_efficiency_regression_report.v1",
        "registry_id": "pe1-fixed-summary-scenarios",
        "registry_sha256": "11".repeat(32),
        "scenario_id": scenario_id,
        "scenario_digest": "22".repeat(32),
        "task_digest": "33".repeat(32),
        "read_only": true,
        "report_only": true,
        "provider_calls": "disabled",
        "mutation_authority": "none",
        "outcome": outcome,
        "reason_codes": if outcome == "regression" { json!(["baseline.state_bytes"]) } else { json!([]) },
        "evidence": {
            "current": {"adapter_run_id": "candidate", "artifact_schema_version": "native_scorecard_artifact.v1", "content_sha256": "44".repeat(32)},
            "baseline": {"adapter_run_id": "baseline", "artifact_schema_version": "native_scorecard_artifact.v1", "content_sha256": "55".repeat(32)},
            "best_known": {"adapter_run_id": "candidate", "artifact_schema_version": "native_scorecard_artifact.v1", "content_sha256": "44".repeat(32)}
        },
        "comparisons": {}
    });
    let hash = hash_payload(&report);
    report["report_sha256"] = json!(hash);
    report
}

fn sample_batch() -> Value {
    let reports = vec![
        sample_report("native_remember_dont_reread_pilot", "regression"),
        sample_report("provider_gated_remember_dont_reread_runner", "pass"),
        sample_report("third_fixed_scenario", "pass"),
    ];
    let mut batch = json!({
        "schema_version": "token_efficiency_regression_batch.v1",
        "registry_id": "pe1-fixed-summary-scenarios",
        "registry_sha256": "11".repeat(32),
        "scenario_count": 3,
        "outcome_counts": {"pass": 2, "regression": 1},
        "read_only": true,
        "report_only": true,
        "provider_calls": "disabled",
        "mutation_authority": "none",
        "reports": reports
    });
    let hash = hash_payload(&batch);
    batch["batch_sha256"] = json!(hash);
    batch
}

fn make_store() -> (LocalProductStore, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let store = LocalProductStore::new(dir.path().join("local.db")).unwrap();
    (store, dir)
}

#[test]
fn regression_report_record_get_list_and_repeat_are_idempotent() {
    let (store, _dir) = make_store();
    let report = sample_report("native_remember_dont_reread_pilot", "regression");

    let first = store
        .record_regression_report_artifact(&report, "pe1-test")
        .unwrap();
    let repeated = store
        .record_regression_report_artifact(&report, "pe1-test")
        .unwrap();

    assert_eq!(first, repeated);
    assert_eq!(
        first["schema_version"],
        "token_efficiency_regression_artifact.v1"
    );
    assert_eq!(first["artifact_kind"], "token_efficiency_regression_report");
    assert_eq!(first["read_only"], true);
    assert_eq!(first["metadata_only"], true);
    assert_eq!(first["report"], report);
    let artifact_id = first["artifact_id"].as_str().unwrap();
    assert_eq!(
        store.get_regression_report_artifact(artifact_id).unwrap(),
        Some(first.clone())
    );
    assert_eq!(
        store
            .regression_report_artifacts_by_scenario("native_remember_dont_reread_pilot", 10,)
            .unwrap(),
        vec![first]
    );
}

#[test]
fn regression_batch_is_stored_in_the_same_bounded_artifact_boundary() {
    let (store, _dir) = make_store();
    let batch = sample_batch();

    let stored = store
        .record_regression_report_artifact(&batch, "pe1-test")
        .unwrap();

    assert_eq!(stored["artifact_kind"], "token_efficiency_regression_batch");
    assert_eq!(stored["report"], batch);
    assert_eq!(store.regression_report_artifacts(10).unwrap(), vec![stored]);
}

#[test]
fn regression_artifact_rejects_hash_tamper_and_sensitive_payloads() {
    let (store, _dir) = make_store();
    let mut tampered = sample_report("native_remember_dont_reread_pilot", "pass");
    tampered["outcome"] = json!("regression");
    let error = store
        .record_regression_report_artifact(&tampered, "pe1-test")
        .unwrap_err();
    assert!(error.contains("report_sha256"));

    let mut sensitive = sample_report("native_remember_dont_reread_pilot", "pass");
    sensitive["raw_prompt"] = json!("must not persist");
    let error = store
        .record_regression_report_artifact(&sensitive, "pe1-test")
        .unwrap_err();
    assert!(error.contains("raw or sensitive"));
}

#[test]
fn regression_artifact_rejects_envelope_tamper_on_read() {
    let (store, _dir) = make_store();
    let stored = store
        .record_regression_report_artifact(
            &sample_report("native_remember_dont_reread_pilot", "pass"),
            "pe1-test",
        )
        .unwrap();
    let artifact_id = stored["artifact_id"].as_str().unwrap().to_string();
    let conn = rusqlite::Connection::open(store.db_path()).unwrap();
    let mut tampered = stored;
    tampered["registry_id"] = json!("different-registry");
    conn.execute(
        "UPDATE regression_report_artifacts SET artifact_json = ?1 WHERE artifact_id = ?2",
        rusqlite::params![tampered.to_string(), &artifact_id],
    )
    .unwrap();

    let error = store
        .get_regression_report_artifact(&artifact_id)
        .unwrap_err();
    assert!(error.contains("envelope does not match report"));
}
