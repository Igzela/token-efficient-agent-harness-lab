use rusqlite::{params, Row};
use serde_json::{json, Value};

use super::{append_audit_locked, collect_values, str_at, DatabaseConnection, LocalProductStore};
use crate::feedback::{
    serialize_candidate_to_proposal_request, AutoAdjustmentGuard, AutoAdjustmentPolicy,
    GeneratedProposalCandidate, PolicySnapshotRecord, ProposalValidator,
};

const AUTO_ADJUSTMENT_RESULT_SCHEMA_VERSION: &str = "auto_adjustment_apply_result.v1";
const AUTO_ADJUSTMENT_ROLLBACK_SCHEMA_VERSION: &str = "auto_adjustment_rollback_result.v1";

impl LocalProductStore {
    pub fn apply_auto_adjustment(&self, request: &Value, actor: &str) -> Result<Value, String> {
        let confirm = request
            .get("confirm_auto_adjustment")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !confirm {
            self.audit_auto_adjustment_rejected(
                actor,
                "auto_adjustment.apply.rejected",
                "pending",
                &json!({
                    "confirmation_flag": false,
                    "blocked_reasons": ["confirm_auto_adjustment is required"],
                    "actor": actor,
                    "source": "auto_adjustment",
                }),
            )?;
            return Err("confirm_auto_adjustment is required".to_string());
        }

        let guard = AutoAdjustmentGuard::from_env();
        if guard.mode != "active" {
            self.audit_auto_adjustment_rejected(
                actor,
                "auto_adjustment.apply.rejected",
                "pending",
                &json!({
                    "confirmation_flag": true,
                    "mode": guard.mode,
                    "blocked_reasons": guard.blocked_reasons,
                    "actor": actor,
                    "source": "auto_adjustment",
                }),
            )?;
            return Err("active auto-adjustment gates are not enabled".to_string());
        }

        let candidate_id = request.get("candidate_id").and_then(Value::as_str);
        let candidate = self.select_auto_adjustment_candidate(candidate_id)?;
        let mut blocked_reasons = Vec::new();
        let policy_key = policy_key(&candidate);

        let validation = ProposalValidator::validate_generated(&candidate);
        if !validation.valid {
            blocked_reasons.extend(validation.errors);
        }

        let policy_decision = AutoAdjustmentPolicy::default().evaluate(&candidate);
        if !policy_decision.eligible {
            blocked_reasons.extend(policy_decision.blocked_reasons.clone());
        }
        blocked_reasons
            .extend(self.active_auto_adjustment_conflicts(&policy_key, &candidate.candidate_id)?);

        if !blocked_reasons.is_empty() {
            self.audit_auto_adjustment_rejected(
                actor,
                "auto_adjustment.apply.rejected",
                &candidate.candidate_id,
                &json!({
                    "candidate_id": candidate.candidate_id,
                    "policy_key": policy_key,
                    "target_tier": candidate.target_tier,
                    "actor": actor,
                    "confirmation_flag": true,
                    "blocked_reasons": blocked_reasons,
                    "source": "auto_adjustment",
                }),
            )?;
            return Ok(apply_result(
                None,
                None,
                None,
                &candidate,
                "blocked",
                false,
                blocked_reasons,
            ));
        }

        let active_policy_before = self.active_policy_value()?;
        let proposal_request = serialize_candidate_to_proposal_request(&candidate);
        let proposal = self.create_policy_proposal(&proposal_request, actor)?;
        let proposal_id = str_at(&proposal, &["proposal_id"])
            .ok_or_else(|| "created proposal missing proposal_id".to_string())?
            .to_string();
        let snapshot =
            self.create_policy_snapshot(actor, &candidate, &proposal_id, active_policy_before)?;
        let snapshot_id = snapshot.snapshot_id.clone();
        let adjustment_id = snapshot.adjustment_id.clone();
        let approved =
            self.approve_policy_proposal(&proposal_id, actor, Some("auto_adjustment"), true)?;

        self.audit_auto_adjustment_rejected(
            actor,
            "auto_adjustment.apply.accepted",
            &adjustment_id,
            &json!({
                "adjustment_id": adjustment_id,
                "snapshot_id": snapshot_id,
                "policy_key": snapshot.policy_key,
                "target_tier": snapshot.target_tier,
                "candidate_id": snapshot.candidate_id,
                "proposal_id": proposal_id,
                "actor": actor,
                "confirmation_flag": true,
                "blocked_reasons": [],
                "source": "auto_adjustment",
            }),
        )?;

        Ok(apply_result(
            Some(adjustment_id),
            Some(snapshot_id),
            Some(proposal_id),
            &candidate,
            str_at(&approved, &["status"]).unwrap_or("active"),
            true,
            Vec::new(),
        ))
    }

    pub fn rollback_auto_adjustment(
        &self,
        adjustment_id: &str,
        request: &Value,
        actor: &str,
    ) -> Result<Value, String> {
        let confirm = request
            .get("confirm_auto_adjustment_rollback")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !confirm {
            self.audit_auto_adjustment_rejected(
                actor,
                "auto_adjustment.rollback.rejected",
                adjustment_id,
                &json!({
                    "adjustment_id": adjustment_id,
                    "confirmation_flag": false,
                    "blocked_reasons": ["confirm_auto_adjustment_rollback is required"],
                    "actor": actor,
                    "source": "auto_adjustment",
                }),
            )?;
            return Err("confirm_auto_adjustment_rollback is required".to_string());
        }

        let mut snapshot = match self.get_policy_snapshot(adjustment_id)? {
            Some(snapshot) => snapshot,
            None => {
                self.audit_auto_adjustment_rejected(
                    actor,
                    "auto_adjustment.rollback.rejected",
                    adjustment_id,
                    &json!({
                        "adjustment_id": adjustment_id,
                        "actor": actor,
                        "confirmation_flag": true,
                        "blocked_reasons": [format!("auto-adjustment not found: {adjustment_id}")],
                        "source": "auto_adjustment",
                    }),
                )?;
                return Err(format!("auto-adjustment not found: {adjustment_id}"));
            }
        };
        let mut blocked_reasons = Vec::new();
        if snapshot.status != "active" {
            blocked_reasons.push(format!(
                "auto-adjustment {adjustment_id} is not active: {}",
                snapshot.status
            ));
        }
        if !snapshot.hash_is_valid() {
            blocked_reasons.push("snapshot safety hash mismatch".to_string());
        }
        blocked_reasons.extend(self.rollback_proposal_state_reasons(&snapshot)?);
        if !blocked_reasons.is_empty() {
            self.audit_auto_adjustment_rejected(
                actor,
                "auto_adjustment.rollback.rejected",
                adjustment_id,
                &json!({
                    "adjustment_id": adjustment_id,
                    "snapshot_id": snapshot.snapshot_id,
                    "policy_key": snapshot.policy_key,
                    "target_tier": snapshot.target_tier,
                    "candidate_id": snapshot.candidate_id,
                    "proposal_id": snapshot.proposal_id,
                    "actor": actor,
                    "confirmation_flag": true,
                    "blocked_reasons": blocked_reasons,
                    "source": "auto_adjustment",
                }),
            )?;
            return Ok(rollback_result(
                &snapshot,
                "blocked",
                false,
                blocked_reasons,
            ));
        }

        self.rollback_policy_proposal(
            &snapshot.proposal_id,
            actor,
            Some("auto_adjustment_rollback"),
            true,
        )?;
        self.restore_superseded_policy_ids(&snapshot)?;
        snapshot.status = "rolled_back".to_string();
        snapshot.updated_at = self.now();
        self.update_policy_snapshot_status(&snapshot)?;

        self.audit_auto_adjustment_rejected(
            actor,
            "auto_adjustment.rollback.accepted",
            adjustment_id,
            &json!({
                "adjustment_id": adjustment_id,
                "snapshot_id": snapshot.snapshot_id,
                "policy_key": snapshot.policy_key,
                "target_tier": snapshot.target_tier,
                "candidate_id": snapshot.candidate_id,
                "proposal_id": snapshot.proposal_id,
                "actor": actor,
                "confirmation_flag": true,
                "blocked_reasons": [],
                "source": "auto_adjustment",
            }),
        )?;

        Ok(rollback_result(&snapshot, "rolled_back", true, Vec::new()))
    }

    pub fn active_auto_adjustments(&self) -> Result<Vec<Value>, String> {
        self.list_policy_snapshots(Some("active"))
    }

    pub fn get_auto_adjustment(&self, adjustment_id: &str) -> Result<Option<Value>, String> {
        Ok(self
            .get_policy_snapshot(adjustment_id)?
            .map(|snapshot| serde_json::to_value(snapshot).unwrap_or(Value::Null)))
    }

    fn active_policy_value(&self) -> Result<Value, String> {
        self.active_routing_policy()?
            .map(|policy| serde_json::to_value(policy).map_err(|e| e.to_string()))
            .transpose()
            .map(|value| value.unwrap_or(Value::Null))
    }

    fn select_auto_adjustment_candidate(
        &self,
        candidate_id: Option<&str>,
    ) -> Result<GeneratedProposalCandidate, String> {
        let candidates = self.generated_proposal_candidates(50)?;
        match candidate_id {
            Some(id) => candidates
                .into_iter()
                .find(|candidate| candidate.candidate_id == id)
                .ok_or_else(|| format!("candidate not found: {id}")),
            None => candidates
                .into_iter()
                .next()
                .ok_or_else(|| "no generated candidate available".to_string()),
        }
    }

    fn active_auto_adjustment_conflicts(
        &self,
        policy_key: &str,
        candidate_id: &str,
    ) -> Result<Vec<String>, String> {
        let active = self.list_policy_snapshots(Some("active"))?;
        let mut reasons = Vec::new();
        for snapshot in active {
            if snapshot["policy_key"].as_str() == Some(policy_key) {
                reasons.push(format!(
                    "active auto-adjustment already exists for policy_key {policy_key}"
                ));
            }
            if snapshot["candidate_id"].as_str() == Some(candidate_id) {
                reasons.push(format!("candidate_id {candidate_id} is already active"));
            }
        }
        reasons.sort();
        reasons.dedup();
        Ok(reasons)
    }

    fn rollback_proposal_state_reasons(
        &self,
        snapshot: &PolicySnapshotRecord,
    ) -> Result<Vec<String>, String> {
        let proposal = self.get_policy_proposal(&snapshot.proposal_id)?;
        let Some(proposal) = proposal else {
            return Ok(vec![format!(
                "linked proposal {} is missing",
                snapshot.proposal_id
            )]);
        };
        let status = str_at(&proposal, &["status"]).unwrap_or("");
        if status != "active" {
            return Ok(vec![format!(
                "linked proposal {} is not active: {status}",
                snapshot.proposal_id
            )]);
        }
        Ok(Vec::new())
    }

    fn create_policy_snapshot(
        &self,
        actor: &str,
        candidate: &GeneratedProposalCandidate,
        proposal_id: &str,
        active_policy_before: Value,
    ) -> Result<PolicySnapshotRecord, String> {
        let now = self.now();
        let restore_active_proposal_ids =
            self.active_proposal_ids_for_key(&policy_key(candidate))?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let sequence = next_snapshot_sequence(conn)?;
                let adjustment_id = format!("auto-adjustment-{sequence:04}");
                let snapshot_id = format!("policy-snapshot-{sequence:04}");
                let record = PolicySnapshotRecord::new(
                    adjustment_id,
                    snapshot_id,
                    now.clone(),
                    "auto_adjustment".to_string(),
                    actor.to_string(),
                    candidate,
                    proposal_id.to_string(),
                    active_policy_before,
                    restore_active_proposal_ids.clone(),
                );
                insert_snapshot_sqlite(conn, &record)?;
                append_audit_locked(
                    conn,
                    &self.now(),
                    actor,
                    "auto_adjustment.snapshot.created",
                    &record.adjustment_id,
                    &json!({
                        "adjustment_id": record.adjustment_id,
                        "snapshot_id": record.snapshot_id,
                        "policy_key": record.policy_key,
                        "target_tier": record.target_tier,
                        "candidate_id": record.candidate_id,
                        "proposal_id": record.proposal_id,
                        "actor": actor,
                        "confirmation_flag": true,
                        "blocked_reasons": [],
                        "source": "auto_adjustment",
                    }),
                )?;
                Ok(record)
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let sequence = next_pg_snapshot_sequence(client)?;
                let adjustment_id = format!("auto-adjustment-{sequence:04}");
                let snapshot_id = format!("policy-snapshot-{sequence:04}");
                let record = PolicySnapshotRecord::new(
                    adjustment_id,
                    snapshot_id,
                    now.clone(),
                    "auto_adjustment".to_string(),
                    actor.to_string(),
                    candidate,
                    proposal_id.to_string(),
                    active_policy_before,
                    restore_active_proposal_ids.clone(),
                );
                insert_snapshot_pg(client, &record)?;
                let details = json!({
                    "adjustment_id": record.adjustment_id,
                    "snapshot_id": record.snapshot_id,
                    "policy_key": record.policy_key,
                    "target_tier": record.target_tier,
                    "candidate_id": record.candidate_id,
                    "proposal_id": record.proposal_id,
                    "actor": actor,
                    "confirmation_flag": true,
                    "blocked_reasons": [],
                    "source": "auto_adjustment",
                })
                .to_string();
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, 'auto_adjustment.snapshot.created', $3, $4)",
                        &[&self.now(), &actor, &record.adjustment_id, &details],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(record)
            }),
        }
    }

    fn get_policy_snapshot(
        &self,
        adjustment_id: &str,
    ) -> Result<Option<PolicySnapshotRecord>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(SNAPSHOT_SELECT_SQL)
                    .map_err(|e| e.to_string())?;
                let mut rows = stmt
                    .query_map(params![adjustment_id], snapshot_row)
                    .map_err(|e| e.to_string())?;
                match rows.next() {
                    Some(Ok(value)) => Ok(Some(value)),
                    Some(Err(err)) => Err(err.to_string()),
                    None => Ok(None),
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(&SNAPSHOT_SELECT_SQL.replace("?1", "$1"), &[&adjustment_id])
                    .map_err(|e| e.to_string())?;
                rows.first().map(pg_snapshot_row).transpose()
            }),
        }
    }

    fn list_policy_snapshots(&self, status: Option<&str>) -> Result<Vec<Value>, String> {
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                if let Some(status) = status {
                    let mut stmt = conn
                        .prepare(SNAPSHOT_LIST_STATUS_SQL)
                        .map_err(|e| e.to_string())?;
                    let rows = stmt
                        .query_map(params![status], snapshot_value_row)
                        .map_err(|e| e.to_string())?;
                    collect_values(rows)
                } else {
                    let mut stmt = conn.prepare(SNAPSHOT_LIST_SQL).map_err(|e| e.to_string())?;
                    let rows = stmt
                        .query_map([], snapshot_value_row)
                        .map_err(|e| e.to_string())?;
                    collect_values(rows)
                }
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = if let Some(status) = status {
                    client
                        .query(&SNAPSHOT_LIST_STATUS_SQL.replace("?1", "$1"), &[&status])
                        .map_err(|e| e.to_string())?
                } else {
                    client
                        .query(SNAPSHOT_LIST_SQL, &[])
                        .map_err(|e| e.to_string())?
                };
                rows.iter()
                    .map(pg_snapshot_row)
                    .map(|row| {
                        row.map(|record| serde_json::to_value(record).unwrap_or(Value::Null))
                    })
                    .collect()
            }),
        }
    }

    fn update_policy_snapshot_status(&self, snapshot: &PolicySnapshotRecord) -> Result<(), String> {
        let snapshot_json = serde_json::to_string(snapshot).map_err(|e| e.to_string())?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                conn.execute(
                    "UPDATE controlled_loop_policy_snapshots
                     SET status = ?1, updated_at = ?2, snapshot_json = ?3
                     WHERE adjustment_id = ?4",
                    params![
                        snapshot.status,
                        snapshot.updated_at,
                        snapshot_json,
                        snapshot.adjustment_id
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "UPDATE controlled_loop_policy_snapshots
                         SET status = $1, updated_at = $2, snapshot_json = $3
                         WHERE adjustment_id = $4",
                        &[
                            &snapshot.status,
                            &snapshot.updated_at,
                            &snapshot_json,
                            &snapshot.adjustment_id,
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }

    fn restore_superseded_policy_ids(&self, snapshot: &PolicySnapshotRecord) -> Result<(), String> {
        let restore_ids = snapshot
            .rollback_target
            .get("restore_active_proposal_ids")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if restore_ids.is_empty() {
            return Ok(());
        }
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                for proposal_id in restore_ids.iter().filter_map(Value::as_str) {
                    conn.execute(
                        "UPDATE controlled_loop_policy_proposals
                         SET status = 'active', updated_at = ?1
                         WHERE proposal_id = ?2 AND status = 'superseded'",
                        params![self.now(), proposal_id],
                    )
                    .map_err(|e| e.to_string())?;
                }
                Ok(())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                for proposal_id in restore_ids.iter().filter_map(Value::as_str) {
                    client
                        .execute(
                            "UPDATE controlled_loop_policy_proposals
                             SET status = 'active', updated_at = $1
                             WHERE proposal_id = $2 AND status = 'superseded'",
                            &[&self.now(), &proposal_id],
                        )
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            }),
        }
    }

    fn active_proposal_ids_for_key(&self, policy_key: &str) -> Result<Vec<String>, String> {
        let Some((task_domain, task_intent)) = policy_key.split_once('_') else {
            return Ok(Vec::new());
        };
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT proposal_id FROM controlled_loop_policy_proposals
                         WHERE status = 'active' AND task_domain = ?1 AND task_intent = ?2
                         ORDER BY proposal_sequence ASC",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![task_domain, task_intent], |row| {
                        row.get::<_, String>(0)
                    })
                    .map_err(|e| e.to_string())?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let rows = client
                    .query(
                        "SELECT proposal_id FROM controlled_loop_policy_proposals
                         WHERE status = 'active' AND task_domain = $1 AND task_intent = $2
                         ORDER BY proposal_sequence ASC",
                        &[&task_domain, &task_intent],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(rows.iter().map(|row| row.get(0)).collect())
            }),
        }
    }

    fn audit_auto_adjustment_rejected(
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
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                client
                    .execute(
                        "INSERT INTO audit_log (created_at, actor, action, resource, details_json)
                         VALUES ($1, $2, $3, $4, $5)",
                        &[
                            &self.now(),
                            &actor,
                            &action,
                            &resource,
                            &details.to_string(),
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                Ok(())
            }),
        }
    }
}

const SNAPSHOT_SELECT_SQL: &str =
    "SELECT snapshot_sequence, adjustment_id, snapshot_id, created_at,
updated_at, status, actor, created_by, source, candidate_id, proposal_id, policy_key, target_tier,
active_policy_before_json, rollback_target_json, evidence_ids_json, safety_hash, snapshot_json
FROM controlled_loop_policy_snapshots WHERE adjustment_id = ?1 LIMIT 1";

const SNAPSHOT_LIST_SQL: &str = "SELECT snapshot_sequence, adjustment_id, snapshot_id, created_at,
updated_at, status, actor, created_by, source, candidate_id, proposal_id, policy_key, target_tier,
active_policy_before_json, rollback_target_json, evidence_ids_json, safety_hash, snapshot_json
FROM controlled_loop_policy_snapshots ORDER BY snapshot_sequence DESC";

const SNAPSHOT_LIST_STATUS_SQL: &str =
    "SELECT snapshot_sequence, adjustment_id, snapshot_id, created_at,
updated_at, status, actor, created_by, source, candidate_id, proposal_id, policy_key, target_tier,
active_policy_before_json, rollback_target_json, evidence_ids_json, safety_hash, snapshot_json
FROM controlled_loop_policy_snapshots WHERE status = ?1 ORDER BY snapshot_sequence DESC";

fn policy_key(candidate: &GeneratedProposalCandidate) -> String {
    format!("{}_{}", candidate.task_domain, candidate.task_intent)
}

fn apply_result(
    adjustment_id: Option<String>,
    snapshot_id: Option<String>,
    proposal_id: Option<String>,
    candidate: &GeneratedProposalCandidate,
    status: &str,
    applied: bool,
    blocked_reasons: Vec<String>,
) -> Value {
    json!({
        "schema_version": AUTO_ADJUSTMENT_RESULT_SCHEMA_VERSION,
        "adjustment_id": adjustment_id,
        "snapshot_id": snapshot_id,
        "proposal_id": proposal_id,
        "candidate_id": candidate.candidate_id,
        "policy_key": policy_key(candidate),
        "target_tier": candidate.target_tier,
        "status": status,
        "applied": applied,
        "blocked_reasons": blocked_reasons,
        "rollback_endpoint": adjustment_id
            .as_ref()
            .map(|id| format!("/api/v1/auto-adjustments/{id}/rollback")),
    })
}

fn rollback_result(
    snapshot: &PolicySnapshotRecord,
    status: &str,
    rolled_back: bool,
    blocked_reasons: Vec<String>,
) -> Value {
    json!({
        "schema_version": AUTO_ADJUSTMENT_ROLLBACK_SCHEMA_VERSION,
        "adjustment_id": snapshot.adjustment_id,
        "snapshot_id": snapshot.snapshot_id,
        "proposal_id": snapshot.proposal_id,
        "policy_key": snapshot.policy_key,
        "target_tier": snapshot.target_tier,
        "status": status,
        "rolled_back": rolled_back,
        "blocked_reasons": blocked_reasons,
    })
}

fn insert_snapshot_sqlite(
    conn: &rusqlite::Connection,
    record: &PolicySnapshotRecord,
) -> Result<(), String> {
    let snapshot_json = serde_json::to_string(record).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO controlled_loop_policy_snapshots
         (adjustment_id, snapshot_id, created_at, updated_at, status, actor, created_by, source,
          candidate_id, proposal_id, policy_key, target_tier, active_policy_before_json,
          rollback_target_json, evidence_ids_json, safety_hash, snapshot_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            record.adjustment_id,
            record.snapshot_id,
            record.created_at,
            record.updated_at,
            record.status,
            record.actor,
            record.created_by,
            record.source,
            record.candidate_id,
            record.proposal_id,
            record.policy_key,
            record.target_tier,
            record.active_policy_before.to_string(),
            record.rollback_target.to_string(),
            serde_json::to_string(&record.evidence_ids).map_err(|e| e.to_string())?,
            record.safety_hash,
            snapshot_json,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(feature = "pg")]
fn insert_snapshot_pg(
    client: &mut postgres::Client,
    record: &PolicySnapshotRecord,
) -> Result<(), String> {
    let snapshot_json = serde_json::to_string(record).map_err(|e| e.to_string())?;
    let evidence_ids_json =
        serde_json::to_string(&record.evidence_ids).map_err(|e| e.to_string())?;
    client
        .execute(
            "INSERT INTO controlled_loop_policy_snapshots
             (adjustment_id, snapshot_id, created_at, updated_at, status, actor, created_by, source,
              candidate_id, proposal_id, policy_key, target_tier, active_policy_before_json,
              rollback_target_json, evidence_ids_json, safety_hash, snapshot_json)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)",
            &[
                &record.adjustment_id,
                &record.snapshot_id,
                &record.created_at,
                &record.updated_at,
                &record.status,
                &record.actor,
                &record.created_by,
                &record.source,
                &record.candidate_id,
                &record.proposal_id,
                &record.policy_key,
                &record.target_tier,
                &record.active_policy_before.to_string(),
                &record.rollback_target.to_string(),
                &evidence_ids_json,
                &record.safety_hash,
                &snapshot_json,
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn next_snapshot_sequence(conn: &rusqlite::Connection) -> Result<i64, String> {
    conn.query_row(
        "SELECT COALESCE(MAX(snapshot_sequence), 0) + 1 FROM controlled_loop_policy_snapshots",
        [],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

#[cfg(feature = "pg")]
fn next_pg_snapshot_sequence(client: &mut postgres::Client) -> Result<i64, String> {
    let row = client
        .query_one(
            "SELECT COALESCE(MAX(snapshot_sequence), 0) + 1 FROM controlled_loop_policy_snapshots",
            &[],
        )
        .map_err(|e| e.to_string())?;
    Ok(row.get(0))
}

fn snapshot_value_row(row: &Row<'_>) -> rusqlite::Result<Value> {
    snapshot_row(row).map(|record| serde_json::to_value(record).unwrap_or(Value::Null))
}

fn snapshot_row(row: &Row<'_>) -> rusqlite::Result<PolicySnapshotRecord> {
    let snapshot_json: String = row.get(17)?;
    let mut record: PolicySnapshotRecord =
        serde_json::from_str(&snapshot_json).unwrap_or_else(|_| fallback_snapshot_row(row));
    record.status = row.get(5)?;
    record.updated_at = row.get(4)?;
    record.safety_hash = row.get(16)?;
    Ok(record)
}

fn fallback_snapshot_row(row: &Row<'_>) -> PolicySnapshotRecord {
    let evidence_ids_json: String = row.get(15).unwrap_or_else(|_| "[]".to_string());
    PolicySnapshotRecord {
        schema_version: crate::feedback::POLICY_SNAPSHOT_SCHEMA_VERSION.to_string(),
        adjustment_id: row.get(1).unwrap_or_default(),
        snapshot_id: row.get(2).unwrap_or_default(),
        created_at: row.get(3).unwrap_or_default(),
        updated_at: row.get(4).unwrap_or_default(),
        status: row.get(5).unwrap_or_default(),
        actor: row.get(6).unwrap_or_default(),
        created_by: row.get(7).unwrap_or_default(),
        source: row.get(8).unwrap_or_default(),
        candidate_id: row.get(9).unwrap_or_default(),
        proposal_id: row.get(10).unwrap_or_default(),
        policy_key: row.get(11).unwrap_or_default(),
        target_tier: row.get(12).unwrap_or_default(),
        active_policy_before: serde_json::from_str(&row.get::<_, String>(13).unwrap_or_default())
            .unwrap_or(Value::Null),
        rollback_target: serde_json::from_str(&row.get::<_, String>(14).unwrap_or_default())
            .unwrap_or(Value::Null),
        evidence_ids: serde_json::from_str(&evidence_ids_json).unwrap_or_default(),
        safety_hash: row.get(16).unwrap_or_default(),
    }
}

#[cfg(feature = "pg")]
fn pg_snapshot_row(row: &postgres::Row) -> Result<PolicySnapshotRecord, String> {
    let snapshot_json: String = row.get(17);
    let mut record: PolicySnapshotRecord =
        serde_json::from_str(&snapshot_json).map_err(|e| e.to_string())?;
    record.status = row.get(5);
    record.updated_at = row.get(4);
    record.safety_hash = row.get(16);
    Ok(record)
}
