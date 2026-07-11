use rusqlite::{params, Row};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::event_schema::canonical_event_json;

use super::native_scorecard_artifacts::{
    validate_json_bounds, validate_no_raw_trace_keys, MAX_SCORECARD_ARTIFACT_BYTES,
};
use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

const REGRESSION_ARTIFACT_SCHEMA_VERSION: &str = "token_efficiency_regression_artifact.v1";
const REGRESSION_REPORT_SCHEMA_VERSION: &str = "token_efficiency_regression_report.v1";
const REGRESSION_BATCH_SCHEMA_VERSION: &str = "token_efficiency_regression_batch.v1";
const REGRESSION_TREND_SCHEMA_VERSION: &str = "token_efficiency_regression_trend.v1";
const TREND_METRICS: &[&str] = &[
    "total_tokens",
    "repeated_context_ratio",
    "state_bytes",
    "estimated_cost_usd",
    "duration_ms",
    "retry_count",
    "quality_score",
];

struct ValidatedReport<'a> {
    artifact_kind: &'static str,
    report_schema_version: &'a str,
    registry_id: &'a str,
    registry_sha256: &'a str,
    scenario_id: Option<&'a str>,
    content_sha256: &'a str,
}

impl LocalProductStore {
    pub fn record_regression_report_artifact(
        &self,
        report: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        let validated = validate_report(report)?;
        let artifact_id = format!(
            "regression-{}-{}",
            if validated.scenario_id.is_some() {
                "report"
            } else {
                "batch"
            },
            validated.content_sha256
        );

        if let Some(existing) = self.get_regression_report_artifact(&artifact_id)? {
            if required_str(&existing, "content_sha256")? == validated.content_sha256 {
                return Ok(existing);
            }
            return Err(format!(
                "regression artifact id collision with different content: {artifact_id}"
            ));
        }

        let created_at = self.now();
        let stored = json!({
            "schema_version": REGRESSION_ARTIFACT_SCHEMA_VERSION,
            "artifact_id": artifact_id,
            "artifact_kind": validated.artifact_kind,
            "report_schema_version": validated.report_schema_version,
            "content_sha256": validated.content_sha256,
            "registry_id": validated.registry_id,
            "registry_sha256": validated.registry_sha256,
            "scenario_id": validated.scenario_id,
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
        let artifact_json = stored.to_string();

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_sequence(conn)?;
                conn.execute(
                    "INSERT INTO regression_report_artifacts
                     (artifact_sequence, artifact_id, artifact_kind, report_schema_version,
                      registry_id, registry_sha256, scenario_id, content_sha256,
                      created_at, artifact_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        sequence,
                        artifact_id,
                        validated.artifact_kind,
                        validated.report_schema_version,
                        validated.registry_id,
                        validated.registry_sha256,
                        validated.scenario_id,
                        validated.content_sha256,
                        created_at,
                        artifact_json,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "regression_report_artifact.record",
                    &format!("regression-artifact/{artifact_id}"),
                    &audit_details(&stored),
                )?;
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence = pg_next_sequence(client)?;
                client
                    .execute(
                        "INSERT INTO regression_report_artifacts
                         (artifact_sequence, artifact_id, artifact_kind, report_schema_version,
                          registry_id, registry_sha256, scenario_id, content_sha256,
                          created_at, artifact_json)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                        &[
                            &sequence,
                            &artifact_id,
                            &validated.artifact_kind,
                            &validated.report_schema_version,
                            &validated.registry_id,
                            &validated.registry_sha256,
                            &validated.scenario_id,
                            &validated.content_sha256,
                            &created_at,
                            &artifact_json,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                pg_append_audit(
                    client,
                    &created_at,
                    actor,
                    "regression_report_artifact.record",
                    &format!("regression-artifact/{artifact_id}"),
                    &audit_details(&stored).to_string(),
                )
            })?,
        }

        self.get_regression_report_artifact(&artifact_id)?
            .ok_or_else(|| format!("regression artifact not found after insert: {artifact_id}"))
    }

    pub fn get_regression_report_artifact(
        &self,
        artifact_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT artifact_json FROM regression_report_artifacts
                         WHERE artifact_id = ?1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt.query(params![artifact_id]).map_err(|e| e.to_string())?;
                rows.next()
                    .map_err(|e| e.to_string())?
                    .map(|row| row.get::<_, String>(0).map_err(|e| e.to_string()))
                    .transpose()?
                    .map(|raw| parse_stored_artifact(&raw))
                    .transpose()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query(
                        "SELECT artifact_json FROM regression_report_artifacts WHERE artifact_id = $1",
                        &[&artifact_id],
                    )
                    .map_err(|e| e.to_string())?
                    .first()
                    .map(|row| parse_stored_artifact(&row.get::<_, String>(0)))
                    .transpose()
            }),
        }
    }

    pub fn regression_report_artifacts(&self, limit: i64) -> Result<Vec<Value>, String> {
        self.query_regression_report_artifacts(None, limit)
    }

    pub fn regression_report_artifacts_by_scenario(
        &self,
        scenario_id: &str,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        self.query_regression_report_artifacts(Some(scenario_id), limit)
    }

    pub fn regression_report_trend(&self, scenario_id: &str, limit: i64) -> Result<Value, String> {
        if scenario_id.trim().is_empty() || scenario_id.len() > 128 {
            return Err("regression trend scenario_id must be a bounded id".to_string());
        }
        let artifacts = self.recent_regression_reports_by_scenario(scenario_id, limit)?;
        build_regression_trend(scenario_id, &artifacts)
    }

    fn recent_regression_reports_by_scenario(
        &self,
        scenario_id: &str,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        let capped = limit.clamp(1, 100);
        let mut artifacts = match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT artifact_json FROM regression_report_artifacts
                         WHERE scenario_id = ?1
                         ORDER BY artifact_sequence DESC LIMIT ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![scenario_id, capped], regression_artifact_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT artifact_json FROM regression_report_artifacts
                         WHERE scenario_id = $1
                         ORDER BY artifact_sequence DESC LIMIT $2",
                        &[&scenario_id, &capped],
                    )
                    .map_err(|e| e.to_string())?;
                rows.iter()
                    .map(|row| parse_stored_artifact(&row.get::<_, String>(0)))
                    .collect::<Result<Vec<_>, _>>()
            })?,
        };
        artifacts.reverse();
        Ok(artifacts)
    }

    fn query_regression_report_artifacts(
        &self,
        scenario_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        let capped = limit.clamp(1, 100);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let (sql, filter): (&str, Option<&str>) = if scenario_id.is_some() {
                    (
                        "SELECT artifact_json FROM regression_report_artifacts
                         WHERE scenario_id = ?1
                         ORDER BY artifact_sequence ASC LIMIT ?2",
                        scenario_id,
                    )
                } else {
                    (
                        "SELECT artifact_json FROM regression_report_artifacts
                         WHERE ?1 IS NULL
                         ORDER BY artifact_sequence ASC LIMIT ?2",
                        None,
                    )
                };
                let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![filter, capped], regression_artifact_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = if let Some(scenario_id) = scenario_id {
                    client.query(
                        "SELECT artifact_json FROM regression_report_artifacts
                         WHERE scenario_id = $1 ORDER BY artifact_sequence ASC LIMIT $2",
                        &[&scenario_id, &capped],
                    )
                } else {
                    client.query(
                        "SELECT artifact_json FROM regression_report_artifacts
                         ORDER BY artifact_sequence ASC LIMIT $1",
                        &[&capped],
                    )
                }
                .map_err(|e| e.to_string())?;
                rows.iter()
                    .map(|row| parse_stored_artifact(&row.get::<_, String>(0)))
                    .collect()
            }),
        }
    }
}

fn build_regression_trend(scenario_id: &str, artifacts: &[Value]) -> Result<Value, String> {
    let points = artifacts
        .iter()
        .map(trend_point)
        .collect::<Result<Vec<_>, _>>()?;
    let transitions = points
        .windows(2)
        .map(|window| trend_transition(&window[0], &window[1]))
        .collect::<Result<Vec<_>, _>>()?;
    let mut trend = json!({
        "schema_version": REGRESSION_TREND_SCHEMA_VERSION,
        "scenario_id": scenario_id,
        "read_only": true,
        "report_only": true,
        "provider_calls": "disabled",
        "mutation_authority": "none",
        "point_count": points.len(),
        "points": points,
        "transitions": transitions,
        "latest": points.last().cloned().unwrap_or(Value::Null),
    });
    let canonical = canonical_event_json(&trend).map_err(|e| e.to_string())?;
    trend["trend_sha256"] = json!(hex::encode(Sha256::digest(canonical.as_bytes())));
    if serde_json::to_vec(&trend).map_err(|e| e.to_string())?.len() > MAX_SCORECARD_ARTIFACT_BYTES {
        return Err("regression trend exceeds bounded size".to_string());
    }
    validate_json_bounds(&trend, "regression_trend", 0)?;
    validate_no_raw_trace_keys(&trend)
        .map_err(|_| "raw or sensitive payload is not allowed in regression trend".to_string())?;
    Ok(trend)
}

fn trend_point(artifact: &Value) -> Result<Value, String> {
    let report = artifact
        .get("report")
        .ok_or_else(|| "regression trend artifact missing report".to_string())?;
    let metrics = report
        .pointer("/comparisons/best_known/metrics")
        .or_else(|| report.pointer("/comparisons/baseline/metrics"))
        .and_then(Value::as_object)
        .map(|metrics| {
            TREND_METRICS
                .iter()
                .filter_map(|name| {
                    let metric = metrics.get(*name)?;
                    metric
                        .get("current")
                        .filter(|value| value.is_number())
                        .map(|value| ((*name).to_string(), value.clone()))
                })
                .collect::<Map<_, _>>()
        })
        .unwrap_or_default();
    Ok(json!({
        "artifact_id": required_str(artifact, "artifact_id")?,
        "created_at": required_str(artifact, "created_at")?,
        "registry_id": required_str(report, "registry_id")?,
        "registry_sha256": required_str(report, "registry_sha256")?,
        "report_sha256": required_str(report, "report_sha256")?,
        "outcome": required_str(report, "outcome")?,
        "reason_codes": required_array(report, "reason_codes")?,
        "evidence": trend_evidence_links(report),
        "current_metrics": metrics,
    }))
}

fn trend_evidence_links(report: &Value) -> Value {
    let mut links = Map::new();
    let evidence = report.get("evidence").and_then(Value::as_object);
    for role in ["current", "baseline", "best_known"] {
        let Some(source) = evidence
            .and_then(|value| value.get(role))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let mut link = Map::new();
        for field in [
            "adapter_run_id",
            "artifact_schema_version",
            "content_sha256",
        ] {
            if let Some(value) = source.get(field).filter(|value| value.is_string()) {
                link.insert(field.to_string(), value.clone());
            }
        }
        links.insert(role.to_string(), Value::Object(link));
    }
    Value::Object(links)
}

fn trend_transition(from: &Value, to: &Value) -> Result<Value, String> {
    let from_reasons = string_set(required_array(from, "reason_codes")?);
    let to_reasons = string_set(required_array(to, "reason_codes")?);
    let from_metrics = required_object(from, "current_metrics")?;
    let to_metrics = required_object(to, "current_metrics")?;
    let mut metric_deltas = Map::new();
    for (name, to_value) in to_metrics {
        let Some(from_number) = from_metrics.get(name).and_then(Value::as_f64) else {
            continue;
        };
        let Some(to_number) = to_value.as_f64() else {
            continue;
        };
        let delta = round_six(to_number - from_number);
        let harmful_delta = if name == "quality_score" {
            -delta
        } else {
            delta
        };
        let direction = if harmful_delta > 0.0 {
            "regressed"
        } else if harmful_delta < 0.0 {
            "improved"
        } else {
            "unchanged"
        };
        metric_deltas.insert(
            name.clone(),
            json!({"delta": delta, "direction": direction}),
        );
    }
    Ok(json!({
        "from_artifact_id": required_str(from, "artifact_id")?,
        "to_artifact_id": required_str(to, "artifact_id")?,
        "from_outcome": required_str(from, "outcome")?,
        "to_outcome": required_str(to, "outcome")?,
        "outcome_changed": required_str(from, "outcome")? != required_str(to, "outcome")?,
        "new_reason_codes": to_reasons.difference(&from_reasons).cloned().collect::<Vec<_>>(),
        "resolved_reason_codes": from_reasons.difference(&to_reasons).cloned().collect::<Vec<_>>(),
        "metric_deltas": metric_deltas,
    }))
}

fn string_set(values: &[Value]) -> std::collections::BTreeSet<String> {
    values
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn round_six(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn validate_report(report: &Value) -> Result<ValidatedReport<'_>, String> {
    let bytes = serde_json::to_vec(report).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_SCORECARD_ARTIFACT_BYTES {
        return Err("regression report exceeds bounded artifact size".to_string());
    }
    validate_json_bounds(report, "regression_report", 0)?;
    validate_no_raw_trace_keys(report).map_err(|_| {
        "raw or sensitive payload is not allowed in regression report artifact".to_string()
    })?;
    require_safety_boundary(report)?;

    let report_schema_version = required_str(report, "schema_version")?;
    let registry_id = bounded_id(report, "registry_id")?;
    let registry_sha256 = required_hash(report, "registry_sha256")?;
    match report_schema_version {
        REGRESSION_REPORT_SCHEMA_VERSION => {
            let scenario_id = bounded_id(report, "scenario_id")?;
            required_hash(report, "scenario_digest")?;
            required_hash(report, "task_digest")?;
            required_object(report, "evidence")?;
            required_object(report, "comparisons")?;
            let reason_codes = required_array(report, "reason_codes")?;
            if reason_codes.len() > 32
                || reason_codes.iter().any(|value| {
                    value
                        .as_str()
                        .is_none_or(|reason| reason.is_empty() || reason.len() > 128)
                })
            {
                return Err("regression report reason_codes must be bounded strings".to_string());
            }
            match required_str(report, "outcome")? {
                "pass" | "regression" | "missing_baseline" | "missing_best_known"
                | "incomparable" | "quality_failure" => {}
                other => return Err(format!("unsupported regression outcome: {other}")),
            }
            let content_sha256 = validate_self_hash(report, "report_sha256")?;
            Ok(ValidatedReport {
                artifact_kind: "token_efficiency_regression_report",
                report_schema_version,
                registry_id,
                registry_sha256,
                scenario_id: Some(scenario_id),
                content_sha256,
            })
        }
        REGRESSION_BATCH_SCHEMA_VERSION => {
            let reports = required_array(report, "reports")?;
            let count = required_i64(report, "scenario_count")?;
            if !(3..=100).contains(&count) || reports.len() != count as usize {
                return Err(
                    "regression batch scenario_count must match 3..=100 reports".to_string()
                );
            }
            let mut prior = None;
            for item in reports {
                let nested = validate_report(item)?;
                let scenario_id = nested
                    .scenario_id
                    .ok_or_else(|| "regression batch cannot contain another batch".to_string())?;
                if nested.registry_id != registry_id || nested.registry_sha256 != registry_sha256 {
                    return Err("regression batch reports must share registry identity".to_string());
                }
                if prior.is_some_and(|value: &str| value >= scenario_id) {
                    return Err(
                        "regression batch scenario ids must be unique and sorted".to_string()
                    );
                }
                prior = Some(scenario_id);
            }
            validate_outcome_counts(report, reports)?;
            let content_sha256 = validate_self_hash(report, "batch_sha256")?;
            Ok(ValidatedReport {
                artifact_kind: "token_efficiency_regression_batch",
                report_schema_version,
                registry_id,
                registry_sha256,
                scenario_id: None,
                content_sha256,
            })
        }
        other => Err(format!("unsupported regression report schema: {other}")),
    }
}

fn validate_outcome_counts(batch: &Value, reports: &[Value]) -> Result<(), String> {
    let expected = required_object(batch, "outcome_counts")?;
    let mut actual = Map::new();
    for report in reports {
        let outcome = required_str(report, "outcome")?;
        let count = actual.get(outcome).and_then(Value::as_i64).unwrap_or(0) + 1;
        actual.insert(outcome.to_string(), json!(count));
    }
    if expected != &actual {
        return Err("regression batch outcome_counts must match reports".to_string());
    }
    Ok(())
}

fn validate_stored_artifact(artifact: &Value) -> Result<(), String> {
    if serde_json::to_vec(artifact)
        .map_err(|e| e.to_string())?
        .len()
        > MAX_SCORECARD_ARTIFACT_BYTES
    {
        return Err("stored regression artifact exceeds bounded size".to_string());
    }
    if required_str(artifact, "schema_version")? != REGRESSION_ARTIFACT_SCHEMA_VERSION {
        return Err("unsupported stored regression artifact schema".to_string());
    }
    validate_json_bounds(artifact, "regression_artifact", 0)?;
    validate_no_raw_trace_keys(artifact).map_err(|_| {
        "raw or sensitive payload is not allowed in regression report artifact".to_string()
    })?;
    let report = artifact
        .get("report")
        .ok_or_else(|| "stored regression artifact missing report".to_string())?;
    let validated = validate_report(report)?;
    if required_str(artifact, "artifact_kind")? != validated.artifact_kind
        || required_str(artifact, "report_schema_version")? != validated.report_schema_version
        || required_str(artifact, "registry_id")? != validated.registry_id
        || required_str(artifact, "registry_sha256")? != validated.registry_sha256
        || required_str(artifact, "content_sha256")? != validated.content_sha256
        || artifact.get("scenario_id").and_then(Value::as_str) != validated.scenario_id
    {
        return Err("stored regression artifact envelope does not match report".to_string());
    }
    let expected_id = format!(
        "regression-{}-{}",
        if validated.scenario_id.is_some() {
            "report"
        } else {
            "batch"
        },
        validated.content_sha256
    );
    if required_str(artifact, "artifact_id")? != expected_id {
        return Err("stored regression artifact id does not match content".to_string());
    }
    Ok(())
}

fn parse_stored_artifact(raw: &str) -> Result<Value, String> {
    if raw.len() > MAX_SCORECARD_ARTIFACT_BYTES {
        return Err("stored regression artifact exceeds bounded size".to_string());
    }
    let artifact: Value = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    validate_stored_artifact(&artifact)?;
    Ok(artifact)
}

fn validate_self_hash<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    let supplied = required_hash(value, field)?;
    let mut canonical = value.clone();
    canonical
        .as_object_mut()
        .ok_or_else(|| "regression report must be an object".to_string())?
        .remove(field);
    let calculated = hex::encode(Sha256::digest(
        canonical_event_json(&canonical)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    ));
    if supplied != calculated {
        return Err(format!("{field} does not match canonical content"));
    }
    Ok(supplied)
}

fn require_safety_boundary(value: &Value) -> Result<(), String> {
    if value.get("read_only").and_then(Value::as_bool) != Some(true)
        || value.get("report_only").and_then(Value::as_bool) != Some(true)
        || value.get("provider_calls").and_then(Value::as_str) != Some("disabled")
        || value.get("mutation_authority").and_then(Value::as_str) != Some("none")
    {
        return Err("regression report must preserve read-only report-only safety".to_string());
    }
    Ok(())
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("regression report {key} must be a non-empty string"))
}

fn bounded_id<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    let id = required_str(value, key)?;
    if id.len() > 128
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(format!("regression report {key} is not a bounded id"));
    }
    Ok(id)
}

fn required_hash<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    let hash = required_str(value, key)?;
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("regression report {key} must be lowercase sha256"));
    }
    Ok(hash)
}

fn required_i64(value: &Value, key: &str) -> Result<i64, String> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("regression report {key} must be an integer"))
}

fn required_object<'a>(value: &'a Value, key: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("regression report {key} must be an object"))
}

fn required_array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("regression report {key} must be an array"))
}

fn audit_details(stored: &Value) -> Value {
    json!({
        "artifact_id": stored.get("artifact_id"),
        "artifact_kind": stored.get("artifact_kind"),
        "report_schema_version": stored.get("report_schema_version"),
        "registry_id": stored.get("registry_id"),
        "scenario_id": stored.get("scenario_id"),
        "content_sha256": stored.get("content_sha256"),
        "read_only": true,
        "metadata_only": true,
        "provider_calls": "disabled",
        "mutation_authority": "none",
        "raw_payload_persisted": false,
    })
}

fn regression_artifact_row(row: &Row<'_>) -> rusqlite::Result<Value> {
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
        "SELECT COALESCE(MAX(artifact_sequence), 0) + 1 FROM regression_report_artifacts",
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn pg_next_sequence(client: &mut postgres::Client) -> Result<i64, String> {
    client
        .query_one(
            "SELECT COALESCE(MAX(artifact_sequence), 0) + 1 FROM regression_report_artifacts",
            &[],
        )
        .map(|row| row.get(0))
        .map_err(|e| e.to_string())
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
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
             VALUES ($1, $2, $3, $4, $5)",
            &[&timestamp, &actor, &action, &resource, &details],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}
