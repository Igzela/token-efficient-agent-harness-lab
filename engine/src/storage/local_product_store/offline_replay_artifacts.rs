use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value};

use crate::feedback::{
    judge_calibration_is_acceptable, offline_replay_report_sha256,
    validate_offline_replay_report_bounds, OfflineEvaluationEngine, OfflineReplayReport,
    OfflineReplayRequest, OfflineReplayStatus, LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION,
    OFFLINE_REPLAY_SCHEMA_VERSION,
};
use sha2::{Digest, Sha256};

use super::native_scorecard_artifacts::{validate_json_bounds, MAX_SCORECARD_ARTIFACT_BYTES};
use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

const OFFLINE_REPLAY_ARTIFACT_SCHEMA_VERSION: &str = "offline_replay_artifact.v1";

impl LocalProductStore {
    /// Evaluate a trace-backed replay and persist only its bounded, immutable
    /// read model. This helper does not grant replay or production authority.
    pub fn record_offline_replay(
        &self,
        request: &OfflineReplayRequest,
        actor: &str,
    ) -> Result<Value, String> {
        let report =
            OfflineEvaluationEngine::replay_policies(request).map_err(|error| error.to_string())?;
        self.record_offline_replay_artifact(&report, actor)
    }

    pub fn record_offline_replay_artifact(
        &self,
        report: &OfflineReplayReport,
        actor: &str,
    ) -> Result<Value, String> {
        let stored = build_stored_artifact(report, self.now())?;
        let artifact_id = required_str(&stored, "artifact_id")?.to_string();
        let report_schema_version = required_str(&stored, "report_schema_version")?.to_string();
        let status = required_str(&stored, "status")?.to_string();
        let eligibility_content_sha256 =
            required_str(&stored, "eligibility_content_sha256")?.to_string();
        let content_sha256 = required_str(&stored, "content_sha256")?.to_string();
        let artifact_json = stored.to_string();
        let created_at = required_str(&stored, "created_at")?.to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx=rusqlite::Transaction::new_unchecked(conn,rusqlite::TransactionBehavior::Immediate).map_err(|error|error.to_string())?;
                let existing:Option<String>=tx.query_row("SELECT artifact_json FROM offline_replay_artifacts WHERE artifact_id=?1",params![artifact_id],|row|row.get(0)).optional().map_err(|error|error.to_string())?;
                if let Some(existing)=existing{return parse_stored_artifact(&existing)}
                let sequence = next_sequence(&tx)?;
                tx.execute(
                        "INSERT INTO offline_replay_artifacts
                         (artifact_sequence, artifact_id, report_schema_version, status,
                          eligibility_content_sha256, content_sha256, created_at, artifact_json)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                         ON CONFLICT(artifact_id) DO NOTHING",
                        params![
                            sequence,
                            artifact_id,
                            report_schema_version,
                            status,
                            eligibility_content_sha256,
                            content_sha256,
                            created_at,
                            artifact_json,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                append_audit_locked(
                        &tx,
                        &created_at,
                        actor,
                        "offline_replay_artifact.record",
                        &format!("offline-replay/{artifact_id}"),
                        &audit_details(&stored),
                    )?;
                tx.commit().map_err(|error|error.to_string())?;
                Ok(stored.clone())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx=client.transaction().map_err(|error|error.to_string())?;
                let existing=tx.query_opt("SELECT artifact_json FROM offline_replay_artifacts WHERE artifact_id=$1 FOR UPDATE",&[&artifact_id]).map_err(|error|error.to_string())?;
                if let Some(existing)=existing{return parse_stored_artifact(&existing.get::<_,String>(0))}
                tx.execute(
                        "INSERT INTO offline_replay_artifacts
                         (artifact_id, report_schema_version, status,
                          eligibility_content_sha256, content_sha256, created_at, artifact_json)
                         VALUES ($1, $2, $3, $4, $5, $6, $7)
                         ON CONFLICT (artifact_id) DO NOTHING",
                        &[
                            &artifact_id,
                            &report_schema_version,
                            &status,
                            &eligibility_content_sha256,
                            &content_sha256,
                            &created_at,
                            &artifact_json,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                pg_append_audit(
                        &mut tx,
                        &created_at,
                        actor,
                        "offline_replay_artifact.record",
                        &format!("offline-replay/{artifact_id}"),
                        &audit_details(&stored).to_string(),
                    )?;
                tx.commit().map_err(|error|error.to_string())?;
                Ok(stored.clone())
            }),
        }
    }

    pub fn offline_replay_artifacts(
        &self,
        status: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Value>, String> {
        if status.is_some_and(|status| !is_status(status)) {
            return Err("offline replay artifact status is invalid".to_string());
        }
        let limit = limit.clamp(1, 100);
        let offset = offset.clamp(0, 10_000);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let rows = if let Some(status) = status {
                    let mut statement = conn
                        .prepare(
                            "SELECT artifact_json FROM offline_replay_artifacts
                             WHERE status = ?1 ORDER BY artifact_sequence ASC LIMIT ?2 OFFSET ?3",
                        )
                        .map_err(|error| error.to_string())?;
                    let rows = statement
                        .query_map(params![status, limit, offset], offline_replay_artifact_row)
                        .map_err(|error| error.to_string())?;
                    collect_values(rows)?
                } else {
                    let mut statement = conn
                        .prepare(
                            "SELECT artifact_json FROM offline_replay_artifacts
                             ORDER BY artifact_sequence ASC LIMIT ?1 OFFSET ?2",
                        )
                        .map_err(|error| error.to_string())?;
                    let rows = statement
                        .query_map(params![limit, offset], offline_replay_artifact_row)
                        .map_err(|error| error.to_string())?;
                    collect_values(rows)?
                };
                Ok(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = if let Some(status) = status {
                    client.query(
                        "SELECT artifact_json FROM offline_replay_artifacts
                             WHERE status = $1 ORDER BY artifact_sequence ASC LIMIT $2 OFFSET $3",
                        &[&status, &limit, &offset],
                    )
                } else {
                    client.query(
                        "SELECT artifact_json FROM offline_replay_artifacts
                         ORDER BY artifact_sequence ASC LIMIT $1 OFFSET $2",
                        &[&limit, &offset],
                    )
                }
                .map_err(|error| error.to_string())?;
                rows.iter()
                    .map(|row| parse_stored_artifact(&row.get::<_, String>(0)))
                    .collect()
            }),
        }
    }

    pub fn get_offline_replay_artifact(&self, artifact_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut statement = conn
                    .prepare(
                        "SELECT artifact_json FROM offline_replay_artifacts WHERE artifact_id = ?1",
                    )
                    .map_err(|error| error.to_string())?;
                let mut rows = statement
                    .query(params![artifact_id])
                    .map_err(|error| error.to_string())?;
                rows.next()
                    .map_err(|error| error.to_string())?
                    .map(|row| row.get::<_, String>(0).map_err(|error| error.to_string()))
                    .transpose()?
                    .map(|raw| parse_stored_artifact(&raw))
                    .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query(
                        "SELECT artifact_json FROM offline_replay_artifacts WHERE artifact_id = $1",
                        &[&artifact_id],
                    )
                    .map_err(|error| error.to_string())?
                    .first()
                    .map(|row| parse_stored_artifact(&row.get::<_, String>(0)))
                    .transpose()
            }),
        }
    }
}

fn build_stored_artifact(
    report: &OfflineReplayReport,
    created_at: String,
) -> Result<Value, String> {
    validate_report(report)?;
    let stored = json!({
        "schema_version": OFFLINE_REPLAY_ARTIFACT_SCHEMA_VERSION,
        "artifact_id": format!("offline-replay-{}", report.content_sha256),
        "report_schema_version": report.schema_version,
        "status": status_string(report.status),
        "eligibility_content_sha256": report.eligibility_content_sha256,
        "content_sha256": report.content_sha256,
        "created_at": created_at,
        "storage": "local_product_store",
        "read_only": true,
        "metadata_only": true,
        "provider_calls": "disabled",
        "mutation_authority": "none",
        "target_repository_writes": "disabled",
        "report": report,
    });
    validate_stored_artifact(&stored)?;
    Ok(stored)
}

fn validate_report(report: &OfflineReplayReport) -> Result<(), String> {
    validate_report_for_schema(report, false)
}

fn validate_report_for_schema(
    report: &OfflineReplayReport,
    allow_legacy: bool,
) -> Result<(), String> {
    validate_offline_replay_report_bounds(report)?;
    let is_current = report.schema_version == OFFLINE_REPLAY_SCHEMA_VERSION;
    let is_legacy = report.schema_version == LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION;
    if !(is_current || allow_legacy && is_legacy)
        || !report.shadow_only
        || report.influence_selected_tier
        || report.influence_executor_type
        || report.influence_retry_path
        || report.influence_routing_policy
    {
        return Err("offline replay report is not a valid read-only report".to_string());
    }
    let report_hash = if is_current {
        offline_replay_report_sha256(report).map_err(|error| error.to_string())?
    } else {
        legacy_sha256_without_content_hash(report)?
    };
    if report_hash != report.content_sha256 {
        return Err("offline replay report hash is invalid".to_string());
    }
    if !valid_sha256(&report.content_sha256) || !valid_sha256(&report.eligibility_content_sha256) {
        return Err("offline replay report hash is invalid".to_string());
    }
    if is_current
        && report
            .replay_judge_calibrations
            .iter()
            .any(|calibration| !judge_calibration_is_acceptable(calibration))
    {
        return Err("offline replay judge calibration is invalid".to_string());
    }
    for hash in report
        .source_evidence_content_sha256
        .iter()
        .chain(
            report
                .observed_facts
                .iter()
                .flat_map(|fact| fact.evidence_content_sha256.iter()),
        )
        .chain(
            report
                .counterfactual_estimates
                .iter()
                .flat_map(|estimate| estimate.source_evidence_content_sha256.iter()),
        )
    {
        if !valid_sha256(hash) {
            return Err("offline replay source evidence hash is invalid".to_string());
        }
    }
    for policy in std::iter::once(&report.current_policy).chain(report.candidate_policies.iter()) {
        if is_current && policy.schema_version != OFFLINE_REPLAY_SCHEMA_VERSION {
            return Err("offline replay policy schema is invalid".to_string());
        }
        let policy_hash = if is_current {
            policy.content_sha256().map_err(|error| error.to_string())?
        } else {
            legacy_sha256_without_policy_hash(policy)?
        };
        if policy_hash != policy.policy_hash {
            return Err("offline replay policy hash is invalid".to_string());
        }
    }
    Ok(())
}

fn validate_stored_artifact(artifact: &Value) -> Result<(), String> {
    if serde_json::to_vec(artifact)
        .map_err(|error| error.to_string())?
        .len()
        > MAX_SCORECARD_ARTIFACT_BYTES
    {
        return Err("offline replay artifact exceeds bounded size".to_string());
    }
    validate_json_bounds(artifact, "offline_replay_artifact", 0)?;
    let report_schema_version = required_str(artifact, "report_schema_version")?;
    if required_str(artifact, "schema_version")? != OFFLINE_REPLAY_ARTIFACT_SCHEMA_VERSION
        || (report_schema_version != OFFLINE_REPLAY_SCHEMA_VERSION
            && report_schema_version != LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION)
        || required_str(artifact, "artifact_id")?
            != format!(
                "offline-replay-{}",
                required_str(artifact, "content_sha256")?
            )
        || artifact.get("read_only").and_then(Value::as_bool) != Some(true)
        || artifact.get("metadata_only").and_then(Value::as_bool) != Some(true)
        || required_str(artifact, "provider_calls")? != "disabled"
        || required_str(artifact, "mutation_authority")? != "none"
        || required_str(artifact, "target_repository_writes")? != "disabled"
    {
        return Err("offline replay artifact envelope does not match report".to_string());
    }
    let report: OfflineReplayReport = serde_json::from_value(
        artifact
            .get("report")
            .cloned()
            .ok_or_else(|| "offline replay artifact missing report".to_string())?,
    )
    .map_err(|error| error.to_string())?;
    validate_report_for_schema(&report, true)?;
    if required_str(artifact, "status")? != status_string(report.status)
        || required_str(artifact, "eligibility_content_sha256")?
            != report.eligibility_content_sha256
        || required_str(artifact, "content_sha256")? != report.content_sha256
    {
        return Err("offline replay artifact metadata does not match report".to_string());
    }
    Ok(())
}

fn parse_stored_artifact(raw: &str) -> Result<Value, String> {
    let mut artifact: Value = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    validate_stored_artifact(&artifact)?;
    if artifact
        .get("report_schema_version")
        .and_then(Value::as_str)
        == Some(LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION)
    {
        if let Value::Object(map) = &mut artifact {
            map.insert("historical_only".to_string(), Value::Bool(true));
            map.insert(
                "authorization".to_string(),
                Value::String("none".to_string()),
            );
        }
    }
    Ok(artifact)
}

fn legacy_sha256_without_content_hash(report: &OfflineReplayReport) -> Result<String, String> {
    let mut unsigned = report.clone();
    unsigned.content_sha256.clear();
    legacy_sha256(&unsigned)
}

fn legacy_sha256_without_policy_hash(
    policy: &crate::feedback::OfflinePolicyDefinition,
) -> Result<String, String> {
    let mut unsigned = policy.clone();
    unsigned.policy_hash.clear();
    legacy_sha256(&unsigned)
}

fn legacy_sha256<T: serde::Serialize>(value: &T) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("offline replay artifact {key} must be a non-empty string"))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_status(value: &str) -> bool {
    matches!(
        value,
        "sufficient"
            | "insufficient_evidence"
            | "incompatible_cohort"
            | "stale_evidence"
            | "tampered_evidence"
            | "uncalibrated_evidence"
            | "out_of_distribution"
    )
}

fn status_string(status: OfflineReplayStatus) -> &'static str {
    match status {
        OfflineReplayStatus::Sufficient => "sufficient",
        OfflineReplayStatus::InsufficientEvidence => "insufficient_evidence",
        OfflineReplayStatus::IncompatibleCohort => "incompatible_cohort",
        OfflineReplayStatus::StaleEvidence => "stale_evidence",
        OfflineReplayStatus::TamperedEvidence => "tampered_evidence",
        OfflineReplayStatus::UncalibratedEvidence => "uncalibrated_evidence",
        OfflineReplayStatus::OutOfDistribution => "out_of_distribution",
    }
}

fn audit_details(artifact: &Value) -> Value {
    json!({
        "artifact_id": artifact["artifact_id"],
        "status": artifact["status"],
        "eligibility_content_sha256": artifact["eligibility_content_sha256"],
        "content_sha256": artifact["content_sha256"],
        "read_only": true,
        "metadata_only": true,
        "mutation_authority": "none"
    })
}

fn offline_replay_artifact_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let raw: String = row.get(0)?;
    parse_stored_artifact(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        )
    })
}

fn next_sequence(conn: &rusqlite::Connection) -> Result<i64, String> {
    conn.query_row(
        "SELECT COALESCE(MAX(artifact_sequence), 0) + 1 FROM offline_replay_artifacts",
        [],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

#[cfg(feature = "pg")]
fn pg_append_audit(
    client: &mut impl postgres::GenericClient,
    timestamp: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &str,
) -> Result<(), String> {
    client
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json) VALUES ($1, $2, $3, $4, $5)",
            &[&timestamp, &actor, &action, &resource, &details],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::{OfflinePolicyDefinition, OfflinePolicySelection, ShadowRouter};
    use serde_json::json;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn report() -> OfflineReplayReport {
        let policy = OfflinePolicyDefinition::new("current", "v1", BTreeMap::new()).unwrap();
        let mut report = OfflineReplayReport {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            status: OfflineReplayStatus::InsufficientEvidence,
            reason_codes: vec!["insufficient_replay_observations".to_string()],
            current_policy: policy.clone(),
            candidate_policies: vec![OfflinePolicyDefinition::new(
                "candidate",
                "v1",
                BTreeMap::from([(
                    "task".to_string(),
                    OfflinePolicySelection {
                        candidate_id: "candidate-a".to_string(),
                        candidate_version: "v1".to_string(),
                        candidate_definition_sha256: format!("{:064x}", 1),
                    },
                )]),
            )
            .unwrap()],
            observed_facts: Vec::new(),
            counterfactual_estimates: Vec::new(),
            comparisons: Vec::new(),
            outcomes: Vec::new(),
            eligibility_content_sha256: format!("{:064x}", 2),
            replay_judge_calibrations: Vec::new(),
            source_trace_ids: Vec::new(),
            source_evidence_content_sha256: Vec::new(),
            shadow_only: true,
            influence_selected_tier: false,
            influence_executor_type: false,
            influence_retry_path: false,
            influence_routing_policy: false,
            content_sha256: String::new(),
        };
        report.content_sha256 = offline_replay_report_sha256(&report).unwrap();
        report
    }

    #[test]
    fn offline_replay_artifacts_are_idempotent_bounded_and_tamper_checked() {
        let directory = tempdir().unwrap();
        let store = LocalProductStore::new(directory.path().join("store.db")).unwrap();
        let report = report();
        let first = store
            .record_offline_replay_artifact(&report, "test")
            .unwrap();
        assert_eq!(
            first,
            store
                .record_offline_replay_artifact(&report, "test")
                .unwrap()
        );
        assert_eq!(
            store.offline_replay_artifacts(None, 100, 0).unwrap().len(),
            1
        );
        assert_eq!(
            store
                .offline_replay_artifacts(Some("insufficient_evidence"), 100, 0)
                .unwrap()
                .len(),
            1
        );
        assert!(store.offline_replay_artifacts(Some("bad"), 10, 0).is_err());
        let artifact_id = first["artifact_id"].as_str().unwrap();
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "UPDATE offline_replay_artifacts SET artifact_json = '{\"tampered\":true}' WHERE artifact_id = ?1",
                params![artifact_id],
            )
            .unwrap();
        assert!(store.get_offline_replay_artifact(artifact_id).is_err());
    }

    #[test]
    fn legacy_replay_artifacts_are_readable_but_non_authorizing() {
        let directory = tempdir().unwrap();
        let store = LocalProductStore::new(directory.path().join("legacy-store.db")).unwrap();
        let mut legacy = report();
        legacy.schema_version = LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION.to_string();
        legacy.current_policy.schema_version = LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION.to_string();
        for policy in &mut legacy.candidate_policies {
            policy.schema_version = LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION.to_string();
            policy.policy_hash = legacy_sha256_without_policy_hash(policy).unwrap();
        }
        legacy.current_policy.policy_hash =
            legacy_sha256_without_policy_hash(&legacy.current_policy).unwrap();
        legacy.content_sha256 = legacy_sha256_without_content_hash(&legacy).unwrap();
        let artifact = json!({
            "schema_version": OFFLINE_REPLAY_ARTIFACT_SCHEMA_VERSION,
            "artifact_id": format!("offline-replay-{}", legacy.content_sha256),
            "report_schema_version": LEGACY_OFFLINE_REPLAY_SCHEMA_VERSION,
            "status": status_string(legacy.status),
            "eligibility_content_sha256": legacy.eligibility_content_sha256,
            "content_sha256": legacy.content_sha256,
            "created_at": "2026-07-12T00:00:00Z",
            "storage": "local_product_store",
            "read_only": true,
            "metadata_only": true,
            "provider_calls": "disabled",
            "mutation_authority": "none",
            "target_repository_writes": "disabled",
            "report": legacy,
        });
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute(
                "INSERT INTO offline_replay_artifacts
                 (artifact_sequence, artifact_id, report_schema_version, status,
                  eligibility_content_sha256, content_sha256, created_at, artifact_json)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    artifact["artifact_id"].as_str().unwrap(),
                    artifact["report_schema_version"].as_str().unwrap(),
                    artifact["status"].as_str().unwrap(),
                    artifact["eligibility_content_sha256"].as_str().unwrap(),
                    artifact["content_sha256"].as_str().unwrap(),
                    artifact["created_at"].as_str().unwrap(),
                    artifact.to_string(),
                ],
            )
            .unwrap();

        let loaded = store
            .get_offline_replay_artifact(artifact["artifact_id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(loaded["historical_only"], true);
        assert_eq!(loaded["authorization"], "none");
        let parsed: OfflineReplayReport = serde_json::from_value(loaded["report"].clone()).unwrap();
        assert!(ShadowRouter::compare_replay_report(&parsed).is_err());
    }
}
