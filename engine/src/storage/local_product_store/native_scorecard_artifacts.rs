use rusqlite::{params, Row};
use serde_json::{json, Value};

use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

pub const NATIVE_SCORECARD_ARTIFACT_SCHEMA_VERSION: &str = "native_scorecard_artifact.v1";
pub const TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION: &str = "token_efficiency_scorecard.v1";

impl LocalProductStore {
    pub fn record_native_scorecard_artifact(
        &self,
        artifact: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        validate_native_scorecard_artifact(artifact)?;
        let artifact_id = required_str(artifact, "artifact_id")?;
        let scorecard = artifact
            .get("scorecard")
            .ok_or_else(|| "native scorecard artifact missing scorecard".to_string())?;
        let run_id = required_str(scorecard, "adapter_run_id")?;
        let dispatch_id = optional_str(scorecard, "dispatch_id");
        let scorecard_schema_version = required_str(artifact, "scorecard_schema_version")?;
        let content_sha256 = required_str(artifact, "content_sha256")?;
        let redaction_status = required_str(scorecard, "redaction_status")?;
        let mut stored = artifact.clone();
        let created_at = self.now();

        if let Some(obj) = stored.as_object_mut() {
            obj.insert("created_at".to_string(), json!(created_at.clone()));
            obj.insert("storage".to_string(), json!("local_product_store"));
            obj.insert("read_only".to_string(), json!(true));
            obj.insert("target_repository_writes".to_string(), json!("disabled"));
            obj.insert("metadata_only".to_string(), json!(true));
            obj.remove("next_storage_integration");
        }
        let artifact_json = stored.to_string();

        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence =
                    next_sequence(conn, "native_scorecard_artifacts", "artifact_sequence")?;
                conn.execute(
                    "INSERT INTO native_scorecard_artifacts
                     (artifact_sequence, artifact_id, run_id, dispatch_id,
                      scorecard_schema_version, content_sha256, read_only, redaction_status,
                      created_at, artifact_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?8, ?9)",
                    params![
                        sequence,
                        artifact_id,
                        run_id,
                        dispatch_id,
                        scorecard_schema_version,
                        content_sha256,
                        redaction_status,
                        created_at,
                        artifact_json,
                    ],
                )
                .map_err(|e| e.to_string())?;
                append_audit_locked(
                    conn,
                    &created_at,
                    actor,
                    "native_scorecard_artifact.record",
                    &format!("run/{run_id}/artifact/{artifact_id}"),
                    &json!({
                        "run_id": run_id,
                        "dispatch_id": dispatch_id,
                        "schema_version": NATIVE_SCORECARD_ARTIFACT_SCHEMA_VERSION,
                        "scorecard_schema_version": scorecard_schema_version,
                        "read_only": true,
                        "metadata_only": true,
                        "raw_trace_persisted": false,
                        "target_repository_writes": "disabled",
                    }),
                )?;
                Ok(())
            })?,
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence =
                    pg_next_sequence(client, "native_scorecard_artifacts", "artifact_sequence")?;
                client
                    .execute(
                        "INSERT INTO native_scorecard_artifacts
                     (artifact_sequence, artifact_id, run_id, dispatch_id,
                      scorecard_schema_version, content_sha256, read_only, redaction_status,
                      created_at, artifact_json)
                     VALUES ($1, $2, $3, $4, $5, $6, TRUE, $7, $8, $9)",
                        &[
                            &sequence,
                            &artifact_id,
                            &run_id,
                            &dispatch_id,
                            &scorecard_schema_version,
                            &content_sha256,
                            &redaction_status,
                            &created_at,
                            &artifact_json,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                let audit_details = json!({
                    "run_id": run_id,
                    "dispatch_id": dispatch_id,
                    "schema_version": NATIVE_SCORECARD_ARTIFACT_SCHEMA_VERSION,
                    "scorecard_schema_version": scorecard_schema_version,
                    "read_only": true,
                    "metadata_only": true,
                    "raw_trace_persisted": false,
                    "target_repository_writes": "disabled",
                })
                .to_string();
                pg_append_audit(
                    client,
                    &created_at,
                    actor,
                    "native_scorecard_artifact.record",
                    &format!("run/{run_id}/artifact/{artifact_id}"),
                    &audit_details,
                )?;
                Ok(())
            })?,
        }

        self.get_native_scorecard_artifact(artifact_id)?
            .ok_or_else(|| {
                format!("native scorecard artifact not found after insert: {artifact_id}")
            })
    }

    pub fn get_native_scorecard_artifact(
        &self,
        artifact_id: &str,
    ) -> Result<Option<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT artifact_json FROM native_scorecard_artifacts
                         WHERE artifact_id = ?1",
                    )
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt.query(params![artifact_id]).map_err(|e| e.to_string())?;
                if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                    let artifact_json: String = row.get(0).map_err(|e| e.to_string())?;
                    Ok(Some(parse_artifact_json(&artifact_json)?))
                } else {
                    Ok(None)
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT artifact_json FROM native_scorecard_artifacts WHERE artifact_id = $1",
                        &[&artifact_id],
                    )
                    .map_err(|e| e.to_string())?;
                rows.first()
                    .map(|row| row.get::<_, String>(0))
                    .map(|artifact_json| parse_artifact_json(&artifact_json))
                    .transpose()
            }),
        }
    }

    pub fn native_scorecard_artifacts_by_run(
        &self,
        run_id: &str,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        let capped = limit.clamp(1, 100);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT artifact_json FROM native_scorecard_artifacts
                         WHERE run_id = ?1
                         ORDER BY created_at DESC, artifact_sequence DESC
                         LIMIT ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![run_id, capped], native_scorecard_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT artifact_json FROM native_scorecard_artifacts
                         WHERE run_id = $1
                         ORDER BY created_at DESC, artifact_sequence DESC
                         LIMIT $2",
                        &[&run_id, &capped],
                    )
                    .map_err(|e| e.to_string())?;
                rows.iter()
                    .map(|row| parse_artifact_json(&row.get::<_, String>(0)))
                    .collect()
            }),
        }
    }

    pub fn native_scorecard_artifacts_by_dispatch(
        &self,
        dispatch_id: &str,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        let capped = limit.clamp(1, 100);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT artifact_json FROM native_scorecard_artifacts
                         WHERE dispatch_id = ?1
                         ORDER BY created_at DESC, artifact_sequence DESC
                         LIMIT ?2",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![dispatch_id, capped], native_scorecard_row)
                    .map_err(|e| e.to_string())?;
                collect_values(rows)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT artifact_json FROM native_scorecard_artifacts
                         WHERE dispatch_id = $1
                         ORDER BY created_at DESC, artifact_sequence DESC
                         LIMIT $2",
                        &[&dispatch_id, &capped],
                    )
                    .map_err(|e| e.to_string())?;
                rows.iter()
                    .map(|row| parse_artifact_json(&row.get::<_, String>(0)))
                    .collect()
            }),
        }
    }
}

fn native_scorecard_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    let artifact_json: String = row.get(0)?;
    Ok(parse_artifact_json(&artifact_json).unwrap_or(Value::Null))
}

fn parse_artifact_json(artifact_json: &str) -> Result<Value, String> {
    let value: Value = serde_json::from_str(artifact_json).map_err(|e| e.to_string())?;
    validate_no_raw_trace_keys(&value)?;
    Ok(value)
}

fn validate_native_scorecard_artifact(artifact: &Value) -> Result<(), String> {
    validate_no_raw_trace_keys(artifact)?;
    if required_str(artifact, "schema_version")? != NATIVE_SCORECARD_ARTIFACT_SCHEMA_VERSION {
        return Err(
            "native scorecard artifact schema_version must be native_scorecard_artifact.v1"
                .to_string(),
        );
    }
    if required_str(artifact, "artifact_kind")? != "token_efficiency_scorecard" {
        return Err(
            "native scorecard artifact_kind must be token_efficiency_scorecard".to_string(),
        );
    }
    if artifact.get("read_only").and_then(Value::as_bool) != Some(true) {
        return Err("native scorecard artifact must be read_only".to_string());
    }
    if required_str(artifact, "scorecard_schema_version")?
        != TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION
    {
        return Err("scorecard_schema_version must be token_efficiency_scorecard.v1".to_string());
    }
    let scorecard = artifact
        .get("scorecard")
        .ok_or_else(|| "native scorecard artifact missing scorecard".to_string())?;
    if required_str(scorecard, "schema_version")? != TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION {
        return Err("scorecard.schema_version must be token_efficiency_scorecard.v1".to_string());
    }
    if required_str(scorecard, "runtime_kind")? != "native_harness" {
        return Err("scorecard.runtime_kind must be native_harness".to_string());
    }
    let redaction_status = required_str(scorecard, "redaction_status")?;
    if !matches!(redaction_status, "redacted" | "not_applicable") {
        return Err("scorecard.redaction_status must be redacted or not_applicable".to_string());
    }
    Ok(())
}

fn validate_no_raw_trace_keys(value: &Value) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                if is_raw_trace_key(key) {
                    return Err(format!(
                        "raw trace field is not allowed in scorecard artifact: {key}"
                    ));
                }
                validate_no_raw_trace_keys(nested)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                validate_no_raw_trace_keys(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_raw_trace_key(key: &str) -> bool {
    matches!(
        key,
        "raw_trace"
            | "raw_trace_json"
            | "raw_prompt"
            | "raw_output"
            | "transcript"
            | "messages"
            | "repository_text"
            | "repo_full_text"
            | "private_path"
            | "secret"
            | "secrets"
    )
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("missing required string field: {key}"))
}

fn optional_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
}

fn next_sequence(
    conn: &rusqlite::Connection,
    table: &str,
    sequence_column: &str,
) -> Result<i64, String> {
    let sql = format!("SELECT COALESCE(MAX({sequence_column}), 0) + 1 FROM {table}");
    conn.query_row(&sql, [], |row| row.get::<_, i64>(0))
        .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn pg_next_sequence(
    client: &mut impl postgres::GenericClient,
    table: &str,
    column: &str,
) -> Result<i64, String> {
    let sql = format!("SELECT COALESCE(MAX({column}), 0) + 1 FROM {table}");
    let val: i64 = client
        .query_one(&sql, &[])
        .map_err(|e| e.to_string())?
        .get(0);
    Ok(val)
}

#[cfg(feature = "pg")]
fn pg_append_audit(
    client: &mut impl postgres::GenericClient,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &str,
) -> Result<(), String> {
    client
        .execute(
            "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
             VALUES ($1, $2, $3, $4, $5)",
            &[&now, &actor, &action, &resource, &details],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}
