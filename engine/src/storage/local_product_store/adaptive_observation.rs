use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::feedback::policy_snapshot::stable_hash;
use crate::feedback::{
    ContextualBanditObservation, ContextualPolicyRequest, ObjectiveProfile,
    CONTEXTUAL_POLICY_SCHEMA_VERSION,
};
use crate::provider::redaction::contains_sensitive_patterns;

pub const ADAPTIVE_OBSERVATION_SCHEMA_VERSION: &str = "adaptive_observation.v1";

const OBSERVATIONS_KEY: &str = "adaptive_fusion_observations";
const MAX_OBSERVATIONS: usize = 10_000;
const MAX_ID_BYTES: usize = 160;
const MAX_COST_USD: f64 = 1_000_000.0;
const MAX_LATENCY_MS: u64 = 86_400_000;
const MAX_TOKEN_COUNT: u64 = 1_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveObservationInput {
    pub schema_version: String,
    pub run_id: String,
    pub request_id: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub risk_level: String,
    pub candidate_id: String,
    pub candidate_hash: String,
    pub policy_hash: Option<String>,
    pub candidate_kind: String,
    pub success: bool,
    pub quality_score: f64,
    pub quality_score_source: String,
    pub tool_success_score: f64,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveObservationSummary {
    pub schema_version: String,
    pub observation_id: String,
    pub sequence: u64,
    pub created_at: String,
    pub run_id: String,
    pub request_id: String,
    pub task_class: String,
    pub objective: ObjectiveProfile,
    pub risk_level: String,
    pub candidate_id: String,
    pub candidate_hash: String,
    pub policy_hash: Option<String>,
    pub candidate_kind: String,
    pub success: bool,
    pub quality_score: f64,
    pub quality_score_source: String,
    pub tool_success_score: f64,
    pub cost_usd: f64,
    pub latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl AdaptiveObservationSummary {
    fn new(input: &AdaptiveObservationInput, sequence: u64, created_at: String) -> Self {
        let observation_hash = stable_hash(&json!({
            "run_id": input.run_id,
            "candidate_id": input.candidate_id,
            "candidate_hash": input.candidate_hash,
        }));
        Self {
            schema_version: ADAPTIVE_OBSERVATION_SCHEMA_VERSION.to_string(),
            observation_id: format!("adaptive-observation-{}", &observation_hash[..24]),
            sequence,
            created_at,
            run_id: input.run_id.clone(),
            request_id: input.request_id.clone(),
            task_class: input.task_class.clone(),
            objective: input.objective,
            risk_level: input.risk_level.clone(),
            candidate_id: input.candidate_id.clone(),
            candidate_hash: input.candidate_hash.clone(),
            policy_hash: input.policy_hash.clone(),
            candidate_kind: input.candidate_kind.clone(),
            success: input.success,
            quality_score: input.quality_score,
            quality_score_source: input.quality_score_source.clone(),
            tool_success_score: input.tool_success_score,
            cost_usd: input.cost_usd,
            latency_ms: input.latency_ms,
            input_tokens: input.input_tokens,
            output_tokens: input.output_tokens,
        }
    }

    fn matches_input(&self, input: &AdaptiveObservationInput) -> bool {
        self.schema_version == input.schema_version
            && self.run_id == input.run_id
            && self.request_id == input.request_id
            && self.task_class == input.task_class
            && self.objective == input.objective
            && self.risk_level == input.risk_level
            && self.candidate_id == input.candidate_id
            && self.candidate_hash == input.candidate_hash
            && self.policy_hash == input.policy_hash
            && self.candidate_kind == input.candidate_kind
            && self.success == input.success
            && self.quality_score == input.quality_score
            && self.quality_score_source == input.quality_score_source
            && self.tool_success_score == input.tool_success_score
            && self.cost_usd == input.cost_usd
            && self.latency_ms == input.latency_ms
            && self.input_tokens == input.input_tokens
            && self.output_tokens == input.output_tokens
    }

    fn is_valid(&self) -> bool {
        let input = self.as_input();
        self.sequence > 0
            && !self.created_at.is_empty()
            && self.created_at.len() <= 64
            && !contains_sensitive_patterns(
                &serde_json::to_string(self).unwrap_or_else(|_| "invalid".to_string()),
            )
            && validate_input(&input).is_ok()
            && self.observation_id
                == Self::new(&input, self.sequence, self.created_at.clone()).observation_id
    }

    fn to_contextual(&self) -> ContextualBanditObservation {
        ContextualBanditObservation {
            schema_version: CONTEXTUAL_POLICY_SCHEMA_VERSION.to_string(),
            observation_id: self.observation_id.clone(),
            run_id: self.run_id.clone(),
            task_class: self.task_class.clone(),
            objective: self.objective,
            candidate_id: self.candidate_id.clone(),
            sequence: self.sequence,
            success: self.success,
            quality_score: self.quality_score,
            tool_success_score: self.tool_success_score,
            cost_efficiency_score: 1.0 / (1.0 + self.cost_usd),
            latency_efficiency_score: 1.0 / (1.0 + self.latency_ms as f64 / 1_000.0),
            human_score: None,
        }
    }

    fn as_input(&self) -> AdaptiveObservationInput {
        AdaptiveObservationInput {
            schema_version: self.schema_version.clone(),
            run_id: self.run_id.clone(),
            request_id: self.request_id.clone(),
            task_class: self.task_class.clone(),
            objective: self.objective,
            risk_level: self.risk_level.clone(),
            candidate_id: self.candidate_id.clone(),
            candidate_hash: self.candidate_hash.clone(),
            policy_hash: self.policy_hash.clone(),
            candidate_kind: self.candidate_kind.clone(),
            success: self.success,
            quality_score: self.quality_score,
            quality_score_source: self.quality_score_source.clone(),
            tool_success_score: self.tool_success_score,
            cost_usd: self.cost_usd,
            latency_ms: self.latency_ms,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
        }
    }
}

impl LocalProductStore {
    pub fn adaptive_observations(&self) -> Result<Vec<AdaptiveObservationSummary>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(observations_sqlite),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let raw = client
                    .query_opt(
                        "SELECT value_json FROM local_config WHERE key = $1",
                        &[&OBSERVATIONS_KEY],
                    )
                    .map_err(|error| error.to_string())?
                    .map(|row| row.get::<_, String>(0));
                Ok(observations_from_raw(raw.as_deref()))
            }),
        }
    }

    pub fn record_adaptive_observation(
        &self,
        input: &AdaptiveObservationInput,
        actor: &str,
    ) -> Result<AdaptiveObservationSummary, String> {
        validate_input(input)?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn
                    .unchecked_transaction()
                    .map_err(|error| error.to_string())?;
                let mut observations = observations_sqlite(&tx)?;
                let result = record_in_state(&mut observations, input, self.now())?;
                write_observations_sqlite(&tx, &observations, actor, &self.now())?;
                append_audit_locked(
                    &tx,
                    &self.now(),
                    actor,
                    "adaptive_observation.recorded",
                    &result.observation_id,
                    &json!({
                        "observation_id": result.observation_id,
                        "run_id": result.run_id,
                        "candidate_id": result.candidate_id,
                        "success": result.success,
                    }),
                )?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(result)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let now = self.now();
                tx.execute(
                    "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT DO NOTHING",
                    &[&OBSERVATIONS_KEY, &"[]", &now, &actor],
                )
                .map_err(|error| error.to_string())?;
                let raw = tx
                    .query_one(
                        "SELECT value_json FROM local_config WHERE key = $1 FOR UPDATE",
                        &[&OBSERVATIONS_KEY],
                    )
                    .map_err(|error| error.to_string())?
                    .get::<_, String>(0);
                let mut observations = observations_from_raw(Some(&raw));
                let result = record_in_state(&mut observations, input, self.now())?;
                let serialized =
                    serde_json::to_string(&observations).map_err(|error| error.to_string())?;
                tx.execute(
                    "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                     VALUES ($1, $2, $3, $4)
                     ON CONFLICT(key) DO UPDATE SET
                        value_json = EXCLUDED.value_json,
                        updated_at = EXCLUDED.updated_at,
                        updated_by = EXCLUDED.updated_by",
                    &[&OBSERVATIONS_KEY, &serialized, &now, &actor],
                )
                .map_err(|error| error.to_string())?;
                let details = json!({
                    "observation_id": result.observation_id,
                    "run_id": result.run_id,
                    "candidate_id": result.candidate_id,
                    "success": result.success,
                })
                .to_string();
                tx.execute(
                    "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                     VALUES ($1, $2, $3, $4, $5)",
                    &[
                        &now,
                        &actor,
                        &"adaptive_observation.recorded",
                        &result.observation_id,
                        &details,
                    ],
                )
                .map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(result)
            }),
        }
    }

    pub fn adaptive_contextual_observations(
        &self,
        request: &ContextualPolicyRequest,
        candidate_ids: &[String],
    ) -> Result<Vec<ContextualBanditObservation>, String> {
        let allowed = candidate_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let mut observations = self
            .adaptive_observations()?
            .into_iter()
            .filter(|observation| {
                observation.task_class == request.task_class
                    && observation.objective == request.objective
            })
            .collect::<Vec<_>>();
        if observations
            .iter()
            .any(|observation| !allowed.contains(observation.candidate_id.as_str()))
        {
            return Err("adaptive observation references unknown candidate".to_string());
        }
        observations.sort_by_key(|observation| observation.sequence);
        Ok(observations
            .iter()
            .map(AdaptiveObservationSummary::to_contextual)
            .collect())
    }

    pub fn adaptive_bandit_observations(&self) -> Result<Vec<ContextualBanditObservation>, String> {
        let mut observations = self.adaptive_observations()?;
        observations.sort_by_key(|observation| observation.sequence);
        Ok(observations
            .iter()
            .map(AdaptiveObservationSummary::to_contextual)
            .collect())
    }
}

fn record_in_state(
    observations: &mut Vec<AdaptiveObservationSummary>,
    input: &AdaptiveObservationInput,
    now: String,
) -> Result<AdaptiveObservationSummary, String> {
    if let Some(existing) = observations.iter().find(|observation| {
        observation.run_id == input.run_id && observation.candidate_id == input.candidate_id
    }) {
        return if existing.matches_input(input) {
            Ok(existing.clone())
        } else {
            Err("adaptive observation run/candidate conflict".to_string())
        };
    }
    if observations.len() >= MAX_OBSERVATIONS {
        return Err("adaptive observation limit exceeded".to_string());
    }
    let sequence = observations
        .last()
        .map(|observation| observation.sequence.saturating_add(1))
        .unwrap_or(1);
    let observation = AdaptiveObservationSummary::new(input, sequence, now);
    observations.push(observation.clone());
    Ok(observation)
}

fn validate_input(input: &AdaptiveObservationInput) -> Result<(), String> {
    let serialized = serde_json::to_string(input).map_err(|error| error.to_string())?;
    if contains_sensitive_patterns(&serialized) {
        return Err("adaptive observation contains sensitive data".to_string());
    }
    if input.schema_version != ADAPTIVE_OBSERVATION_SCHEMA_VERSION
        || [
            &input.run_id,
            &input.request_id,
            &input.task_class,
            &input.risk_level,
            &input.candidate_id,
            &input.candidate_kind,
            &input.quality_score_source,
        ]
        .iter()
        .any(|value| !valid_identity(value))
        || !valid_hash(&input.candidate_hash)
        || input
            .policy_hash
            .as_ref()
            .is_some_and(|hash| !valid_hash(hash))
        || !matches!(
            input.risk_level.as_str(),
            "low" | "medium" | "high" | "critical"
        )
        || !matches!(
            input.candidate_kind.as_str(),
            "single" | "ordered_fallback" | "fusion"
        )
    {
        return Err("adaptive observation identity is invalid".to_string());
    }
    if !normalized(input.quality_score) || !normalized(input.tool_success_score) {
        return Err("adaptive observation score is invalid".to_string());
    }
    if !input.cost_usd.is_finite()
        || !(0.0..=MAX_COST_USD).contains(&input.cost_usd)
        || input.latency_ms > MAX_LATENCY_MS
        || input.input_tokens > MAX_TOKEN_COUNT
        || input.output_tokens > MAX_TOKEN_COUNT
    {
        return Err("adaptive observation metrics are invalid".to_string());
    }
    Ok(())
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn normalized(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn observations_from_raw(raw: Option<&str>) -> Vec<AdaptiveObservationSummary> {
    let mut observations =
        serde_json::from_str::<Vec<AdaptiveObservationSummary>>(raw.unwrap_or("[]"))
            .unwrap_or_default();
    observations.sort_by_key(|observation| observation.sequence);
    let mut seen_ids = BTreeSet::new();
    let mut seen_run_candidates = BTreeSet::new();
    observations
        .into_iter()
        .filter(|observation| {
            observation.is_valid()
                && seen_ids.insert(observation.observation_id.clone())
                && seen_run_candidates
                    .insert((observation.run_id.clone(), observation.candidate_id.clone()))
        })
        .collect()
}

fn observations_sqlite(conn: &Connection) -> Result<Vec<AdaptiveObservationSummary>, String> {
    let raw = conn
        .query_row(
            "SELECT value_json FROM local_config WHERE key = ?1",
            params![OBSERVATIONS_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(observations_from_raw(raw.as_deref()))
}

fn write_observations_sqlite(
    conn: &Connection,
    observations: &[AdaptiveObservationSummary],
    actor: &str,
    now: &str,
) -> Result<(), String> {
    let serialized = serde_json::to_string(observations).map_err(|error| error.to_string())?;
    conn.execute(
        "INSERT INTO local_config (key, value_json, updated_at, updated_by)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at,
            updated_by = excluded.updated_by",
        params![OBSERVATIONS_KEY, serialized, now, actor],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
