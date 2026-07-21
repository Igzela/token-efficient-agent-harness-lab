//! Durable storage for PE7 Harness Evolution B1 evidence foundation.

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use super::{append_audit_locked, LocalProductStore};
use crate::harness_evolution::{
    build_admission_receipt, validate_candidate_for_admission, validate_proposal,
    ActiveHarnessIdentity, CandidateStatus, CandidateTerminalReason, EvolutionAdmissionError,
    EvolutionCandidate, EvolutionProposal, EvolutionReceipt, ACTIVE_VERSION_SCHEMA,
    CANDIDATE_SCHEMA_VERSION, EVOLUTION_LAB_SCHEMA_VERSION, RECEIPT_SCHEMA_VERSION,
};

impl LocalProductStore {
    /// Record the immutable active-Harness + evaluator identity for the lab epoch.
    pub fn set_harness_evolution_active_identity(
        &self,
        identity: &ActiveHarnessIdentity,
    ) -> Result<(), String> {
        if identity.schema_version != ACTIVE_VERSION_SCHEMA {
            return Err("active harness identity schema_version mismatch".into());
        }
        let body = serde_json::to_string(identity).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO harness_evolution_active_identity
                    (active_version_id, active_version_hash, evaluator_identity_hash, body_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(active_version_id) DO UPDATE SET
                    active_version_hash=excluded.active_version_hash,
                    evaluator_identity_hash=excluded.evaluator_identity_hash,
                    body_json=excluded.body_json,
                    updated_at=excluded.updated_at",
                params![
                    identity.active_version_id,
                    identity.active_version_hash,
                    identity.evaluator_identity_hash,
                    body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn get_harness_evolution_active_identity(
        &self,
        active_version_id: &str,
    ) -> Result<Option<ActiveHarnessIdentity>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_active_identity WHERE active_version_id=?1",
                    params![active_version_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => {
                    let identity: ActiveHarnessIdentity =
                        serde_json::from_str(&body).map_err(|e| e.to_string())?;
                    Ok(Some(identity))
                }
                None => Ok(None),
            }
        })
    }

    /// Exactly-once proposal admission (duplicate proposal_id is refused).
    pub fn admit_harness_evolution_proposal(
        &self,
        proposal: &EvolutionProposal,
    ) -> Result<EvolutionProposal, String> {
        validate_proposal(proposal).map_err(|e| format!("{}: {}", e.code, e.message))?;
        let body = serde_json::to_string(proposal).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT proposal_id FROM harness_evolution_proposals WHERE proposal_id=?1",
                    params![proposal.proposal_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if existing.is_some() {
                return Err(format!(
                    "evolution_duplicate_proposal: proposal {} already recorded",
                    proposal.proposal_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_proposals
                    (proposal_id, parent_candidate_id, active_version_id, active_version_hash,
                     evaluator_identity_hash, proposal_body_sha256, body_json, seed, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    proposal.proposal_id,
                    proposal.parent_candidate_id,
                    proposal.active_version_id,
                    proposal.active_version_hash,
                    proposal.evaluator_identity_hash,
                    proposal.proposal_body_sha256,
                    body,
                    proposal.seed as i64,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                "system",
                "harness_evolution.proposal_admitted",
                &proposal.proposal_id,
                &serde_json::json!({
                    "schema_version": EVOLUTION_LAB_SCHEMA_VERSION,
                    "proposal_id": proposal.proposal_id,
                    "active_version_id": proposal.active_version_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(proposal.clone())
        })
    }

    /// Exactly-once candidate admission with immutable active-version binding.
    pub fn admit_harness_evolution_candidate(
        &self,
        mut candidate: EvolutionCandidate,
        current_active: &ActiveHarnessIdentity,
    ) -> Result<(EvolutionCandidate, EvolutionReceipt), String> {
        let parent_valid = if let Some(parent_id) = &candidate.parent_candidate_id {
            self.get_harness_evolution_candidate(parent_id)?
                .map(|p| p.status == CandidateStatus::Admitted)
                .unwrap_or(false)
        } else {
            true
        };
        validate_candidate_for_admission(&candidate, current_active, parent_valid).map_err(
            |e: EvolutionAdmissionError| {
                candidate.status = CandidateStatus::Rejected;
                candidate.terminal_reason = match e.code.as_str() {
                    "evolution_stale_parent" => CandidateTerminalReason::RejectedStaleParent,
                    "evolution_changed_active_version" => {
                        CandidateTerminalReason::RejectedChangedActiveVersion
                    }
                    "evolution_kill_switch" => CandidateTerminalReason::RejectedKillSwitch,
                    "evolution_workspace_escape" => {
                        CandidateTerminalReason::RejectedWorkspaceEscape
                    }
                    "evolution_forbidden_surface" | "evolution_unknown_surface" => {
                        CandidateTerminalReason::RejectedForbiddenSurface
                    }
                    "evolution_sensitive_payload" => CandidateTerminalReason::RejectedTamper,
                    _ => CandidateTerminalReason::RejectedMalformed,
                };
                format!("{}: {}", e.code, e.message)
            },
        )?;

        candidate.status = CandidateStatus::Admitted;
        candidate.terminal_reason = CandidateTerminalReason::Admitted;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        candidate.created_at = now.clone();
        let receipt = build_admission_receipt(&candidate, &now);
        let body = serde_json::to_string(&candidate).map_err(|e| e.to_string())?;
        let receipt_body = serde_json::to_string(&receipt).map_err(|e| e.to_string())?;

        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;

            // Late-write refusal: proposal must exist and not already have a terminal candidate.
            let proposal_exists: Option<String> = tx
                .query_row(
                    "SELECT proposal_id FROM harness_evolution_proposals WHERE proposal_id=?1",
                    params![candidate.proposal_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if proposal_exists.is_none() {
                return Err(format!(
                    "evolution_late_write: proposal {} missing",
                    candidate.proposal_id
                ));
            }

            let existing: Option<String> = tx
                .query_row(
                    "SELECT candidate_id FROM harness_evolution_candidates WHERE candidate_id=?1",
                    params![candidate.candidate_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if existing.is_some() {
                return Err(format!(
                    "evolution_duplicate_candidate: candidate {} already recorded",
                    candidate.candidate_id
                ));
            }

            // Duplicate content under same lineage is refused.
            let dup_content: Option<String> = tx
                .query_row(
                    "SELECT candidate_id FROM harness_evolution_candidates
                     WHERE lineage_id=?1 AND content_hash=?2",
                    params![candidate.lineage_id, candidate.content_hash],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if dup_content.is_some() {
                return Err(
                    "evolution_duplicate_candidate: identical content already admitted in lineage"
                        .into(),
                );
            }

            tx.execute(
                "INSERT INTO harness_evolution_candidates
                    (candidate_id, lineage_id, parent_candidate_id, proposal_id,
                     active_version_id, active_version_hash, evaluator_identity_hash,
                     content_hash, status, terminal_reason, workspace_id, workspace_rel_path,
                     body_json, seed, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15)",
                params![
                    candidate.candidate_id,
                    candidate.lineage_id,
                    candidate.parent_candidate_id,
                    candidate.proposal_id,
                    candidate.active_version_id,
                    candidate.active_version_hash,
                    candidate.evaluator_identity_hash,
                    candidate.content_hash,
                    candidate.status.as_str(),
                    candidate.terminal_reason.as_str(),
                    candidate.workspace.workspace_id,
                    candidate.workspace.relative_path,
                    body,
                    candidate.seed as i64,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;

            tx.execute(
                "INSERT INTO harness_evolution_receipts
                    (receipt_id, candidate_id, proposal_id, lineage_id, active_version_id,
                     terminal_reason, content_hash, body_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    receipt.receipt_id,
                    receipt.candidate_id,
                    receipt.proposal_id,
                    receipt.lineage_id,
                    receipt.active_version_id,
                    receipt.terminal_reason.as_str(),
                    receipt.content_hash,
                    receipt_body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;

            append_audit_locked(
                &tx,
                &now,
                "system",
                "harness_evolution.candidate_admitted",
                &candidate.candidate_id,
                &serde_json::json!({
                    "schema_version": EVOLUTION_LAB_SCHEMA_VERSION,
                    "candidate_id": candidate.candidate_id,
                    "lineage_id": candidate.lineage_id,
                    "proposal_id": candidate.proposal_id,
                    "receipt_id": receipt.receipt_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok((candidate, receipt))
        })
    }

    pub fn get_harness_evolution_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Option<EvolutionCandidate>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id=?1",
                    params![candidate_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => {
                    let c: EvolutionCandidate =
                        serde_json::from_str(&body).map_err(|e| e.to_string())?;
                    if c.schema_version != CANDIDATE_SCHEMA_VERSION {
                        return Err("stored candidate schema_version mismatch".into());
                    }
                    Ok(Some(c))
                }
                None => Ok(None),
            }
        })
    }

    pub fn get_harness_evolution_receipt(
        &self,
        receipt_id: &str,
    ) -> Result<Option<EvolutionReceipt>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_receipts WHERE receipt_id=?1",
                    params![receipt_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => {
                    let r: EvolutionReceipt =
                        serde_json::from_str(&body).map_err(|e| e.to_string())?;
                    if r.schema_version != RECEIPT_SCHEMA_VERSION {
                        return Err("stored receipt schema_version mismatch".into());
                    }
                    Ok(Some(r))
                }
                None => Ok(None),
            }
        })
    }

    pub fn list_harness_evolution_candidates_for_lineage(
        &self,
        lineage_id: &str,
    ) -> Result<Vec<EvolutionCandidate>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT body_json FROM harness_evolution_candidates
                     WHERE lineage_id=?1 ORDER BY created_at ASC, candidate_id ASC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![lineage_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for row in rows {
                let body = row.map_err(|e| e.to_string())?;
                let c: EvolutionCandidate =
                    serde_json::from_str(&body).map_err(|e| e.to_string())?;
                out.push(c);
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness_evolution::{
        candidate_from_proposal, proposal_from_body, sample_active_identity, sha256_hex,
        ENABLE_ENV, KILL_SWITCH_ENV,
    };
    use serde_json::json;

    static LAB_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct LabEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_enable: Option<String>,
        prev_kill: Option<String>,
    }

    impl LabEnvGuard {
        fn enable() -> Self {
            let lock = LAB_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev_enable = std::env::var(ENABLE_ENV).ok();
            let prev_kill = std::env::var(KILL_SWITCH_ENV).ok();
            std::env::set_var(ENABLE_ENV, "1");
            std::env::remove_var(KILL_SWITCH_ENV);
            Self {
                _lock: lock,
                prev_enable,
                prev_kill,
            }
        }
    }

    impl Drop for LabEnvGuard {
        fn drop(&mut self) {
            match &self.prev_enable {
                Some(v) => std::env::set_var(ENABLE_ENV, v),
                None => std::env::remove_var(ENABLE_ENV),
            }
            match &self.prev_kill {
                Some(v) => std::env::set_var(KILL_SWITCH_ENV, v),
                None => std::env::remove_var(KILL_SWITCH_ENV),
            }
        }
    }

    #[test]
    fn admits_proposal_and_candidate_exactly_once() {
        let _env = LabEnvGuard::enable();
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .set_harness_evolution_active_identity(&active)
            .unwrap();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind":"prompt","digest":"x"}),
            vec![sha256_hex("ev")],
            11,
        )
        .unwrap();
        store.admit_harness_evolution_proposal(&proposal).unwrap();
        let dup = store.admit_harness_evolution_proposal(&proposal);
        assert!(dup.unwrap_err().contains("duplicate"));

        let candidate = candidate_from_proposal(
            &proposal,
            &sha256_hex("content-1"),
            "candidates/c-a",
            &sha256_hex("ws-a"),
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        let (admitted, receipt) = store
            .admit_harness_evolution_candidate(candidate.clone(), &active)
            .unwrap();
        assert_eq!(admitted.status, CandidateStatus::Admitted);
        assert_eq!(receipt.terminal_reason, CandidateTerminalReason::Admitted);
        let again = store.admit_harness_evolution_candidate(candidate, &active);
        assert!(again.unwrap_err().contains("duplicate"));
        let loaded = store
            .get_harness_evolution_candidate(&admitted.candidate_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.candidate_id, admitted.candidate_id);
        let r = store
            .get_harness_evolution_receipt(&receipt.receipt_id)
            .unwrap()
            .unwrap();
        assert_eq!(r.receipt_id, receipt.receipt_id);
    }

    #[test]
    fn refuses_stale_parent_and_changed_active_version() {
        let _env = LabEnvGuard::enable();
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .set_harness_evolution_active_identity(&active)
            .unwrap();
        let parent_proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind":"parent"}),
            vec![],
            1,
        )
        .unwrap();
        store
            .admit_harness_evolution_proposal(&parent_proposal)
            .unwrap();
        // Parent never admitted as candidate → child sees stale parent.
        let child_proposal = proposal_from_body(
            &active,
            Some("hevc-missing-parent".into()),
            &["prompts_and_bounded_rules"],
            &json!({"kind":"child"}),
            vec![],
            2,
        )
        .unwrap();
        store
            .admit_harness_evolution_proposal(&child_proposal)
            .unwrap();
        let child = candidate_from_proposal(
            &child_proposal,
            &sha256_hex("child-content"),
            "candidates/child",
            &sha256_hex("ws-c"),
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        let err = store
            .admit_harness_evolution_candidate(child, &active)
            .unwrap_err();
        assert!(
            err.contains("stale_parent") || err.contains("evolution_stale_parent"),
            "unexpected error: {err}"
        );

        let mut other_active = active.clone();
        other_active.active_version_hash = sha256_hex("moved");
        let proposal = proposal_from_body(
            &active,
            None,
            &["retry_and_stop_policy"],
            &json!({"kind":"retry"}),
            vec![],
            5,
        )
        .unwrap();
        store.admit_harness_evolution_proposal(&proposal).unwrap();
        let candidate = candidate_from_proposal(
            &proposal,
            &sha256_hex("content-x"),
            "candidates/cx",
            &sha256_hex("ws-x"),
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        let err = store
            .admit_harness_evolution_candidate(candidate, &other_active)
            .unwrap_err();
        assert!(
            err.contains("changed_active_version"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn durable_restart_preserves_candidate_and_receipt() {
        let _env = LabEnvGuard::enable();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("evo.db");
        let db_s = db.to_str().unwrap();
        let (candidate_id, receipt_id) = {
            let store = LocalProductStore::new(db_s).unwrap();
            let active = sample_active_identity();
            store
                .set_harness_evolution_active_identity(&active)
                .unwrap();
            let proposal = proposal_from_body(
                &active,
                None,
                &["context_selection_and_summarization"],
                &json!({"kind":"ctx"}),
                vec![],
                42,
            )
            .unwrap();
            store.admit_harness_evolution_proposal(&proposal).unwrap();
            let candidate = candidate_from_proposal(
                &proposal,
                &sha256_hex("restart-content"),
                "candidates/restart",
                &sha256_hex("ws-r"),
                "2026-07-21T00:00:00Z",
            )
            .unwrap();
            let (admitted, receipt) = store
                .admit_harness_evolution_candidate(candidate, &active)
                .unwrap();
            (admitted.candidate_id, receipt.receipt_id)
        };
        let store = LocalProductStore::new(db_s).unwrap();
        let loaded = store
            .get_harness_evolution_candidate(&candidate_id)
            .unwrap()
            .expect("candidate survives reopen");
        assert_eq!(loaded.candidate_id, candidate_id);
        assert_eq!(loaded.status, CandidateStatus::Admitted);
        let receipt = store
            .get_harness_evolution_receipt(&receipt_id)
            .unwrap()
            .expect("receipt survives reopen");
        assert_eq!(receipt.receipt_id, receipt_id);
        // Replay must not create a second success receipt.
        let again = store.admit_harness_evolution_candidate(loaded, &sample_active_identity());
        assert!(again.unwrap_err().contains("duplicate"));
    }

    #[test]
    fn refuses_candidate_without_prior_proposal() {
        let _env = LabEnvGuard::enable();
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind":"late"}),
            vec![],
            8,
        )
        .unwrap();
        // Intentionally skip proposal admission.
        let candidate = candidate_from_proposal(
            &proposal,
            &sha256_hex("late-content"),
            "candidates/late",
            &sha256_hex("ws-l"),
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        let err = store
            .admit_harness_evolution_candidate(candidate, &active)
            .unwrap_err();
        assert!(
            err.contains("late_write") || err.contains("missing"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn schema_versions_are_recorded() {
        assert_eq!(
            crate::harness_evolution::PROPOSAL_SCHEMA_VERSION,
            "harness_evolution_proposal.v1"
        );
        assert_eq!(CANDIDATE_SCHEMA_VERSION, "harness_evolution_candidate.v1");
    }
}
