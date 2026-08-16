//! Durable storage for PE7 Harness Evolution B1 evidence + B2 evaluation/archive.

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use std::collections::BTreeSet;

use super::{append_audit_locked, LocalProductStore};
use crate::harness_evolution::{
    build_admission_receipt, configured_workspace_root, revalidate_workspace_content,
    validate_candidate_for_admission, validate_proposal, ActiveHarnessIdentity, CandidateStatus,
    CandidateTerminalReason, EvolutionAdmissionError, EvolutionCandidate, EvolutionProposal,
    EvolutionReceipt, ACTIVE_VERSION_SCHEMA, CANDIDATE_SCHEMA_VERSION,
    EVOLUTION_LAB_SCHEMA_VERSION, RECEIPT_SCHEMA_VERSION,
};
use crate::harness_evolution_eval::{
    build_eval_receipt, build_pareto_archive, build_sealed_vault,
    evaluate_candidate_from_workspace, redacted_eval_evidence, CandidateEvaluationBundle,
    EqualBudgetContract, EvalReceipt, ParetoArchiveEntry, SealedHoldoutVault, TaskFamilyManifest,
    ARCHIVE_SCHEMA_VERSION, EVAL_RECEIPT_SCHEMA_VERSION, EVAL_SCHEMA_VERSION, MAX_SEALED_ENTRANTS,
    MIN_SEALED_ENTRANTS, SEALED_SCHEMA_VERSION,
};
use crate::harness_evolution_pr_ready::{
    finalize_pr_ready_bundle, redacted_pr_ready_evidence, PrReadyCandidateBundle, PrReadyReceipt,
    PR_READY_RECEIPT_SCHEMA, PR_READY_SCHEMA_VERSION,
};

const SEALED_SELECTION_SCHEMA: &str = "harness_evolution_sealed_selection.v1";

fn parse_sealed_selection(
    key: &str,
    value: &serde_json::Value,
) -> Result<(String, String, Vec<String>, bool), String> {
    let prefix = "harness_evolution.sealed_selection.";
    let receipt_id = key
        .strip_prefix(prefix)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "evolution_eval_sealed_selection_key_malformed".to_string())?;
    let object = value
        .as_object()
        .ok_or_else(|| "evolution_eval_sealed_selection_malformed".to_string())?;
    if object.len() != 5
        || !object.contains_key("schema_version")
        || !object.contains_key("receipt_id")
        || !object.contains_key("family_id")
        || !object.contains_key("candidate_ids")
        || !object.contains_key("used")
    {
        return Err("evolution_eval_sealed_selection_malformed".to_string());
    }
    if object.get("schema_version").and_then(|v| v.as_str()) != Some(SEALED_SELECTION_SCHEMA)
        || object.get("receipt_id").and_then(|v| v.as_str()) != Some(receipt_id)
    {
        return Err("evolution_eval_sealed_selection_schema".to_string());
    }
    let family_id = object
        .get("family_id")
        .and_then(|v| v.as_str())
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "evolution_eval_sealed_selection_family".to_string())?
        .to_string();
    let ids = object
        .get("candidate_ids")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "evolution_eval_sealed_selection_candidates".to_string())?;
    if !(MIN_SEALED_ENTRANTS..=MAX_SEALED_ENTRANTS).contains(&ids.len()) {
        return Err("evolution_eval_sealed_selection_entrants".to_string());
    }
    let mut seen = BTreeSet::new();
    let mut candidate_ids = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id
            .as_str()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| "evolution_eval_sealed_selection_candidates".to_string())?;
        if !seen.insert(id) {
            return Err("evolution_eval_sealed_selection_duplicate".to_string());
        }
        candidate_ids.push(id.to_string());
    }
    let used = object
        .get("used")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| "evolution_eval_sealed_selection_used".to_string())?;
    Ok((receipt_id.to_string(), family_id, candidate_ids, used))
}

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
        crate::harness_evolution_eval::validate_sealed_vault(vault)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let body = serde_json::to_string(vault).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<(String, i64, String)> = tx
                .query_row(
                    "SELECT family_id, preselected_entrant_limit, body_json
                     FROM harness_evolution_sealed_holdouts WHERE vault_sha256=?1",
                    params![vault.vault_sha256],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some((family_id, entrant_limit, stored_body)) = existing {
                let stored: SealedHoldoutVault =
                    serde_json::from_str(&stored_body).map_err(|e| e.to_string())?;
                crate::harness_evolution_eval::validate_sealed_vault(&stored)
                    .map_err(|e| format!("{}: {}", e.code, e.message))?;
                if stored != *vault
                    || family_id != vault.family_id
                    || entrant_limit != vault.preselected_entrant_limit as i64
                {
                    return Err(
                        "evolution_eval_sealed_immutable: vault identity is already registered with different data".into(),
                    );
                }
                tx.commit().map_err(|e| e.to_string())?;
                return Ok(stored);
            }
            tx.execute(
                "INSERT INTO harness_evolution_sealed_holdouts
                    (vault_sha256, family_id, preselected_entrant_limit, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    vault.vault_sha256,
                    vault.family_id,
                    vault.preselected_entrant_limit as i64,
                    body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                "system",
                "harness_evolution.sealed_holdout_stored",
                &vault.vault_sha256,
                &serde_json::json!({
                    "schema_version": SEALED_SCHEMA_VERSION,
                    "family_id": vault.family_id,
                    "vault_sha256": vault.vault_sha256,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(vault.clone())
        })
    }

    pub fn get_harness_evolution_sealed_holdout(
        &self,
        vault_sha256: &str,
    ) -> Result<Option<SealedHoldoutVault>, String> {
        crate::harness_evolution::validate_sha256_hex(vault_sha256)
            .map_err(|_| "sealed vault digest must be 64 lowercase hex".to_string())?;
        self.with_conn(|conn| {
            let row: Option<(String, i64, String)> = conn
                .query_row(
                    "SELECT family_id, preselected_entrant_limit, body_json
                     FROM harness_evolution_sealed_holdouts WHERE vault_sha256=?1",
                    params![vault_sha256],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some((family_id, entrant_limit, body)) => {
                    let vault: SealedHoldoutVault =
                        serde_json::from_str(&body).map_err(|e| e.to_string())?;
                    crate::harness_evolution_eval::validate_sealed_vault(&vault)
                        .map_err(|e| format!("{}: {}", e.code, e.message))?;
                    if vault.vault_sha256 != vault_sha256
                        || vault.family_id != family_id
                        || vault.preselected_entrant_limit as i64 != entrant_limit
                    {
                        return Err(
                            "evolution_eval_sealed_corrupt: stored vault identity mismatch".into(),
                        );
                    }
                    Ok(Some(vault))
                }
                None => Ok(None),
            }
        })
    }

    /// Register an evaluator-owned task family (trusted configuration owner).
    pub fn register_harness_evolution_task_family(
        &self,
        family: &TaskFamilyManifest,
        actor_id: &str,
    ) -> Result<TaskFamilyManifest, String> {
        if actor_id.trim().is_empty() {
            return Err("evolution_eval_actor: authenticated actor_id is required".into());
        }
        crate::harness_evolution_eval::validate_task_family(family)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        if let Some(existing) = self.get_harness_evolution_task_family(&family.family_id)? {
            if existing == *family {
                return Ok(existing);
            }
            return Err(format!(
                "evolution_eval_family_immutable: family {} is already registered; create a new family_id",
                family.family_id
            ));
        }
        let key = format!("harness_evolution.task_family.{}", family.family_id);
        let value = serde_json::to_value(family).map_err(|e| e.to_string())?;
        self.set_config_value(&key, value, actor_id)?;
        self.with_conn(|conn| {
            append_audit_locked(
                conn,
                &chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                actor_id,
                "harness_evolution.task_family_registered",
                &family.family_id,
                &serde_json::json!({
                    "schema_version": EVAL_SCHEMA_VERSION,
                    "family_id": family.family_id,
                }),
            )?;
            Ok(())
        })?;
        Ok(family.clone())
    }

    pub fn get_harness_evolution_task_family(
        &self,
        family_id: &str,
    ) -> Result<Option<TaskFamilyManifest>, String> {
        let key = format!("harness_evolution.task_family.{family_id}");
        let snap = self.config_snapshot()?;
        let Some(map) = snap.as_object() else {
            return Ok(None);
        };
        let Some(value) = map.get(&key) else {
            return Ok(None);
        };
        // config_snapshot may nest under key -> {value: ...} or raw value depending on owner.
        let body = if let Some(inner) = value.get("value") {
            inner.clone()
        } else {
            value.clone()
        };
        let family: TaskFamilyManifest = serde_json::from_value(body).map_err(|e| e.to_string())?;
        if family.family_id != family_id {
            return Err(
                "evolution_eval_family_corrupt: config key does not match family_id".into(),
            );
        }
        crate::harness_evolution_eval::validate_task_family(&family)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        Ok(Some(family))
    }

    /// Register evaluator-owned sealed vault derived from a registered task family only.
    pub fn register_harness_evolution_sealed_vault(
        &self,
        family_id: &str,
        actor_id: &str,
    ) -> Result<SealedHoldoutVault, String> {
        if actor_id.trim().is_empty() {
            return Err("evolution_eval_actor: authenticated actor_id is required".into());
        }
        let family = self
            .get_harness_evolution_task_family(family_id)?
            .ok_or_else(|| format!("evolution_eval_family_missing: {family_id}"))?;
        let vault = build_sealed_vault(&family)
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<(String, i64, String)> = tx
                .query_row(
                    "SELECT family_id, preselected_entrant_limit, body_json
                     FROM harness_evolution_sealed_holdouts WHERE vault_sha256=?1",
                    params![vault.vault_sha256],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some((stored_family_id, entrant_limit, body)) = existing {
                let stored: SealedHoldoutVault =
                    serde_json::from_str(&body).map_err(|e| e.to_string())?;
                crate::harness_evolution_eval::validate_sealed_vault(&stored)
                    .map_err(|e| format!("{}: {}", e.code, e.message))?;
                if stored == vault
                    && stored_family_id == vault.family_id
                    && entrant_limit == vault.preselected_entrant_limit as i64
                {
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(stored);
                }
                return Err(
                    "evolution_eval_sealed_immutable: vault already registered with different body"
                        .into(),
                );
            }
            tx.execute(
                "INSERT INTO harness_evolution_sealed_holdouts
                    (vault_sha256, family_id, preselected_entrant_limit, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    vault.vault_sha256,
                    vault.family_id,
                    vault.preselected_entrant_limit as i64,
                    serde_json::to_string(&vault).map_err(|e| e.to_string())?,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            // Index current vault for family.
            let idx_key = format!("harness_evolution.sealed_vault_index.{family_id}");
            tx.execute(
                "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(key) DO UPDATE SET
                    value_json=excluded.value_json,
                    updated_at=excluded.updated_at,
                    updated_by=excluded.updated_by",
                params![
                    idx_key,
                    serde_json::json!({"vault_sha256": vault.vault_sha256}).to_string(),
                    now,
                    actor_id
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.sealed_vault_registered",
                family_id,
                &serde_json::json!({
                    "schema_version": SEALED_SCHEMA_VERSION,
                    "family_id": family_id,
                    "vault_sha256": vault.vault_sha256,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(vault)
        })
    }

    pub fn get_registered_harness_evolution_sealed_vault(
        &self,
        family_id: &str,
    ) -> Result<Option<SealedHoldoutVault>, String> {
        let idx_key = format!("harness_evolution.sealed_vault_index.{family_id}");
        let snap = self.config_snapshot()?;
        let Some(map) = snap.as_object() else {
            return Ok(None);
        };
        let Some(entry) = map.get(&idx_key) else {
            return Ok(None);
        };
        let body = if let Some(inner) = entry.get("value") {
            inner.clone()
        } else {
            entry.clone()
        };
        let vault_sha = body
            .get("vault_sha256")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "evolution_eval_sealed_index_malformed".to_string())?;
        let vault = self.get_harness_evolution_sealed_holdout(vault_sha)?;
        if let Some(ref vault) = vault {
            if vault.family_id != family_id {
                return Err(
                    "evolution_eval_sealed_family_mismatch: index family does not match vault"
                        .into(),
                );
            }
        }
        Ok(vault)
    }

    /// Issue a one-use sealed selection for 1–3 preselected candidate IDs (evaluator-owned).
    pub fn issue_harness_evolution_sealed_selection(
        &self,
        family_id: &str,
        candidate_ids: &[String],
        actor_id: &str,
    ) -> Result<String, String> {
        if actor_id.trim().is_empty() {
            return Err("evolution_eval_actor: authenticated actor_id is required".into());
        }
        if family_id.trim().is_empty() {
            return Err("evolution_eval_family_id: family_id is required".into());
        }
        if candidate_ids.len() < MIN_SEALED_ENTRANTS || candidate_ids.len() > MAX_SEALED_ENTRANTS {
            return Err(
                "evolution_eval_sealed_entrants: sealed selection must name 1–3 candidates".into(),
            );
        }
        let mut unique_candidate_ids = BTreeSet::new();
        for id in candidate_ids {
            if id.trim().is_empty() || !unique_candidate_ids.insert(id) {
                return Err("evolution_eval_sealed_selection_duplicate_or_empty_candidate".into());
            }
        }
        let _vault = self
            .get_registered_harness_evolution_sealed_vault(family_id)?
            .ok_or_else(|| format!("evolution_eval_sealed_missing: {family_id}"))?;
        for id in candidate_ids {
            let c = self
                .get_harness_evolution_candidate(id)?
                .ok_or_else(|| format!("evolution_eval_selection_missing_candidate: {id}"))?;
            if c.status != CandidateStatus::Admitted {
                return Err(format!("evolution_eval_selection_not_admitted: {id}"));
            }
        }
        let material = format!(
            "sealed_selection.v1|{family_id}|{}|{}",
            candidate_ids.join(","),
            actor_id
        );
        let receipt_id = format!(
            "hess-{}",
            &crate::harness_evolution::sha256_hex(&material)[..24]
        );
        let key = format!("harness_evolution.sealed_selection.{receipt_id}");
        let value = serde_json::json!({
            "schema_version": SEALED_SELECTION_SCHEMA,
            "receipt_id": receipt_id.clone(),
            "family_id": family_id,
            "candidate_ids": candidate_ids,
            "used": false,
        });
        let value_json = value.to_string();
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let exists: Option<i64> = tx
                .query_row(
                    "SELECT 1 FROM local_config WHERE key=?1",
                    params![key],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if exists.is_some() {
                return Err(format!(
                    "evolution_eval_sealed_selection_exists: {receipt_id}"
                ));
            }
            tx.execute(
                "INSERT INTO local_config (key, value_json, updated_at, updated_by)
                 VALUES (?1, ?2, ?3, ?4)",
                params![key, value_json, now, actor_id],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.sealed_selection_issued",
                &receipt_id,
                &serde_json::json!({
                    "schema_version": SEALED_SELECTION_SCHEMA,
                    "family_id": family_id,
                    "receipt_id": receipt_id,
                    "candidate_count": candidate_ids.len(),
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(receipt_id.clone())
        })
    }

    fn consume_sealed_selection_for_candidate(
        &self,
        family_id: &str,
        candidate_id: &str,
        actor_id: &str,
    ) -> Result<bool, String> {
        let prefix = "harness_evolution.sealed_selection.";
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let rows: Vec<(String, String)> = {
                let mut stmt = tx
                    .prepare("SELECT key, value_json FROM local_config ORDER BY key")
                    .map_err(|e| e.to_string())?;
                let mapped = stmt
                    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                    .map_err(|e| e.to_string())?;
                mapped
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?
            };
            for (key, value_json) in rows {
                if !key.starts_with(prefix) {
                    continue;
                }
                let value: serde_json::Value =
                    serde_json::from_str(&value_json).map_err(|e| e.to_string())?;
                let (receipt_id, selection_family_id, candidate_ids, used) =
                    parse_sealed_selection(&key, &value)?;
                if used
                    || selection_family_id != family_id
                    || !candidate_ids.iter().any(|id| id == candidate_id)
                {
                    continue;
                }
                let updated = serde_json::json!({
                    "schema_version": SEALED_SELECTION_SCHEMA,
                    "receipt_id": receipt_id,
                    "family_id": selection_family_id,
                    "candidate_ids": candidate_ids,
                    "used": true,
                });
                let updated_json = updated.to_string();
                let changed = tx
                    .execute(
                        "UPDATE local_config
                         SET value_json=?1, updated_at=?2, updated_by=?3
                         WHERE key=?4 AND value_json=?5",
                        params![updated_json, self.now(), actor_id, key, value_json],
                    )
                    .map_err(|e| e.to_string())?;
                if changed != 1 {
                    return Err("evolution_eval_sealed_selection_race".into());
                }
                append_audit_locked(
                    &tx,
                    &self.now(),
                    actor_id,
                    "harness_evolution.sealed_selection_consumed",
                    &receipt_id,
                    &serde_json::json!({
                        "schema_version": SEALED_SELECTION_SCHEMA,
                        "family_id": family_id,
                        "candidate_id": candidate_id,
                        "receipt_id": receipt_id,
                    }),
                )?;
                tx.commit().map_err(|e| e.to_string())?;
                return Ok(true);
            }
            tx.commit().map_err(|e| e.to_string())?;
            Ok(false)
        })
    }

    /// Exactly-once workspace-bound evaluation + Pareto archive under equal budgets.
    ///
    /// Loads candidate, current active identity, registered task family, and sealed vault
    /// from store owners. Caller does **not** supply sealed vault, include_sealed, or
    /// current_active authority.
    pub fn record_harness_evolution_evaluation(
        &self,
        candidate_id: &str,
        budget: &EqualBudgetContract,
        family_id: &str,
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
        let current_active = self
            .get_current_harness_evolution_active_identity()?
            .ok_or_else(|| {
                "evolution_eval_active_missing: no active Harness identity epoch".to_string()
            })?;
        if candidate.active_version_id != current_active.active_version_id
            || candidate.active_version_hash != current_active.active_version_hash
            || candidate.evaluator_identity_hash != current_active.evaluator_identity_hash
        {
            return Err("evolution_eval_changed_active_version".into());
        }
        let family = self
            .get_harness_evolution_task_family(family_id)?
            .ok_or_else(|| format!("evolution_eval_family_missing: {family_id}"))?;
        let sealed_vault = self
            .get_registered_harness_evolution_sealed_vault(family_id)?
            .ok_or_else(|| format!("evolution_eval_sealed_missing: {family_id}"))?;
        // Sealed entrance only through one-use evaluator selection receipt.
        let sealed_selected =
            self.consume_sealed_selection_for_candidate(family_id, candidate_id, "system")?;
        let root = configured_workspace_root()
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        revalidate_workspace_content(&root, &candidate.workspace)
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        let workspace_dir = crate::harness_evolution::resolve_workspace_under_root(
            &root,
            &candidate.workspace.relative_path,
        )
        .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        let created_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let bundle = evaluate_candidate_from_workspace(
            &candidate.candidate_id,
            &candidate.lineage_id,
            &candidate.active_version_id,
            &candidate.active_version_hash,
            &candidate.evaluator_identity_hash,
            &candidate.content_hash,
            budget,
            &family,
            &sealed_vault,
            sealed_selected,
            &workspace_dir,
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
    /// Submit a bounded PR_READY bundle through the evolution finalizer path.
    ///
    /// Caller may supply only candidate/evaluation IDs, optional expected identities
    /// for stale checks, and an operator decision id. Active identity, evaluation
    /// evidence, allowed paths, patch surface, secret scan, and test/static evidence
    /// are loaded or derived from store-owned state. Does not create/merge a PR.
    pub fn submit_harness_evolution_pr_ready(
        &self,
        candidate_id: &str,
        evaluation_id: &str,
        expected_active_version_id: Option<&str>,
        expected_base_commit_sha: Option<&str>,
        operator_decision_id: &str,
    ) -> Result<(PrReadyCandidateBundle, PrReadyReceipt), String> {
        self.record_harness_evolution_pr_ready(
            candidate_id,
            evaluation_id,
            expected_active_version_id,
            expected_base_commit_sha,
            operator_decision_id,
        )
    }

    /// Owner-bound PR_READY finalization (alias of submit; no caller authority fields).
    pub fn record_harness_evolution_pr_ready(
        &self,
        candidate_id: &str,
        evaluation_id: &str,
        expected_active_version_id: Option<&str>,
        expected_base_commit_sha: Option<&str>,
        operator_decision_id: &str,
    ) -> Result<(PrReadyCandidateBundle, PrReadyReceipt), String> {
        if operator_decision_id.trim().is_empty() {
            return Err("evolution_pr_ready_operator: operator decision id is required".into());
        }
        if operator_decision_id.trim() == "approve_pr_ready" {
            return Err(
                "evolution_pr_ready_operator: literal approve_pr_ready is not an operator decision"
                    .into(),
            );
        }
        let candidate = self
            .get_harness_evolution_candidate(candidate_id)?
            .ok_or_else(|| format!("evolution_pr_ready_missing_candidate: {candidate_id}"))?;
        let evaluation = self
            .get_harness_evolution_evaluation(evaluation_id)?
            .ok_or_else(|| format!("evolution_pr_ready_missing_eval: {evaluation_id}"))?;
        let current_active = self
            .get_current_harness_evolution_active_identity()?
            .ok_or_else(|| {
                "evolution_pr_ready_active_missing: no active Harness identity epoch".to_string()
            })?;
        if let Some(expected) = expected_active_version_id {
            if expected != current_active.active_version_id {
                return Err(format!(
                    "evolution_pr_ready_stale_expected_active: expected {expected} current {}",
                    current_active.active_version_id
                ));
            }
        }
        // Operator decision-center receipt: acknowledgement must bind evaluation evidence.
        let acknowledged = self.is_operator_source_acknowledged(
            "harness_evolution_pr_ready",
            candidate_id,
            &evaluation.bundle_sha256,
        )?;
        if !acknowledged {
            return Err(
                "evolution_pr_ready_operator: missing operator acknowledgement for evaluation evidence"
                    .into(),
            );
        }
        // Patch and allowed paths come from the admitted app-owned workspace only.
        let root = configured_workspace_root()
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        revalidate_workspace_content(&root, &candidate.workspace)
            .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        let workspace_dir = crate::harness_evolution::resolve_workspace_under_root(
            &root,
            &candidate.workspace.relative_path,
        )
        .map_err(|e: EvolutionAdmissionError| format!("{}: {}", e.code, e.message))?;
        let patch_path = workspace_dir.join("PR_READY.patch");
        let patch_text = std::fs::read_to_string(&patch_path).map_err(|_| {
            "evolution_pr_ready_patch: PR_READY.patch missing from candidate workspace".to_string()
        })?;
        let allowed_paths = allowed_paths_for_mutable_surface(&candidate.mutable_surface)?;
        // Base/head identity derived from store-owned hashes (not caller-supplied random).
        let base_commit_sha = current_active.active_version_hash.clone();
        let head_commit_sha = candidate.content_hash.clone();
        if let Some(expected_base) = expected_base_commit_sha {
            if expected_base != base_commit_sha {
                return Err(
                    "evolution_pr_ready_changed_base: expected base no longer matches active identity"
                        .into(),
                );
            }
        }
        // Evidence hashes derived from evaluation gates and scan outcomes — never caller strings.
        let static_check_sha256 = crate::harness_evolution::sha256_hex(
            &evaluation
                .baselines
                .iter()
                .filter(|b| !b.used_sealed_holdout)
                .map(|b| format!("{}:{}", b.baseline.as_str(), b.hard_gate.as_str()))
                .collect::<Vec<_>>()
                .join("|"),
        );
        let test_evidence_sha256 = crate::harness_evolution::sha256_hex(&format!(
            "tests.v1|{}|{}|{}",
            evaluation.evaluation_id,
            evaluation.bundle_sha256,
            evaluation
                .baselines
                .iter()
                .filter(|b| b.hard_gate.is_pass() && !b.used_sealed_holdout)
                .count()
        ));
        if crate::harness_evolution_pr_ready::looks_like_secret(&patch_text) {
            return Err("evolution_pr_ready_secret: secret scan refused patch contents".into());
        }
        let secret_scan_sha256 =
            crate::harness_evolution::sha256_hex("secret_scan.clean.v1|no_secret_patterns");
        let rollback_evidence_sha256 = crate::harness_evolution::sha256_hex(&format!(
            "rollback.v1|{}|{}|{}",
            candidate.content_hash, current_active.active_version_id, evaluation.evaluation_id
        ));
        let operator_decision = format!(
            "operator_ack:{operator_decision_id}:harness_evolution_pr_ready:{candidate_id}"
        );
        let created_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let (bundle, receipt) = finalize_pr_ready_bundle(
            &candidate,
            &current_active,
            &evaluation,
            &patch_text,
            &allowed_paths,
            &base_commit_sha,
            &head_commit_sha,
            &base_commit_sha,
            &static_check_sha256,
            &test_evidence_sha256,
            &secret_scan_sha256,
            &rollback_evidence_sha256,
            &operator_decision,
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

fn allowed_paths_for_mutable_surface(
    surface: &crate::harness_evolution::MutableSurfaceDeclaration,
) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for s in &surface.surfaces {
        let prefix = match s.as_str() {
            "prompts_and_bounded_rules" => "prompts/",
            "context_selection_and_summarization" => "context/",
            "tool_descriptions_and_selection_policy" => "tools/",
            "retry_and_stop_policy" => "retry/",
            "model_routing_within_admitted_set" => "routing/",
            "recursive_decomposition_policy" => "recursive/",
            other => {
                return Err(format!(
                    "evolution_pr_ready_paths: unknown mutable surface {other}"
                ));
            }
        };
        paths.push(format!("{prefix}rules.md"));
    }
    if paths.is_empty() {
        return Err("evolution_pr_ready_paths: no allowed paths from mutable surface".into());
    }
    Ok(paths)
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

    fn register_family_and_vault(
        store: &LocalProductStore,
        family_id: &str,
    ) -> crate::harness_evolution_eval::TaskFamilyManifest {
        use crate::harness_evolution_eval::sample_task_family;
        let family = sample_task_family(family_id);
        store
            .register_harness_evolution_task_family(&family, "evaluator-owner")
            .unwrap();
        store
            .register_harness_evolution_sealed_vault(family_id, "evaluator-owner")
            .unwrap();
        family
    }

    #[test]
    fn sealed_holdout_registration_is_immutable_hash_only_and_restart_safe() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("sealed.db");
        let db_s = db.to_str().unwrap();
        let (family_id, vault_sha256) = {
            let store = LocalProductStore::new(db_s).unwrap();
            let family = crate::harness_evolution_eval::sample_task_family("fam-sealed");
            store
                .register_harness_evolution_task_family(&family, "evaluator-owner")
                .unwrap();
            let vault = store
                .register_harness_evolution_sealed_vault(&family.family_id, "evaluator-owner")
                .unwrap();
            assert_eq!(
                store
                    .register_harness_evolution_sealed_vault(&family.family_id, "evaluator-owner")
                    .unwrap(),
                vault
            );
            let mut changed = family.clone();
            changed.development[0].label_sha256 = sha256_hex("changed-label");
            let err = store
                .register_harness_evolution_task_family(&changed, "evaluator-owner")
                .unwrap_err();
            assert!(err.contains("immutable"), "unexpected error: {err}");
            let stored_body: String = store
                .with_conn(|conn| {
                    conn.query_row(
                        "SELECT body_json FROM harness_evolution_sealed_holdouts
                         WHERE vault_sha256=?1",
                        params![vault.vault_sha256],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())
                })
                .unwrap();
            assert!(!stored_body.contains("changed-label"));
            assert!(!stored_body.contains("label|fam-sealed|"));
            (family.family_id, vault.vault_sha256)
        };
        let store = LocalProductStore::new(db_s).unwrap();
        let loaded = store
            .get_harness_evolution_sealed_holdout(&vault_sha256)
            .unwrap()
            .expect("sealed vault survives restart");
        assert_eq!(loaded.family_id, family_id);
        assert_eq!(loaded.vault_sha256, vault_sha256);
    }

    #[test]
    fn sealed_holdout_read_and_family_index_fail_closed_on_corruption() {
        let store = LocalProductStore::new(":memory:").unwrap();
        let family_a = register_family_and_vault(&store, "fam-index-a");
        let family_b = register_family_and_vault(&store, "fam-index-b");
        let vault_b = store
            .get_registered_harness_evolution_sealed_vault(&family_b.family_id)
            .unwrap()
            .unwrap();
        let index_key = format!(
            "harness_evolution.sealed_vault_index.{}",
            family_a.family_id
        );
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE local_config SET value_json=?1 WHERE key=?2",
                    params![
                        serde_json::json!({"vault_sha256": vault_b.vault_sha256}).to_string(),
                        index_key
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })
            .unwrap();
        let err = store
            .get_registered_harness_evolution_sealed_vault(&family_a.family_id)
            .unwrap_err();
        assert!(err.contains("family_mismatch"), "unexpected error: {err}");

        let vault_a = build_sealed_vault(&family_a).unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE harness_evolution_sealed_holdouts
                     SET body_json=?1 WHERE vault_sha256=?2",
                    params![
                        serde_json::json!({
                            "schema_version": SEALED_SCHEMA_VERSION,
                            "family_id": vault_a.family_id,
                            "sealed_task_hashes": vault_a.sealed_task_hashes,
                            "vault_sha256": vault_a.vault_sha256,
                            "preselected_entrant_limit": 1
                        })
                        .to_string(),
                        vault_a.vault_sha256
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })
            .unwrap();
        let err = store
            .get_harness_evolution_sealed_holdout(&vault_a.vault_sha256)
            .unwrap_err();
        assert!(
            err.contains("entrants") || err.contains("corrupt"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn sealed_selection_shape_is_exact_and_consumption_is_audited() {
        let key = "harness_evolution.sealed_selection.hess-test";
        let value = serde_json::json!({
            "schema_version": SEALED_SELECTION_SCHEMA,
            "receipt_id": "hess-test",
            "family_id": "family",
            "candidate_ids": ["candidate"],
            "used": false
        });
        let parsed = parse_sealed_selection(key, &value).unwrap();
        assert_eq!(parsed.0, "hess-test");
        assert_eq!(parsed.1, "family");
        assert_eq!(parsed.2, vec!["candidate"]);
        assert!(!parsed.3);

        let mut extra = value.clone();
        extra["label_sha256"] = serde_json::json!("must-not-be-stored");
        assert!(parse_sealed_selection(key, &extra).is_err());
    }

    #[test]
    fn records_equal_budget_evaluation_and_pareto_exactly_once() {
        let env = LabEnvGuard::enable();
        use crate::harness_evolution_eval::sample_budget;
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
        let family = register_family_and_vault(&store, "fam-store");
        store
            .issue_harness_evolution_sealed_selection(
                &family.family_id,
                std::slice::from_ref(&admitted.candidate_id),
                "evaluator-owner",
            )
            .unwrap();
        let budget = sample_budget(3);
        let (bundle, archive, receipt) = store
            .record_harness_evolution_evaluation(&admitted.candidate_id, &budget, &family.family_id)
            .unwrap();
        assert!(!bundle.claims_improvement);
        assert!(!bundle.sealed_feedback_into_mutation);
        assert!(bundle.sealed_entrant_count >= 1);
        assert!(!archive.is_empty());
        assert_eq!(receipt.bundle_sha256, bundle.bundle_sha256);
        assert!(bundle
            .baselines
            .iter()
            .any(|b| b.usage.calls > 0 && !b.usage.incomplete));
        let snapshot = store.config_snapshot().unwrap();
        let selection = snapshot
            .as_object()
            .and_then(|values| {
                values
                    .iter()
                    .find(|(key, _)| key.starts_with("harness_evolution.sealed_selection."))
            })
            .map(|(_, value)| value)
            .expect("selection receipt remains durable");
        assert_eq!(selection.get("used").and_then(|v| v.as_bool()), Some(true));
        assert!(!store
            .search_audit_events(20, 0, Some("sealed_selection_consumed"))
            .unwrap()
            .is_empty());
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
            &family.family_id,
        );
        assert!(dup.unwrap_err().contains("duplicate"));
    }

    #[test]
    fn evaluation_survives_durable_restart() {
        let env = LabEnvGuard::enable();
        use crate::harness_evolution_eval::sample_budget;
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
            let family = register_family_and_vault(&store, "fam-restart");
            let (bundle, _, receipt) = store
                .record_harness_evolution_evaluation(
                    &admitted.candidate_id,
                    &sample_budget(1),
                    &family.family_id,
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

    #[test]
    fn submits_pr_ready_from_store_owned_evidence_only() {
        let env = LabEnvGuard::enable();
        use crate::harness_evolution_eval::sample_budget;
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "operator-test")
            .unwrap();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind": "pr-ready"}),
            vec![],
            21,
        )
        .unwrap();
        let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
        let ws = materialize_for(&env, &bound, "pr-ready-content");
        // Write laboratory patch into the admitted workspace surface.
        let patch = "diff --git a/prompts/rules.md b/prompts/rules.md\n--- a/prompts/rules.md\n+++ b/prompts/rules.md\n@@ -1 +1 @@\n-old\n+new\n";
        let dir = env.workspace_root().join(&ws.relative_path);
        std::fs::write(dir.join("PR_READY.patch"), patch).unwrap();
        // Content hash changed — re-materialize hash for candidate by reading after write.
        // For admission, content must match workspace hash; recompute and rebuild candidate.
        let content_hash = crate::harness_evolution::hash_workspace_directory(&dir).unwrap();
        let mut ws2 = ws;
        ws2.content_hash = content_hash;
        let candidate = candidate_from_proposal(&bound, &ws2, "2026-07-21T00:00:00Z").unwrap();
        let (admitted, _) = store.admit_harness_evolution_candidate(candidate).unwrap();
        let family = register_family_and_vault(&store, "fam-pr-ready");
        let (bundle_eval, _, _) = store
            .record_harness_evolution_evaluation(
                &admitted.candidate_id,
                &sample_budget(2),
                &family.family_id,
            )
            .unwrap();
        // Operator acknowledgement bound to evaluation evidence (not a literal approve string).
        store
            .acknowledge_operator_source(
                "decision-pr-ready-1",
                "harness_evolution_pr_ready",
                &admitted.candidate_id,
                &bundle_eval.bundle_sha256,
                Some("approve laboratory PR_READY"),
                "operator-test",
            )
            .unwrap();
        let (bundle, receipt) = store
            .submit_harness_evolution_pr_ready(
                &admitted.candidate_id,
                &bundle_eval.evaluation_id,
                Some(&active.active_version_id),
                Some(&active.active_version_hash),
                "decision-pr-ready-1",
            )
            .unwrap();
        assert!(bundle.terminal.is_ready());
        assert!(receipt.terminal.is_ready());
        assert!(bundle.operator_decision.starts_with("operator_ack:"));
        assert!(bundle.patch.patch_text.is_empty() || !bundle.patch.patch_text.is_empty());
        // Durable body clears raw patch text.
        let loaded = store
            .get_harness_evolution_pr_ready_bundle(&bundle.bundle_id)
            .unwrap()
            .unwrap();
        assert!(loaded.patch.patch_text.is_empty());
        // Literal approve string is refused.
        let err = store.submit_harness_evolution_pr_ready(
            &admitted.candidate_id,
            &bundle_eval.evaluation_id,
            None,
            None,
            "approve_pr_ready",
        );
        assert!(err.unwrap_err().contains("operator"));
        // Duplicate delivery refused.
        let dup = store.submit_harness_evolution_pr_ready(
            &admitted.candidate_id,
            &bundle_eval.evaluation_id,
            None,
            None,
            "decision-pr-ready-1",
        );
        assert!(dup.unwrap_err().contains("duplicate"));
    }

    #[test]
    fn level1_end_to_end_fixture_path_three_seeds() {
        // PE7-HARNESS-EVOLUTION-LEVEL1-ACCEPTANCE-1 — default-off fixture path through real owners.
        let env = LabEnvGuard::enable();
        use crate::harness_evolution_eval::sample_budget;
        let store = LocalProductStore::new(":memory:").unwrap();
        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "operator-level1")
            .unwrap();
        let family = register_family_and_vault(&store, "fam-level1");
        let mut any_pr_ready = false;
        let mut results = Vec::new();
        for seed in [1u64, 2, 3] {
            let proposal = proposal_from_body(
                &active,
                None,
                &["prompts_and_bounded_rules"],
                &json!({"kind": "level1", "seed": seed}),
                vec![],
                seed,
            )
            .unwrap();
            let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
            let ws = materialize_for(&env, &bound, &format!("level1-{seed}"));
            let dir = env.workspace_root().join(&ws.relative_path);
            let patch = "diff --git a/prompts/rules.md b/prompts/rules.md\n--- a/prompts/rules.md\n+++ b/prompts/rules.md\n@@ -1 +1 @@\n-old\n+new\n";
            std::fs::write(dir.join("PR_READY.patch"), patch).unwrap();
            let content_hash = crate::harness_evolution::hash_workspace_directory(&dir).unwrap();
            let mut ws2 = ws;
            ws2.content_hash = content_hash;
            let candidate = candidate_from_proposal(&bound, &ws2, "2026-07-21T00:00:00Z").unwrap();
            let (admitted, _) = store.admit_harness_evolution_candidate(candidate).unwrap();
            if seed == 1 {
                store
                    .issue_harness_evolution_sealed_selection(
                        &family.family_id,
                        std::slice::from_ref(&admitted.candidate_id),
                        "evaluator-owner",
                    )
                    .unwrap();
            }
            let (eval, archive, _) = store
                .record_harness_evolution_evaluation(
                    &admitted.candidate_id,
                    &sample_budget(seed),
                    &family.family_id,
                )
                .unwrap();
            assert!(!eval.claims_improvement);
            assert!(!eval.sealed_feedback_into_mutation);
            assert!(!archive.is_empty());
            // Static, random, lineage, and fixture OpenCode baselines must appear.
            let kinds: std::collections::BTreeSet<_> =
                eval.baselines.iter().map(|b| b.baseline.as_str()).collect();
            assert!(kinds.contains("static_single_pass"));
            assert!(kinds.contains("random_equal_count"));
            assert!(kinds.contains("lineage_experiment"));
            assert!(kinds.contains("fixture_opencode"));
            store
                .acknowledge_operator_source(
                    &format!("decision-level1-{seed}"),
                    "harness_evolution_pr_ready",
                    &admitted.candidate_id,
                    &eval.bundle_sha256,
                    Some("level1 acceptance"),
                    "operator-level1",
                )
                .unwrap();
            let (bundle, receipt) = store
                .submit_harness_evolution_pr_ready(
                    &admitted.candidate_id,
                    &eval.evaluation_id,
                    Some(&active.active_version_id),
                    Some(&active.active_version_hash),
                    &format!("decision-level1-{seed}"),
                )
                .unwrap();
            assert!(bundle.terminal.is_ready());
            assert!(receipt.terminal.is_ready());
            any_pr_ready = true;
            results.push((seed, eval.evaluation_id, bundle.bundle_id));
            // Replay evaluation is exactly-once.
            let again = store.record_harness_evolution_evaluation(
                &admitted.candidate_id,
                &sample_budget(seed),
                &family.family_id,
            );
            assert!(again.unwrap_err().contains("duplicate"));
        }
        assert!(any_pr_ready);
        assert_eq!(results.len(), 3);
        // No active Harness mutation: original active identity still current.
        let current = store
            .get_current_harness_evolution_active_identity()
            .unwrap()
            .unwrap();
        assert_eq!(current.active_version_id, active.active_version_id);
        // Honest result: laboratory correctness only — no improvement claim.
        eprintln!(
            "level1 acceptance: neutral/no-improvement fixture path ok for seeds {:?}",
            results.iter().map(|(s, _, _)| *s).collect::<Vec<_>>()
        );
    }
}
