use rusqlite::{params, Row};
use serde::Serialize;
use serde_json::{json, Value};

use crate::budget_manager::{BudgetAnomalyFinding, BudgetForecastEvidence};

use super::native_scorecard_artifacts::{validate_json_bounds, MAX_SCORECARD_ARTIFACT_BYTES};
use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

const BUDGET_EVIDENCE_ARTIFACT_SCHEMA_VERSION: &str = "budget_evidence_artifact.v1";

impl LocalProductStore {
    pub fn record_budget_forecast_evidence(
        &self,
        evidence: &BudgetForecastEvidence,
        actor: &str,
    ) -> Result<Value, String> {
        evidence.validate()?;
        self.record_budget_evidence_artifact(
            "forecast",
            &evidence.forecast_id,
            &evidence.evidence_sha256,
            evidence,
            actor,
        )
    }

    pub fn record_budget_anomaly_finding(
        &self,
        finding: &BudgetAnomalyFinding,
        actor: &str,
    ) -> Result<Value, String> {
        finding.validate()?;
        self.record_budget_evidence_artifact(
            "anomaly",
            &finding.finding_id,
            &finding.evidence_sha256,
            finding,
            actor,
        )
    }

    pub fn budget_evidence_artifacts(
        &self,
        kind: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        let kind = kind.filter(|kind| matches!(*kind, "forecast" | "anomaly"));
        if kind.is_none() && limit < 0 {
            return Err("budget evidence artifact kind must be forecast or anomaly".to_string());
        }
        let capped = limit.clamp(1, 100);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let rows = if let Some(kind) = kind {
                    let mut statement = conn
                        .prepare(
                            "SELECT artifact_json FROM budget_evidence_artifacts
                             WHERE artifact_kind = ?1 ORDER BY artifact_sequence ASC LIMIT ?2",
                        )
                        .map_err(|error| error.to_string())?;
                    let rows = statement
                        .query_map(params![kind, capped], budget_evidence_artifact_row)
                        .map_err(|error| error.to_string())?;
                    collect_values(rows)?
                } else {
                    let mut statement = conn
                        .prepare(
                            "SELECT artifact_json FROM budget_evidence_artifacts
                             ORDER BY artifact_sequence ASC LIMIT ?1",
                        )
                        .map_err(|error| error.to_string())?;
                    let rows = statement
                        .query_map(params![capped], budget_evidence_artifact_row)
                        .map_err(|error| error.to_string())?;
                    collect_values(rows)?
                };
                Ok(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = if let Some(kind) = kind {
                    client.query(
                        "SELECT artifact_json FROM budget_evidence_artifacts
                         WHERE artifact_kind = $1 ORDER BY artifact_sequence ASC LIMIT $2",
                        &[&kind, &capped],
                    )
                } else {
                    client.query(
                        "SELECT artifact_json FROM budget_evidence_artifacts
                         ORDER BY artifact_sequence ASC LIMIT $1",
                        &[&capped],
                    )
                }
                .map_err(|error| error.to_string())?;
                rows.iter()
                    .map(|row| parse_stored_artifact(&row.get::<_, String>(0)))
                    .collect()
            }),
        }
    }

    pub fn get_budget_evidence_artifact(&self, artifact_id: &str) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut statement = conn
                    .prepare("SELECT artifact_json FROM budget_evidence_artifacts WHERE artifact_id = ?1")
                    .map_err(|error| error.to_string())?;
                let mut rows = statement.query(params![artifact_id]).map_err(|error| error.to_string())?;
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
                    .query("SELECT artifact_json FROM budget_evidence_artifacts WHERE artifact_id = $1", &[&artifact_id])
                    .map_err(|error| error.to_string())?
                    .first()
                    .map(|row| parse_stored_artifact(&row.get::<_, String>(0)))
                    .transpose()
            }),
        }
    }

    fn record_budget_evidence_artifact<T: Serialize>(
        &self,
        kind: &str,
        evidence_id: &str,
        evidence_sha256: &str,
        evidence: &T,
        actor: &str,
    ) -> Result<Value, String> {
        let artifact_id = format!("budget-{kind}-{evidence_sha256}");
        if let Some(existing) = self.get_budget_evidence_artifact(&artifact_id)? {
            return Ok(existing);
        }
        let evidence = serde_json::to_value(evidence).map_err(|error| error.to_string())?;
        let created_at = self.now();
        let stored = json!({
            "schema_version": BUDGET_EVIDENCE_ARTIFACT_SCHEMA_VERSION,
            "artifact_id": artifact_id,
            "artifact_kind": kind,
            "evidence_id": evidence_id,
            "evidence_sha256": evidence_sha256,
            "created_at": created_at,
            "storage": "local_product_store",
            "read_only": true,
            "metadata_only": true,
            "provider_calls": "disabled",
            "mutation_authority": "none",
            "target_repository_writes": "disabled",
            "evidence": evidence,
        });
        validate_stored_artifact(&stored)?;
        let artifact_json = stored.to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_sequence(conn)?;
                conn.execute(
                    "INSERT INTO budget_evidence_artifacts
                     (artifact_sequence, artifact_id, artifact_kind, evidence_id, evidence_sha256, created_at, artifact_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![sequence, artifact_id, kind, evidence_id, evidence_sha256, created_at, artifact_json],
                ).map_err(|error| error.to_string())?;
                append_audit_locked(conn, &created_at, actor, "budget_evidence_artifact.record", &format!("budget-evidence/{artifact_id}"), &audit_details(&stored))?;
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence = pg_next_sequence(client)?;
                client.execute(
                    "INSERT INTO budget_evidence_artifacts
                     (artifact_sequence, artifact_id, artifact_kind, evidence_id, evidence_sha256, created_at, artifact_json)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)",
                    &[&sequence, &artifact_id, &kind, &evidence_id, &evidence_sha256, &created_at, &artifact_json],
                ).map_err(|error| error.to_string())?;
                pg_append_audit(client, &created_at, actor, "budget_evidence_artifact.record", &format!("budget-evidence/{artifact_id}"), &audit_details(&stored).to_string())
            })?,
        }
        self.get_budget_evidence_artifact(&artifact_id)?
            .ok_or_else(|| "budget evidence artifact not found after insert".to_string())
    }
}

fn validate_stored_artifact(artifact: &Value) -> Result<(), String> {
    if serde_json::to_vec(artifact)
        .map_err(|error| error.to_string())?
        .len()
        > MAX_SCORECARD_ARTIFACT_BYTES
    {
        return Err("budget evidence artifact exceeds bounded size".to_string());
    }
    validate_json_bounds(artifact, "budget_evidence_artifact", 0)?;
    let kind = required_str(artifact, "artifact_kind")?;
    let evidence = artifact
        .get("evidence")
        .ok_or_else(|| "budget evidence artifact missing evidence".to_string())?;
    let (id, hash) = match kind {
        "forecast" => {
            let value: BudgetForecastEvidence =
                serde_json::from_value(evidence.clone()).map_err(|error| error.to_string())?;
            value.validate()?;
            (value.forecast_id, value.evidence_sha256)
        }
        "anomaly" => {
            let value: BudgetAnomalyFinding =
                serde_json::from_value(evidence.clone()).map_err(|error| error.to_string())?;
            value.validate()?;
            (value.finding_id, value.evidence_sha256)
        }
        _ => return Err("budget evidence artifact kind must be forecast or anomaly".to_string()),
    };
    if required_str(artifact, "schema_version")? != BUDGET_EVIDENCE_ARTIFACT_SCHEMA_VERSION
        || required_str(artifact, "evidence_id")? != id
        || required_str(artifact, "evidence_sha256")? != hash
        || required_str(artifact, "artifact_id")? != format!("budget-{kind}-{hash}")
        || artifact.get("read_only").and_then(Value::as_bool) != Some(true)
        || artifact.get("metadata_only").and_then(Value::as_bool) != Some(true)
        || artifact.get("provider_calls").and_then(Value::as_str) != Some("disabled")
        || artifact.get("mutation_authority").and_then(Value::as_str) != Some("none")
    {
        return Err("budget evidence artifact envelope does not match evidence".to_string());
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
        .ok_or_else(|| format!("budget evidence artifact {key} must be a non-empty string"))
}

fn audit_details(artifact: &Value) -> Value {
    json!({"artifact_id": artifact["artifact_id"], "artifact_kind": artifact["artifact_kind"], "evidence_id": artifact["evidence_id"], "evidence_sha256": artifact["evidence_sha256"], "read_only": true, "metadata_only": true, "mutation_authority": "none"})
}

fn budget_evidence_artifact_row(row: &Row<'_>) -> rusqlite::Result<Value> {
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
        "SELECT COALESCE(MAX(artifact_sequence), 0) + 1 FROM budget_evidence_artifacts",
        [],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

#[cfg(feature = "pg")]
fn pg_next_sequence(client: &mut postgres::Client) -> Result<i64, String> {
    client
        .query_one(
            "SELECT COALESCE(MAX(artifact_sequence), 0) + 1 FROM budget_evidence_artifacts",
            &[],
        )
        .map(|row| row.get(0))
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
    client.execute("INSERT INTO audit_log (created_at, actor, action, resource, details_json) VALUES ($1, $2, $3, $4, $5)", &[&timestamp, &actor, &action, &resource, &details]).map(|_| ()).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget_anomaly::{
        detect_budget_anomaly, BudgetAnomalyObservation, BudgetAnomalyRequest,
    };
    use crate::budget_forecast::{
        build_budget_forecast, BudgetForecastRequest, BudgetUsageObservation,
    };
    use crate::budget_manager::{BudgetAnomalyKind, BudgetEvidenceScope};
    use tempfile::tempdir;

    fn scope() -> BudgetEvidenceScope {
        BudgetEvidenceScope {
            provider_id: Some("provider-a".to_string()),
            ..Default::default()
        }
    }

    fn forecast() -> BudgetForecastEvidence {
        let observations = (0..3)
            .map(|index| BudgetUsageObservation {
                evidence_type: "provider_audit_event".to_string(),
                evidence_id: format!("forecast-{index}"),
                content_sha256: Some(format!("{:064x}", index)),
                occurred_at: format!("2026-07-10T00:{:02}:00Z", 10 + index),
                run_id: None,
                workspace_id: None,
                provider_id: Some("provider-a".to_string()),
                model_id: Some("model-a".to_string()),
                input_tokens: Some(10),
                output_tokens: Some(10),
                total_tokens: Some(20),
                cost_usd: Some(0.01),
            })
            .collect::<Vec<_>>();
        build_budget_forecast(
            &BudgetForecastRequest {
                forecast_id: "forecast-1".to_string(),
                scope: scope(),
                start_inclusive: "2026-07-10T00:00:00Z".to_string(),
                end_exclusive: "2026-07-10T01:00:00Z".to_string(),
                generated_at: "2026-07-10T01:01:00Z".to_string(),
                horizon_seconds: 60,
                remaining_tokens: Some(100),
                remaining_cost_usd: Some(1.0),
                required_dimensions: vec!["provider_id".to_string()],
                min_samples: 3,
                max_freshness_seconds: 600,
                max_duplicate_events: 1,
            },
            &observations,
        )
        .unwrap()
    }

    fn anomaly() -> BudgetAnomalyFinding {
        let observations = (0..6)
            .map(|index| BudgetAnomalyObservation {
                evidence_type: "provider_audit_event".to_string(),
                evidence_id: format!("anomaly-{index}"),
                content_sha256: Some(format!("{:064x}", index)),
                occurred_at: format!(
                    "2026-07-10T0{}:{:02}:00Z",
                    if index < 3 { 0 } else { 1 },
                    10 + index
                ),
                run_id: None,
                workspace_id: None,
                provider_id: Some("provider-a".to_string()),
                model_id: Some("model-a".to_string()),
                total_tokens: Some(if index < 3 { 10 } else { 30 }),
                cost_usd: Some(0.01),
                retry_count: Some(1),
                latency_ms: Some(10),
                context_bytes: Some(10),
            })
            .collect::<Vec<_>>();
        detect_budget_anomaly(
            &BudgetAnomalyRequest {
                finding_id: "finding-1".to_string(),
                scope: scope(),
                anomaly_kind: BudgetAnomalyKind::TokenSpike,
                baseline_start_inclusive: "2026-07-10T00:00:00Z".to_string(),
                current_start_inclusive: "2026-07-10T01:00:00Z".to_string(),
                current_end_exclusive: "2026-07-10T02:00:00Z".to_string(),
                generated_at: "2026-07-10T02:01:00Z".to_string(),
                min_samples_per_window: 3,
                max_freshness_seconds: 600,
                max_duplicate_events: 1,
                required_dimensions: vec!["provider_id".to_string()],
                relative_increase_threshold: 0.5,
                absolute_increase_threshold: 10.0,
                critical_increase_threshold: 1.0,
            },
            &observations,
        )
        .unwrap()
    }

    #[test]
    fn budget_evidence_artifacts_are_idempotent_sorted_and_tamper_checked() {
        let directory = tempdir().unwrap();
        let store = LocalProductStore::new(directory.path().join("store.db")).unwrap();
        let forecast = forecast();
        let finding = anomaly();
        let first = store
            .record_budget_forecast_evidence(&forecast, "test")
            .unwrap();
        assert_eq!(
            first,
            store
                .record_budget_forecast_evidence(&forecast, "test")
                .unwrap()
        );
        store
            .record_budget_anomaly_finding(&finding, "test")
            .unwrap();
        let artifacts = store.budget_evidence_artifacts(None, 100).unwrap();
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0]["artifact_kind"], "forecast");
        assert_eq!(artifacts[1]["artifact_kind"], "anomaly");
        let artifact_id = first["artifact_id"].as_str().unwrap();
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection.execute("UPDATE budget_evidence_artifacts SET artifact_json = '{\"tampered\":true}' WHERE artifact_id = ?1", params![artifact_id]).unwrap();
        assert!(store.get_budget_evidence_artifact(artifact_id).is_err());
    }
}
