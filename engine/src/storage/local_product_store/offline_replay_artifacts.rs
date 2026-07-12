use rusqlite::{params, Row};
use serde_json::{json, Value};

use crate::feedback::{
    offline_replay_report_sha256, OfflineEvaluationEngine, OfflineReplayReport,
    OfflineReplayRequest, OfflineReplayStatus, OFFLINE_REPLAY_SCHEMA_VERSION,
};

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
        let artifact_json = stored.to_string();
        let created_at = required_str(&stored, "created_at")?.to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_sequence(conn)?;
                let changed = conn
                    .execute(
                        "INSERT INTO offline_replay_artifacts
                         (artifact_sequence, artifact_id, report_schema_version, status,
                          eligibility_content_sha256, content_sha256, created_at, artifact_json)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                         ON CONFLICT(artifact_id) DO NOTHING",
                        params![
                            sequence,
                            artifact_id,
                            stored["report_schema_version"].as_str().unwrap_or_default(),
                            stored["status"].as_str().unwrap_or_default(),
                            stored["eligibility_content_sha256"]
                                .as_str()
                                .unwrap_or_default(),
                            stored["content_sha256"].as_str().unwrap_or_default(),
                            created_at,
                            artifact_json,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if changed == 1 {
                    append_audit_locked(
                        conn,
                        &created_at,
                        actor,
                        "offline_replay_artifact.record",
                        &format!("offline-replay/{artifact_id}"),
                        &audit_details(&stored),
                    )?;
                }
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let row = client
                    .query_opt(
                        "INSERT INTO offline_replay_artifacts
                         (artifact_id, report_schema_version, status,
                          eligibility_content_sha256, content_sha256, created_at, artifact_json)
                         VALUES ($1, $2, $3, $4, $5, $6, $7)
                         ON CONFLICT (artifact_id) DO NOTHING
                         RETURNING artifact_sequence",
                        &[
                            &artifact_id,
                            &stored["report_schema_version"].as_str().unwrap_or_default(),
                            &stored["status"].as_str().unwrap_or_default(),
                            &stored["eligibility_content_sha256"]
                                .as_str()
                                .unwrap_or_default(),
                            &stored["content_sha256"].as_str().unwrap_or_default(),
                            &created_at,
                            &artifact_json,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if row.is_some() {
                    pg_append_audit(
                        client,
                        &created_at,
                        actor,
                        "offline_replay_artifact.record",
                        &format!("offline-replay/{artifact_id}"),
                        &audit_details(&stored).to_string(),
                    )?;
                }
                Ok(())
            })?,
        }
        self.get_offline_replay_artifact(&artifact_id)?
            .ok_or_else(|| "offline replay artifact not found after insert".to_string())
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
    if report.schema_version != OFFLINE_REPLAY_SCHEMA_VERSION
        || offline_replay_report_sha256(report).map_err(|error| error.to_string())?
            != report.content_sha256
        || !report.shadow_only
        || report.influence_selected_tier
        || report.influence_executor_type
        || report.influence_retry_path
        || report.influence_routing_policy
    {
        return Err("offline replay report is not a valid read-only report".to_string());
    }
    if !valid_sha256(&report.content_sha256) || !valid_sha256(&report.eligibility_content_sha256) {
        return Err("offline replay report hash is invalid".to_string());
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
        if policy.content_sha256().map_err(|error| error.to_string())? != policy.policy_hash {
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
    if required_str(artifact, "schema_version")? != OFFLINE_REPLAY_ARTIFACT_SCHEMA_VERSION
        || required_str(artifact, "report_schema_version")? != OFFLINE_REPLAY_SCHEMA_VERSION
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
    validate_report(&report)?;
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
    let artifact: Value = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    validate_stored_artifact(&artifact)?;
    Ok(artifact)
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
    client: &mut postgres::Client,
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
    use crate::feedback::{OfflinePolicyDefinition, OfflinePolicySelection};
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
}
