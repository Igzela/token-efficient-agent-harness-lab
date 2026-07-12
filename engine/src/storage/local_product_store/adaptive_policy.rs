use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{append_audit_locked, DatabaseConnection, LocalProductStore};
use crate::feedback::policy_snapshot::stable_hash;
use crate::feedback::{
    AdaptiveAutoPromotionController, AdaptiveAutoPromotionGate, AdaptiveAutoPromotionPolicy,
    AdaptiveAutoPromotionRequest, AdaptivePromotionEvidenceChain, ContextualPolicyPromotionVerdict,
    PromotedAdaptivePolicy, CONTEXTUAL_POLICY_SCHEMA_VERSION,
};

const ACTIVE_POLICIES_KEY: &str = "adaptive_fusion_active_policies";
const POLICY_SNAPSHOTS_KEY: &str = "adaptive_fusion_policy_snapshots";
const APPLY_RESULT_SCHEMA_VERSION: &str = "adaptive_policy_apply_result.v1";
const ROLLBACK_RESULT_SCHEMA_VERSION: &str = "adaptive_policy_rollback_result.v1";
const MAX_POLICY_SNAPSHOTS: usize = 100;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptivePolicySnapshot {
    pub schema_version: String,
    pub adjustment_id: String,
    pub snapshot_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub status: String,
    pub actor: String,
    pub policy_key: String,
    pub candidate_id: String,
    pub active_policy_before: Option<PromotedAdaptivePolicy>,
    pub promoted_policy: PromotedAdaptivePolicy,
    pub evidence_run_ids: Vec<String>,
    pub safety_hash: String,
}

impl AdaptivePolicySnapshot {
    fn new(
        sequence: usize,
        now: String,
        actor: &str,
        active_policy_before: Option<PromotedAdaptivePolicy>,
        promoted_policy: PromotedAdaptivePolicy,
    ) -> Self {
        let adjustment_id = format!("adaptive-policy-{sequence:04}");
        let snapshot_id = format!("adaptive-policy-snapshot-{sequence:04}");
        let evidence_run_ids = promoted_policy.evidence_run_ids.clone();
        let mut snapshot = Self {
            schema_version: "adaptive_policy_snapshot.v1".to_string(),
            adjustment_id,
            snapshot_id,
            created_at: now.clone(),
            updated_at: now,
            status: "active".to_string(),
            actor: actor.to_string(),
            policy_key: promoted_policy.policy_key.clone(),
            candidate_id: promoted_policy.candidate_id.clone(),
            active_policy_before,
            promoted_policy,
            evidence_run_ids,
            safety_hash: String::new(),
        };
        snapshot.safety_hash = snapshot.compute_hash();
        snapshot
    }

    fn compute_hash(&self) -> String {
        stable_hash(&json!({
            "schema_version": self.schema_version,
            "adjustment_id": self.adjustment_id,
            "snapshot_id": self.snapshot_id,
            "created_at": self.created_at,
            "actor": self.actor,
            "policy_key": self.policy_key,
            "candidate_id": self.candidate_id,
            "active_policy_before": self.active_policy_before,
            "promoted_policy": self.promoted_policy,
            "evidence_run_ids": self.evidence_run_ids,
        }))
    }

    fn hash_is_valid(&self) -> bool {
        self.safety_hash == self.compute_hash()
            && self.promoted_policy.is_valid()
            && self
                .active_policy_before
                .as_ref()
                .is_none_or(PromotedAdaptivePolicy::is_valid)
    }
}

impl LocalProductStore {
    pub fn active_adaptive_fusion_policies(&self) -> Result<Vec<PromotedAdaptivePolicy>, String> {
        let value = self.config_value(ACTIVE_POLICIES_KEY)?;
        let policies: Vec<PromotedAdaptivePolicy> =
            serde_json::from_value::<Vec<PromotedAdaptivePolicy>>(value)
                .unwrap_or_default()
                .into_iter()
                .filter(PromotedAdaptivePolicy::is_valid)
                .collect();
        Ok(policies)
    }

    pub fn adaptive_fusion_policy_snapshots(&self) -> Result<Vec<AdaptivePolicySnapshot>, String> {
        let value = self.config_value(POLICY_SNAPSHOTS_KEY)?;
        let mut snapshots: Vec<AdaptivePolicySnapshot> =
            serde_json::from_value(value).unwrap_or_default();
        snapshots.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(snapshots)
    }

    pub fn apply_adaptive_fusion_policy(
        &self,
        verdict: &ContextualPolicyPromotionVerdict,
        actor: &str,
    ) -> Result<Value, String> {
        let Some(policy) = verdict.policy.as_ref().filter(|policy| policy.is_valid()) else {
            self.audit_adaptive_policy(
                actor,
                "adaptive_policy.apply.rejected",
                "pending",
                &json!({
                    "actor": actor,
                    "eligible": verdict.eligible,
                    "blocked_reasons": verdict.blocked_reasons,
                    "source": "adaptive_fusion",
                }),
            )?;
            return Ok(apply_result(
                None,
                None,
                None,
                "blocked",
                false,
                verdict.blocked_reasons.clone(),
            ));
        };
        if !verdict.eligible {
            self.audit_adaptive_policy(
                actor,
                "adaptive_policy.apply.rejected",
                &policy.policy_key,
                &json!({
                    "actor": actor,
                    "eligible": false,
                    "blocked_reasons": verdict.blocked_reasons,
                    "source": "adaptive_fusion",
                }),
            )?;
            return Ok(apply_result(
                None,
                None,
                Some(policy),
                "blocked",
                false,
                verdict.blocked_reasons.clone(),
            ));
        }

        self.apply_accepted_adaptive_fusion_policy(policy, actor)
    }

    pub fn auto_promote_adaptive_fusion_policy(
        &self,
        request: &AdaptiveAutoPromotionRequest,
        _policy: &AdaptiveAutoPromotionPolicy,
        _gate: &AdaptiveAutoPromotionGate,
        actor: &str,
    ) -> Result<Value, String> {
        self.audit_adaptive_policy(
            actor,
            "adaptive_policy.apply.rejected",
            &format!("{}:{}", request.task_class, request.candidate_id),
            &json!({
                "actor": actor,
                "blocked_reasons": ["complete_evidence_chain_required"],
                "source": "adaptive_fusion",
            }),
        )?;
        Ok(apply_result(
            None,
            None,
            None,
            "blocked",
            false,
            vec!["complete_evidence_chain_required".to_string()],
        ))
    }

    pub fn promote_adaptive_fusion_policy_with_evidence_chain(
        &self,
        request: &AdaptiveAutoPromotionRequest,
        chain: &AdaptivePromotionEvidenceChain,
        policy: &AdaptiveAutoPromotionPolicy,
        gate: &AdaptiveAutoPromotionGate,
        actor: &str,
        confirm_promotion: bool,
        permission_granted: bool,
    ) -> Result<Value, String> {
        let active = self
            .active_adaptive_fusion_policies()?
            .into_iter()
            .find(|active| {
                active.task_class == request.task_class && active.objective == request.objective
            });
        let verdict = AdaptiveAutoPromotionController::evaluate_with_evidence_chain(
            request,
            chain,
            active.as_ref(),
            policy,
            gate,
            confirm_promotion,
            permission_granted,
        );
        self.apply_adaptive_fusion_policy(&verdict, actor)
    }

    fn apply_accepted_adaptive_fusion_policy(
        &self,
        policy: &PromotedAdaptivePolicy,
        actor: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                let mut policies =
                    active_policies_from_value(sqlite_config_value(&tx, ACTIVE_POLICIES_KEY)?);
                let mut snapshots =
                    snapshots_from_value(sqlite_config_value(&tx, POLICY_SNAPSHOTS_KEY)?);
                let result = match apply_accepted_policy_to_state(
                    &mut policies,
                    &mut snapshots,
                    policy,
                    actor,
                    &self.now(),
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        append_audit_locked(
                            &tx,
                            &self.now(),
                            actor,
                            "adaptive_policy.apply.rejected",
                            &policy.policy_key,
                            &json!({
                                "actor": actor,
                                "blocked_reasons": [error.clone()],
                                "source": "adaptive_fusion",
                            }),
                        )?;
                        tx.commit().map_err(|e| e.to_string())?;
                        return Err(error);
                    }
                };
                self.write_adaptive_policy_state_sqlite_locked(&tx, &policies, &snapshots, actor)?;
                append_audit_locked(
                    &tx,
                    &self.now(),
                    actor,
                    "adaptive_policy.apply.accepted",
                    result["adjustment_id"].as_str().unwrap_or("unknown"),
                    &json!({
                        "adjustment_id": result["adjustment_id"],
                        "snapshot_id": result["snapshot_id"],
                        "policy_key": policy.policy_key,
                        "candidate_id": policy.candidate_id,
                        "actor": actor,
                        "blocked_reasons": [],
                        "source": "adaptive_fusion",
                    }),
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(result)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                pg_ensure_policy_config_rows(&mut tx, &self.now(), actor)?;
                let mut policies = active_policies_from_value(pg_config_value_locked(
                    &mut tx,
                    ACTIVE_POLICIES_KEY,
                )?);
                let mut snapshots =
                    snapshots_from_value(pg_config_value_locked(&mut tx, POLICY_SNAPSHOTS_KEY)?);
                let result = match apply_accepted_policy_to_state(
                    &mut policies,
                    &mut snapshots,
                    policy,
                    actor,
                    &self.now(),
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        pg_insert_audit(
                            &mut tx,
                            &self.now(),
                            actor,
                            "adaptive_policy.apply.rejected",
                            &policy.policy_key,
                            &json!({
                                "actor": actor,
                                "blocked_reasons": [error.clone()],
                                "source": "adaptive_fusion",
                            }),
                        )?;
                        tx.commit().map_err(|e| e.to_string())?;
                        return Err(error);
                    }
                };
                write_adaptive_policy_state_pg_locked(
                    &mut tx,
                    &policies,
                    &snapshots,
                    actor,
                    &self.now(),
                )?;
                pg_insert_audit(
                    &mut tx,
                    &self.now(),
                    actor,
                    "adaptive_policy.apply.accepted",
                    result["adjustment_id"].as_str().unwrap_or("unknown"),
                    &json!({
                        "adjustment_id": result["adjustment_id"],
                        "snapshot_id": result["snapshot_id"],
                        "policy_key": policy.policy_key,
                        "candidate_id": policy.candidate_id,
                        "actor": actor,
                        "blocked_reasons": [],
                        "source": "adaptive_fusion",
                    }),
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                Ok(result)
            }),
        }
    }

    pub fn rollback_adaptive_fusion_policy(
        &self,
        adjustment_id: &str,
        confirm: bool,
        actor: &str,
    ) -> Result<Value, String> {
        if !confirm {
            self.audit_adaptive_policy(
                actor,
                "adaptive_policy.rollback.rejected",
                adjustment_id,
                &json!({
                    "adjustment_id": adjustment_id,
                    "confirmation_flag": false,
                    "blocked_reasons": ["confirm_adaptive_policy_rollback is required"],
                    "source": "adaptive_fusion",
                }),
            )?;
            return Err("confirm_adaptive_policy_rollback is required".to_string());
        }
        self.rollback_confirmed_adaptive_fusion_policy(adjustment_id, actor)
    }

    fn rollback_confirmed_adaptive_fusion_policy(
        &self,
        adjustment_id: &str,
        actor: &str,
    ) -> Result<Value, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
                let mut policies =
                    active_policies_from_value(sqlite_config_value(&tx, ACTIVE_POLICIES_KEY)?);
                let mut snapshots =
                    snapshots_from_value(sqlite_config_value(&tx, POLICY_SNAPSHOTS_KEY)?);
                let result = match rollback_policy_in_state(
                    &mut policies,
                    &mut snapshots,
                    adjustment_id,
                    &self.now(),
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        append_audit_locked(
                            &tx,
                            &self.now(),
                            actor,
                            "adaptive_policy.rollback.rejected",
                            adjustment_id,
                            &json!({
                                "adjustment_id": adjustment_id,
                                "blocked_reasons": [error.clone()],
                                "source": "adaptive_fusion",
                            }),
                        )?;
                        tx.commit().map_err(|e| e.to_string())?;
                        return Err(error);
                    }
                };
                if result["rolled_back"].as_bool().unwrap_or(false) {
                    self.write_adaptive_policy_state_sqlite_locked(
                        &tx, &policies, &snapshots, actor,
                    )?;
                    append_audit_locked(
                        &tx,
                        &self.now(),
                        actor,
                        "adaptive_policy.rollback.accepted",
                        adjustment_id,
                        &json!({
                            "adjustment_id": adjustment_id,
                            "snapshot_id": result["snapshot_id"],
                            "policy_key": result["policy_key"],
                            "blocked_reasons": [],
                            "source": "adaptive_fusion",
                        }),
                    )?;
                } else {
                    append_audit_locked(
                        &tx,
                        &self.now(),
                        actor,
                        "adaptive_policy.rollback.rejected",
                        adjustment_id,
                        &json!({
                            "adjustment_id": adjustment_id,
                            "snapshot_id": result["snapshot_id"],
                            "policy_key": result["policy_key"],
                            "blocked_reasons": result["blocked_reasons"],
                            "source": "adaptive_fusion",
                        }),
                    )?;
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(result)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|e| e.to_string())?;
                pg_ensure_policy_config_rows(&mut tx, &self.now(), actor)?;
                let mut policies = active_policies_from_value(pg_config_value_locked(
                    &mut tx,
                    ACTIVE_POLICIES_KEY,
                )?);
                let mut snapshots =
                    snapshots_from_value(pg_config_value_locked(&mut tx, POLICY_SNAPSHOTS_KEY)?);
                let result = match rollback_policy_in_state(
                    &mut policies,
                    &mut snapshots,
                    adjustment_id,
                    &self.now(),
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        pg_insert_audit(
                            &mut tx,
                            &self.now(),
                            actor,
                            "adaptive_policy.rollback.rejected",
                            adjustment_id,
                            &json!({
                                "adjustment_id": adjustment_id,
                                "blocked_reasons": [error.clone()],
                                "source": "adaptive_fusion",
                            }),
                        )?;
                        tx.commit().map_err(|e| e.to_string())?;
                        return Err(error);
                    }
                };
                if result["rolled_back"].as_bool().unwrap_or(false) {
                    write_adaptive_policy_state_pg_locked(
                        &mut tx,
                        &policies,
                        &snapshots,
                        actor,
                        &self.now(),
                    )?;
                    pg_insert_audit(
                        &mut tx,
                        &self.now(),
                        actor,
                        "adaptive_policy.rollback.accepted",
                        adjustment_id,
                        &json!({
                            "adjustment_id": adjustment_id,
                            "snapshot_id": result["snapshot_id"],
                            "policy_key": result["policy_key"],
                            "blocked_reasons": [],
                            "source": "adaptive_fusion",
                        }),
                    )?;
                } else {
                    pg_insert_audit(
                        &mut tx,
                        &self.now(),
                        actor,
                        "adaptive_policy.rollback.rejected",
                        adjustment_id,
                        &json!({
                            "adjustment_id": adjustment_id,
                            "snapshot_id": result["snapshot_id"],
                            "policy_key": result["policy_key"],
                            "blocked_reasons": result["blocked_reasons"],
                            "source": "adaptive_fusion",
                        }),
                    )?;
                }
                tx.commit().map_err(|e| e.to_string())?;
                Ok(result)
            }),
        }
    }

    fn config_value(&self, key: &str) -> Result<Value, String> {
        Ok(self
            .config_snapshot()?
            .get(key)
            .cloned()
            .unwrap_or_else(|| json!([])))
    }

    fn write_adaptive_policy_state_sqlite_locked(
        &self,
        conn: &Connection,
        policies: &[PromotedAdaptivePolicy],
        snapshots: &[AdaptivePolicySnapshot],
        actor: &str,
    ) -> Result<(), String> {
        if policies.iter().any(|policy| !policy.is_valid())
            || snapshots.iter().any(|snapshot| !snapshot.hash_is_valid())
        {
            return Err("invalid adaptive policy state".to_string());
        }
        let now = self.now();
        conn.execute(
            "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(key) DO UPDATE SET
                        value_json = excluded.value_json,
                        updated_at = excluded.updated_at,
                        updated_by = excluded.updated_by",
            params![ACTIVE_POLICIES_KEY, json!(policies).to_string(), now, actor],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(key) DO UPDATE SET
                        value_json = excluded.value_json,
                        updated_at = excluded.updated_at,
                        updated_by = excluded.updated_by",
            params![
                POLICY_SNAPSHOTS_KEY,
                json!(snapshots).to_string(),
                self.now(),
                actor
            ],
        )
        .map_err(|e| e.to_string())?;
        append_audit_locked(
            conn,
            &self.now(),
            actor,
            "adaptive_policy.state.updated",
            ACTIVE_POLICIES_KEY,
            &json!({"active_count": policies.len(), "snapshot_count": snapshots.len()}),
        )?;
        Ok(())
    }

    fn audit_adaptive_policy(
        &self,
        actor: &str,
        action: &str,
        resource: &str,
        details: &Value,
    ) -> Result<(), String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                append_audit_locked(conn, &self.now(), actor, action, resource, details)?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => {
                self.with_pg_conn(|client| {
                    client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[&self.now(), &actor, &action, &resource, &details.to_string()],
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(())
                })
            }
        }
    }
}

fn active_policies_from_value(value: Value) -> Vec<PromotedAdaptivePolicy> {
    serde_json::from_value::<Vec<PromotedAdaptivePolicy>>(value)
        .unwrap_or_default()
        .into_iter()
        .filter(PromotedAdaptivePolicy::is_valid)
        .collect()
}

fn snapshots_from_value(value: Value) -> Vec<AdaptivePolicySnapshot> {
    let mut snapshots: Vec<AdaptivePolicySnapshot> =
        serde_json::from_value::<Vec<AdaptivePolicySnapshot>>(value)
            .unwrap_or_default()
            .into_iter()
            .filter(AdaptivePolicySnapshot::hash_is_valid)
            .collect();
    snapshots.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    snapshots
}

fn sqlite_config_value(conn: &Connection, key: &str) -> Result<Value, String> {
    let raw = conn
        .query_row(
            "SELECT value_json FROM local_config WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    Ok(raw
        .and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_else(|| json!([])))
}

fn apply_accepted_policy_to_state(
    policies: &mut Vec<PromotedAdaptivePolicy>,
    snapshots: &mut Vec<AdaptivePolicySnapshot>,
    policy: &PromotedAdaptivePolicy,
    actor: &str,
    now: &str,
) -> Result<Value, String> {
    let existing = policies
        .iter()
        .position(|existing| existing.policy_key == policy.policy_key);
    let active_policy_before = existing.map(|index| policies[index].clone());
    if policy.auto_promoted {
        match (&active_policy_before, &policy.previous_policy_hash) {
            (Some(active), Some(previous)) if active.policy_hash == *previous => {}
            (None, None) => {}
            _ => return Err("adaptive auto promotion active policy hash is stale".to_string()),
        }
    }
    if let Some(index) = existing {
        policies[index] = policy.clone();
    } else {
        policies.push(policy.clone());
    }
    policies.sort_by(|left, right| left.policy_key.cmp(&right.policy_key));

    if snapshots.len() >= MAX_POLICY_SNAPSHOTS {
        return Err("adaptive policy snapshot limit exceeded".to_string());
    }
    let sequence = snapshots.len() + 1;
    let snapshot = AdaptivePolicySnapshot::new(
        sequence,
        now.to_string(),
        actor,
        active_policy_before,
        policy.clone(),
    );
    let adjustment_id = snapshot.adjustment_id.clone();
    let snapshot_id = snapshot.snapshot_id.clone();
    snapshots.push(snapshot);
    Ok(apply_result(
        Some(adjustment_id),
        Some(snapshot_id),
        Some(policy),
        "active",
        true,
        Vec::new(),
    ))
}

fn rollback_policy_in_state(
    policies: &mut Vec<PromotedAdaptivePolicy>,
    snapshots: &mut [AdaptivePolicySnapshot],
    adjustment_id: &str,
    now: &str,
) -> Result<Value, String> {
    let Some(index) = snapshots
        .iter()
        .position(|snapshot| snapshot.adjustment_id == adjustment_id)
    else {
        return Err(format!("adaptive policy not found: {adjustment_id}"));
    };
    let mut snapshot = snapshots[index].clone();
    let mut blocked_reasons = Vec::new();
    if snapshot.status != "active" {
        blocked_reasons.push(format!(
            "adaptive policy {adjustment_id} is not active: {}",
            snapshot.status
        ));
    }
    if !snapshot.hash_is_valid() {
        blocked_reasons.push("adaptive policy snapshot safety hash mismatch".to_string());
    }
    let current_matches = policies.iter().any(|policy| {
        policy.policy_key == snapshot.policy_key
            && policy.policy_hash == snapshot.promoted_policy.policy_hash
    });
    if !current_matches {
        blocked_reasons.push("active adaptive policy no longer matches snapshot".to_string());
    }
    if !blocked_reasons.is_empty() {
        return Ok(rollback_result(
            &snapshot,
            "blocked",
            false,
            blocked_reasons,
        ));
    }

    policies.retain(|policy| policy.policy_key != snapshot.policy_key);
    if let Some(previous) = snapshot.active_policy_before.clone() {
        policies.push(previous);
    }
    policies.sort_by(|left, right| left.policy_key.cmp(&right.policy_key));
    snapshot.status = "rolled_back".to_string();
    snapshot.updated_at = now.to_string();
    snapshots[index] = snapshot.clone();
    Ok(rollback_result(&snapshot, "rolled_back", true, Vec::new()))
}

#[cfg(feature = "pg")]
fn pg_ensure_policy_config_rows(
    tx: &mut postgres::Transaction<'_>,
    now: &str,
    actor: &str,
) -> Result<(), String> {
    for key in [ACTIVE_POLICIES_KEY, POLICY_SNAPSHOTS_KEY] {
        tx.execute(
            "INSERT INTO local_config (key, value_json, updated_at, updated_by)
             VALUES ($1, '[]', $2, $3)
             ON CONFLICT(key) DO NOTHING",
            &[&key, &now, &actor],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_config_value_locked(tx: &mut postgres::Transaction<'_>, key: &str) -> Result<Value, String> {
    let row = tx
        .query_one(
            "SELECT value_json FROM local_config WHERE key = $1 FOR UPDATE",
            &[&key],
        )
        .map_err(|e| e.to_string())?;
    let raw: String = row.get(0);
    Ok(serde_json::from_str(&raw).unwrap_or_else(|_| json!([])))
}

#[cfg(feature = "pg")]
fn write_adaptive_policy_state_pg_locked(
    tx: &mut postgres::Transaction<'_>,
    policies: &[PromotedAdaptivePolicy],
    snapshots: &[AdaptivePolicySnapshot],
    actor: &str,
    now: &str,
) -> Result<(), String> {
    if policies.iter().any(|policy| !policy.is_valid())
        || snapshots.iter().any(|snapshot| !snapshot.hash_is_valid())
    {
        return Err("invalid adaptive policy state".to_string());
    }
    tx.execute(
        "INSERT INTO local_config (key, value_json, updated_at, updated_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at,
            updated_by = excluded.updated_by",
        &[
            &ACTIVE_POLICIES_KEY,
            &json!(policies).to_string(),
            &now,
            &actor,
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO local_config (key, value_json, updated_at, updated_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT(key) DO UPDATE SET
            value_json = excluded.value_json,
            updated_at = excluded.updated_at,
            updated_by = excluded.updated_by",
        &[
            &POLICY_SNAPSHOTS_KEY,
            &json!(snapshots).to_string(),
            &now,
            &actor,
        ],
    )
    .map_err(|e| e.to_string())?;
    pg_insert_audit(
        tx,
        now,
        actor,
        "adaptive_policy.state.updated",
        ACTIVE_POLICIES_KEY,
        &json!({"active_count": policies.len(), "snapshot_count": snapshots.len()}),
    )?;
    Ok(())
}

#[cfg(feature = "pg")]
fn pg_insert_audit(
    tx: &mut postgres::Transaction<'_>,
    now: &str,
    actor: &str,
    action: &str,
    resource: &str,
    details: &Value,
) -> Result<(), String> {
    tx.execute(
        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
         VALUES ($1, $2, $3, $4, $5)",
        &[&now, &actor, &action, &resource, &details.to_string()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn apply_result(
    adjustment_id: Option<String>,
    snapshot_id: Option<String>,
    policy: Option<&PromotedAdaptivePolicy>,
    status: &str,
    applied: bool,
    blocked_reasons: Vec<String>,
) -> Value {
    json!({
        "schema_version": APPLY_RESULT_SCHEMA_VERSION,
        "adjustment_id": adjustment_id,
        "snapshot_id": snapshot_id,
        "policy_key": policy.map(|policy| policy.policy_key.clone()),
        "candidate_id": policy.map(|policy| policy.candidate_id.clone()),
        "status": status,
        "applied": applied,
        "blocked_reasons": blocked_reasons,
        "live_execution_authority": false,
        "requires_explicit_adaptive_plan": true,
        "policy_schema_version": CONTEXTUAL_POLICY_SCHEMA_VERSION,
        "rollback_endpoint": adjustment_id
            .as_ref()
            .map(|id| format!("/api/v1/adaptive-fusion/policies/{id}/rollback")),
    })
}

fn rollback_result(
    snapshot: &AdaptivePolicySnapshot,
    status: &str,
    rolled_back: bool,
    blocked_reasons: Vec<String>,
) -> Value {
    json!({
        "schema_version": ROLLBACK_RESULT_SCHEMA_VERSION,
        "adjustment_id": snapshot.adjustment_id,
        "snapshot_id": snapshot.snapshot_id,
        "policy_key": snapshot.policy_key,
        "candidate_id": snapshot.candidate_id,
        "status": status,
        "rolled_back": rolled_back,
        "blocked_reasons": blocked_reasons,
    })
}
