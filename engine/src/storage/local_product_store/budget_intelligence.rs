use chrono::{Duration, SecondsFormat, Utc};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::budget_anomaly::{
    detect_budget_anomaly, BudgetAnomalyObservation, BudgetAnomalyRequest,
};
use crate::budget_forecast::{
    build_budget_forecast, BudgetForecastRequest, BudgetUsageObservation,
};
use crate::budget_manager::{BudgetAnomalyKind, BudgetEvidenceScope};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

pub const NORMALIZED_USAGE_SCHEMA_VERSION: &str = "normalized_usage_observation.v1";
pub const BUDGET_PRODUCER_SCHEMA_VERSION: &str = "budget_intelligence_producer.v1";
const MAX_SOURCE_OBSERVATIONS: usize = 64;
const BUDGET_RECOVERY_STATE_CONFIG_KEY: &str = "budget_intelligence_recovery_state.v1";
const MAX_BUDGET_RECOVERY_PENDING: usize = 128;
const MAX_BUDGET_RECOVERY_BATCH: usize = 32;
const MAX_CONSECUTIVE_RECOVERY_ATTEMPTS: u64 = 3;
type ProductionJobRow = (String, String, String, Option<String>, Option<String>);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetRecoveryState {
    #[serde(default)]
    cursor_run_sequence: i64,
    #[serde(default)]
    pending: Vec<BudgetRecoveryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BudgetRecoveryItem {
    run_sequence: i64,
    run_id: String,
    #[serde(default)]
    attempt_count: u64,
}

#[derive(Debug)]
pub(crate) enum ProductionJobClaim {
    Completed(Value),
    Acquired { lease_token: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedUsageObservation {
    pub schema_version: String,
    pub observation_id: String,
    pub dedupe_key: String,
    pub source_kind: String,
    pub source_id: String,
    pub source_sha256: String,
    pub occurred_at: String,
    pub tenant_id: String,
    pub workspace_id: Option<String>,
    pub run_id: Option<String>,
    pub dispatch_id: Option<String>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub pricing_identity: Option<String>,
    pub pricing_effective_date: Option<String>,
    pub currency: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub context_tokens: Option<i64>,
    pub retry_count: Option<i64>,
    pub latency_ms: Option<i64>,
    pub cost: Option<f64>,
    pub metric_provenance: Value,
    pub completeness: String,
    pub confidence: f64,
    pub record_sha256: String,
}

impl LocalProductStore {
    pub(crate) fn recover_budget_intelligence_for_terminal_runs(
        &self,
        limit: i64,
        actor: &str,
    ) -> Result<Value, String> {
        let bounded_limit = usize::try_from(limit.clamp(1, MAX_BUDGET_RECOVERY_BATCH as i64))
            .map_err(|_| "budget recovery limit is invalid".to_string())?;
        let mut state = self.budget_recovery_state()?;
        let capacity = MAX_BUDGET_RECOVERY_PENDING.saturating_sub(state.pending.len());
        let mut discovered = self.workflow_runs_after_sequence(
            state.cursor_run_sequence,
            capacity.min(MAX_BUDGET_RECOVERY_BATCH),
        )?;
        if discovered.is_empty() && state.cursor_run_sequence > 0 {
            state.cursor_run_sequence = 0;
            discovered =
                self.workflow_runs_after_sequence(0, capacity.min(MAX_BUDGET_RECOVERY_BATCH))?;
        }
        if let Some(last) = discovered.last() {
            state.cursor_run_sequence = last.0;
        }
        let mut known = state
            .pending
            .iter()
            .map(|item| item.run_id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for (run_sequence, run_id, status) in &discovered {
            if matches!(status.as_str(), "completed" | "failed" | "cancelled")
                && known.insert(run_id.clone())
            {
                state.pending.push(BudgetRecoveryItem {
                    run_sequence: *run_sequence,
                    run_id: run_id.clone(),
                    attempt_count: 0,
                });
            }
        }

        let mut attempted = 0usize;
        let mut completed = 0usize;
        let mut failures = 0usize;
        let attempts = bounded_limit.min(state.pending.len());
        for _ in 0..attempts {
            let mut item = state.pending.remove(0);
            attempted += 1;
            match self.produce_budget_intelligence_for_run(&item.run_id, actor) {
                Ok(_) => completed += 1,
                Err(error) => {
                    failures += 1;
                    item.attempt_count = item.attempt_count.saturating_add(1);
                    self.append_audit(
                        actor,
                        "budget_intelligence.recovery_deferred",
                        &item.run_id,
                        &json!({
                            "run_id": item.run_id,
                            "run_sequence": item.run_sequence,
                            "attempt_count": item.attempt_count,
                            "error_sha256": sha256_bytes(error.as_bytes()),
                            "raw_error_stored": false,
                            "retry_on_next_scheduler_tick": true,
                            "queue_slot_released": item.attempt_count >= MAX_CONSECUTIVE_RECOVERY_ATTEMPTS,
                        }),
                    )?;
                    if item.attempt_count < MAX_CONSECUTIVE_RECOVERY_ATTEMPTS {
                        state.pending.push(item);
                    }
                }
            }
        }
        self.set_config_value(
            BUDGET_RECOVERY_STATE_CONFIG_KEY,
            serde_json::to_value(&state).map_err(|error| error.to_string())?,
            actor,
        )?;
        Ok(json!({
            "schema_version": "budget_intelligence_recovery.v1",
            "bounded_limit": bounded_limit,
            "attempted": attempted,
            "completed_or_idempotent": completed,
            "deferred": failures,
            "pending": state.pending.len(),
            "cursor_run_sequence": state.cursor_run_sequence,
        }))
    }

    fn budget_recovery_state(&self) -> Result<BudgetRecoveryState, String> {
        self.config_snapshot()?
            .get(BUDGET_RECOVERY_STATE_CONFIG_KEY)
            .cloned()
            .map(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
            .transpose()
            .map(|value| value.unwrap_or_default())
    }

    fn workflow_runs_after_sequence(
        &self,
        cursor: i64,
        limit: usize,
    ) -> Result<Vec<(i64, String, String)>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit =
            i64::try_from(limit).map_err(|_| "budget recovery limit overflow".to_string())?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut statement = conn
                    .prepare("SELECT run_sequence,run_id,status FROM workflow_runs WHERE run_sequence>?1 ORDER BY run_sequence ASC LIMIT ?2")
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map(params![cursor, limit], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .map_err(|error| error.to_string())?;
                rows
                    .map(|row| row.map_err(|error| error.to_string()))
                    .collect()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                Ok(client
                    .query(
                        "SELECT run_sequence,run_id,status FROM workflow_runs WHERE run_sequence>$1 ORDER BY run_sequence ASC LIMIT $2",
                        &[&cursor, &limit],
                    )
                    .map_err(|error| error.to_string())?
                    .iter()
                    .map(|row| (row.get(0), row.get(1), row.get(2)))
                    .collect())
            }),
        }
    }

    pub fn produce_budget_intelligence_for_run(
        &self,
        run_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        let run = self
            .get_workflow_run(run_id)?
            .ok_or_else(|| "workflow run not found".to_string())?;
        let tenant_id = run
            .get("tenant_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "workflow run has no tenant binding".to_string())?;
        let workspace_id = run
            .get("workspace_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        let dispatch_id = run
            .get("dispatch_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let projected = self.project_usage_sources(
            tenant_id,
            workspace_id.as_deref(),
            run_id,
            dispatch_id.as_deref(),
        )?;
        let observations = self.persist_normalized_usage(&projected, actor)?;
        let input_sha256 = sha256_json(&json!(observations))?;
        let scope_sha256 = sha256_json(&json!({
            "tenant_id": tenant_id,
            "workspace_id": workspace_id,
            "run_id": run_id,
        }))?;
        let job_key = format!("budget-run-{run_id}-{input_sha256}");
        let lease_token = match self.claim_production_job(
            &job_key,
            "budget_intelligence",
            &scope_sha256,
            &input_sha256,
            actor,
        )? {
            ProductionJobClaim::Completed(result) => return Ok(result),
            ProductionJobClaim::Acquired { lease_token } => lease_token,
        };

        let generated = chrono::DateTime::parse_from_rfc3339(&self.now())
            .map_err(|_| "store clock returned invalid RFC3339 timestamp".to_string())?
            .with_timezone(&Utc);
        // Stored event timestamps are second-resolution and evidence windows are
        // end-exclusive. Advance the logical evidence boundary by one second so
        // observations written in the current second are included; generated_at
        // must match that boundary because evidence windows cannot end in its future.
        let end = generated + Duration::seconds(1);
        let start = end - Duration::hours(24);
        let baseline_start = end - Duration::hours(2);
        let current_start = end - Duration::hours(1);
        let timestamp =
            |value: chrono::DateTime<Utc>| value.to_rfc3339_opts(SecondsFormat::Secs, true);
        let generated_at = timestamp(end);
        let common_provider = common_dimension(&observations, |item| item.provider_id.as_deref());
        let common_model = common_dimension(&observations, |item| item.model_id.as_deref());
        let scope = BudgetEvidenceScope {
            run_id: Some(run_id.to_string()),
            workspace_id: workspace_id.clone(),
            provider_id: common_provider,
            model_id: common_model,
        };
        let usage = observations
            .iter()
            .map(normalized_to_forecast)
            .collect::<Vec<_>>();
        let anomaly_usage = observations
            .iter()
            .map(normalized_to_anomaly)
            .collect::<Vec<_>>();
        let forecast_id = format!("forecast-{}", &input_sha256[..32]);
        let forecast = build_budget_forecast(
            &BudgetForecastRequest {
                forecast_id,
                scope: scope.clone(),
                start_inclusive: timestamp(start),
                end_exclusive: timestamp(end),
                generated_at: generated_at.clone(),
                horizon_seconds: 86_400,
                remaining_tokens: None,
                remaining_cost_usd: None,
                required_dimensions: vec![
                    "model_id".to_string(),
                    "provider_id".to_string(),
                    "run_id".to_string(),
                ],
                min_samples: 3,
                max_freshness_seconds: 300,
                max_duplicate_events: 0,
            },
            &usage,
        )?;
        let finding_id = format!("anomaly-{}", &input_sha256[..32]);
        let finding = detect_budget_anomaly(
            &BudgetAnomalyRequest {
                finding_id,
                scope,
                anomaly_kind: BudgetAnomalyKind::CostSpike,
                baseline_start_inclusive: timestamp(baseline_start),
                current_start_inclusive: timestamp(current_start),
                current_end_exclusive: timestamp(end),
                generated_at,
                min_samples_per_window: 3,
                max_freshness_seconds: 300,
                max_duplicate_events: 0,
                required_dimensions: vec![
                    "model_id".to_string(),
                    "provider_id".to_string(),
                    "run_id".to_string(),
                ],
                relative_increase_threshold: 0.5,
                absolute_increase_threshold: 0.01,
                critical_increase_threshold: 2.0,
            },
            &anomaly_usage,
        )?;
        let forecast_artifact = self.record_budget_forecast_evidence(&forecast, actor)?;
        let anomaly_artifact = self.record_budget_anomaly_finding(&finding, actor)?;
        let result = json!({
            "schema_version": BUDGET_PRODUCER_SCHEMA_VERSION,
            "job_key": job_key,
            "scope_sha256": scope_sha256,
            "input_sha256": input_sha256,
            "observation_count": observations.len(),
            "observations": observations.iter().map(observation_evidence).collect::<Vec<_>>(),
            "forecast_artifact_id": forecast_artifact["artifact_id"],
            "forecast_outcome": forecast_artifact.pointer("/evidence/outcome"),
            "anomaly_artifact_id": anomaly_artifact["artifact_id"],
            "anomaly_outcome": anomaly_artifact.pointer("/evidence/outcome"),
            "provider_calls": "disabled",
        });
        self.complete_production_job(&job_key, &input_sha256, &lease_token, &result, actor)?;
        Ok(result)
    }

    pub fn normalized_usage_observations_for_run(
        &self,
        run_id: &str,
        limit: i64,
    ) -> Result<Vec<Value>, String> {
        let limit = limit.clamp(1, MAX_SOURCE_OBSERVATIONS as i64);
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare("SELECT usage_json FROM normalized_usage_observations WHERE run_id=?1 ORDER BY occurred_at, observation_id LIMIT ?2")
                    .map_err(|error| error.to_string())?;
                let rows = stmt
                    .query_map(params![run_id, limit], |row| row.get::<_, String>(0))
                    .map_err(|error| error.to_string())?;
                rows.map(|row| {
                    row.map_err(|error| error.to_string())
                        .and_then(|raw| serde_json::from_str(&raw).map_err(|error| error.to_string()))
                })
                .collect()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query("SELECT usage_json FROM normalized_usage_observations WHERE run_id=$1 ORDER BY occurred_at, observation_id LIMIT $2", &[&run_id, &limit])
                    .map_err(|error| error.to_string())?
                    .iter()
                    .map(|row| serde_json::from_str(&row.get::<_, String>(0)).map_err(|error| error.to_string()))
                    .collect()
            }),
        }
    }

    fn project_usage_sources(
        &self,
        tenant_id: &str,
        workspace_id: Option<&str>,
        run_id: &str,
        dispatch_id: Option<&str>,
    ) -> Result<Vec<NormalizedUsageObservation>, String> {
        let mut candidates = Vec::new();
        let scorecards =
            self.native_scorecard_artifacts_by_run(run_id, MAX_SOURCE_OBSERVATIONS as i64)?;
        let has_run_aggregate_scorecard = !scorecards.is_empty();
        for artifact in &scorecards {
            if let Some(observation) =
                normalize_scorecard_artifact(artifact, tenant_id, workspace_id, run_id)?
            {
                candidates.push(observation);
            }
        }
        if let Some(dispatch_id) = dispatch_id {
            for event in self
                .provider_audit_events_for_dispatch(dispatch_id)?
                .into_iter()
                .filter(|event| {
                    event.get("event_type").and_then(Value::as_str) == Some("response_received")
                })
            {
                candidates.push(normalize_provider_event(
                    &event,
                    tenant_id,
                    workspace_id,
                    run_id,
                )?);
            }
            if let Some(dispatch) = self.get_dispatch(dispatch_id)? {
                candidates.push(normalize_dispatch_history(
                    &dispatch,
                    tenant_id,
                    workspace_id,
                    run_id,
                )?);
            }
        }
        for observation in self
            .adaptive_observations()?
            .into_iter()
            .filter(|observation| observation.run_id == run_id)
        {
            candidates.push(normalize_adaptive_observation(
                &observation,
                tenant_id,
                workspace_id,
                run_id,
            )?);
        }
        // A native scorecard is the bounded aggregate of this run's workflow
        // node executions. Project node events only while that aggregate is
        // unavailable; retaining both would count the same execution twice.
        if !has_run_aggregate_scorecard {
            for event in self.workflow_usage_events(run_id)? {
                candidates.push(normalize_workflow_event(
                    &event,
                    tenant_id,
                    workspace_id,
                    run_id,
                    dispatch_id,
                )?);
            }
        }
        Ok(select_canonical_usage(candidates))
    }

    fn workflow_usage_events(&self, run_id: &str) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn.prepare("SELECT event_id, node_id, event_type, created_at, details_json FROM workflow_run_events WHERE run_id=?1 AND event_type IN ('node.completed','node.failed') ORDER BY event_sequence LIMIT 64").map_err(|error| error.to_string())?;
                let rows = stmt.query_map(params![run_id], |row| {
                    let details: String = row.get(4)?;
                    Ok(json!({"event_id":row.get::<_,String>(0)?,"node_id":row.get::<_,Option<String>>(1)?,"event_type":row.get::<_,String>(2)?,"created_at":row.get::<_,String>(3)?,"details":serde_json::from_str::<Value>(&details).unwrap_or(Value::Null)}))
                }).map_err(|error| error.to_string())?;
                rows.map(|row| row.map_err(|error| error.to_string())).collect()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client.query("SELECT event_id, node_id, event_type, created_at, details_json FROM workflow_run_events WHERE run_id=$1 AND event_type IN ('node.completed','node.failed') ORDER BY event_sequence LIMIT 64", &[&run_id]).map_err(|error| error.to_string())?.iter().map(|row| {
                    let details:String=row.get(4);Ok(json!({"event_id":row.get::<_,String>(0),"node_id":row.get::<_,Option<String>>(1),"event_type":row.get::<_,String>(2),"created_at":row.get::<_,String>(3),"details":serde_json::from_str::<Value>(&details).unwrap_or(Value::Null)}))
                }).collect()
            }),
        }
    }

    fn persist_normalized_usage(
        &self,
        observations: &[NormalizedUsageObservation],
        actor: &str,
    ) -> Result<Vec<NormalizedUsageObservation>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                for observation in observations {
                    sqlite_insert_observation(&tx, observation, actor, &self.now())?;
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(observations.to_vec())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                for observation in observations {
                    pg_insert_observation(&mut tx, observation, actor, &self.now())?;
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(observations.to_vec())
            }),
        }
    }

    pub(crate) fn claim_production_job(
        &self,
        job_key: &str,
        job_kind: &str,
        scope_sha256: &str,
        input_sha256: &str,
        actor: &str,
    ) -> Result<ProductionJobClaim, String> {
        let now = self.now();
        let now_parsed = chrono::DateTime::parse_from_rfc3339(&now)
            .map_err(|_| "store clock returned invalid RFC3339 timestamp".to_string())?
            .with_timezone(&Utc);
        let lease_expires_at =
            (now_parsed + Duration::seconds(60)).to_rfc3339_opts(SecondsFormat::Secs, true);
        let lease_token = uuid::Uuid::new_v4().simple().to_string();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                    .map_err(|error| error.to_string())?;
                let existing: Option<ProductionJobRow> = tx
                    .query_row(
                        "SELECT state,scope_sha256,input_sha256,result_json,lease_expires_at FROM production_jobs WHERE job_key=?1",
                        params![job_key],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                if let Some((state, scope, input, result, existing_expiry)) = existing {
                    if scope != scope_sha256 || input != input_sha256 {
                        return Err("production job binding conflict".to_string());
                    }
                    if state == "completed" {
                        let raw = result
                            .ok_or_else(|| "completed production job has no result".to_string())?;
                        return serde_json::from_str(&raw)
                            .map(ProductionJobClaim::Completed)
                            .map_err(|error| error.to_string());
                    }
                    if state == "running"
                        && existing_expiry.as_deref().is_some_and(|expiry| expiry > now.as_str())
                    {
                        return Err(format!(
                            "production job already leased until {}",
                            existing_expiry.unwrap_or_default()
                        ));
                    }
                    tx.execute(
                        "UPDATE production_jobs SET state='running',lease_owner=?1,lease_token=?2,lease_expires_at=?3,updated_at=?4 WHERE job_key=?5",
                        params![actor, lease_token, lease_expires_at, now, job_key],
                    )
                    .map_err(|error| error.to_string())?;
                } else {
                    tx.execute(
                        "INSERT INTO production_jobs (job_key,job_kind,scope_sha256,input_sha256,state,lease_owner,lease_token,lease_expires_at,created_at,updated_at) VALUES (?1,?2,?3,?4,'running',?5,?6,?7,?8,?8)",
                        params![job_key, job_kind, scope_sha256, input_sha256, actor, lease_token, lease_expires_at, now],
                    )
                    .map_err(|error| error.to_string())?;
                }
                append_audit_locked(&tx, &now, actor, "production_job.claim", job_key, &json!({"job_kind":job_kind,"scope_sha256":scope_sha256,"input_sha256":input_sha256,"lease_token_sha256":sha256_bytes(lease_token.as_bytes())}))?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(ProductionJobClaim::Acquired { lease_token })
            }),
            #[cfg(feature="pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                tx.execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&job_key])
                    .map_err(|error| error.to_string())?;
                let existing = tx.query_opt("SELECT state,scope_sha256,input_sha256,result_json,lease_expires_at FROM production_jobs WHERE job_key=$1 FOR UPDATE", &[&job_key]).map_err(|error| error.to_string())?;
                if let Some(row) = existing {
                    let state: String = row.get(0);
                    let scope: String = row.get(1);
                    let input: String = row.get(2);
                    let result: Option<String> = row.get(3);
                    let existing_expiry: Option<String> = row.get(4);
                    if scope != scope_sha256 || input != input_sha256 {
                        return Err("production job binding conflict".to_string());
                    }
                    if state == "completed" {
                        let raw = result.ok_or_else(|| "completed production job has no result".to_string())?;
                        return serde_json::from_str(&raw).map(ProductionJobClaim::Completed).map_err(|error| error.to_string());
                    }
                    if state == "running" && existing_expiry.as_deref().is_some_and(|expiry| expiry > now.as_str()) {
                        return Err(format!("production job already leased until {}", existing_expiry.unwrap_or_default()));
                    }
                    tx.execute("UPDATE production_jobs SET state='running',lease_owner=$1,lease_token=$2,lease_expires_at=$3,updated_at=$4 WHERE job_key=$5", &[&actor,&lease_token,&lease_expires_at,&now,&job_key]).map_err(|error|error.to_string())?;
                } else {
                    tx.execute("INSERT INTO production_jobs (job_key,job_kind,scope_sha256,input_sha256,state,lease_owner,lease_token,lease_expires_at,created_at,updated_at) VALUES ($1,$2,$3,$4,'running',$5,$6,$7,$8,$8)",&[&job_key,&job_kind,&scope_sha256,&input_sha256,&actor,&lease_token,&lease_expires_at,&now]).map_err(|error|error.to_string())?;
                }
                pg_audit(&mut tx,&now,actor,"production_job.claim",job_key,&json!({"job_kind":job_kind,"scope_sha256":scope_sha256,"input_sha256":input_sha256,"lease_token_sha256":sha256_bytes(lease_token.as_bytes())}))?;
                tx.commit().map_err(|error|error.to_string())?;
                Ok(ProductionJobClaim::Acquired { lease_token })
            }),
        }
    }

    pub(crate) fn complete_production_job(
        &self,
        job_key: &str,
        input_sha256: &str,
        lease_token: &str,
        result: &Value,
        actor: &str,
    ) -> Result<(), String> {
        let now = self.now();
        let result_json = result.to_string();
        match &self.db{DatabaseConnection::Sqlite(_)=>self.with_conn(|conn|{let tx=conn.unchecked_transaction().map_err(|error|error.to_string())?;let changed=tx.execute("UPDATE production_jobs SET state='completed',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,result_json=?1,updated_at=?2 WHERE job_key=?3 AND input_sha256=?4 AND state='running' AND lease_owner=?5 AND lease_token=?6",params![result_json,now,job_key,input_sha256,actor,lease_token]).map_err(|error|error.to_string())?;if changed!=1{return Err("production job completion binding changed".to_string())}append_audit_locked(&tx,&now,actor,"production_job.complete",job_key,&json!({"input_sha256":input_sha256,"lease_token_sha256":sha256_bytes(lease_token.as_bytes()),"result_sha256":sha256_json(result)?}))?;tx.commit().map_err(|error|error.to_string())}),#[cfg(feature="pg")]DatabaseConnection::Pg(_)=>self.with_pg_conn(|client|{let mut tx=client.transaction().map_err(|error|error.to_string())?;let changed=tx.execute("UPDATE production_jobs SET state='completed',lease_owner=NULL,lease_token=NULL,lease_expires_at=NULL,result_json=$1,updated_at=$2 WHERE job_key=$3 AND input_sha256=$4 AND state='running' AND lease_owner=$5 AND lease_token=$6",&[&result_json,&now,&job_key,&input_sha256,&actor,&lease_token]).map_err(|error|error.to_string())?;if changed!=1{return Err("production job completion binding changed".to_string())}pg_audit(&mut tx,&now,actor,"production_job.complete",job_key,&json!({"input_sha256":input_sha256,"lease_token_sha256":sha256_bytes(lease_token.as_bytes()),"result_sha256":sha256_json(result)?}))?;tx.commit().map_err(|error|error.to_string())})}
    }
}

fn normalize_scorecard_artifact(
    artifact: &Value,
    tenant_id: &str,
    workspace_id: Option<&str>,
    run_id: &str,
) -> Result<Option<NormalizedUsageObservation>, String> {
    let Some(scorecard) = artifact.get("scorecard") else {
        return Ok(None);
    };
    let contract = scorecard
        .get("comparison_contract")
        .filter(|value| value.is_object())
        .unwrap_or(&Value::Null);
    let source_id = required(artifact, "artifact_id")?;
    let source_sha256 = required(artifact, "content_sha256")?;
    let provider_id = contract
        .get("provider_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let model_id = contract
        .get("model_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let pricing_identity = contract
        .get("pricing_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let pricing_effective_date = contract
        .get("pricing_effective_date")
        .and_then(Value::as_str)
        .map(str::to_string);
    let input = optional_i64(scorecard, "input_token_total");
    let output = optional_i64(scorecard, "output_token_total");
    let context = optional_i64(scorecard, "context_token_total");
    let cost = optional_f64(scorecard, "estimated_cost_usd");
    let required_complete = provider_id.is_some()
        && model_id.is_some()
        && pricing_identity.is_some()
        && pricing_effective_date.is_some()
        && input.is_some()
        && output.is_some()
        && cost.is_some();
    seal_observation(NormalizedUsageObservation {
        schema_version: NORMALIZED_USAGE_SCHEMA_VERSION.into(),
        observation_id: format!("usage-scorecard-{source_sha256}"),
        dedupe_key: scorecard
            .get("dispatch_id")
            .and_then(Value::as_str)
            .map(|dispatch_id| format!("provider_call:{dispatch_id}"))
            .unwrap_or_else(|| format!("native_scorecard:{source_id}")),
        source_kind: "native_scorecard".into(),
        source_id: source_id.into(),
        source_sha256: source_sha256.into(),
        occurred_at: artifact
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                scorecard
                    .get("generated_at")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            })
            .to_string(),
        tenant_id: tenant_id.into(),
        workspace_id: workspace_id.map(str::to_string),
        run_id: Some(run_id.into()),
        dispatch_id: scorecard
            .get("dispatch_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        provider_id,
        model_id,
        pricing_identity,
        pricing_effective_date,
        currency: cost.map(|_| "USD".to_string()),
        input_tokens: input,
        output_tokens: output,
        cached_tokens: optional_i64(scorecard, "cached_token_total"),
        cache_write_tokens: optional_i64(scorecard, "cache_write_token_total"),
        reasoning_tokens: optional_i64(scorecard, "reasoning_token_total"),
        context_tokens: context,
        retry_count: optional_i64(scorecard, "retry_count"),
        latency_ms: optional_i64(scorecard, "duration_ms"),
        cost,
        metric_provenance: json!({
            "input_tokens": "harness_derived",
            "output_tokens": "harness_derived",
            "context_tokens": if context.is_some() { "harness_derived" } else { "unavailable" },
            "cached_tokens": "unavailable",
            "cache_write_tokens": "unavailable",
            "reasoning_tokens": "unavailable",
            "retry_count": "harness_derived",
            "latency_ms": "harness_derived",
            "cost": if cost.is_some() { "estimated" } else { "unavailable" },
            "pricing": if required_complete { "explicit" } else { "incomplete" },
        }),
        completeness: if required_complete {
            "complete"
        } else {
            "partial"
        }
        .into(),
        confidence: if required_complete { 0.9 } else { 0.6 },
        record_sha256: String::new(),
    })
    .map(Some)
}

fn normalize_provider_event(
    event: &Value,
    tenant_id: &str,
    workspace_id: Option<&str>,
    run_id: &str,
) -> Result<NormalizedUsageObservation, String> {
    let source_id = required(event, "event_id")?;
    let source_sha256 = sha256_json(event)?;
    let input = optional_i64(event, "input_token_count");
    let output = optional_i64(event, "output_token_count");
    let cost = if event.get("currency").and_then(Value::as_str) == Some("USD") {
        optional_f64(event, "cost")
    } else {
        None
    };
    seal_observation(NormalizedUsageObservation {
        schema_version: NORMALIZED_USAGE_SCHEMA_VERSION.into(),
        observation_id: format!("usage-{source_sha256}"),
        dedupe_key: event
            .get("dispatch_id")
            .and_then(Value::as_str)
            .map(|dispatch_id| format!("provider_call:{dispatch_id}"))
            .unwrap_or_else(|| format!("provider:{source_id}")),
        source_kind: "provider_audit".into(),
        source_id: source_id.into(),
        source_sha256,
        occurred_at: required(event, "created_at")?.into(),
        tenant_id: tenant_id.into(),
        workspace_id: workspace_id.map(str::to_string),
        run_id: Some(run_id.into()),
        dispatch_id: event
            .get("dispatch_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        provider_id: event
            .get("provider_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        model_id: None,
        pricing_identity: None,
        pricing_effective_date: None,
        currency: event
            .get("currency")
            .and_then(Value::as_str)
            .map(str::to_string),
        input_tokens: input,
        output_tokens: output,
        cached_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
        context_tokens: None,
        retry_count: None,
        latency_ms: optional_i64(event, "latency_ms"),
        cost,
        metric_provenance: json!({
            "input_tokens": if input.is_some() { "provider_reported" } else { "unavailable" },
            "output_tokens": if output.is_some() { "provider_reported" } else { "unavailable" },
            "cost": if cost.is_some() { "provider_reported" } else { "unavailable" },
            "latency_ms": if event.get("latency_ms").and_then(Value::as_i64).is_some() { "provider_reported" } else { "unavailable" },
            "cached_tokens":"unavailable",
            "cache_write_tokens":"unavailable",
            "reasoning_tokens":"unavailable",
            "context_tokens":"unavailable",
            "retry_count":"unavailable"
        }),
        // Provider audit currently has no model or effective pricing identity.
        // Preserve reported values, but never advertise this record as a
        // complete supported-cost observation.
        completeness: "partial".into(),
        confidence: if input.is_some() && output.is_some() {
            0.85
        } else {
            0.7
        },
        record_sha256: String::new(),
    })
}

fn normalize_workflow_event(
    event: &Value,
    tenant_id: &str,
    workspace_id: Option<&str>,
    run_id: &str,
    dispatch_id: Option<&str>,
) -> Result<NormalizedUsageObservation, String> {
    let source_id = required(event, "event_id")?;
    let source_sha256 = sha256_json(event)?;
    let result = event.pointer("/details/result").unwrap_or(&Value::Null);
    let input = optional_i64(result, "input_tokens");
    let output = optional_i64(result, "output_tokens");
    let cost = optional_f64(result, "estimated_cost");
    let attempt = event.pointer("/details/attempt").and_then(Value::as_i64);
    seal_observation(NormalizedUsageObservation {
        schema_version: NORMALIZED_USAGE_SCHEMA_VERSION.into(),
        observation_id: format!("usage-{source_sha256}"),
        dedupe_key: format!("workflow:{source_id}"),
        source_kind: "workflow_node_execution".into(),
        source_id: source_id.into(),
        source_sha256,
        occurred_at: required(event, "created_at")?.into(),
        tenant_id: tenant_id.into(),
        workspace_id: workspace_id.map(str::to_string),
        run_id: Some(run_id.into()),
        dispatch_id: dispatch_id.map(str::to_string),
        provider_id: None,
        model_id: None,
        pricing_identity: None,
        pricing_effective_date: None,
        currency: cost.map(|_| "USD".into()),
        input_tokens: input,
        output_tokens: output,
        cached_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
        context_tokens: None,
        retry_count: attempt.map(|value| value.saturating_sub(1)),
        latency_ms: optional_i64(result, "latency_ms"),
        cost,
        metric_provenance: json!({
            "input_tokens":if input.is_some(){"harness_derived"}else{"unavailable"},
            "output_tokens":if output.is_some(){"harness_derived"}else{"unavailable"},
            "cost":if cost.is_some(){"estimated"}else{"unavailable"},
            "latency_ms":if optional_i64(result, "latency_ms").is_some(){"harness_derived"}else{"unavailable"},
            "retry_count":if attempt.is_some(){"harness_derived"}else{"unavailable"},
            "cached_tokens":"unavailable",
            "cache_write_tokens":"unavailable",
            "reasoning_tokens":"unavailable",
            "context_tokens":"unavailable"
        }),
        completeness: "partial".into(),
        confidence: 0.6,
        record_sha256: String::new(),
    })
}

fn normalize_dispatch_history(
    dispatch: &Value,
    tenant_id: &str,
    workspace_id: Option<&str>,
    run_id: &str,
) -> Result<NormalizedUsageObservation, String> {
    let dispatch_id = required(dispatch, "dispatch_id")?;
    let source_id = dispatch
        .get("history_id")
        .and_then(Value::as_i64)
        .map(|value| value.to_string())
        .ok_or_else(|| "dispatch history source has no history_id".to_string())?;
    let source_sha256 = sha256_json(dispatch)?;
    let bundle = dispatch.get("bundle").unwrap_or(&Value::Null);
    let execution = bundle.get("execution_result").unwrap_or(&Value::Null);
    let provider_id = execution
        .get("provider_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let model_id = execution
        .get("model_id")
        .or_else(|| execution.get("model"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let pricing_identity = execution
        .get("pricing_identity")
        .or_else(|| execution.get("pricing_id"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let pricing_effective_date = execution
        .get("pricing_effective_date")
        .and_then(Value::as_str)
        .map(str::to_string);
    let input = optional_i64(dispatch, "input_tokens");
    let output = optional_i64(dispatch, "output_tokens");
    let latency = optional_i64(dispatch, "latency_ms");
    let cost = optional_f64(dispatch, "estimated_cost_usd");
    let executor_type = dispatch
        .get("executor_type")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let complete = provider_id.is_some()
        && model_id.is_some()
        && pricing_identity.is_some()
        && pricing_effective_date.is_some()
        && input.is_some()
        && output.is_some()
        && cost.is_some();
    seal_observation(NormalizedUsageObservation {
        schema_version: NORMALIZED_USAGE_SCHEMA_VERSION.into(),
        observation_id: format!("usage-dispatch-{source_sha256}"),
        dedupe_key: format!("provider_call:{dispatch_id}"),
        source_kind: if executor_type.ends_with("_cli") {
            "cli_execution"
        } else {
            "dispatch_history"
        }
        .into(),
        source_id,
        source_sha256,
        occurred_at: required(dispatch, "created_at")?.into(),
        tenant_id: tenant_id.into(),
        workspace_id: workspace_id.map(str::to_string),
        run_id: Some(run_id.into()),
        dispatch_id: Some(dispatch_id.into()),
        provider_id,
        model_id,
        pricing_identity,
        pricing_effective_date,
        currency: cost.map(|_| "USD".to_string()),
        input_tokens: input,
        output_tokens: output,
        cached_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
        context_tokens: None,
        retry_count: None,
        latency_ms: latency,
        cost,
        metric_provenance: json!({
            "input_tokens": if input.is_some() { "harness_derived" } else { "unavailable" },
            "output_tokens": if output.is_some() { "harness_derived" } else { "unavailable" },
            "cost": if cost.is_some() { "estimated" } else { "unavailable" },
            "latency_ms": if latency.is_some() { "harness_derived" } else { "unavailable" },
            "cached_tokens":"unavailable","cache_write_tokens":"unavailable",
            "reasoning_tokens":"unavailable","context_tokens":"unavailable","retry_count":"unavailable",
            "pricing": if complete { "explicit" } else { "incomplete" }
        }),
        completeness: if complete { "complete" } else { "partial" }.into(),
        confidence: if complete { 0.8 } else { 0.55 },
        record_sha256: String::new(),
    })
}

fn normalize_adaptive_observation(
    observation: &super::AdaptiveObservationSummary,
    tenant_id: &str,
    workspace_id: Option<&str>,
    run_id: &str,
) -> Result<NormalizedUsageObservation, String> {
    let source_value = serde_json::to_value(observation).map_err(|error| error.to_string())?;
    let source_sha256 = sha256_json(&source_value)?;
    let input = i64::try_from(observation.input_tokens).ok();
    let output = i64::try_from(observation.output_tokens).ok();
    let latency = i64::try_from(observation.latency_ms).ok();
    seal_observation(NormalizedUsageObservation {
        schema_version: NORMALIZED_USAGE_SCHEMA_VERSION.into(),
        observation_id: format!("usage-adaptive-{source_sha256}"),
        dedupe_key: if observation.request_id.is_empty() {
            format!("adaptive:{}", observation.observation_id)
        } else {
            format!("provider_call:{}", observation.request_id)
        },
        source_kind: "adaptive_observation".into(),
        source_id: observation.observation_id.clone(),
        source_sha256,
        occurred_at: observation.created_at.clone(),
        tenant_id: tenant_id.into(),
        workspace_id: workspace_id.map(str::to_string),
        run_id: Some(run_id.into()),
        dispatch_id: None,
        provider_id: None,
        model_id: None,
        pricing_identity: None,
        pricing_effective_date: None,
        currency: Some("USD".into()),
        input_tokens: input,
        output_tokens: output,
        cached_tokens: None,
        cache_write_tokens: None,
        reasoning_tokens: None,
        context_tokens: None,
        retry_count: None,
        latency_ms: latency,
        cost: Some(observation.cost_usd),
        metric_provenance: json!({
            "input_tokens":"harness_derived","output_tokens":"harness_derived",
            "cost":"harness_derived","latency_ms":"harness_derived",
            "cached_tokens":"unavailable","cache_write_tokens":"unavailable",
            "reasoning_tokens":"unavailable","context_tokens":"unavailable","retry_count":"unavailable",
            "pricing":"unavailable"
        }),
        completeness: "partial".into(),
        confidence: 0.65,
        record_sha256: String::new(),
    })
}

fn select_canonical_usage(
    candidates: Vec<NormalizedUsageObservation>,
) -> Vec<NormalizedUsageObservation> {
    let mut selected = std::collections::BTreeMap::<String, NormalizedUsageObservation>::new();
    for candidate in candidates {
        let replace = selected
            .get(&candidate.dedupe_key)
            .is_none_or(|current| usage_priority(&candidate) > usage_priority(current));
        if replace {
            selected.insert(candidate.dedupe_key.clone(), candidate);
        }
    }
    let mut values = selected.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.occurred_at
            .cmp(&right.occurred_at)
            .then_with(|| left.observation_id.cmp(&right.observation_id))
    });
    values.truncate(MAX_SOURCE_OBSERVATIONS);
    values
}

fn usage_priority(observation: &NormalizedUsageObservation) -> (u8, u8) {
    let source = match observation.source_kind.as_str() {
        "provider_audit" => 5,
        "native_scorecard" => 4,
        "adaptive_observation" => 3,
        "cli_execution" | "dispatch_history" => 2,
        "workflow_node_execution" => 1,
        _ => 0,
    };
    (u8::from(observation.completeness == "complete"), source)
}

fn seal_observation(
    mut observation: NormalizedUsageObservation,
) -> Result<NormalizedUsageObservation, String> {
    let mut value = serde_json::to_value(&observation).map_err(|error| error.to_string())?;
    value.as_object_mut().unwrap().remove("record_sha256");
    observation.record_sha256 = sha256_json(&value)?;
    Ok(observation)
}
fn observation_evidence(observation: &NormalizedUsageObservation) -> Value {
    json!({"observation_id":observation.observation_id,"source_kind":observation.source_kind,"source_id":observation.source_id,"source_sha256":observation.source_sha256,"record_sha256":observation.record_sha256,"completeness":observation.completeness,"confidence":observation.confidence,"metric_provenance":observation.metric_provenance})
}
fn normalized_to_forecast(value: &NormalizedUsageObservation) -> BudgetUsageObservation {
    let pricing_complete = value.pricing_identity.is_some()
        && value.pricing_effective_date.is_some()
        && value.provider_id.is_some()
        && value.model_id.is_some()
        && value.completeness == "complete";
    BudgetUsageObservation {
        evidence_type: "normalized_usage_observation".into(),
        evidence_id: value.observation_id.clone(),
        content_sha256: Some(value.record_sha256.clone()),
        occurred_at: value.occurred_at.clone(),
        run_id: value.run_id.clone(),
        workspace_id: value.workspace_id.clone(),
        provider_id: value.provider_id.clone(),
        model_id: value.model_id.clone(),
        input_tokens: value.input_tokens,
        output_tokens: value.output_tokens,
        total_tokens: value
            .input_tokens
            .zip(value.output_tokens)
            .and_then(|(left, right)| left.checked_add(right)),
        cost_usd: if value.currency.as_deref() == Some("USD") && pricing_complete {
            value.cost
        } else {
            None
        },
    }
}

fn common_dimension<F>(observations: &[NormalizedUsageObservation], select: F) -> Option<String>
where
    F: Fn(&NormalizedUsageObservation) -> Option<&str>,
{
    let first = observations.first().and_then(&select)?;
    observations
        .iter()
        .all(|item| select(item) == Some(first))
        .then(|| first.to_string())
}
fn normalized_to_anomaly(value: &NormalizedUsageObservation) -> BudgetAnomalyObservation {
    let forecast = normalized_to_forecast(value);
    BudgetAnomalyObservation {
        evidence_type: forecast.evidence_type,
        evidence_id: forecast.evidence_id,
        content_sha256: forecast.content_sha256,
        occurred_at: forecast.occurred_at,
        run_id: forecast.run_id,
        workspace_id: forecast.workspace_id,
        provider_id: forecast.provider_id,
        model_id: forecast.model_id,
        total_tokens: forecast.total_tokens,
        cost_usd: forecast.cost_usd,
        retry_count: value.retry_count,
        latency_ms: value.latency_ms,
        context_bytes: value
            .context_tokens
            .and_then(|tokens| tokens.checked_mul(4)),
    }
}

fn sqlite_insert_observation(
    conn: &rusqlite::Connection,
    observation: &NormalizedUsageObservation,
    actor: &str,
    now: &str,
) -> Result<(), String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT record_sha256 FROM normalized_usage_observations WHERE dedupe_key=?1",
            params![observation.dedupe_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(existing) = existing {
        if existing == observation.record_sha256 {
            return Ok(());
        }
        return Err("normalized usage dedupe binding conflict".to_string());
    }
    let usage_json = serde_json::to_string(observation).map_err(|error| error.to_string())?;
    conn.execute("INSERT INTO normalized_usage_observations (observation_id,dedupe_key,source_kind,source_id,source_sha256,occurred_at,tenant_id,workspace_id,run_id,dispatch_id,provider_id,model_id,pricing_identity,pricing_effective_date,currency,provenance_json,usage_json,completeness,confidence,record_sha256,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",params![observation.observation_id,observation.dedupe_key,observation.source_kind,observation.source_id,observation.source_sha256,observation.occurred_at,observation.tenant_id,observation.workspace_id,observation.run_id,observation.dispatch_id,observation.provider_id,observation.model_id,observation.pricing_identity,observation.pricing_effective_date,observation.currency,observation.metric_provenance.to_string(),usage_json,observation.completeness,observation.confidence,observation.record_sha256,now]).map_err(|error|error.to_string())?;
    append_audit_locked(
        conn,
        now,
        actor,
        "normalized_usage.record",
        &format!("normalized-usage/{}", observation.observation_id),
        &observation_evidence(observation),
    )
    .map(|_| ())
}

#[cfg(feature = "pg")]
fn pg_insert_observation(
    tx: &mut postgres::Transaction<'_>,
    observation: &NormalizedUsageObservation,
    actor: &str,
    now: &str,
) -> Result<(), String> {
    let existing=tx.query_opt("SELECT record_sha256 FROM normalized_usage_observations WHERE dedupe_key=$1 FOR UPDATE",&[&observation.dedupe_key]).map_err(|error|error.to_string())?;
    if let Some(row) = existing {
        let existing: String = row.get(0);
        if existing == observation.record_sha256 {
            return Ok(());
        }
        return Err("normalized usage dedupe binding conflict".to_string());
    }
    let usage_json = serde_json::to_string(observation).map_err(|error| error.to_string())?;
    tx.execute("INSERT INTO normalized_usage_observations (observation_id,dedupe_key,source_kind,source_id,source_sha256,occurred_at,tenant_id,workspace_id,run_id,dispatch_id,provider_id,model_id,pricing_identity,pricing_effective_date,currency,provenance_json,usage_json,completeness,confidence,record_sha256,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)",&[&observation.observation_id,&observation.dedupe_key,&observation.source_kind,&observation.source_id,&observation.source_sha256,&observation.occurred_at,&observation.tenant_id,&observation.workspace_id,&observation.run_id,&observation.dispatch_id,&observation.provider_id,&observation.model_id,&observation.pricing_identity,&observation.pricing_effective_date,&observation.currency,&observation.metric_provenance.to_string(),&usage_json,&observation.completeness,&observation.confidence,&observation.record_sha256,&now]).map_err(|error|error.to_string())?;
    pg_audit(
        tx,
        now,
        actor,
        "normalized_usage.record",
        &format!("normalized-usage/{}", observation.observation_id),
        &observation_evidence(observation),
    )
}

#[cfg(feature = "pg")]
fn pg_audit(
    tx: &mut postgres::Transaction<'_>,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    tx.execute("INSERT INTO audit_log (created_at,actor,action,resource,details_json) VALUES ($1,$2,$3,$4,$5)",&[&now,&actor,&action,&resource,&details.to_string()]).map_err(|error|error.to_string())?;
    Ok(())
}
fn required<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("usage source missing {field}"))
}
fn optional_i64(value: &Value, field: &str) -> Option<i64> {
    value.get(field).and_then(Value::as_i64)
}
fn optional_f64(value: &Value, field: &str) -> Option<f64> {
    value
        .get(field)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
}
fn sha256_json(value: &Value) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|error| error.to_string())
}
fn sha256_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    #[test]
    fn production_job_lease_is_owner_bound_and_idempotent() {
        let dir = TempDir::new().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("budget.db"), || {
            "2026-07-14T00:00:00Z".to_string()
        })
        .unwrap();
        let hash = "a".repeat(64);
        let lease_token = match store
            .claim_production_job("job-1", "budget", &hash, &hash, "owner-a")
            .unwrap()
        {
            ProductionJobClaim::Acquired { lease_token } => lease_token,
            ProductionJobClaim::Completed(_) => panic!("unexpected completed job"),
        };
        let conflict = store
            .claim_production_job("job-1", "budget", &hash, &hash, "owner-b")
            .unwrap_err();
        assert!(conflict.contains("already leased"));
        assert!(store
            .complete_production_job("job-1", &hash, &lease_token, &json!({"ok":true}), "owner-b",)
            .unwrap_err()
            .contains("binding changed"));
        store
            .complete_production_job("job-1", &hash, &lease_token, &json!({"ok":true}), "owner-a")
            .unwrap();
        match store
            .claim_production_job("job-1", "budget", &hash, &hash, "owner-b")
            .unwrap()
        {
            ProductionJobClaim::Completed(result) => assert_eq!(result, json!({"ok":true})),
            ProductionJobClaim::Acquired { .. } => panic!("completed job was reclaimed"),
        }
    }

    #[test]
    fn production_job_fencing_rejects_stale_same_owner_completion() {
        let dir = TempDir::new().unwrap();
        let clock = Arc::new(Mutex::new("2026-07-14T00:00:00Z".to_string()));
        let clock_reader = Arc::clone(&clock);
        let store = LocalProductStore::new_with_clock(dir.path().join("fencing.db"), move || {
            clock_reader.lock().unwrap().clone()
        })
        .unwrap();
        let hash = "b".repeat(64);
        let first = match store
            .claim_production_job("job-fence", "budget", &hash, &hash, "scheduler")
            .unwrap()
        {
            ProductionJobClaim::Acquired { lease_token } => lease_token,
            ProductionJobClaim::Completed(_) => panic!("unexpected completed job"),
        };
        *clock.lock().unwrap() = "2026-07-14T00:01:01Z".to_string();
        let second = match store
            .claim_production_job("job-fence", "budget", &hash, &hash, "scheduler")
            .unwrap()
        {
            ProductionJobClaim::Acquired { lease_token } => lease_token,
            ProductionJobClaim::Completed(_) => panic!("unexpected completed job"),
        };
        assert_ne!(first, second);
        assert!(store
            .complete_production_job(
                "job-fence",
                &hash,
                &first,
                &json!({"worker":"stale"}),
                "scheduler",
            )
            .unwrap_err()
            .contains("binding changed"));
        store
            .complete_production_job(
                "job-fence",
                &hash,
                &second,
                &json!({"worker":"current"}),
                "scheduler",
            )
            .unwrap();
    }

    #[test]
    fn scorecard_cost_requires_complete_pricing_identity() {
        let scorecard = json!({
            "artifact_id":"scorecard-1",
            "content_sha256":"b".repeat(64),
            "created_at":"2026-07-14T00:00:00Z",
            "scorecard":{
                "adapter_run_id":"run-1",
                "input_token_total":100,
                "output_token_total":20,
                "context_token_total":80,
                "retry_count":0,
                "duration_ms":10,
                "estimated_cost_usd":0.01,
                "comparison_contract":{
                    "provider_id":"fixture-provider",
                    "model_id":"fixture-model",
                    "pricing_id":"fixture-pricing-v1",
                    "pricing_effective_date":"2026-07-01"
                }
            }
        });
        let complete = normalize_scorecard_artifact(&scorecard, "local", Some("ws"), "run-1")
            .unwrap()
            .unwrap();
        assert_eq!(complete.completeness, "complete");
        assert_eq!(normalized_to_forecast(&complete).cost_usd, Some(0.01));

        let mut incomplete_artifact = scorecard;
        incomplete_artifact["scorecard"]["comparison_contract"]
            .as_object_mut()
            .unwrap()
            .remove("pricing_effective_date");
        let incomplete =
            normalize_scorecard_artifact(&incomplete_artifact, "local", Some("ws"), "run-1")
                .unwrap()
                .unwrap();
        assert_eq!(incomplete.completeness, "partial");
        assert_eq!(normalized_to_forecast(&incomplete).cost_usd, None);
        assert_eq!(incomplete.cost, Some(0.01));
    }

    #[test]
    fn provider_metrics_are_unavailable_when_not_reported() {
        let observation = normalize_provider_event(
            &json!({
                "event_id":"provider-event-1","dispatch_id":"dispatch-1",
                "provider_id":"provider-a","event_type":"response_received",
                "input_token_count":12,"output_token_count":null,"cost":null,
                "currency":"USD","latency_ms":null,"created_at":"2026-07-14T00:00:00Z"
            }),
            "local",
            Some("workspace-a"),
            "run-1",
        )
        .unwrap();
        assert_eq!(observation.completeness, "partial");
        assert_eq!(
            observation.metric_provenance["input_tokens"],
            "provider_reported"
        );
        assert_eq!(
            observation.metric_provenance["output_tokens"],
            "unavailable"
        );
        assert_eq!(observation.metric_provenance["cost"], "unavailable");
        assert_eq!(observation.metric_provenance["latency_ms"], "unavailable");
    }

    #[test]
    fn canonical_usage_deduplicates_one_provider_call_across_sources() {
        let provider = normalize_provider_event(
            &json!({
                "event_id":"provider-event-1","dispatch_id":"dispatch-1",
                "provider_id":"provider-a","event_type":"response_received",
                "input_token_count":12,"output_token_count":3,"cost":0.01,
                "currency":"USD","latency_ms":10,"created_at":"2026-07-14T00:00:00Z"
            }),
            "local",
            Some("workspace-a"),
            "run-1",
        )
        .unwrap();
        let dispatch = normalize_dispatch_history(
            &json!({
                "history_id":1,"dispatch_id":"dispatch-1","created_at":"2026-07-14T00:00:00Z",
                "input_tokens":12,"output_tokens":3,"estimated_cost_usd":0.01,
                "executor_type":"provider","latency_ms":10,"bundle":{"execution_result":{}}
            }),
            "local",
            Some("workspace-a"),
            "run-1",
        )
        .unwrap();
        let selected = select_canonical_usage(vec![dispatch, provider]);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].source_kind, "provider_audit");
    }

    #[test]
    fn recovery_cursor_advances_beyond_the_first_thirty_two_runs() {
        let dir = TempDir::new().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("recovery.db"), || {
            "2026-07-14T00:00:00Z".to_string()
        })
        .unwrap();
        store
            .with_conn(|conn| {
                for sequence in 1..=40_i64 {
                    conn.execute(
                        "INSERT INTO workflow_runs (run_sequence,run_id,created_at,updated_at,status,workflow_id,boundaries_json,run_json) VALUES (?1,?2,?3,?3,'completed',?4,'{}','{}')",
                        params![sequence, format!("recovery-run-{sequence}"), "2026-07-14T00:00:00Z", format!("workflow-{sequence}")],
                    )
                    .map_err(|error| error.to_string())?;
                }
                Ok(())
            })
            .unwrap();
        let first = store
            .recover_budget_intelligence_for_terminal_runs(32, "scheduler")
            .unwrap();
        assert_eq!(first["cursor_run_sequence"], 32);
        let second = store
            .recover_budget_intelligence_for_terminal_runs(32, "scheduler")
            .unwrap();
        assert_eq!(second["cursor_run_sequence"], 40);
    }
}
