use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::feedback::{
    AdaptiveAutoPromotionGate, AdaptiveAutoPromotionPolicy, AdaptiveAutoPromotionRequest,
    AdaptiveCanaryRequest, AdaptiveExperimentController, AdaptiveExperimentGate,
    AdaptivePromotionEvidenceChain, OfflineEvaluationEngine, OfflinePolicyDefinition,
    OfflineReplayReport, OfflineReplayRequest, OfflineReplayStatus, ReplayEvidenceScope,
    ShadowRouter, ADAPTIVE_PROMOTION_EVIDENCE_CHAIN_SCHEMA_VERSION, OFFLINE_REPLAY_SCHEMA_VERSION,
};

use super::{
    append_audit_locked, budget_intelligence::ProductionJobClaim, DatabaseConnection,
    LocalProductStore,
};

pub const REPLAY_PRODUCER_SCHEMA_VERSION: &str = "trace_replay_producer.v1";
const REPLAY_PRODUCTION_PROFILE_CONFIG_KEY: &str = "offline_replay_production_profile.v1";
const REPLAY_RECOVERY_STATE_CONFIG_KEY: &str = "offline_replay_recovery_state.v1";
const MAX_REPLAY_RECOVERY_PENDING: usize = 128;
const MAX_REPLAY_RECOVERY_BATCH: usize = 32;
const MAX_CONSECUTIVE_REPLAY_ATTEMPTS: u64 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayProductionRequest {
    pub dispatch_ids: Vec<String>,
    pub maximum_trace_age_seconds: u64,
    #[serde(default)]
    pub scope: ReplayEvidenceScope,
    pub current_policy: OfflinePolicyDefinition,
    pub candidate_policies: Vec<OfflinePolicyDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayProductionProfile {
    pub enabled: bool,
    pub bounded_dispatch_window: usize,
    pub maximum_trace_age_seconds: u64,
    #[serde(default)]
    pub scope: ReplayEvidenceScope,
    pub current_policy: OfflinePolicyDefinition,
    pub candidate_policies: Vec<OfflinePolicyDefinition>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayRecoveryState {
    #[serde(default)]
    cursor_history_id: i64,
    #[serde(default)]
    pending: Vec<ReplayRecoveryItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplayRecoveryItem {
    history_id: i64,
    dispatch_id: String,
    #[serde(default)]
    attempt_count: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceChainPromotionRequest {
    pub replay_artifact_id: String,
    pub promotion: AdaptiveAutoPromotionRequest,
    pub canary: AdaptiveCanaryRequest,
    pub rollout_scope: String,
    pub rollback_target: String,
    pub confirm_promotion: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReplayBinding {
    artifact_id: String,
    input_sha256: String,
    dispatch_ids: Vec<String>,
    maximum_trace_age_seconds: u64,
    scope: ReplayEvidenceScope,
    current_policy: OfflinePolicyDefinition,
    candidate_policies: Vec<OfflinePolicyDefinition>,
    created_at: String,
}

impl LocalProductStore {
    pub fn configure_replay_production_profile(
        &self,
        profile: &ReplayProductionProfile,
        actor: &str,
    ) -> Result<Value, String> {
        validate_replay_profile(profile)?;
        self.set_config_value(
            REPLAY_PRODUCTION_PROFILE_CONFIG_KEY,
            serde_json::to_value(profile).map_err(|error| error.to_string())?,
            actor,
        )
    }

    pub fn replay_production_profile(&self) -> Result<Option<ReplayProductionProfile>, String> {
        self.config_snapshot()?
            .get(REPLAY_PRODUCTION_PROFILE_CONFIG_KEY)
            .cloned()
            .map(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
            .transpose()
    }

    pub fn produce_registered_offline_replay_for_dispatch(
        &self,
        dispatch_id: &str,
        actor: &str,
    ) -> Result<Option<Value>, String> {
        let Some(profile) = self.replay_production_profile()? else {
            return Ok(None);
        };
        if !profile.enabled {
            return Ok(None);
        }
        let history_id = self
            .latest_dispatch_history_id(dispatch_id)?
            .ok_or_else(|| "automatic replay source dispatch is not persisted".to_string())?;
        self.produce_registered_offline_replay_for_history(&profile, history_id, dispatch_id, actor)
    }

    /// Recover automatic replay work from persisted dispatch ownership. The
    /// scheduler is the only caller: this is a bounded cursor, not a second
    /// queue or background scheduler. Failed items rotate behind later work
    /// and remain durable until an exact retry succeeds.
    pub fn recover_registered_offline_replays(
        &self,
        limit: usize,
        actor: &str,
    ) -> Result<Value, String> {
        let Some(profile) = self.replay_production_profile()? else {
            return Ok(replay_recovery_result(0, 0, 0, 0));
        };
        if !profile.enabled {
            return Ok(replay_recovery_result(0, 0, 0, 0));
        }
        let bounded_limit = limit.clamp(1, MAX_REPLAY_RECOVERY_BATCH);
        let mut state = self.replay_recovery_state()?;
        let capacity = MAX_REPLAY_RECOVERY_PENDING.saturating_sub(state.pending.len());
        let mut discovered = self.dispatch_history_after(
            state.cursor_history_id,
            capacity.min(MAX_REPLAY_RECOVERY_BATCH),
        )?;
        if discovered.is_empty() && state.cursor_history_id > 0 {
            state.cursor_history_id = 0;
            discovered = self.dispatch_history_after(0, capacity.min(MAX_REPLAY_RECOVERY_BATCH))?;
        }
        if let Some(last) = discovered.last() {
            state.cursor_history_id = last.history_id;
        }
        let mut known = state
            .pending
            .iter()
            .map(|item| item.history_id)
            .collect::<std::collections::BTreeSet<_>>();
        for item in discovered {
            if known.insert(item.history_id) {
                state.pending.push(item);
            }
        }

        let attempts = bounded_limit.min(state.pending.len());
        let mut completed = 0usize;
        let mut deferred = 0usize;
        for _ in 0..attempts {
            let mut item = state.pending.remove(0);
            match self.produce_registered_offline_replay_for_history(
                &profile,
                item.history_id,
                &item.dispatch_id,
                actor,
            ) {
                Ok(_) => completed += 1,
                Err(error) => {
                    item.attempt_count = item.attempt_count.saturating_add(1);
                    self.append_audit(
                        actor,
                        "offline_replay.recovery_deferred",
                        &item.dispatch_id,
                        &json!({
                            "history_id": item.history_id,
                            "dispatch_id": item.dispatch_id,
                            "attempt_count": item.attempt_count,
                            "error_sha256": sha256_bytes(error.as_bytes()),
                            "raw_error_stored": false,
                            "retry_on_next_scheduler_tick": true,
                        }),
                    )?;
                    if item.attempt_count < MAX_CONSECUTIVE_REPLAY_ATTEMPTS {
                        state.pending.push(item);
                    }
                    deferred += 1;
                }
            }
        }
        let pending = state.pending.len();
        self.set_config_value(
            REPLAY_RECOVERY_STATE_CONFIG_KEY,
            serde_json::to_value(&state).map_err(|error| error.to_string())?,
            actor,
        )?;
        Ok(replay_recovery_result(
            attempts, completed, deferred, pending,
        ))
    }

    fn produce_registered_offline_replay_for_history(
        &self,
        profile: &ReplayProductionProfile,
        history_id: i64,
        dispatch_id: &str,
        actor: &str,
    ) -> Result<Option<Value>, String> {
        let mut dispatch_ids =
            self.dispatch_window_at(history_id, profile.bounded_dispatch_window)?;
        if !dispatch_ids.iter().any(|value| value == dispatch_id) {
            return Err("automatic replay source dispatch is not persisted".to_string());
        }
        dispatch_ids.sort();
        dispatch_ids.dedup();
        self.produce_offline_replay(
            &ReplayProductionRequest {
                dispatch_ids,
                maximum_trace_age_seconds: profile.maximum_trace_age_seconds,
                scope: profile.scope.clone(),
                current_policy: profile.current_policy.clone(),
                candidate_policies: profile.candidate_policies.clone(),
            },
            actor,
        )
        .map(Some)
    }

    fn replay_recovery_state(&self) -> Result<ReplayRecoveryState, String> {
        self.config_snapshot()?
            .get(REPLAY_RECOVERY_STATE_CONFIG_KEY)
            .cloned()
            .map(|value| serde_json::from_value(value).map_err(|error| error.to_string()))
            .transpose()
            .map(|value| value.unwrap_or_default())
    }

    fn latest_dispatch_history_id(&self, dispatch_id: &str) -> Result<Option<i64>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.query_row(
                    "SELECT history_id FROM dispatch_history WHERE dispatch_id=?1 ORDER BY history_id DESC LIMIT 1",
                    params![dispatch_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .query_opt(
                        "SELECT history_id FROM dispatch_history WHERE dispatch_id=$1 ORDER BY history_id DESC LIMIT 1",
                        &[&dispatch_id],
                    )
                    .map(|row| row.map(|row| row.get(0)))
                    .map_err(|error| error.to_string())
            }),
        }
    }

    fn dispatch_history_after(
        &self,
        cursor_history_id: i64,
        limit: usize,
    ) -> Result<Vec<ReplayRecoveryItem>, String> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let limit = limit as i64;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare("SELECT history_id,dispatch_id FROM dispatch_history WHERE history_id>?1 ORDER BY history_id ASC LIMIT ?2")
                    .map_err(|error| error.to_string())?;
                let rows = stmt
                    .query_map(params![cursor_history_id, limit], |row| {
                        Ok(ReplayRecoveryItem {
                            history_id: row.get(0)?,
                            dispatch_id: row.get(1)?,
                            attempt_count: 0,
                        })
                    })
                    .map_err(|error| error.to_string())?;
                rows.map(|row| row.map_err(|error| error.to_string())).collect()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                Ok(client
                    .query(
                        "SELECT history_id,dispatch_id FROM dispatch_history WHERE history_id>$1 ORDER BY history_id ASC LIMIT $2",
                        &[&cursor_history_id, &limit],
                    )
                    .map_err(|error| error.to_string())?
                    .iter()
                    .map(|row| ReplayRecoveryItem {
                        history_id: row.get(0),
                        dispatch_id: row.get(1),
                        attempt_count: 0,
                    })
                    .collect::<Vec<_>>())
            }),
        }
    }

    fn dispatch_window_at(&self, history_id: i64, limit: usize) -> Result<Vec<String>, String> {
        let limit = limit as i64;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare("SELECT dispatch_id FROM dispatch_history WHERE history_id<=?1 ORDER BY history_id DESC LIMIT ?2")
                    .map_err(|error| error.to_string())?;
                let rows = stmt
                    .query_map(params![history_id, limit], |row| row.get::<_, String>(0))
                    .map_err(|error| error.to_string())?;
                rows.map(|row| row.map_err(|error| error.to_string())).collect()
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                Ok(client
                    .query(
                        "SELECT dispatch_id FROM dispatch_history WHERE history_id<=$1 ORDER BY history_id DESC LIMIT $2",
                        &[&history_id, &limit],
                    )
                    .map_err(|error| error.to_string())?
                    .iter()
                    .map(|row| row.get(0))
                    .collect())
            }),
        }
    }

    pub fn produce_offline_replay(
        &self,
        request: &ReplayProductionRequest,
        actor: &str,
    ) -> Result<Value, String> {
        validate_replay_production_request(request)?;
        let generated_at = self.now();
        let eligibility = self.trusted_replay_eligibility_request(
            &request.dispatch_ids,
            generated_at.clone(),
            request.maximum_trace_age_seconds,
            request.scope.clone(),
        )?;
        let replay_request = OfflineReplayRequest {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            eligibility,
            current_policy: request.current_policy.clone(),
            candidate_policies: request.candidate_policies.clone(),
        };
        let source_binding_sha256 = replay_source_binding_sha256(&replay_request.eligibility)?;
        let input_sha256 = replay_input_sha256(request, &source_binding_sha256)?;
        let scope_sha256 = sha256_json(&json!({
            "dispatch_ids": sorted_ids(&request.dispatch_ids),
            "maximum_trace_age_seconds": request.maximum_trace_age_seconds,
            "scope": request.scope,
            "source_binding_sha256": source_binding_sha256,
        }))?;
        let job_key = format!("replay-{input_sha256}");
        let lease_token = match self.claim_production_job(
            &job_key,
            "offline_replay",
            &scope_sha256,
            &input_sha256,
            actor,
        )? {
            ProductionJobClaim::Completed(result) => return Ok(result),
            ProductionJobClaim::Acquired { lease_token } => lease_token,
        };
        let artifact = self.record_offline_replay(&replay_request, actor)?;
        let artifact_id = artifact
            .get("artifact_id")
            .and_then(Value::as_str)
            .ok_or_else(|| "offline replay artifact missing id".to_string())?;
        let binding = ReplayBinding {
            artifact_id: artifact_id.to_string(),
            input_sha256: input_sha256.clone(),
            dispatch_ids: sorted_ids(&request.dispatch_ids),
            maximum_trace_age_seconds: request.maximum_trace_age_seconds,
            scope: request.scope.clone(),
            current_policy: request.current_policy.clone(),
            candidate_policies: request.candidate_policies.clone(),
            created_at: generated_at,
        };
        self.record_replay_binding(&binding, actor)?;
        let result = json!({
            "schema_version": REPLAY_PRODUCER_SCHEMA_VERSION,
            "job_key": job_key,
            "input_sha256": input_sha256,
            "artifact_id": artifact_id,
            "status": artifact["status"],
            "content_sha256": artifact["content_sha256"],
            "dispatch_count": binding.dispatch_ids.len(),
            "dispatch_ids_sha256": sha256_json(&json!(binding.dispatch_ids))?,
            "source_binding_sha256": source_binding_sha256,
            "shadow_only": true,
            "provider_calls": "disabled",
        });
        self.complete_production_job(&job_key, &input_sha256, &lease_token, &result, actor)?;
        Ok(result)
    }

    pub fn promote_replay_with_evidence_chain(
        &self,
        request: &EvidenceChainPromotionRequest,
        actor: &str,
        permission_granted: bool,
    ) -> Result<Value, String> {
        if !request.confirm_promotion {
            return Err("explicit evidence-chain promotion confirmation is required".to_string());
        }
        if !permission_granted {
            return Err("evidence-chain promotion permission is required".to_string());
        }
        let binding = self
            .get_replay_binding(&request.replay_artifact_id)?
            .ok_or_else(|| "replay producer binding not found".to_string())?;
        let stored = self
            .get_offline_replay_artifact(&request.replay_artifact_id)?
            .ok_or_else(|| "offline replay artifact not found".to_string())?;
        let stored_report: OfflineReplayReport = serde_json::from_value(
            stored
                .get("report")
                .cloned()
                .ok_or_else(|| "offline replay artifact report missing".to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if stored_report.status != OfflineReplayStatus::Sufficient {
            return Err("offline replay evidence is not sufficient".to_string());
        }

        let eligibility = self.trusted_replay_eligibility_request(
            &binding.dispatch_ids,
            self.now(),
            binding.maximum_trace_age_seconds,
            binding.scope.clone(),
        )?;
        let source_binding_sha256 = replay_source_binding_sha256(&eligibility)?;
        let rebound_input_sha256 = replay_input_sha256(
            &ReplayProductionRequest {
                dispatch_ids: binding.dispatch_ids.clone(),
                maximum_trace_age_seconds: binding.maximum_trace_age_seconds,
                scope: binding.scope.clone(),
                current_policy: binding.current_policy.clone(),
                candidate_policies: binding.candidate_policies.clone(),
            },
            &source_binding_sha256,
        )?;
        if rebound_input_sha256 != binding.input_sha256 {
            return Err(
                "replay producer binding changed during mutation-time rebinding".to_string(),
            );
        }
        let fresh_request = OfflineReplayRequest {
            schema_version: OFFLINE_REPLAY_SCHEMA_VERSION.to_string(),
            eligibility,
            current_policy: binding.current_policy.clone(),
            candidate_policies: binding.candidate_policies.clone(),
        };
        let fresh_report = OfflineEvaluationEngine::replay_policies(&fresh_request)
            .map_err(|error| error.to_string())?;
        if fresh_report.status != OfflineReplayStatus::Sufficient
            || fresh_report.source_evidence_content_sha256
                != stored_report.source_evidence_content_sha256
            || fresh_report.current_policy.policy_hash != stored_report.current_policy.policy_hash
            || fresh_report
                .candidate_policies
                .iter()
                .map(|policy| &policy.policy_hash)
                .collect::<Vec<_>>()
                != stored_report
                    .candidate_policies
                    .iter()
                    .map(|policy| &policy.policy_hash)
                    .collect::<Vec<_>>()
        {
            return Err("replay evidence changed during mutation-time rebinding".to_string());
        }
        let shadow = ShadowRouter::compare_replay_report(&fresh_report)?;
        let canary = AdaptiveExperimentController::start_canary(
            &request.canary,
            &shadow,
            &AdaptiveExperimentGate::from_env(),
        )
        .map_err(|error| format!("{}: {}", error.code, error.violations.join(",")))?;
        if canary.status != "started" {
            return Err(format!(
                "canary evidence is not started: {}",
                canary.blocked_reasons.join(",")
            ));
        }
        let mut chain = AdaptivePromotionEvidenceChain {
            schema_version: ADAPTIVE_PROMOTION_EVIDENCE_CHAIN_SCHEMA_VERSION.to_string(),
            offline: fresh_report,
            shadow,
            canary,
            rollout_scope: request.rollout_scope.clone(),
            rollback_target: request.rollback_target.clone(),
            content_sha256: String::new(),
        };
        chain.finalize();
        self.promote_adaptive_fusion_policy_with_evidence_chain(
            &request.promotion,
            &chain,
            &AdaptiveAutoPromotionPolicy::from_env(),
            &AdaptiveAutoPromotionGate::from_env(),
            actor,
            request.confirm_promotion,
            permission_granted,
        )
    }

    fn record_replay_binding(&self, binding: &ReplayBinding, actor: &str) -> Result<(), String> {
        let dispatch_ids =
            serde_json::to_string(&binding.dispatch_ids).map_err(|error| error.to_string())?;
        let scope = serde_json::to_string(&binding.scope).map_err(|error| error.to_string())?;
        let current =
            serde_json::to_string(&binding.current_policy).map_err(|error| error.to_string())?;
        let candidates = serde_json::to_string(&binding.candidate_policies)
            .map_err(|error| error.to_string())?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx=rusqlite::Transaction::new_unchecked(conn,TransactionBehavior::Immediate).map_err(|error|error.to_string())?;
                let existing:Option<String>=tx.query_row("SELECT input_sha256 FROM replay_producer_bindings WHERE artifact_id=?1",params![binding.artifact_id],|row|row.get(0)).optional().map_err(|error|error.to_string())?;
                if let Some(existing)=existing {if existing!=binding.input_sha256{return Err("replay producer binding conflict".to_string())}return Ok(())}
                tx.execute("INSERT INTO replay_producer_bindings (artifact_id,input_sha256,dispatch_ids_json,maximum_trace_age_seconds,scope_json,current_policy_json,candidate_policies_json,created_at,created_by) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![binding.artifact_id,binding.input_sha256,dispatch_ids,binding.maximum_trace_age_seconds as i64,scope,current,candidates,binding.created_at,actor]).map_err(|error|error.to_string())?;
                append_audit_locked(&tx,&binding.created_at,actor,"offline_replay.binding_recorded",&format!("offline-replay/{}",binding.artifact_id),&json!({"artifact_id":binding.artifact_id,"input_sha256":binding.input_sha256,"dispatch_ids_sha256":sha256_json(&json!(binding.dispatch_ids))?,"dispatch_count":binding.dispatch_ids.len()}))?;
                tx.commit().map_err(|error|error.to_string())
            }),
            #[cfg(feature="pg")]
            DatabaseConnection::Pg(_)=>self.with_pg_conn(|client|{let mut tx=client.transaction().map_err(|error|error.to_string())?;tx.execute("SELECT pg_advisory_xact_lock(hashtext($1))", &[&binding.artifact_id]).map_err(|error|error.to_string())?;let existing=tx.query_opt("SELECT input_sha256 FROM replay_producer_bindings WHERE artifact_id=$1 FOR UPDATE",&[&binding.artifact_id]).map_err(|error|error.to_string())?;if let Some(row)=existing{if row.get::<_,String>(0)!=binding.input_sha256{return Err("replay producer binding conflict".to_string())}return Ok(())}let max_age=binding.maximum_trace_age_seconds as i64;tx.execute("INSERT INTO replay_producer_bindings (artifact_id,input_sha256,dispatch_ids_json,maximum_trace_age_seconds,scope_json,current_policy_json,candidate_policies_json,created_at,created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",&[&binding.artifact_id,&binding.input_sha256,&dispatch_ids,&max_age,&scope,&current,&candidates,&binding.created_at,&actor]).map_err(|error|error.to_string())?;pg_audit(&mut tx,&binding.created_at,actor,"offline_replay.binding_recorded",&format!("offline-replay/{}",binding.artifact_id),&json!({"artifact_id":binding.artifact_id,"input_sha256":binding.input_sha256,"dispatch_ids_sha256":sha256_json(&json!(binding.dispatch_ids))?,"dispatch_count":binding.dispatch_ids.len()}))?;tx.commit().map_err(|error|error.to_string())}),
        }
    }

    fn get_replay_binding(&self, artifact_id: &str) -> Result<Option<ReplayBinding>, String> {
        let parse = |input_sha256: String,
                     dispatch_ids: String,
                     max_age: i64,
                     scope: String,
                     current: String,
                     candidates: String,
                     created_at: String|
         -> Result<ReplayBinding, String> {
            Ok(ReplayBinding {
                artifact_id: artifact_id.to_string(),
                input_sha256,
                dispatch_ids: serde_json::from_str(&dispatch_ids)
                    .map_err(|error| error.to_string())?,
                maximum_trace_age_seconds: max_age
                    .try_into()
                    .map_err(|_| "invalid replay maximum age".to_string())?,
                scope: serde_json::from_str(&scope).map_err(|error| error.to_string())?,
                current_policy: serde_json::from_str(&current)
                    .map_err(|error| error.to_string())?,
                candidate_policies: serde_json::from_str(&candidates)
                    .map_err(|error| error.to_string())?,
                created_at,
            })
        };
        match &self.db{DatabaseConnection::Sqlite(_)=>self.with_conn(|conn|conn.query_row("SELECT input_sha256,dispatch_ids_json,maximum_trace_age_seconds,scope_json,current_policy_json,candidate_policies_json,created_at FROM replay_producer_bindings WHERE artifact_id=?1",params![artifact_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?))).optional().map_err(|error|error.to_string())?.map(|row|parse(row.0,row.1,row.2,row.3,row.4,row.5,row.6)).transpose()),#[cfg(feature="pg")]DatabaseConnection::Pg(_)=>self.with_pg_conn(|client|client.query_opt("SELECT input_sha256,dispatch_ids_json,maximum_trace_age_seconds,scope_json,current_policy_json,candidate_policies_json,created_at FROM replay_producer_bindings WHERE artifact_id=$1",&[&artifact_id]).map_err(|error|error.to_string())?.map(|row|parse(row.get(0),row.get(1),row.get(2),row.get(3),row.get(4),row.get(5),row.get(6))).transpose())}
    }
}

fn validate_replay_production_request(request: &ReplayProductionRequest) -> Result<(), String> {
    if request.dispatch_ids.is_empty() || request.dispatch_ids.len() > 10_000 {
        return Err("replay dispatch count is outside bounds".to_string());
    }
    if request.maximum_trace_age_seconds == 0
        || request.maximum_trace_age_seconds > 30 * 24 * 60 * 60
    {
        return Err("replay maximum trace age is outside bounds".to_string());
    }
    if request.candidate_policies.is_empty() || request.candidate_policies.len() > 64 {
        return Err("replay candidate policy count is outside bounds".to_string());
    }
    validate_policy(&request.current_policy)?;
    for policy in &request.candidate_policies {
        validate_policy(policy)?;
    }
    Ok(())
}
fn validate_replay_profile(profile: &ReplayProductionProfile) -> Result<(), String> {
    if profile.bounded_dispatch_window == 0 || profile.bounded_dispatch_window > 10_000 {
        return Err("replay profile dispatch window is outside bounds".to_string());
    }
    validate_replay_production_request(&ReplayProductionRequest {
        dispatch_ids: vec!["profile-validation".to_string()],
        maximum_trace_age_seconds: profile.maximum_trace_age_seconds,
        scope: profile.scope.clone(),
        current_policy: profile.current_policy.clone(),
        candidate_policies: profile.candidate_policies.clone(),
    })
}
fn validate_policy(policy: &OfflinePolicyDefinition) -> Result<(), String> {
    if policy.schema_version != OFFLINE_REPLAY_SCHEMA_VERSION {
        return Err("offline policy definition schema is unsupported".to_string());
    }
    if !valid_id(&policy.policy_id) || !valid_id(&policy.policy_version) {
        return Err("offline policy definition identity is invalid".to_string());
    }
    if policy.selections.is_empty() || policy.selections.len() > 256 {
        return Err("offline policy definition selections are outside bounds".to_string());
    }
    for (task_class, selection) in &policy.selections {
        if !valid_id(task_class)
            || !valid_id(&selection.candidate_id)
            || !valid_id(&selection.candidate_version)
            || !valid_hash(&selection.candidate_definition_sha256)
        {
            return Err("offline policy candidate binding is invalid".to_string());
        }
    }
    let hash = policy.content_sha256().map_err(|error| error.to_string())?;
    if hash != policy.policy_hash {
        return Err("offline policy definition hash mismatch".to_string());
    }
    Ok(())
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn replay_source_binding_sha256(
    request: &crate::feedback::ReplayEligibilityRequest,
) -> Result<String, String> {
    sha256_json(&serde_json::to_value(&request.traces).map_err(|error| error.to_string())?)
}
fn replay_input_sha256(
    request: &ReplayProductionRequest,
    source_binding_sha256: &str,
) -> Result<String, String> {
    sha256_json(
        &json!({"dispatch_ids":sorted_ids(&request.dispatch_ids),"maximum_trace_age_seconds":request.maximum_trace_age_seconds,"scope":request.scope,"current_policy":request.current_policy,"candidate_policies":request.candidate_policies,"source_binding_sha256":source_binding_sha256}),
    )
}
fn sorted_ids(ids: &[String]) -> Vec<String> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids.dedup();
    ids
}
fn sha256_json(value: &Value) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|error| error.to_string())
}
fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn replay_recovery_result(
    attempted: usize,
    completed: usize,
    deferred: usize,
    pending: usize,
) -> Value {
    json!({
        "schema_version": "offline_replay_recovery.v1",
        "bounded_limit": MAX_REPLAY_RECOVERY_BATCH,
        "attempted": attempted,
        "completed_or_idempotent": completed,
        "deferred": deferred,
        "pending": pending,
        "provider_calls": "disabled",
    })
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
