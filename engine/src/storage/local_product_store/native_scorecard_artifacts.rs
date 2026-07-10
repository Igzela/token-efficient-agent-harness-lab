use rusqlite::{params, Row};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{append_audit_locked, collect_values, DatabaseConnection, LocalProductStore};

pub const NATIVE_SCORECARD_ARTIFACT_SCHEMA_VERSION: &str = "native_scorecard_artifact.v1";
pub const TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION: &str = "token_efficiency_scorecard.v1";

impl LocalProductStore {
    pub fn record_automatic_native_scorecard_for_run(
        &self,
        run_id: &str,
        actor: &str,
    ) -> Result<Option<Value>, String> {
        let Some(run) = self.get_workflow_run(run_id)? else {
            return Ok(None);
        };
        let status = run.get("status").and_then(Value::as_str).unwrap_or("");
        if !is_scorecard_terminal_status(status) {
            return Ok(None);
        }
        let artifact = build_native_scorecard_artifact_from_workflow_run(&run, &self.now())?;
        self.record_native_scorecard_artifact(&artifact, actor)
            .map(Some)
    }

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

        if let Some(existing) = self.get_native_scorecard_artifact(artifact_id)? {
            let existing_hash = required_str(&existing, "content_sha256")?;
            if existing_hash == content_sha256 {
                return Ok(existing);
            }
            return Err(format!(
                "native scorecard artifact id collision with different content: {artifact_id}"
            ));
        }

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

fn build_native_scorecard_artifact_from_workflow_run(
    run: &Value,
    created_at: &str,
) -> Result<Value, String> {
    let scorecard = build_token_efficiency_scorecard_from_workflow_run(run)?;
    let canonical = serde_json::to_string(&scorecard).map_err(|e| e.to_string())?;
    let content_sha256 = hex::encode(Sha256::digest(canonical.as_bytes()));
    let run_id = required_str(&scorecard, "adapter_run_id")?;
    Ok(json!({
        "schema_version": NATIVE_SCORECARD_ARTIFACT_SCHEMA_VERSION,
        "artifact_kind": "token_efficiency_scorecard",
        "storage": "app_owned_artifact_json_export",
        "read_only": true,
        "created_at": created_at,
        "artifact_id": format!("scorecard-{run_id}-{}", &content_sha256[..12]),
        "content_sha256": content_sha256,
        "scorecard_schema_version": TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION,
        "scorecard": scorecard,
    }))
}

fn build_token_efficiency_scorecard_from_workflow_run(run: &Value) -> Result<Value, String> {
    let run_id = required_str(run, "run_id")?;
    let status = run_status_to_scorecard_status(run.get("status").and_then(Value::as_str));
    let nodes = run
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let steps = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| workflow_node_to_scorecard_step(node, run_id, index))
        .collect::<Result<Vec<_>, _>>()?;
    let input_token_total: i64 = steps
        .iter()
        .map(|step| number_i64(step, "input_tokens"))
        .sum();
    let output_token_total: i64 = steps
        .iter()
        .map(|step| number_i64(step, "output_tokens"))
        .sum();
    let context_token_total: i64 = steps
        .iter()
        .map(|step| number_i64(step, "context_tokens"))
        .sum();
    let repeated_context_token_total: i64 = steps
        .iter()
        .map(|step| number_i64(step, "repeated_context_tokens"))
        .sum();
    let retrieved_ref_token_total: i64 = steps
        .iter()
        .map(|step| number_i64(step, "retrieved_ref_tokens"))
        .sum();
    let tool_call_count = steps
        .iter()
        .filter(|step| step.get("operation_kind").and_then(Value::as_str) == Some("tool_call"))
        .count() as i64;
    let retry_count: i64 = nodes
        .iter()
        .map(|node| {
            positive_i64(node.get("attempt_count"))
                .saturating_sub(1)
                .max(0)
        })
        .sum();
    let duration_ms = run_duration_ms(run).unwrap_or_else(|| {
        steps
            .iter()
            .map(|step| number_i64(step, "duration_ms"))
            .sum()
    });
    let estimated_cost_usd = nodes
        .iter()
        .filter_map(|node| {
            node.pointer("/result/estimated_cost")
                .and_then(Value::as_f64)
        })
        .filter(|cost| *cost >= 0.0)
        .sum::<f64>();
    // Native scorecard projection is evidence-only: it reads metrics already
    // persisted on workflow runs/nodes by native executors and keeps unknown
    // fields at conservative zero defaults instead of inferring from raw text.
    let metric_sources = json!({
        "input_output_tokens": "workflow_run_nodes.node_json.result.input_tokens/output_tokens when reported by native provider/CLI/adaptive executors; otherwise 0",
        "context_tokens": "workflow_run_nodes.node_json.context_tokens when bounded context assembly records it; otherwise 0",
        "retrieved_refs": "workflow_run_nodes.node_json.retrieved_refs_count/retrieved_ref_tokens when recorded; otherwise 0",
        "repeated_context_tokens": "workflow_run_nodes.node_json.repeated_context_tokens when recorded; otherwise 0",
        "tool_call_count": "workflow node task_type/executor classification for native command/tool nodes",
        "redundant_tool_call_count": "0 until native execution persists stable tool-call identity/hash evidence",
        "retry_count": "workflow_run_nodes.attempt_count - 1",
        "duration_ms": "workflow run started_at/completed_at, falling back to node result latency_ms or node started_at/completed_at",
        "status_quality": "workflow run/node terminal status and terminal audit reason",
        "source_trace_payload": "not persisted"
    });
    let mut scorecard = json!({
        "schema_version": TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION,
        "adapter_run_id": run_id,
        "runtime_kind": "native_harness",
        "runtime_version": "native-harness",
        "scenario_id": run.get("workflow_id").and_then(Value::as_str).unwrap_or("native-run"),
        "mode": "native_control_plane",
        "state_strategy": "mixed",
        "status": status,
        "pass_fail_reason": terminal_reason(run),
        "quality_method": if status == "pass" { "test" } else { "none" },
        "input_token_total": input_token_total,
        "output_token_total": output_token_total,
        "context_token_total": context_token_total,
        "repeated_context_token_total": repeated_context_token_total,
        "retrieved_ref_token_total": retrieved_ref_token_total,
        "tool_call_count": tool_call_count,
        "redundant_tool_call_count": 0,
        "retry_count": retry_count,
        "step_count": steps.len() as i64,
        "duration_ms": duration_ms,
        "estimated_cost_usd": estimated_cost_usd,
        "raw_trace_artifact_id": format!("native-scorecard-source-{run_id}"),
        "redaction_status": "redacted",
        "metric_sources": metric_sources,
        "steps": steps,
    });
    if status == "pass" {
        scorecard["quality_score"] = json!(1.0);
    }
    if let Some(dispatch_id) = run
        .get("dispatch_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        scorecard["dispatch_id"] = json!(dispatch_id);
    }
    let derived = derived_metrics(&scorecard);
    scorecard["derived_metrics"] = derived;
    validate_native_scorecard_scorecard(&scorecard)?;
    Ok(scorecard)
}

fn workflow_node_to_scorecard_step(
    node: &Value,
    run_id: &str,
    index: usize,
) -> Result<Value, String> {
    let node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{run_id}-step-{index}"));
    let result = node.get("result").unwrap_or(&Value::Null);
    let status = run_status_to_scorecard_status(
        node.get("db_status")
            .or_else(|| node.get("status"))
            .and_then(Value::as_str),
    );
    let input_tokens = positive_i64(result.get("input_tokens"));
    let output_tokens = positive_i64(result.get("output_tokens"));
    let context_tokens = positive_i64(node.get("context_tokens"));
    let retrieved_ref_tokens = positive_i64(node.get("retrieved_ref_tokens"));
    let repeated_context_tokens = positive_i64(node.get("repeated_context_tokens"));
    let output_metrics = result
        .get("output")
        .and_then(Value::as_str)
        .filter(|output| output.len() <= 64 * 1024)
        .and_then(|output| serde_json::from_str::<Value>(output).ok());
    let state_read_bytes = positive_i64(node.get("state_read_bytes")).max(positive_i64(
        output_metrics
            .as_ref()
            .and_then(|metrics| metrics.get("state_read_bytes")),
    ));
    let state_write_bytes = positive_i64(node.get("state_write_bytes")).max(positive_i64(
        output_metrics
            .as_ref()
            .and_then(|metrics| metrics.get("state_write_bytes")),
    ));
    if repeated_context_tokens > context_tokens {
        return Err("step repeated_context_tokens cannot exceed context_tokens".to_string());
    }
    if retrieved_ref_tokens > context_tokens {
        return Err("step retrieved_ref_tokens cannot exceed context_tokens".to_string());
    }
    Ok(json!({
        "adapter_step_id": node_id,
        "adapter_run_id": run_id,
        "step_index": index as i64,
        "node_name": node.get("name").and_then(Value::as_str).unwrap_or_else(|| {
            node.get("node_id").and_then(Value::as_str).unwrap_or("workflow_node")
        }),
        "agent_role": "unknown",
        "operation_kind": operation_kind_for_node(node),
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "context_tokens": context_tokens,
        "repeated_context_tokens": repeated_context_tokens,
        "retrieved_refs_count": positive_i64(node.get("retrieved_refs_count")),
        "retrieved_ref_tokens": retrieved_ref_tokens,
        "tool_name": Value::Null,
        "tool_call_id": Value::Null,
        "status": status,
        "error_kind": result
            .get("error_domain")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(if status == "pass" { "none" } else { status }),
        "state_read_bytes": state_read_bytes,
        "state_write_bytes": state_write_bytes,
        "duration_ms": positive_i64(result.get("latency_ms"))
            .max(node_duration_ms(node).unwrap_or(0)),
    }))
}

fn validate_native_scorecard_scorecard(scorecard: &Value) -> Result<(), String> {
    validate_no_raw_trace_keys(scorecard)?;
    for key in [
        "adapter_run_id",
        "runtime_kind",
        "runtime_version",
        "scenario_id",
        "mode",
        "state_strategy",
        "status",
        "pass_fail_reason",
        "quality_method",
        "raw_trace_artifact_id",
        "redaction_status",
    ] {
        required_str(scorecard, key)?;
    }
    if required_str(scorecard, "schema_version")? != TOKEN_EFFICIENCY_SCORECARD_SCHEMA_VERSION {
        return Err("scorecard.schema_version must be token_efficiency_scorecard.v1".to_string());
    }
    if required_str(scorecard, "runtime_kind")? != "native_harness" {
        return Err("scorecard.runtime_kind must be native_harness".to_string());
    }
    if !matches!(
        required_str(scorecard, "mode")?,
        "native_control_plane"
            | "stateless_reread"
            | "stateful_store"
            | "pruned_context"
            | "external_runtime"
    ) {
        return Err("scorecard.mode is not allowed".to_string());
    }
    if !matches!(
        required_str(scorecard, "state_strategy")?,
        "none" | "full_history" | "durable_state" | "memory_digest" | "retrieval_refs" | "mixed"
    ) {
        return Err("scorecard.state_strategy is not allowed".to_string());
    }
    if !matches!(
        required_str(scorecard, "status")?,
        "pass" | "fail" | "error" | "blocked"
    ) {
        return Err("scorecard.status must be pass, fail, error, or blocked".to_string());
    }
    if !matches!(
        required_str(scorecard, "quality_method")?,
        "rule" | "test" | "human_review" | "model_judge" | "mixed" | "none"
    ) {
        return Err("scorecard.quality_method is not allowed".to_string());
    }
    if required_str(scorecard, "status")? == "pass"
        && required_str(scorecard, "quality_method")? == "none"
    {
        return Err("passing runs require a non-none quality_method".to_string());
    }
    if !matches!(
        required_str(scorecard, "redaction_status")?,
        "not_needed" | "redacted" | "rejected"
    ) {
        return Err("scorecard.redaction_status is not allowed".to_string());
    }
    for key in [
        "input_token_total",
        "output_token_total",
        "context_token_total",
        "repeated_context_token_total",
        "retrieved_ref_token_total",
        "tool_call_count",
        "redundant_tool_call_count",
        "retry_count",
        "step_count",
        "duration_ms",
    ] {
        require_nonnegative_number(scorecard, key)?;
    }
    if scorecard.get("estimated_cost_usd").is_some() && !scorecard["estimated_cost_usd"].is_null() {
        require_nonnegative_number(scorecard, "estimated_cost_usd")?;
    }
    if number_i64(scorecard, "redundant_tool_call_count") > number_i64(scorecard, "tool_call_count")
    {
        return Err("redundant_tool_call_count cannot exceed tool_call_count".to_string());
    }
    if number_i64(scorecard, "repeated_context_token_total")
        > number_i64(scorecard, "context_token_total")
    {
        return Err("repeated_context_token_total cannot exceed context_token_total".to_string());
    }
    if number_i64(scorecard, "retrieved_ref_token_total")
        > number_i64(scorecard, "context_token_total")
    {
        return Err("retrieved_ref_token_total cannot exceed context_token_total".to_string());
    }
    let derived = scorecard
        .get("derived_metrics")
        .ok_or_else(|| "scorecard.derived_metrics must be present".to_string())?;
    require_nonnegative_number(derived, "total_tokens")?;
    for key in [
        "context_share",
        "repeated_context_ratio",
        "tool_redundancy_ratio",
        "step_retry_ratio",
    ] {
        require_nonnegative_number(derived, key)?;
    }
    let steps = scorecard
        .get("steps")
        .and_then(Value::as_array)
        .ok_or_else(|| "scorecard.steps must be a list".to_string())?;
    if number_i64(scorecard, "step_count") != steps.len() as i64 {
        return Err("step_count must match scorecard steps".to_string());
    }
    let run_id = required_str(scorecard, "adapter_run_id")?;
    for (index, step) in steps.iter().enumerate() {
        validate_scorecard_step(step, run_id, index)?;
    }
    Ok(())
}

fn validate_scorecard_step(step: &Value, run_id: &str, index: usize) -> Result<(), String> {
    for key in [
        "adapter_step_id",
        "adapter_run_id",
        "node_name",
        "agent_role",
        "operation_kind",
        "status",
        "error_kind",
    ] {
        required_str(step, key)?;
    }
    if required_str(step, "adapter_run_id")? != run_id {
        return Err(
            "scorecard step adapter_run_id must match scorecard adapter_run_id".to_string(),
        );
    }
    for key in [
        "step_index",
        "input_tokens",
        "output_tokens",
        "context_tokens",
        "repeated_context_tokens",
        "retrieved_refs_count",
        "retrieved_ref_tokens",
        "state_read_bytes",
        "state_write_bytes",
    ] {
        require_nonnegative_number(step, key)?;
    }
    if number_i64(step, "step_index") != index as i64 {
        return Err("scorecard step_index must equal zero-based order".to_string());
    }
    if !matches!(
        required_str(step, "agent_role")?,
        "planner" | "executor" | "reviewer" | "evaluator" | "unknown"
    ) {
        return Err("scorecard step agent_role is not allowed".to_string());
    }
    if !matches!(
        required_str(step, "operation_kind")?,
        "model_call"
            | "tool_call"
            | "state_read"
            | "state_write"
            | "retrieval"
            | "evaluation"
            | "control"
    ) {
        return Err("scorecard step operation_kind is not allowed".to_string());
    }
    if !matches!(
        required_str(step, "status")?,
        "pass" | "fail" | "error" | "blocked"
    ) {
        return Err("scorecard step status is not allowed".to_string());
    }
    if number_i64(step, "repeated_context_tokens") > number_i64(step, "context_tokens") {
        return Err("step repeated_context_tokens cannot exceed context_tokens".to_string());
    }
    if number_i64(step, "retrieved_ref_tokens") > number_i64(step, "context_tokens") {
        return Err("step retrieved_ref_tokens cannot exceed context_tokens".to_string());
    }
    Ok(())
}

fn derived_metrics(scorecard: &Value) -> Value {
    let total_tokens =
        number_i64(scorecard, "input_token_total") + number_i64(scorecard, "output_token_total");
    let status = scorecard
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("");
    json!({
        "total_tokens": total_tokens,
        "context_share": ratio(number_i64(scorecard, "context_token_total"), total_tokens),
        "repeated_context_ratio": ratio(
            number_i64(scorecard, "repeated_context_token_total"),
            number_i64(scorecard, "context_token_total"),
        ),
        "tool_redundancy_ratio": ratio(
            number_i64(scorecard, "redundant_tool_call_count"),
            number_i64(scorecard, "tool_call_count"),
        ),
        "tokens_per_passing_run": if status == "pass" { json!(total_tokens) } else { Value::Null },
        "cost_per_passing_run": if status == "pass" {
            scorecard.get("estimated_cost_usd").cloned().unwrap_or(Value::Null)
        } else {
            Value::Null
        },
        "step_retry_ratio": ratio(number_i64(scorecard, "retry_count"), number_i64(scorecard, "step_count")),
    })
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
    validate_native_scorecard_scorecard(scorecard)?;
    let redaction_status = required_str(scorecard, "redaction_status")?;
    if !matches!(redaction_status, "redacted" | "not_needed" | "rejected") {
        return Err("scorecard.redaction_status is not allowed".to_string());
    }
    let content_sha256 = required_str(artifact, "content_sha256")?;
    if content_sha256.len() != 64 || !content_sha256.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("native scorecard artifact content_sha256 must be 64 hex chars".to_string());
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
        Value::String(text) if is_sensitive_value(text) => {
            return Err("sensitive trace value is not allowed in scorecard artifact".to_string());
        }
        _ => {}
    }
    Ok(())
}

fn is_sensitive_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("sk-")
        || lower.contains("ghp_")
        || lower.contains("gho_")
        || lower.contains("ghu_")
        || lower.contains("ghs_")
        || lower.contains("ghr_")
        || lower.contains("api_key=")
        || lower.contains("api-key=")
        || lower.contains("auth_token=")
        || lower.contains("auth-token=")
        || lower.contains("password=")
        || lower.contains("password:")
        || value.contains("/home/")
        || value.contains("/Users/")
        || value.contains("\\Users\\")
}

fn is_raw_trace_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    if key == "raw_trace_artifact_id" {
        return false;
    }
    [
        "raw_trace",
        "raw_prompt",
        "raw_output",
        "transcript",
        "conversation",
        "message_history",
        "repository_text",
        "repo_full_text",
        "repo_content",
        "private_path",
        "credential",
        "secret",
        "password",
    ]
    .iter()
    .any(|fragment| key.contains(fragment))
}

fn is_scorecard_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "cancelled" | "blocked" | "error"
    )
}

fn run_status_to_scorecard_status(status: Option<&str>) -> &'static str {
    match status.unwrap_or("").to_ascii_lowercase().as_str() {
        "completed" | "success" | "succeeded" | "passed" | "pass" => "pass",
        "failed" | "failure" | "fail" => "fail",
        "error" | "errored" => "error",
        "blocked" | "cancelled" | "canceled" | "timeout" | "timed_out" => "blocked",
        _ => "blocked",
    }
}

fn operation_kind_for_node(node: &Value) -> &'static str {
    let task_type = node
        .get("task_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let executor_type = node
        .pointer("/result/executor_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        executor_type.as_str(),
        "provider" | "adaptive_provider" | "claude_code_cli" | "codex_cli"
    ) {
        "model_call"
    } else if task_type.contains("tool") || task_type == "command" {
        "tool_call"
    } else {
        "control"
    }
}

fn terminal_reason(run: &Value) -> String {
    run.get("events")
        .and_then(Value::as_array)
        .and_then(|events| {
            events.iter().rev().find_map(|event| {
                let event_type = event.get("event_type").and_then(Value::as_str)?;
                if !event_type.starts_with("workflow_run.") {
                    return None;
                }
                event
                    .pointer("/details/reason")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
        })
        .unwrap_or_else(|| "bounded native summary exported".to_string())
}

fn run_duration_ms(run: &Value) -> Option<i64> {
    let started = run.get("started_at").and_then(Value::as_str)?;
    let completed = run.get("completed_at").and_then(Value::as_str)?;
    let started = chrono::DateTime::parse_from_rfc3339(started).ok()?;
    let completed = chrono::DateTime::parse_from_rfc3339(completed).ok()?;
    Some((completed - started).num_milliseconds().max(0))
}

fn node_duration_ms(node: &Value) -> Option<i64> {
    let started = node.get("started_at").and_then(Value::as_str)?;
    let completed = node.get("completed_at").and_then(Value::as_str)?;
    let started = chrono::DateTime::parse_from_rfc3339(started).ok()?;
    let completed = chrono::DateTime::parse_from_rfc3339(completed).ok()?;
    Some((completed - started).num_milliseconds().max(0))
}

fn positive_i64(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Number(number)) => number.as_i64().unwrap_or(0).max(0),
        _ => 0,
    }
}

fn require_nonnegative_number(value: &Value, key: &str) -> Result<(), String> {
    match value.get(key) {
        Some(Value::Number(number)) if number.as_f64().is_some_and(|number| number >= 0.0) => {
            Ok(())
        }
        _ => Err(format!("missing required non-negative number field: {key}")),
    }
}

fn number_i64(value: &Value, key: &str) -> i64 {
    positive_i64(value.get(key))
}

fn ratio(numerator: i64, denominator: i64) -> f64 {
    let denominator = denominator.max(1) as f64;
    ((numerator as f64 / denominator) * 1_000_000.0).round() / 1_000_000.0
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
