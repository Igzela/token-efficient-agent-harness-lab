use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::budget_manager::{
    BudgetAnomalyFinding, BudgetAnomalySeverity, BudgetConfidenceLevel, BudgetEvidenceOutcome,
};

use super::workflow_runs::require_run_execution_owner;
use super::{append_audit_locked, DatabaseConnection, LocalProductStore};

pub const BUDGET_AUTO_PAUSE_POLICY_SCHEMA_VERSION: &str = "budget_auto_pause_policy.v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BudgetAutoPausePolicy {
    pub schema_version: String,
    pub enabled: bool,
    pub minimum_confidence_score: f64,
    pub maximum_freshness_seconds: u64,
    pub require_critical_severity: bool,
}

impl Default for BudgetAutoPausePolicy {
    fn default() -> Self {
        Self {
            schema_version: BUDGET_AUTO_PAUSE_POLICY_SCHEMA_VERSION.to_string(),
            enabled: false,
            minimum_confidence_score: 0.9,
            maximum_freshness_seconds: 300,
            require_critical_severity: true,
        }
    }
}

impl BudgetAutoPausePolicy {
    fn validate(&self) -> Result<(), String> {
        if self.schema_version != BUDGET_AUTO_PAUSE_POLICY_SCHEMA_VERSION {
            return Err("unsupported budget auto-pause policy version".to_string());
        }
        if !self.minimum_confidence_score.is_finite()
            || !(0.0..=1.0).contains(&self.minimum_confidence_score)
        {
            return Err("budget auto-pause confidence threshold must be within [0,1]".to_string());
        }
        if self.maximum_freshness_seconds == 0 || self.maximum_freshness_seconds > 86_400 {
            return Err("budget auto-pause freshness must be within 1..=86400 seconds".to_string());
        }
        Ok(())
    }
}

impl LocalProductStore {
    pub fn apply_budget_auto_pause(
        &self,
        artifact_id: &str,
        run_id: &str,
        policy: &BudgetAutoPausePolicy,
        actor: &str,
    ) -> Result<Value, String> {
        policy.validate()?;
        if !policy.enabled {
            return Err("budget auto-pause policy is disabled".to_string());
        }
        let artifact = self
            .get_budget_evidence_artifact(artifact_id)?
            .ok_or_else(|| format!("budget evidence artifact not found: {artifact_id}"))?;
        if artifact["artifact_kind"] != "anomaly" {
            return Err("budget auto-pause requires anomaly evidence".to_string());
        }
        let finding: BudgetAnomalyFinding = serde_json::from_value(artifact["evidence"].clone())
            .map_err(|error| format!("invalid budget anomaly evidence: {error}"))?;
        finding.validate()?;
        validate_pause_eligibility(&finding, run_id, policy)?;
        let now = self.now();
        let generated = chrono::DateTime::parse_from_rfc3339(&finding.window.generated_at)
            .map_err(|_| "budget anomaly generated_at is invalid".to_string())?;
        let current = chrono::DateTime::parse_from_rfc3339(&now)
            .map_err(|_| "store clock is not RFC3339".to_string())?;
        let age_seconds = (current - generated).num_seconds();
        if age_seconds < -1 || age_seconds.max(0) as u64 > policy.maximum_freshness_seconds {
            return Err("budget anomaly is stale at mutation time".to_string());
        }
        let evidence_sha256 = finding.evidence_sha256.clone();
        let decision_id = pause_decision_id(run_id, artifact_id, &evidence_sha256);
        let anomaly_kind =
            serde_json::to_value(finding.anomaly_kind.as_ref().expect("eligibility checked"))
                .map_err(|error| error.to_string())?
                .as_str()
                .ok_or_else(|| "budget anomaly kind is invalid".to_string())?
                .to_string();
        let cause = format!("{anomaly_kind}:{}", finding.reason_codes.join(","));
        let policy_json = serde_json::to_string(policy).map_err(|error| error.to_string())?;
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = conn.unchecked_transaction().map_err(|error| error.to_string())?;
                if let Some(existing) = sqlite_pause_decision(&tx, run_id, artifact_id)? {
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(existing);
                }
                let current_owner: Option<Option<String>> = tx
                    .query_row(
                        "SELECT pause_reason FROM workflow_runs WHERE run_id = ?1",
                        params![run_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                let current_owner = current_owner
                    .ok_or_else(|| format!("workflow run not found: {run_id}"))?;
                require_run_execution_owner(run_id, current_owner.as_deref(), None)?;
                tx.execute(
                    "INSERT INTO budget_pause_decisions
                     (decision_id, run_id, artifact_id, evidence_sha256, state, cause, policy_json, actor, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, 'paused', ?5, ?6, ?7, ?8, ?8)",
                    params![decision_id, run_id, artifact_id, evidence_sha256, cause, policy_json, actor, now],
                )
                .map_err(|error| error.to_string())?;
                append_audit_locked(
                    &tx,
                    &now,
                    actor,
                    "budget.auto_pause.applied",
                    run_id,
                    &json!({"decision_id": decision_id, "run_id": run_id, "artifact_id": artifact_id, "evidence_sha256": evidence_sha256, "cause": cause}),
                )?;
                let pause_reason = format!("budget_auto_pause:{decision_id}");
                let updated = tx.execute(
                    "UPDATE workflow_runs SET pause_reason = ?1, updated_at = ?2 WHERE run_id = ?3",
                    params![pause_reason, now, run_id],
                )
                .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err(format!("workflow run pause owner unavailable: {run_id}"));
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(pause_decision_value(&decision_id, run_id, artifact_id, &evidence_sha256, "paused", &cause, actor, &now, None))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                if let Some(existing) = pg_pause_decision(&mut tx, run_id, artifact_id)? {
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(existing);
                }
                let current_owner = tx
                    .query_opt(
                        "SELECT pause_reason FROM workflow_runs WHERE run_id = $1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("workflow run not found: {run_id}"))?;
                let current_owner: Option<String> = current_owner.get(0);
                require_run_execution_owner(run_id, current_owner.as_deref(), None)?;
                let inserted = tx.execute(
                    "INSERT INTO budget_pause_decisions
                     (decision_id, run_id, artifact_id, evidence_sha256, state, cause, policy_json, actor, created_at, updated_at)
                     VALUES ($1, $2, $3, $4, 'paused', $5, $6, $7, $8, $8)
                     ON CONFLICT (run_id, artifact_id) DO NOTHING",
                    &[&decision_id, &run_id, &artifact_id, &evidence_sha256, &cause, &policy_json, &actor, &now],
                )
                .map_err(|error| error.to_string())?;
                if inserted == 0 {
                    let existing = pg_pause_decision(&mut tx, run_id, artifact_id)?
                        .ok_or_else(|| "concurrent budget pause decision unavailable".to_string())?;
                    tx.commit().map_err(|error| error.to_string())?;
                    return Ok(existing);
                }
                let details = json!({"decision_id": decision_id, "run_id": run_id, "artifact_id": artifact_id, "evidence_sha256": evidence_sha256, "cause": cause}).to_string();
                tx.execute(
                    "INSERT INTO audit_log (created_at, actor, action, resource, details_json) VALUES ($1, $2, 'budget.auto_pause.applied', $3, $4)",
                    &[&now, &actor, &run_id, &details],
                )
                .map_err(|error| error.to_string())?;
                let pause_reason = format!("budget_auto_pause:{decision_id}");
                let updated = tx.execute(
                    "UPDATE workflow_runs SET pause_reason = $1, updated_at = $2 WHERE run_id = $3",
                    &[&pause_reason, &now, &run_id],
                )
                .map_err(|error| error.to_string())?;
                if updated != 1 {
                    return Err(format!("workflow run pause owner unavailable: {run_id}"));
                }
                tx.commit().map_err(|error| error.to_string())?;
                Ok(pause_decision_value(&decision_id, run_id, artifact_id, &evidence_sha256, "paused", &cause, actor, &now, None))
            }),
        }
    }

    pub fn recover_budget_auto_pause(
        &self,
        run_id: &str,
        recovery: &str,
        reason: &str,
        actor: &str,
    ) -> Result<Value, String> {
        if !matches!(recovery, "resume" | "override") {
            return Err("budget pause recovery must be resume or override".to_string());
        }
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > 512 {
            return Err("budget pause recovery requires a bounded operator reason".to_string());
        }
        let now = self.now();
        match &self.db {
            DatabaseConnection::Sqlite(_) => self.with_conn(|conn| {
                let tx = rusqlite::Transaction::new_unchecked(
                    conn,
                    rusqlite::TransactionBehavior::Immediate,
                )
                .map_err(|error| error.to_string())?;
                let existing = sqlite_active_pause_decision(&tx, run_id)?
                    .ok_or_else(|| format!("active budget pause not found: {run_id}"))?;
                let current_owner: Option<String> = tx
                    .query_row(
                        "SELECT pause_reason FROM workflow_runs WHERE run_id = ?1",
                        params![run_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("workflow run not found: {run_id}"))?;
                require_run_execution_owner(run_id, current_owner.as_deref(), None)?;
                let expected_owner = format!(
                    "budget_auto_pause:{}",
                    existing["decision_id"].as_str().unwrap_or_default()
                );
                if current_owner.as_deref() != Some(expected_owner.as_str()) {
                    return Err(format!("workflow run pause owner unavailable: {run_id}"));
                }
                append_audit_locked(&tx, &now, actor, &format!("budget.auto_pause.{recovery}"), run_id, &json!({"decision_id": existing["decision_id"], "run_id": run_id, "reason": reason, "preserved_cause": existing["cause"], "evidence_sha256": existing["evidence_sha256"]}))?;
                let updated = tx.execute("UPDATE workflow_runs SET pause_reason = NULL, updated_at = ?1 WHERE run_id = ?2", params![now, run_id]).map_err(|error| error.to_string())?;
                if updated != 1 { return Err(format!("workflow run pause owner unavailable: {run_id}")); }
                tx.execute("UPDATE budget_pause_decisions SET state = ?1, recovery_reason = ?2, actor = ?3, updated_at = ?4 WHERE decision_id = ?5", params![recovery, reason, actor, now, existing["decision_id"].as_str()]).map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({"decision_id": existing["decision_id"], "run_id": run_id, "state": recovery, "reason": reason, "cause": existing["cause"], "evidence_sha256": existing["evidence_sha256"], "updated_at": now}))
            }),
            #[cfg(feature = "pg")]
            DatabaseConnection::Pg(_) => self.with_pg_conn(|client| {
                let mut tx = client.transaction().map_err(|error| error.to_string())?;
                let existing = pg_active_pause_decision(&mut tx, run_id)?
                    .ok_or_else(|| format!("active budget pause not found: {run_id}"))?;
                let current_owner = tx
                    .query_opt(
                        "SELECT pause_reason FROM workflow_runs WHERE run_id = $1 FOR UPDATE",
                        &[&run_id],
                    )
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("workflow run not found: {run_id}"))?;
                let current_owner: Option<String> = current_owner.get(0);
                require_run_execution_owner(run_id, current_owner.as_deref(), None)?;
                let expected_owner = format!(
                    "budget_auto_pause:{}",
                    existing["decision_id"].as_str().unwrap_or_default()
                );
                if current_owner.as_deref() != Some(expected_owner.as_str()) {
                    return Err(format!("workflow run pause owner unavailable: {run_id}"));
                }
                let details = json!({"decision_id": existing["decision_id"], "run_id": run_id, "reason": reason, "preserved_cause": existing["cause"], "evidence_sha256": existing["evidence_sha256"]}).to_string();
                let action = format!("budget.auto_pause.{recovery}");
                tx.execute("INSERT INTO audit_log (created_at, actor, action, resource, details_json) VALUES ($1, $2, $3, $4, $5)", &[&now, &actor, &action, &run_id, &details]).map_err(|error| error.to_string())?;
                let updated = tx.execute("UPDATE workflow_runs SET pause_reason = NULL, updated_at = $1 WHERE run_id = $2", &[&now, &run_id]).map_err(|error| error.to_string())?;
                if updated != 1 { return Err(format!("workflow run pause owner unavailable: {run_id}")); }
                tx.execute("UPDATE budget_pause_decisions SET state = $1, recovery_reason = $2, actor = $3, updated_at = $4 WHERE decision_id = $5", &[&recovery, &reason, &actor, &now, &existing["decision_id"].as_str()]).map_err(|error| error.to_string())?;
                tx.commit().map_err(|error| error.to_string())?;
                Ok(json!({"decision_id": existing["decision_id"], "run_id": run_id, "state": recovery, "reason": reason, "cause": existing["cause"], "evidence_sha256": existing["evidence_sha256"], "updated_at": now}))
            }),
        }
    }
}

fn validate_pause_eligibility(
    finding: &BudgetAnomalyFinding,
    run_id: &str,
    policy: &BudgetAutoPausePolicy,
) -> Result<(), String> {
    if finding.scope.run_id.as_deref() != Some(run_id) {
        return Err("budget anomaly run scope does not match pause target".to_string());
    }
    if !matches!(finding.outcome, BudgetEvidenceOutcome::Supported) || !finding.detected {
        return Err("budget anomaly does not support a pause".to_string());
    }
    if !matches!(finding.confidence.level, BudgetConfidenceLevel::High)
        || finding.confidence.score < policy.minimum_confidence_score
    {
        return Err("budget anomaly confidence is below auto-pause policy".to_string());
    }
    if finding.window.freshness_seconds > policy.maximum_freshness_seconds {
        return Err("budget anomaly evidence is stale".to_string());
    }
    if !finding.coverage.pricing_complete {
        return Err("budget anomaly pricing evidence is incomplete".to_string());
    }
    if policy.require_critical_severity
        && !matches!(finding.severity, Some(BudgetAnomalySeverity::Critical))
    {
        return Err("budget anomaly severity is below auto-pause policy".to_string());
    }
    Ok(())
}

fn pause_decision_id(run_id: &str, artifact_id: &str, evidence_sha256: &str) -> String {
    let digest = Sha256::digest(format!("{run_id}\0{artifact_id}\0{evidence_sha256}").as_bytes());
    format!("budget-pause-{digest:x}")
}

fn pause_decision_value(
    decision_id: &str,
    run_id: &str,
    artifact_id: &str,
    evidence_sha256: &str,
    state: &str,
    cause: &str,
    actor: &str,
    updated_at: &str,
    recovery_reason: Option<&str>,
) -> Value {
    json!({"schema_version": "budget_pause_decision.v1", "decision_id": decision_id, "run_id": run_id, "artifact_id": artifact_id, "evidence_sha256": evidence_sha256, "state": state, "cause": cause, "actor": actor, "updated_at": updated_at, "recovery_reason": recovery_reason})
}

fn sqlite_pause_decision(
    conn: &rusqlite::Connection,
    run_id: &str,
    artifact_id: &str,
) -> Result<Option<Value>, String> {
    conn.query_row("SELECT decision_id, evidence_sha256, state, cause, actor, updated_at, recovery_reason FROM budget_pause_decisions WHERE run_id = ?1 AND artifact_id = ?2", params![run_id, artifact_id], |row| Ok(pause_decision_value(&row.get::<_, String>(0)?, run_id, artifact_id, &row.get::<_, String>(1)?, &row.get::<_, String>(2)?, &row.get::<_, String>(3)?, &row.get::<_, String>(4)?, &row.get::<_, String>(5)?, row.get::<_, Option<String>>(6)?.as_deref()))).optional().map_err(|error| error.to_string())
}

fn sqlite_active_pause_decision(
    conn: &rusqlite::Connection,
    run_id: &str,
) -> Result<Option<Value>, String> {
    conn.query_row("SELECT decision_id, artifact_id, evidence_sha256, state, cause, actor, updated_at, recovery_reason FROM budget_pause_decisions WHERE run_id = ?1 AND state = 'paused' ORDER BY created_at DESC LIMIT 1", params![run_id], |row| Ok(pause_decision_value(&row.get::<_, String>(0)?, run_id, &row.get::<_, String>(1)?, &row.get::<_, String>(2)?, &row.get::<_, String>(3)?, &row.get::<_, String>(4)?, &row.get::<_, String>(5)?, &row.get::<_, String>(6)?, row.get::<_, Option<String>>(7)?.as_deref()))).optional().map_err(|error| error.to_string())
}

#[cfg(feature = "pg")]
fn pg_pause_decision(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
    artifact_id: &str,
) -> Result<Option<Value>, String> {
    tx.query_opt("SELECT decision_id, evidence_sha256, state, cause, actor, updated_at, recovery_reason FROM budget_pause_decisions WHERE run_id = $1 AND artifact_id = $2 FOR UPDATE", &[&run_id, &artifact_id]).map_err(|error| error.to_string()).map(|row| row.map(|row| {
        let decision_id: String = row.get(0); let evidence: String = row.get(1); let state: String = row.get(2); let cause: String = row.get(3); let actor: String = row.get(4); let updated: String = row.get(5); let recovery: Option<String> = row.get(6);
        pause_decision_value(&decision_id, run_id, artifact_id, &evidence, &state, &cause, &actor, &updated, recovery.as_deref())
    }))
}

#[cfg(feature = "pg")]
fn pg_active_pause_decision(
    tx: &mut postgres::Transaction<'_>,
    run_id: &str,
) -> Result<Option<Value>, String> {
    tx.query_opt("SELECT decision_id, artifact_id, evidence_sha256, state, cause, actor, updated_at, recovery_reason FROM budget_pause_decisions WHERE run_id = $1 AND state = 'paused' ORDER BY created_at DESC LIMIT 1 FOR UPDATE", &[&run_id]).map_err(|error| error.to_string()).map(|row| row.map(|row| {
        let decision_id: String = row.get(0); let artifact_id: String = row.get(1); let evidence: String = row.get(2); let state: String = row.get(3); let cause: String = row.get(4); let actor: String = row.get(5); let updated: String = row.get(6); let recovery: Option<String> = row.get(7);
        pause_decision_value(&decision_id, run_id, &artifact_id, &evidence, &state, &cause, &actor, &updated, recovery.as_deref())
    }))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::tempdir;

    use crate::budget_manager::{
        BudgetAnomalyKind, BudgetAnomalyMeasurement, BudgetConfidence, BudgetEvidenceCoverage,
        BudgetEvidenceReference, BudgetEvidenceScope, BudgetEvidenceWindow,
    };

    use super::*;

    fn finding(id: &str, run_id: &str) -> BudgetAnomalyFinding {
        let mut finding = BudgetAnomalyFinding {
            schema_version: "budget_anomaly_finding.v1".to_string(),
            finding_id: id.to_string(),
            scope: BudgetEvidenceScope {
                run_id: Some(run_id.to_string()),
                ..Default::default()
            },
            outcome: BudgetEvidenceOutcome::Supported,
            window: BudgetEvidenceWindow {
                start_inclusive: "2026-07-11T00:00:00Z".to_string(),
                end_exclusive: "2026-07-11T00:10:00Z".to_string(),
                generated_at: "2026-07-11T00:10:10Z".to_string(),
                freshness_seconds: 10,
                sample_count: 3,
            },
            coverage: BudgetEvidenceCoverage {
                required_dimensions: vec!["run_id".to_string()],
                observed_dimensions: vec!["run_id".to_string()],
                pricing_complete: true,
                duplicate_events: 0,
                missing_fields: vec![],
            },
            confidence: BudgetConfidence {
                level: BudgetConfidenceLevel::High,
                score: 0.99,
                reason_codes: vec!["stable_baseline".to_string()],
            },
            reason_codes: vec!["token_spike".to_string()],
            evidence_references: vec![BudgetEvidenceReference {
                evidence_type: "provider_audit_event".to_string(),
                evidence_id: format!("event-{id}"),
                content_sha256: Some("a".repeat(64)),
            }],
            detected: true,
            anomaly_kind: Some(BudgetAnomalyKind::TokenSpike),
            severity: Some(BudgetAnomalySeverity::Critical),
            measurement: Some(BudgetAnomalyMeasurement {
                metric: "total_tokens".to_string(),
                observed: 200.0,
                baseline: 100.0,
                threshold: 1.5,
                normalized_delta: 1.0,
            }),
            evidence_sha256: String::new(),
        };
        finding.seal().unwrap();
        finding
    }

    fn store_with_run(path: &std::path::Path, run_id: &str) -> LocalProductStore {
        let store =
            LocalProductStore::new_with_clock(path, || "2026-07-11T00:11:00Z".to_string()).unwrap();
        store.with_conn(|conn| conn.execute("INSERT OR IGNORE INTO workflow_runs (run_sequence, run_id, created_at, updated_at, status, workflow_id, boundaries_json, run_json) VALUES (1, ?1, '2026-07-11T00:00:00Z', '2026-07-11T00:00:00Z', 'created', 'wf', '{}', '{}')", params![run_id]).map(|_| ()).map_err(|error| error.to_string())).unwrap();
        store
    }

    fn record(store: &LocalProductStore, finding: &BudgetAnomalyFinding) -> String {
        store
            .record_budget_anomaly_finding(finding, "test")
            .unwrap()["artifact_id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn enabled_policy() -> BudgetAutoPausePolicy {
        BudgetAutoPausePolicy {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn auto_pause_is_default_off_strict_and_idempotently_recoverable() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("pause.db");
        let store = store_with_run(&path, "run-1");
        let artifact_id = record(&store, &finding("finding-1", "run-1"));
        assert!(store
            .apply_budget_auto_pause(
                &artifact_id,
                "run-1",
                &BudgetAutoPausePolicy::default(),
                "operator"
            )
            .unwrap_err()
            .contains("disabled"));
        let first = store
            .apply_budget_auto_pause(&artifact_id, "run-1", &enabled_policy(), "operator")
            .unwrap();
        let repeated = store
            .apply_budget_auto_pause(&artifact_id, "run-1", &enabled_policy(), "operator")
            .unwrap();
        assert_eq!(first, repeated);
        assert!(store
            .update_run_pause_reason("run-1", None)
            .unwrap_err()
            .contains("audited recovery"));
        assert!(store
            .recover_budget_auto_pause("run-1", "resume", "", "operator")
            .is_err());
        let recovered = store
            .recover_budget_auto_pause("run-1", "resume", "reviewed evidence", "operator")
            .unwrap();
        assert_eq!(recovered["state"], "resume");
        let after_recovery = store
            .apply_budget_auto_pause(&artifact_id, "run-1", &enabled_policy(), "operator")
            .unwrap();
        assert_eq!(after_recovery["state"], "resume");
        let second_artifact = record(&store, &finding("finding-2", "run-1"));
        store
            .apply_budget_auto_pause(&second_artifact, "run-1", &enabled_policy(), "operator")
            .unwrap();
        let overridden = store
            .recover_budget_auto_pause(
                "run-1",
                "override",
                "operator accepts bounded risk",
                "operator",
            )
            .unwrap();
        assert_eq!(overridden["state"], "override");
        drop(store);
        let reopened = LocalProductStore::new(&path).unwrap();
        let run = reopened.get_workflow_run("run-1").unwrap().unwrap();
        assert!(run["pause_reason"].is_null());
        let audit = reopened.audit_events(100).unwrap();
        assert!(audit
            .iter()
            .any(|event| event["action"] == "budget.auto_pause.applied"));
        assert!(audit
            .iter()
            .any(|event| event["action"] == "budget.auto_pause.resume"
                && event["details"]["preserved_cause"].is_string()));
        assert!(audit
            .iter()
            .any(|event| event["action"] == "budget.auto_pause.override"));
    }

    #[test]
    fn auto_pause_rejects_false_positive_sparse_stale_and_wrong_scope_evidence() {
        let dir = tempdir().unwrap();
        let store = store_with_run(&dir.path().join("reject.db"), "run-1");
        let mut cases = Vec::new();
        let mut low = finding("low", "run-1");
        low.confidence.level = BudgetConfidenceLevel::Low;
        low.confidence.score = 0.2;
        low.seal().unwrap();
        cases.push((low, "confidence"));
        let mut stale = finding("stale", "run-1");
        stale.window.freshness_seconds = 301;
        stale.window.generated_at = "2026-07-11T00:15:01Z".to_string();
        stale.seal().unwrap();
        cases.push((stale, "stale"));
        let mut pricing = finding("pricing", "run-1");
        pricing.coverage.pricing_complete = false;
        pricing.seal().unwrap();
        cases.push((pricing, "pricing"));
        let mut warning = finding("warning", "run-1");
        warning.severity = Some(BudgetAnomalySeverity::Warning);
        warning.seal().unwrap();
        cases.push((warning, "severity"));
        let normal = {
            let mut value = finding("normal", "run-1");
            value.detected = false;
            value.anomaly_kind = None;
            value.severity = None;
            value.measurement = None;
            value.reason_codes.clear();
            value.seal().unwrap();
            value
        };
        cases.push((normal, "does not support"));
        for (case, expected) in cases {
            let artifact = record(&store, &case);
            assert!(store
                .apply_budget_auto_pause(&artifact, "run-1", &enabled_policy(), "operator")
                .unwrap_err()
                .contains(expected));
        }
        let wrong = record(&store, &finding("wrong", "run-2"));
        assert!(store
            .apply_budget_auto_pause(&wrong, "run-1", &enabled_policy(), "operator")
            .unwrap_err()
            .contains("scope"));
    }

    #[test]
    fn audit_and_pause_failures_roll_back_the_whole_decision() {
        for target in ["audit", "pause"] {
            let dir = tempdir().unwrap();
            let store = store_with_run(&dir.path().join(format!("{target}.db")), "run-1");
            let artifact = record(&store, &finding(&format!("finding-{target}"), "run-1"));
            store.with_conn(|conn| conn.execute_batch(if target == "audit" { "CREATE TRIGGER fail_budget_audit BEFORE INSERT ON audit_log WHEN NEW.action = 'budget.auto_pause.applied' BEGIN SELECT RAISE(ABORT, 'audit unavailable'); END;" } else { "CREATE TRIGGER fail_budget_pause BEFORE UPDATE OF pause_reason ON workflow_runs WHEN NEW.pause_reason LIKE 'budget_auto_pause:%' BEGIN SELECT RAISE(ABORT, 'pause unavailable'); END;" }).map_err(|error| error.to_string())).unwrap();
            assert!(store
                .apply_budget_auto_pause(&artifact, "run-1", &enabled_policy(), "operator")
                .is_err());
            store
                .with_conn(|conn| {
                    let paused: Option<String> = conn
                        .query_row(
                            "SELECT pause_reason FROM workflow_runs WHERE run_id = 'run-1'",
                            [],
                            |row| row.get(0),
                        )
                        .map_err(|error| error.to_string())?;
                    let decisions: i64 = conn
                        .query_row("SELECT COUNT(*) FROM budget_pause_decisions", [], |row| {
                            row.get(0)
                        })
                        .map_err(|error| error.to_string())?;
                    assert!(paused.is_none());
                    assert_eq!(decisions, 0);
                    Ok(())
                })
                .unwrap();
        }
    }

    #[test]
    fn concurrent_duplicate_triggers_create_one_pause_decision() {
        let dir = tempdir().unwrap();
        let store = Arc::new(store_with_run(&dir.path().join("concurrent.db"), "run-1"));
        let artifact = record(&store, &finding("finding-concurrent", "run-1"));
        let handles = (0..8)
            .map(|_| {
                let store = Arc::clone(&store);
                let artifact = artifact.clone();
                std::thread::spawn(move || {
                    store
                        .apply_budget_auto_pause(&artifact, "run-1", &enabled_policy(), "operator")
                        .unwrap()["decision_id"]
                        .as_str()
                        .unwrap()
                        .to_string()
                })
            })
            .collect::<Vec<_>>();
        let ids = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
        store
            .with_conn(|conn| {
                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM budget_pause_decisions", [], |row| {
                        row.get(0)
                    })
                    .map_err(|error| error.to_string())?;
                assert_eq!(count, 1);
                Ok(())
            })
            .unwrap();
    }
}
