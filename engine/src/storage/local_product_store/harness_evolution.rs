//! Durable storage for PE7 Harness Evolution B1 evidence + B2 evaluation/archive.

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use super::{append_audit_locked, LocalProductStore};
use crate::harness_evolution::{
    build_admission_receipt, configured_workspace_root, revalidate_workspace_content,
    validate_candidate_for_admission, validate_proposal, ActiveHarnessIdentity, CandidateStatus,
    CandidateTerminalReason, EvolutionAdmissionError, EvolutionCandidate, EvolutionProposal,
    EvolutionReceipt, ACTIVE_VERSION_SCHEMA, CANDIDATE_SCHEMA_VERSION,
    EVOLUTION_LAB_SCHEMA_VERSION, RECEIPT_SCHEMA_VERSION,
};
use crate::harness_evolution_eval::{
    build_eval_receipt, build_pareto_archive, evaluate_candidate_fixture, redacted_eval_evidence,
    CandidateEvaluationBundle, EqualBudgetContract, EvalReceipt, ParetoArchiveEntry,
    SealedHoldoutVault, TaskFamilyManifest, ARCHIVE_SCHEMA_VERSION, EVAL_RECEIPT_SCHEMA_VERSION,
    EVAL_SCHEMA_VERSION, SEALED_SCHEMA_VERSION,
};
use crate::harness_evolution_pr_ready::{
    finalize_pr_ready_bundle, redacted_pr_ready_evidence, PrReadyCandidateBundle, PrReadyReceipt,
    PR_READY_RECEIPT_SCHEMA, PR_READY_SCHEMA_VERSION,
};

impl LocalProductStore {
    /// Register an immutable active-Harness + evaluator epoch (insert-only).
    ///
    /// Changing the active Harness or evaluator requires a **new** `active_version_id`.
    /// Existing epochs are never mutated (`ON CONFLICT DO UPDATE` is forbidden).
    /// `actor_id` is bound into the audit receipt for the owner action.
    pub fn register_harness_evolution_active_identity(
        &self,
        identity: &ActiveHarnessIdentity,
        actor_id: &str,
    ) -> Result<ActiveHarnessIdentity, String> {
        if identity.schema_version != ACTIVE_VERSION_SCHEMA {
            return Err("active harness identity schema_version mismatch".into());
        }
        if actor_id.trim().is_empty() {
            return Err(
                "evolution_active_identity_actor: authenticated actor_id is required".into(),
            );
        }
        let body = serde_json::to_string(identity).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<(String, String, String, String)> = tx
                .query_row(
                    "SELECT active_version_hash, evaluator_identity_hash, body_json, created_at
                     FROM harness_evolution_active_identity WHERE active_version_id=?1",
                    params![identity.active_version_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some((hash, eval, existing_body, _)) = existing {
                if hash == identity.active_version_hash
                    && eval == identity.evaluator_identity_hash
                    && existing_body == body
                {
                    // Exact replay of the same epoch registration — return original, no mutation.
                    let stored: ActiveHarnessIdentity =
                        serde_json::from_str(&existing_body).map_err(|e| e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(stored);
                }
                return Err(format!(
                    "evolution_active_identity_immutable: epoch {} already exists and cannot be mutated; create a new active_version_id",
                    identity.active_version_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_active_identity
                    (active_version_id, active_version_hash, evaluator_identity_hash, body_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![
                    identity.active_version_id,
                    identity.active_version_hash,
                    identity.evaluator_identity_hash,
                    body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.active_identity_registered",
                &identity.active_version_id,
                &serde_json::json!({
                    "schema_version": EVOLUTION_LAB_SCHEMA_VERSION,
                    "active_version_id": identity.active_version_id,
                    "active_version_hash": identity.active_version_hash,
                    "evaluator_identity_hash": identity.evaluator_identity_hash,
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(identity.clone())
        })
    }

    /// Compatibility wrapper: register with system actor (prefer explicit actor).
    pub fn set_harness_evolution_active_identity(
        &self,
        identity: &ActiveHarnessIdentity,
    ) -> Result<(), String> {
        self.register_harness_evolution_active_identity(identity, "system")
            .map(|_| ())
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

    /// Load the current (latest created) active identity epoch from the store owner.
    pub fn get_current_harness_evolution_active_identity(
        &self,
    ) -> Result<Option<ActiveHarnessIdentity>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_active_identity
                     ORDER BY created_at DESC, active_version_id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(
                    serde_json::from_str(&body).map_err(|e| e.to_string())?,
                )),
                None => Ok(None),
            }
        })
    }

    /// Exactly-once proposal admission bound to store-owned active identity.
    ///
    /// Caller may supply `expected_active_version_id` for optimistic concurrency only.
    /// Active Harness/evaluator fields on the proposal are overwritten from the store.
    pub fn admit_harness_evolution_proposal(
        &self,
        proposal: &EvolutionProposal,
    ) -> Result<EvolutionProposal, String> {
        self.admit_harness_evolution_proposal_with_expected(proposal, None)
    }

    pub fn admit_harness_evolution_proposal_with_expected(
        &self,
        proposal: &EvolutionProposal,
        expected_active_version_id: Option<&str>,
    ) -> Result<EvolutionProposal, String> {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;

            // Exact replay: identical proposal_id returns original decision without mutation.
            let existing_body: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_proposals WHERE proposal_id=?1",
                    params![proposal.proposal_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(body) = existing_body {
                let stored: EvolutionProposal =
                    serde_json::from_str(&body).map_err(|e| e.to_string())?;
                // Compare durable identity fields; caller may have stale active fields.
                if stored.proposal_body_sha256 == proposal.proposal_body_sha256
                    && stored.seed == proposal.seed
                    && stored.parent_candidate_id == proposal.parent_candidate_id
                    && stored.mutable_surface == proposal.mutable_surface
                    && stored.evidence_hashes == proposal.evidence_hashes
                {
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(stored);
                }
                return Err(format!(
                    "evolution_duplicate_proposal: proposal {} already recorded with conflicting body",
                    proposal.proposal_id
                ));
            }

            let current_active = load_current_active_identity_tx(&tx)?;
            if let Some(expected) = expected_active_version_id {
                if expected != current_active.active_version_id {
                    return Err(format!(
                        "evolution_stale_expected_active: expected {} but current is {}",
                        expected, current_active.active_version_id
                    ));
                }
            }

            // Bind authoritative active identity; ignore caller-supplied authority fields.
            let mut bound = proposal.clone();
            bound.active_version_id = current_active.active_version_id.clone();
            bound.active_version_hash = current_active.active_version_hash.clone();
            bound.evaluator_identity_hash = current_active.evaluator_identity_hash.clone();
            // Re-derive proposal_id under authoritative active epoch.
            bound.proposal_id = crate::harness_evolution::derive_proposal_id(
                &bound.active_version_id,
                &bound.proposal_body_sha256,
                bound.seed,
            );
            validate_proposal(&bound).map_err(|e| format!("{}: {}", e.code, e.message))?;

            // Parent (if any) must exist as an admitted candidate under the same active epoch.
            if let Some(parent_id) = &bound.parent_candidate_id {
                let parent_ok: Option<(String, String)> = tx
                    .query_row(
                        "SELECT status, active_version_id FROM harness_evolution_candidates
                         WHERE candidate_id=?1",
                        params![parent_id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                match parent_ok {
                    Some((status, parent_active))
                        if status == CandidateStatus::Admitted.as_str()
                            && parent_active == current_active.active_version_id => {}
                    _ => {
                        return Err(
                            "evolution_stale_parent: parent candidate missing, not admitted, or wrong epoch"
                                .into(),
                        );
                    }
                }
            }

            let body = serde_json::to_string(&bound).map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO harness_evolution_proposals
                    (proposal_id, parent_candidate_id, active_version_id, active_version_hash,
                     evaluator_identity_hash, proposal_body_sha256, body_json, seed, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    bound.proposal_id,
                    bound.parent_candidate_id,
                    bound.active_version_id,
                    bound.active_version_hash,
                    bound.evaluator_identity_hash,
                    bound.proposal_body_sha256,
                    body,
                    bound.seed as i64,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                "system",
                "harness_evolution.proposal_admitted",
                &bound.proposal_id,
                &serde_json::json!({
                    "schema_version": EVOLUTION_LAB_SCHEMA_VERSION,
                    "proposal_id": bound.proposal_id,
                    "active_version_id": bound.active_version_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(bound)
        })
    }

    /// Exactly-once candidate admission: active identity and proposal loaded inside the transaction.
    ///
    /// Does **not** accept caller-supplied `current_active` authority. Optional
    /// `expected_active_version_id` is optimistic concurrency only.
    pub fn admit_harness_evolution_candidate(
        &self,
        candidate: EvolutionCandidate,
    ) -> Result<(EvolutionCandidate, EvolutionReceipt), String> {
        self.admit_harness_evolution_candidate_with_expected(candidate, None)
    }

    pub fn admit_harness_evolution_candidate_with_expected(
        &self,
        mut candidate: EvolutionCandidate,
        expected_active_version_id: Option<&str>,
    ) -> Result<(EvolutionCandidate, EvolutionReceipt), String> {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        // Workspace revalidation happens outside the DB transaction (filesystem).
        let workspace_root = configured_workspace_root()
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        revalidate_workspace_content(&workspace_root, &candidate.workspace)
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;

        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;

            // Exact replay of same candidate_id returns original decision.
            let existing_body: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id=?1",
                    params![candidate.candidate_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(body) = existing_body {
                let stored: EvolutionCandidate =
                    serde_json::from_str(&body).map_err(|e| e.to_string())?;
                if stored.content_hash == candidate.content_hash
                    && stored.proposal_id == candidate.proposal_id
                    && stored.lineage_id == candidate.lineage_id
                {
                    let receipt_id = crate::harness_evolution::derive_receipt_id(
                        &stored.candidate_id,
                        stored.terminal_reason,
                    );
                    let receipt_body: Option<String> = tx
                        .query_row(
                            "SELECT body_json FROM harness_evolution_receipts WHERE receipt_id=?1",
                            params![receipt_id],
                            |r| r.get(0),
                        )
                        .optional()
                        .map_err(|e| e.to_string())?;
                    let receipt = match receipt_body {
                        Some(rb) => serde_json::from_str(&rb).map_err(|e| e.to_string())?,
                        None => build_admission_receipt(&stored, &stored.created_at),
                    };
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok((stored, receipt));
                }
                return Err(format!(
                    "evolution_duplicate_candidate: candidate {} already recorded with conflicting content",
                    candidate.candidate_id
                ));
            }

            let current_active = load_current_active_identity_tx(&tx)?;
            if let Some(expected) = expected_active_version_id {
                if expected != current_active.active_version_id {
                    return Err(format!(
                        "evolution_stale_expected_active: expected {} but current is {}",
                        expected, current_active.active_version_id
                    ));
                }
            }

            // Load proposal and bind authority from proposal + current active.
            let proposal_body: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_proposals WHERE proposal_id=?1",
                    params![candidate.proposal_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let proposal: EvolutionProposal = match proposal_body {
                Some(body) => serde_json::from_str(&body).map_err(|e| e.to_string())?,
                None => {
                    return Err(format!(
                        "evolution_late_write: proposal {} missing",
                        candidate.proposal_id
                    ));
                }
            };
            if proposal.active_version_id != current_active.active_version_id
                || proposal.active_version_hash != current_active.active_version_hash
                || proposal.evaluator_identity_hash != current_active.evaluator_identity_hash
            {
                return Err(
                    "evolution_changed_active_version: proposal epoch no longer matches current active"
                        .into(),
                );
            }

            // Overwrite candidate authority from store-owned proposal/active (reject caller authority).
            candidate.active_version_id = current_active.active_version_id.clone();
            candidate.active_version_hash = current_active.active_version_hash.clone();
            candidate.evaluator_identity_hash = current_active.evaluator_identity_hash.clone();
            candidate.proposal_id = proposal.proposal_id.clone();
            candidate.parent_candidate_id = proposal.parent_candidate_id.clone();
            candidate.mutable_surface = proposal.mutable_surface.clone();
            candidate.seed = proposal.seed;
            candidate.lineage_id = crate::harness_evolution::derive_lineage_id(
                candidate.parent_candidate_id.as_deref(),
                &candidate.proposal_id,
            );
            candidate.candidate_id = crate::harness_evolution::derive_candidate_id(
                &candidate.proposal_id,
                &candidate.content_hash,
                candidate.seed,
            );

            let parent_valid = if let Some(parent_id) = &candidate.parent_candidate_id {
                let status: Option<String> = tx
                    .query_row(
                        "SELECT status FROM harness_evolution_candidates WHERE candidate_id=?1",
                        params![parent_id],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                status.as_deref() == Some(CandidateStatus::Admitted.as_str())
            } else {
                true
            };

            if let Err(e) =
                validate_candidate_for_admission(&candidate, &current_active, parent_valid)
            {
                candidate.status = CandidateStatus::Rejected;
                candidate.terminal_reason = match e.code.as_str() {
                    "evolution_stale_parent" => CandidateTerminalReason::RejectedStaleParent,
                    "evolution_changed_active_version" => {
                        CandidateTerminalReason::RejectedChangedActiveVersion
                    }
                    "evolution_kill_switch" => CandidateTerminalReason::RejectedKillSwitch,
                    "evolution_workspace_escape" | "evolution_workspace_tamper" => {
                        CandidateTerminalReason::RejectedWorkspaceEscape
                    }
                    "evolution_forbidden_surface" | "evolution_unknown_surface" => {
                        CandidateTerminalReason::RejectedForbiddenSurface
                    }
                    "evolution_sensitive_payload" => CandidateTerminalReason::RejectedTamper,
                    _ => CandidateTerminalReason::RejectedMalformed,
                };
                candidate.created_at = now.clone();
                let receipt = build_admission_receipt(&candidate, &now);
                persist_candidate_and_receipt_tx(&tx, &candidate, &receipt, &now)?;
                append_audit_locked(
                    &tx,
                    &now,
                    "system",
                    "harness_evolution.candidate_rejected",
                    &candidate.candidate_id,
                    &serde_json::json!({
                        "schema_version": EVOLUTION_LAB_SCHEMA_VERSION,
                        "candidate_id": candidate.candidate_id,
                        "terminal_reason": candidate.terminal_reason.as_str(),
                        "code": e.code,
                    }),
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                return Err(format!("{}: {}", e.code, e.message));
            }

            // Duplicate content under same lineage is refused and persisted as rejected when new id.
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

            candidate.status = CandidateStatus::Admitted;
            candidate.terminal_reason = CandidateTerminalReason::Admitted;
            candidate.created_at = now.clone();
            let receipt = build_admission_receipt(&candidate, &now);
            persist_candidate_and_receipt_tx(&tx, &candidate, &receipt, &now)?;
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
                    "workspace_id": candidate.workspace.workspace_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok((candidate, receipt))
        })
    }

    /// Discard an unpromoted candidate workspace and record a discarded terminal when present.
    pub fn discard_harness_evolution_candidate_workspace(
        &self,
        candidate_id: &str,
        actor_id: &str,
    ) -> Result<(), String> {
        let candidate = self
            .get_harness_evolution_candidate(candidate_id)?
            .ok_or_else(|| format!("evolution_candidate_missing: {candidate_id}"))?;
        let root = configured_workspace_root()
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        crate::harness_evolution::discard_candidate_workspace(&root, &candidate.workspace)
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            append_audit_locked(
                conn,
                &now,
                actor_id,
                "harness_evolution.workspace_discarded",
                candidate_id,
                &serde_json::json!({
                    "schema_version": EVOLUTION_LAB_SCHEMA_VERSION,
                    "candidate_id": candidate_id,
                    "workspace_id": candidate.workspace.workspace_id,
                    "terminal_reason": CandidateTerminalReason::WorkspaceDiscarded.as_str(),
                }),
            )?;
            Ok(())
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

    /// Persist evaluator-owned sealed holdout membership (hashes only).
    pub fn store_harness_evolution_sealed_holdout(
        &self,
        vault: &SealedHoldoutVault,
    ) -> Result<SealedHoldoutVault, String> {
        if vault.schema_version != SEALED_SCHEMA_VERSION {
            return Err("sealed holdout schema_version mismatch".into());
        }
        let body = serde_json::to_string(vault).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO harness_evolution_sealed_holdouts
                    (vault_sha256, family_id, preselected_entrant_limit, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(vault_sha256) DO NOTHING",
                params![
                    vault.vault_sha256,
                    vault.family_id,
                    vault.preselected_entrant_limit as i64,
                    body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(vault.clone())
        })
    }

    pub fn get_harness_evolution_sealed_holdout(
        &self,
        vault_sha256: &str,
    ) -> Result<Option<SealedHoldoutVault>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_sealed_holdouts WHERE vault_sha256=?1",
                    params![vault_sha256],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(
                    serde_json::from_str(&body).map_err(|e| e.to_string())?,
                )),
                None => Ok(None),
            }
        })
    }

    /// Exactly-once fixture evaluation + Pareto archive under equal budgets.
    pub fn record_harness_evolution_evaluation(
        &self,
        candidate_id: &str,
        budget: &EqualBudgetContract,
        family: &TaskFamilyManifest,
        sealed_vault: &SealedHoldoutVault,
        include_sealed: bool,
        current_active: &ActiveHarnessIdentity,
    ) -> Result<
        (
            CandidateEvaluationBundle,
            Vec<ParetoArchiveEntry>,
            EvalReceipt,
        ),
        String,
    > {
        let candidate = self
            .get_harness_evolution_candidate(candidate_id)?
            .ok_or_else(|| format!("evolution_eval_missing_candidate: {candidate_id}"))?;
        if candidate.status != CandidateStatus::Admitted {
            return Err("evolution_eval_candidate_not_admitted".into());
        }
        if candidate.active_version_id != current_active.active_version_id
            || candidate.active_version_hash != current_active.active_version_hash
            || candidate.evaluator_identity_hash != current_active.evaluator_identity_hash
        {
            return Err("evolution_eval_changed_active_version".into());
        }
        let created_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let bundle = evaluate_candidate_fixture(
            &candidate.candidate_id,
            &candidate.lineage_id,
            &candidate.active_version_id,
            &candidate.active_version_hash,
            &candidate.evaluator_identity_hash,
            budget,
            family,
            sealed_vault,
            include_sealed,
            &created_at,
        )
        .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        if bundle.schema_version != EVAL_SCHEMA_VERSION {
            return Err("evaluation bundle schema mismatch".into());
        }
        if bundle.claims_improvement || bundle.sealed_feedback_into_mutation {
            return Err("evaluation violated immutable laboratory claims contract".into());
        }
        let archive = build_pareto_archive(&bundle, &created_at)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let receipt = build_eval_receipt(&bundle, "evaluated", &created_at);
        let redacted = redacted_eval_evidence(&bundle);
        let bundle_json = serde_json::to_string(&bundle).map_err(|e| e.to_string())?;
        let receipt_json = serde_json::to_string(&receipt).map_err(|e| e.to_string())?;

        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT evaluation_id FROM harness_evolution_evaluations
                     WHERE candidate_id=?1 AND budget_seed=?2 AND family_id=?3",
                    params![candidate_id, budget.seed as i64, family.family_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if existing.is_some() {
                return Err(format!(
                    "evolution_duplicate_evaluation: candidate {} seed {} family {}",
                    candidate_id, budget.seed, family.family_id
                ));
            }
            // Ensure sealed vault is durable under evaluator ownership.
            tx.execute(
                "INSERT INTO harness_evolution_sealed_holdouts
                    (vault_sha256, family_id, preselected_entrant_limit, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(vault_sha256) DO NOTHING",
                params![
                    sealed_vault.vault_sha256,
                    sealed_vault.family_id,
                    sealed_vault.preselected_entrant_limit as i64,
                    serde_json::to_string(sealed_vault).map_err(|e| e.to_string())?,
                    created_at
                ],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO harness_evolution_evaluations
                    (evaluation_id, candidate_id, lineage_id, active_version_id, active_version_hash,
                     evaluator_identity_hash, family_id, budget_seed, bundle_sha256,
                     sealed_entrant_count, claims_improvement, sealed_feedback_into_mutation,
                     body_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,0,?11,?12)",
                params![
                    bundle.evaluation_id,
                    bundle.candidate_id,
                    bundle.lineage_id,
                    bundle.active_version_id,
                    bundle.active_version_hash,
                    bundle.evaluator_identity_hash,
                    bundle.family_id,
                    budget.seed as i64,
                    bundle.bundle_sha256,
                    bundle.sealed_entrant_count as i64,
                    bundle_json,
                    created_at
                ],
            )
            .map_err(|e| e.to_string())?;
            for entry in &archive {
                if entry.schema_version != ARCHIVE_SCHEMA_VERSION {
                    return Err("pareto archive schema mismatch".into());
                }
                let entry_json = serde_json::to_string(entry).map_err(|e| e.to_string())?;
                tx.execute(
                    "INSERT INTO harness_evolution_pareto_archive
                        (archive_id, evaluation_id, candidate_id, lineage_id, baseline,
                         sequential_rank, dominated, entry_sha256, body_json, created_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![
                        entry.archive_id,
                        entry.evaluation_id,
                        entry.candidate_id,
                        entry.lineage_id,
                        entry.baseline.as_str(),
                        entry.sequential_rank as i64,
                        if entry.dominated { 1 } else { 0 },
                        entry.entry_sha256,
                        entry_json,
                        created_at
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
            if receipt.schema_version != EVAL_RECEIPT_SCHEMA_VERSION {
                return Err("eval receipt schema mismatch".into());
            }
            tx.execute(
                "INSERT INTO harness_evolution_eval_receipts
                    (receipt_id, evaluation_id, candidate_id, terminal, bundle_sha256, body_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    receipt.receipt_id,
                    receipt.evaluation_id,
                    receipt.candidate_id,
                    receipt.terminal,
                    receipt.bundle_sha256,
                    receipt_json,
                    created_at
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &created_at,
                "system",
                "harness_evolution.evaluation_recorded",
                &bundle.evaluation_id,
                &redacted,
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok((bundle, archive, receipt))
        })
    }

    pub fn get_harness_evolution_evaluation(
        &self,
        evaluation_id: &str,
    ) -> Result<Option<CandidateEvaluationBundle>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_evaluations WHERE evaluation_id=?1",
                    params![evaluation_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(
                    serde_json::from_str(&body).map_err(|e| e.to_string())?,
                )),
                None => Ok(None),
            }
        })
    }

    pub fn list_harness_evolution_pareto_for_evaluation(
        &self,
        evaluation_id: &str,
    ) -> Result<Vec<ParetoArchiveEntry>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT body_json FROM harness_evolution_pareto_archive
                     WHERE evaluation_id=?1 ORDER BY sequential_rank ASC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![evaluation_id], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for row in rows {
                let body = row.map_err(|e| e.to_string())?;
                out.push(serde_json::from_str(&body).map_err(|e| e.to_string())?);
            }
            Ok(out)
        })
    }

    pub fn get_harness_evolution_eval_receipt(
        &self,
        receipt_id: &str,
    ) -> Result<Option<EvalReceipt>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_eval_receipts WHERE receipt_id=?1",
                    params![receipt_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(
                    serde_json::from_str(&body).map_err(|e| e.to_string())?,
                )),
                None => Ok(None),
            }
        })
    }
    /// Independently finalize a PR_READY bundle (no PR create/merge).
    pub fn record_harness_evolution_pr_ready(
        &self,
        candidate_id: &str,
        evaluation_id: &str,
        current_active: &ActiveHarnessIdentity,
        patch_text: &str,
        allowed_paths: &[String],
        base_commit_sha: &str,
        head_commit_sha: &str,
        expected_base_commit_sha: &str,
        static_check_sha256: &str,
        test_evidence_sha256: &str,
        secret_scan_sha256: &str,
        rollback_evidence_sha256: &str,
        operator_decision: &str,
    ) -> Result<(PrReadyCandidateBundle, PrReadyReceipt), String> {
        let candidate = self
            .get_harness_evolution_candidate(candidate_id)?
            .ok_or_else(|| format!("evolution_pr_ready_missing_candidate: {candidate_id}"))?;
        let evaluation = self
            .get_harness_evolution_evaluation(evaluation_id)?
            .ok_or_else(|| format!("evolution_pr_ready_missing_eval: {evaluation_id}"))?;
        let created_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let (bundle, receipt) = finalize_pr_ready_bundle(
            &candidate,
            current_active,
            &evaluation,
            patch_text,
            allowed_paths,
            base_commit_sha,
            head_commit_sha,
            expected_base_commit_sha,
            static_check_sha256,
            test_evidence_sha256,
            secret_scan_sha256,
            rollback_evidence_sha256,
            operator_decision,
            &created_at,
        )
        .map_err(|e| format!("{}: {}", e.code, e.message))?;
        if bundle.schema_version != PR_READY_SCHEMA_VERSION
            || receipt.schema_version != PR_READY_RECEIPT_SCHEMA
            || !bundle.terminal.is_ready()
        {
            return Err("PR_READY finalizer contract violation".into());
        }
        // Persist without raw patch text in durable JSON body for audit safety.
        let mut durable = bundle.clone();
        durable.patch.patch_text.clear();
        let bundle_json = serde_json::to_string(&durable).map_err(|e| e.to_string())?;
        let receipt_json = serde_json::to_string(&receipt).map_err(|e| e.to_string())?;
        let redacted = redacted_pr_ready_evidence(&bundle);
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT bundle_id FROM harness_evolution_pr_ready_bundles
                     WHERE candidate_id=?1 AND evaluation_id=?2 AND patch_sha256=?3",
                    params![
                        candidate_id,
                        evaluation_id,
                        bundle.patch.patch_sha256
                    ],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if existing.is_some() {
                return Err(format!(
                    "evolution_duplicate_pr_ready: candidate {} evaluation {}",
                    candidate_id, evaluation_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_pr_ready_bundles
                    (bundle_id, candidate_id, lineage_id, active_version_id, evaluation_id,
                     patch_sha256, base_commit_sha, head_commit_sha, bundle_sha256, terminal,
                     body_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    bundle.bundle_id,
                    bundle.candidate_id,
                    bundle.lineage_id,
                    bundle.active_version_id,
                    bundle.evidence.evaluation_id,
                    bundle.patch.patch_sha256,
                    bundle.patch.base_commit_sha,
                    bundle.patch.head_commit_sha,
                    bundle.bundle_sha256,
                    bundle.terminal.as_str(),
                    bundle_json,
                    created_at
                ],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO harness_evolution_pr_ready_receipts
                    (receipt_id, bundle_id, candidate_id, terminal, bundle_sha256, body_json, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    receipt.receipt_id,
                    receipt.bundle_id,
                    receipt.candidate_id,
                    receipt.terminal.as_str(),
                    receipt.bundle_sha256,
                    receipt_json,
                    created_at
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &created_at,
                "system",
                "harness_evolution.pr_ready_recorded",
                &bundle.bundle_id,
                &redacted,
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok((bundle, receipt))
        })
    }

    pub fn get_harness_evolution_pr_ready_bundle(
        &self,
        bundle_id: &str,
    ) -> Result<Option<PrReadyCandidateBundle>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_pr_ready_bundles WHERE bundle_id=?1",
                    params![bundle_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(
                    serde_json::from_str(&body).map_err(|e| e.to_string())?,
                )),
                None => Ok(None),
            }
        })
    }
}

fn load_current_active_identity_tx(tx: &Transaction<'_>) -> Result<ActiveHarnessIdentity, String> {
    let row: Option<String> = tx
        .query_row(
            "SELECT body_json FROM harness_evolution_active_identity
             ORDER BY created_at DESC, active_version_id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    match row {
        Some(body) => serde_json::from_str(&body).map_err(|e| e.to_string()),
        None => Err(
            "evolution_active_identity_missing: no active Harness identity epoch is registered"
                .into(),
        ),
    }
}

fn persist_candidate_and_receipt_tx(
    tx: &Transaction<'_>,
    candidate: &EvolutionCandidate,
    receipt: &EvolutionReceipt,
    now: &str,
) -> Result<(), String> {
    let body = serde_json::to_string(candidate).map_err(|e| e.to_string())?;
    let receipt_body = serde_json::to_string(receipt).map_err(|e| e.to_string())?;
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness_evolution::{
        candidate_from_proposal, derive_workspace_id, materialize_candidate_workspace,
        proposal_from_body, sample_active_identity, sha256_hex, ENABLE_ENV, KILL_SWITCH_ENV,
        WORKSPACE_ROOT_ENV,
    };
    use serde_json::json;

    struct LabEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_enable: Option<String>,
        prev_kill: Option<String>,
        prev_ws_root: Option<String>,
        _ws_dir: tempfile::TempDir,
    }

    impl LabEnvGuard {
        fn enable() -> Self {
            let lock = crate::harness_evolution::EVOLUTION_LAB_TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let prev_enable = std::env::var(ENABLE_ENV).ok();
            let prev_kill = std::env::var(KILL_SWITCH_ENV).ok();
            let prev_ws_root = std::env::var(WORKSPACE_ROOT_ENV).ok();
            let ws_dir = tempfile::tempdir().unwrap();
            std::env::set_var(ENABLE_ENV, "1");
            std::env::remove_var(KILL_SWITCH_ENV);
            std::env::set_var(WORKSPACE_ROOT_ENV, ws_dir.path());
            Self {
                _lock: lock,
                prev_enable,
                prev_kill,
                prev_ws_root,
                _ws_dir: ws_dir,
            }
        }

        fn workspace_root(&self) -> &std::path::Path {
            self._ws_dir.path()
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
            match &self.prev_ws_root {
                Some(v) => std::env::set_var(WORKSPACE_ROOT_ENV, v),
                None => std::env::remove_var(WORKSPACE_ROOT_ENV),
            }
        }
    }

    fn materialize_for(
        env: &LabEnvGuard,
        proposal: &crate::harness_evolution::EvolutionProposal,
        marker: &str,
    ) -> crate::harness_evolution::CandidateWorkspace {
        let ws_id = derive_workspace_id(&proposal.proposal_id, proposal.seed);
        materialize_candidate_workspace(
            env.workspace_root(),
            &ws_id,
            &[(
                "candidate.json".to_string(),
                format!(r#"{{"marker":"{marker}"}}"#).into_bytes(),
            )],
        )
        .unwrap()
    }

    #[test]
    fn admits_proposal_and_candidate_exactly_once() {
        let env = LabEnvGuard::enable();
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "operator-test")
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
        let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
        // Exact replay returns original without error.
        let again = store.admit_harness_evolution_proposal(&proposal).unwrap();
        assert_eq!(again.proposal_id, bound.proposal_id);

        let ws = materialize_for(&env, &bound, "content-1");
        let candidate = candidate_from_proposal(&bound, &ws, "2026-07-21T00:00:00Z").unwrap();
        let (admitted, receipt) = store
            .admit_harness_evolution_candidate(candidate.clone())
            .unwrap();
        assert_eq!(admitted.status, CandidateStatus::Admitted);
        assert_eq!(receipt.terminal_reason, CandidateTerminalReason::Admitted);
        // Exact replay returns original decision (no second receipt mutation).
        let (replayed, receipt2) = store.admit_harness_evolution_candidate(candidate).unwrap();
        assert_eq!(replayed.candidate_id, admitted.candidate_id);
        assert_eq!(receipt2.receipt_id, receipt.receipt_id);
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
    fn refuses_stale_parent_and_active_identity_mutation() {
        let env = LabEnvGuard::enable();
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "operator-test")
            .unwrap();
        // Child proposal with missing parent fails at proposal admission.
        let child_proposal = proposal_from_body(
            &active,
            Some("hevc-missing-parent".into()),
            &["prompts_and_bounded_rules"],
            &json!({"kind":"child"}),
            vec![],
            2,
        )
        .unwrap();
        let err = store
            .admit_harness_evolution_proposal(&child_proposal)
            .unwrap_err();
        assert!(
            err.contains("stale_parent") || err.contains("evolution_stale_parent"),
            "unexpected error: {err}"
        );

        // Epoch is immutable: same id with different hash is refused.
        let mut mutated = active.clone();
        mutated.active_version_hash = sha256_hex("moved");
        let err = store
            .register_harness_evolution_active_identity(&mutated, "operator-test")
            .unwrap_err();
        assert!(
            err.contains("immutable") || err.contains("cannot be mutated"),
            "unexpected error: {err}"
        );

        // New epoch is allowed and becomes current.
        let mut new_epoch = active.clone();
        new_epoch.active_version_id = "active-harness-v1".to_string();
        new_epoch.active_version_hash = sha256_hex("new-epoch-body");
        store
            .register_harness_evolution_active_identity(&new_epoch, "operator-test")
            .unwrap();
        let current = store
            .get_current_harness_evolution_active_identity()
            .unwrap()
            .unwrap();
        assert_eq!(current.active_version_id, "active-harness-v1");

        // Proposal under stale expected active is refused.
        let proposal = proposal_from_body(
            &new_epoch,
            None,
            &["retry_and_stop_policy"],
            &json!({"kind":"retry"}),
            vec![],
            5,
        )
        .unwrap();
        let err = store
            .admit_harness_evolution_proposal_with_expected(&proposal, Some("active-harness-v0"))
            .unwrap_err();
        assert!(
            err.contains("stale_expected_active"),
            "unexpected error: {err}"
        );
        let _bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
        let _ = env;
    }

    #[test]
    fn durable_restart_preserves_candidate_and_receipt() {
        let env = LabEnvGuard::enable();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("evo.db");
        let db_s = db.to_str().unwrap();
        let (candidate_id, receipt_id, loaded_candidate) = {
            let store = LocalProductStore::new(db_s).unwrap();
            let active = sample_active_identity();
            store
                .register_harness_evolution_active_identity(&active, "operator-test")
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
            let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
            let ws = materialize_for(&env, &bound, "restart-content");
            let candidate = candidate_from_proposal(&bound, &ws, "2026-07-21T00:00:00Z").unwrap();
            let (admitted, receipt) = store.admit_harness_evolution_candidate(candidate).unwrap();
            let candidate_id = admitted.candidate_id.clone();
            let receipt_id = receipt.receipt_id.clone();
            (candidate_id, receipt_id, admitted)
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
        // Exact replay returns original (no second receipt).
        let (replayed, receipt2) = store
            .admit_harness_evolution_candidate(loaded_candidate)
            .unwrap();
        assert_eq!(replayed.candidate_id, candidate_id);
        assert_eq!(receipt2.receipt_id, receipt_id);
    }

    #[test]
    fn refuses_candidate_without_prior_proposal() {
        let env = LabEnvGuard::enable();
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "operator-test")
            .unwrap();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind":"late"}),
            vec![],
            8,
        )
        .unwrap();
        let ws = materialize_for(&env, &proposal, "late-content");
        let candidate = candidate_from_proposal(&proposal, &ws, "2026-07-21T00:00:00Z").unwrap();
        let err = store
            .admit_harness_evolution_candidate(candidate)
            .unwrap_err();
        assert!(
            err.contains("late_write") || err.contains("missing"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn detects_workspace_tamper_before_admission() {
        let env = LabEnvGuard::enable();
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "operator-test")
            .unwrap();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind":"tamper"}),
            vec![],
            13,
        )
        .unwrap();
        let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
        let ws = materialize_for(&env, &bound, "pre-tamper");
        let path = env
            .workspace_root()
            .join(&ws.relative_path)
            .join("extra.txt");
        std::fs::write(path, b"tamper").unwrap();
        let candidate = candidate_from_proposal(&bound, &ws, "2026-07-21T00:00:00Z").unwrap();
        let err = store
            .admit_harness_evolution_candidate(candidate)
            .unwrap_err();
        assert!(
            err.contains("tamper") || err.contains("workspace"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn discards_unpromoted_workspace_without_main_mutation() {
        let env = LabEnvGuard::enable();
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "operator-test")
            .unwrap();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind":"discard"}),
            vec![],
            17,
        )
        .unwrap();
        let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
        let ws = materialize_for(&env, &bound, "discard-me");
        let candidate = candidate_from_proposal(&bound, &ws, "2026-07-21T00:00:00Z").unwrap();
        let (admitted, _) = store.admit_harness_evolution_candidate(candidate).unwrap();
        let dir = env.workspace_root().join(&admitted.workspace.relative_path);
        assert!(dir.exists());
        store
            .discard_harness_evolution_candidate_workspace(&admitted.candidate_id, "operator-test")
            .unwrap();
        assert!(!dir.exists());
    }

    #[test]
    fn schema_versions_are_recorded() {
        assert_eq!(
            crate::harness_evolution::PROPOSAL_SCHEMA_VERSION,
            "harness_evolution_proposal.v1"
        );
        assert_eq!(CANDIDATE_SCHEMA_VERSION, "harness_evolution_candidate.v1");
    }

    #[test]
    fn records_equal_budget_evaluation_and_pareto_exactly_once() {
        let env = LabEnvGuard::enable();
        use crate::harness_evolution_eval::{
            build_sealed_vault, sample_budget, sample_task_family,
        };
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "operator-test")
            .unwrap();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind": "eval"}),
            vec![],
            99,
        )
        .unwrap();
        let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
        let ws = materialize_for(&env, &bound, "eval-content");
        let candidate = candidate_from_proposal(&bound, &ws, "2026-07-21T00:00:00Z").unwrap();
        let (admitted, _) = store.admit_harness_evolution_candidate(candidate).unwrap();
        let family = sample_task_family("fam-store");
        let vault = build_sealed_vault(&family).unwrap();
        let budget = sample_budget(3);
        let (bundle, archive, receipt) = store
            .record_harness_evolution_evaluation(
                &admitted.candidate_id,
                &budget,
                &family,
                &vault,
                true,
                &active,
            )
            .unwrap();
        assert!(!bundle.claims_improvement);
        assert!(!bundle.sealed_feedback_into_mutation);
        assert!(!archive.is_empty());
        assert_eq!(receipt.bundle_sha256, bundle.bundle_sha256);
        let loaded = store
            .get_harness_evolution_evaluation(&bundle.evaluation_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.bundle_sha256, bundle.bundle_sha256);
        let pareto = store
            .list_harness_evolution_pareto_for_evaluation(&bundle.evaluation_id)
            .unwrap();
        assert_eq!(pareto.len(), archive.len());
        let r = store
            .get_harness_evolution_eval_receipt(&receipt.receipt_id)
            .unwrap()
            .unwrap();
        assert_eq!(r.receipt_id, receipt.receipt_id);
        let dup = store.record_harness_evolution_evaluation(
            &admitted.candidate_id,
            &budget,
            &family,
            &vault,
            true,
            &active,
        );
        assert!(dup.unwrap_err().contains("duplicate"));
    }

    #[test]
    fn evaluation_survives_durable_restart() {
        let env = LabEnvGuard::enable();
        use crate::harness_evolution_eval::{
            build_sealed_vault, sample_budget, sample_task_family,
        };
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("evo-eval.db");
        let db_s = db.to_str().unwrap();
        let (evaluation_id, receipt_id) = {
            let store = LocalProductStore::new(db_s).unwrap();
            let active = sample_active_identity();
            store
                .register_harness_evolution_active_identity(&active, "operator-test")
                .unwrap();
            let proposal = proposal_from_body(
                &active,
                None,
                &["tool_descriptions_and_selection_policy"],
                &json!({"kind": "restart-eval"}),
                vec![],
                7,
            )
            .unwrap();
            let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
            let ws = materialize_for(&env, &bound, "restart-eval-content");
            let candidate = candidate_from_proposal(&bound, &ws, "2026-07-21T00:00:00Z").unwrap();
            let (admitted, _) = store.admit_harness_evolution_candidate(candidate).unwrap();
            let family = sample_task_family("fam-restart");
            let vault = build_sealed_vault(&family).unwrap();
            let (bundle, _, receipt) = store
                .record_harness_evolution_evaluation(
                    &admitted.candidate_id,
                    &sample_budget(1),
                    &family,
                    &vault,
                    false,
                    &active,
                )
                .unwrap();
            (bundle.evaluation_id, receipt.receipt_id)
        };
        let store = LocalProductStore::new(db_s).unwrap();
        assert!(store
            .get_harness_evolution_evaluation(&evaluation_id)
            .unwrap()
            .is_some());
        assert!(store
            .get_harness_evolution_eval_receipt(&receipt_id)
            .unwrap()
            .is_some());
        assert!(!store
            .list_harness_evolution_pareto_for_evaluation(&evaluation_id)
            .unwrap()
            .is_empty());
    }
}
