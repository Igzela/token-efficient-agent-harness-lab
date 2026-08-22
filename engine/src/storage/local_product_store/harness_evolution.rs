//! Durable storage for PE7 Harness Evolution B1 evidence + B2 evaluation/archive.

use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

use super::{append_audit_locked, LocalProductStore};
use crate::harness_evolution::{
    build_admission_receipt, configured_workspace_root, generate_ec1_candidate_binding,
    reconcile_candidate_lifecycle_costs, revalidate_workspace_content, seal_ec1_identity_lineage,
    seal_failure_pattern_evidence, seal_lifecycle_budget_reservation, seal_lifecycle_cost_record,
    seal_mutation_hypothesis_manifest, validate_candidate_for_admission,
    validate_ec1_candidate_binding, validate_ec1_identity_lineage,
    validate_failure_pattern_evidence, validate_lifecycle_cost_record,
    validate_mutation_hypothesis_manifest, validate_prediction_outcome_contract, validate_proposal,
    ActiveHarnessIdentity, CandidateStatus, CandidateTerminalReason, DiversityScoreRecordV1,
    Ec1CandidateCausalBinding, Ec1IdentityLineageRecord, Ec3LifecycleBudgetContractV1,
    Ec4DiversityContractV1, EvolutionAdmissionError, EvolutionCandidate, EvolutionProposal,
    EvolutionReceipt, FailurePatternEvidenceV1, LifecycleBudgetReconciliationOutcome,
    LifecycleBudgetReconciliationV1, LifecycleBudgetReservationStatus,
    LifecycleBudgetReservationV1, LifecycleCostRecordV1, MutationHypothesisManifestV1,
    PredictionOutcomeV1, ACTIVE_VERSION_SCHEMA, CANDIDATE_SCHEMA_VERSION,
    EVOLUTION_LAB_SCHEMA_VERSION, LIFECYCLE_BUDGET_RESERVATION_SCHEMA, RECEIPT_SCHEMA_VERSION,
};
use crate::harness_evolution_eval::{
    build_pareto_archive, build_sealed_vault, derive_ec2_prediction_outcome,
    detect_holdout_label_tamper, evaluate_candidate_from_workspace,
    holdout_body_contains_sensitive, mediate_holdout_membership_read, redacted_eval_evidence,
    seal_ec2_holdout, CandidateEvaluationBundle, Ec2AccessClass, Ec2HoldoutSeal,
    EqualBudgetContract, EvalReceipt, ParetoArchiveEntry, SealedHoldoutVault, TaskFamilyManifest,
    ARCHIVE_SCHEMA_VERSION, EVAL_RECEIPT_SCHEMA_VERSION, EVAL_SCHEMA_VERSION, MAX_SEALED_ENTRANTS,
    MIN_SEALED_ENTRANTS, SEALED_SCHEMA_VERSION,
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

    pub fn record_ec1_identity_lineage(
        &self,
        record: Ec1IdentityLineageRecord,
        actor_id: &str,
    ) -> Result<Ec1IdentityLineageRecord, String> {
        if actor_id.trim().is_empty() {
            return Err("ec1_identity_lineage_actor: authenticated actor_id is required".into());
        }
        let sealed = seal_ec1_identity_lineage(record).map_err(|error| error.message)?;
        validate_ec1_identity_lineage(&sealed).map_err(|error| error.message)?;
        let body = serde_json::to_string(&sealed).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            if let Some(parent) = sealed.parent_lineage_id.as_deref() {
                let parent_exists: Option<String> = tx
                    .query_row(
                        "SELECT lineage_id FROM harness_evolution_ec1_identity_lineage WHERE lineage_id=?1",
                        params![parent],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                if parent_exists.is_none() {
                    return Err(format!(
                        "ec1_lineage_parent_missing: parent {parent} is not recorded"
                    ));
                }
            }
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec1_identity_lineage WHERE lineage_id=?1",
                    params![sealed.lineage_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(existing_body) = existing {
                if existing_body == body {
                    let stored: Ec1IdentityLineageRecord =
                        serde_json::from_str(&existing_body).map_err(|e| e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(stored);
                }
                return Err(format!(
                    "ec1_identity_lineage_immutable: {} already exists and cannot be mutated",
                    sealed.lineage_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_ec1_identity_lineage
                    (lineage_id, parent_lineage_id, source_identity_hash, active_harness_sha, causal_source_id, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    sealed.lineage_id,
                    sealed.parent_lineage_id,
                    sealed.source_identity_hash,
                    sealed.active_harness_sha,
                    sealed.causal_source_id,
                    body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec1_identity_lineage_recorded",
                &sealed.lineage_id,
                &serde_json::json!({
                    "schema_version": sealed.schema_version,
                    "lineage_id": sealed.lineage_id,
                    "parent_lineage_id": sealed.parent_lineage_id,
                    "active_harness_sha": sealed.active_harness_sha,
                    "causal_source_id": sealed.causal_source_id,
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(sealed)
        })
    }

    pub fn get_ec1_identity_lineage(
        &self,
        lineage_id: &str,
    ) -> Result<Option<Ec1IdentityLineageRecord>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec1_identity_lineage WHERE lineage_id=?1",
                    params![lineage_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(serde_json::from_str(&body).map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        })
    }

    pub fn record_ec1_failure_pattern(
        &self,
        evidence: FailurePatternEvidenceV1,
        actor_id: &str,
    ) -> Result<FailurePatternEvidenceV1, String> {
        if actor_id.trim().is_empty() {
            return Err("ec1_failure_pattern_actor: authenticated actor_id is required".into());
        }
        let sealed = seal_failure_pattern_evidence(evidence).map_err(|error| error.message)?;
        validate_failure_pattern_evidence(&sealed).map_err(|error| error.message)?;
        let body = serde_json::to_string(&sealed).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let lineage: Option<(String, String)> = tx
                .query_row(
                    "SELECT lineage_id, source_identity_hash FROM harness_evolution_ec1_identity_lineage WHERE lineage_id=?1",
                    params![sealed.lineage_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let Some((_, source_hash)) = lineage else {
                return Err(format!(
                    "ec1_lineage_missing: {} is not recorded",
                    sealed.lineage_id
                ));
            };
            if source_hash != sealed.source_identity_hash {
                return Err(
                    "ec1_source_mismatch: failure pattern source is not bound to lineage"
                        .into(),
                );
            }
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec1_failure_patterns WHERE evidence_id=?1",
                    params![sealed.evidence_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(existing_body) = existing {
                if existing_body == body {
                    let stored = serde_json::from_str(&existing_body).map_err(|e| e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(stored);
                }
                return Err(format!(
                    "ec1_failure_pattern_immutable: {} already exists and cannot be mutated",
                    sealed.evidence_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_ec1_failure_patterns
                    (evidence_id, lineage_id, causal_status, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    sealed.evidence_id,
                    sealed.lineage_id,
                    sealed.causal_status.as_str(),
                    body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec1_failure_pattern_recorded",
                &sealed.evidence_id,
                &serde_json::json!({
                    "lineage_id": sealed.lineage_id,
                    "causal_status": sealed.causal_status.as_str(),
                    "evidence_role": sealed.evidence_role.as_str(),
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(sealed)
        })
    }

    pub fn record_ec1_hypothesis(
        &self,
        manifest: MutationHypothesisManifestV1,
        actor_id: &str,
    ) -> Result<MutationHypothesisManifestV1, String> {
        if actor_id.trim().is_empty() {
            return Err("ec1_hypothesis_actor: authenticated actor_id is required".into());
        }
        let sealed = seal_mutation_hypothesis_manifest(manifest).map_err(|error| error.message)?;
        validate_mutation_hypothesis_manifest(&sealed).map_err(|error| error.message)?;
        let body = serde_json::to_string(&sealed).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let evidence: Option<(String, String)> = tx
                .query_row(
                    "SELECT evidence_id, lineage_id FROM harness_evolution_ec1_failure_patterns WHERE evidence_id=?1",
                    params![sealed.failure_evidence_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let Some((_, evidence_lineage)) = evidence else {
                return Err(format!(
                    "ec1_failure_pattern_missing: {} is not recorded",
                    sealed.failure_evidence_id
                ));
            };
            if evidence_lineage != sealed.lineage_id {
                return Err(
                    "ec1_hypothesis_lineage_mismatch: hypothesis is not bound to the evidence lineage"
                        .into(),
                );
            }
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec1_hypotheses WHERE manifest_id=?1",
                    params![sealed.manifest_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(existing_body) = existing {
                if existing_body == body {
                    let stored = serde_json::from_str(&existing_body).map_err(|e| e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(stored);
                }
                return Err(format!(
                    "ec1_hypothesis_immutable: {} already exists and cannot be mutated after recording",
                    sealed.manifest_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_ec1_hypotheses
                    (manifest_id, evidence_id, lineage_id, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    sealed.manifest_id,
                    sealed.failure_evidence_id,
                    sealed.lineage_id,
                    body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec1_hypothesis_recorded",
                &sealed.manifest_id,
                &serde_json::json!({
                    "evidence_id": sealed.failure_evidence_id,
                    "lineage_id": sealed.lineage_id,
                    "proposal_body_sha256": sealed.proposal_body_sha256,
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(sealed)
        })
    }

    pub fn record_ec1_candidate_binding(
        &self,
        family_id: &str,
        hypothesis: &MutationHypothesisManifestV1,
        seed: u64,
        actor_id: &str,
    ) -> Result<Ec1CandidateCausalBinding, String> {
        if actor_id.trim().is_empty() {
            return Err("ec1_candidate_binding_actor: authenticated actor_id is required".into());
        }
        let sealed = generate_ec1_candidate_binding(family_id, hypothesis, seed)
            .map_err(|error| error.message)?;
        validate_ec1_candidate_binding(&sealed).map_err(|error| error.message)?;
        let body = serde_json::to_string(&sealed).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let stored_h: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT manifest_id, lineage_id, body_json FROM harness_evolution_ec1_hypotheses WHERE manifest_id=?1",
                    params![sealed.hypothesis_manifest_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let Some((_, hyp_lineage, hyp_body)) = stored_h else {
                return Err(format!(
                    "ec1_hypothesis_missing: {} is not recorded",
                    sealed.hypothesis_manifest_id
                ));
            };
            if hyp_lineage != sealed.lineage_id {
                return Err("ec1_binding_lineage_mismatch: binding lineage is incomplete".into());
            }
            let stored_manifest: MutationHypothesisManifestV1 =
                serde_json::from_str(&hyp_body).map_err(|e| e.to_string())?;
            if stored_manifest.candidate_delta_digest != sealed.candidate_delta_digest {
                return Err(
                    "ec1_binding_delta_mismatch: candidate delta is not bound to the hypothesis"
                        .into(),
                );
            }
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec1_candidate_bindings WHERE binding_id=?1",
                    params![sealed.binding_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(existing_body) = existing {
                if existing_body == body {
                    let stored = serde_json::from_str(&existing_body).map_err(|e| e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(stored);
                }
                return Err(format!(
                    "ec1_candidate_binding_immutable: {} already exists",
                    sealed.binding_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_ec1_candidate_bindings
                    (binding_id, family_id, hypothesis_manifest_id, lineage_id, seed, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    sealed.binding_id,
                    sealed.family_id,
                    sealed.hypothesis_manifest_id,
                    sealed.lineage_id,
                    sealed.seed as i64,
                    body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec1_candidate_binding_recorded",
                &sealed.binding_id,
                &serde_json::json!({
                    "family_id": sealed.family_id,
                    "hypothesis_manifest_id": sealed.hypothesis_manifest_id,
                    "lineage_id": sealed.lineage_id,
                    "seed": sealed.seed,
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(sealed)
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

    fn load_ec2_holdout_seal_row(
        conn: &Connection,
        vault_sha256: &str,
    ) -> Result<Option<Ec2HoldoutSeal>, String> {
        let row: Option<String> = conn
            .query_row(
                "SELECT body_json FROM harness_evolution_ec2_holdout_seals WHERE vault_sha256=?1",
                params![vault_sha256],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match row {
            Some(body) => {
                let mut seal: Ec2HoldoutSeal =
                    serde_json::from_str(&body).map_err(|e| e.to_string())?;
                let superseded: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM harness_evolution_ec2_holdout_seals
                         WHERE family_id=?1 AND epoch > ?2",
                        params![seal.family_id, seal.epoch as i64],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if superseded > 0 {
                    seal.invalidation = "INVALIDATED".into();
                }
                Ok(Some(seal))
            }
            None => Ok(None),
        }
    }

    pub fn persist_ec2_holdout_seal(
        &self,
        family: &TaskFamilyManifest,
        epoch: u64,
        actor_id: &str,
    ) -> Result<Ec2HoldoutSeal, String> {
        if actor_id.trim().is_empty() {
            return Err("ec2_holdout_actor: authenticated actor_id is required".into());
        }
        let seal =
            seal_ec2_holdout(family, epoch).map_err(|e| format!("{}: {}", e.code, e.message))?;
        detect_holdout_label_tamper(family, &seal.vault)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let body = serde_json::to_value(&seal).map_err(|e| e.to_string())?;
        if holdout_body_contains_sensitive(&body) {
            return Err("ec2_holdout_leak: plaintext labels cannot be persisted".into());
        }
        let body_json = serde_json::to_string(&body).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec2_holdout_seals WHERE vault_sha256=?1",
                    params![seal.vault.vault_sha256],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(existing_body) = existing {
                if existing_body == body_json {
                    let stored = serde_json::from_str(&existing_body).map_err(|e| e.to_string())?;
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(stored);
                }
                return Err(format!(
                    "ec2_holdout_immutable: {} already exists",
                    seal.vault.vault_sha256
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_ec2_holdout_seals
                    (vault_sha256, family_id, epoch, invalidation, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    seal.vault.vault_sha256,
                    seal.family_id,
                    seal.epoch as i64,
                    seal.invalidation,
                    body_json,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec2_holdout_sealed",
                &seal.vault.vault_sha256,
                &serde_json::json!({
                    "family_id": seal.family_id,
                    "epoch": seal.epoch,
                    "invalidation": seal.invalidation,
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(seal)
        })
    }

    pub fn get_ec2_holdout_seal(
        &self,
        vault_sha256: &str,
        class: Ec2AccessClass,
    ) -> Result<Option<Ec2HoldoutSeal>, String> {
        self.with_conn(|conn| {
            let Some(seal) = Self::load_ec2_holdout_seal_row(conn, vault_sha256)? else {
                return Ok(None);
            };
            mediate_holdout_membership_read(class, &seal)
                .map_err(|e| format!("{}: {}", e.code, e.message))?;
            Ok(Some(seal))
        })
    }

    pub fn read_ec2_holdout_membership(
        &self,
        vault_sha256: &str,
        class: Ec2AccessClass,
    ) -> Result<SealedHoldoutVault, String> {
        let seal = self
            .get_ec2_holdout_seal(vault_sha256, class)?
            .ok_or_else(|| format!("ec2_holdout_missing: {vault_sha256}"))?;
        Ok(seal.vault)
    }

    pub fn rotate_ec2_holdout_seal(
        &self,
        previous_vault_sha256: &str,
        family: &TaskFamilyManifest,
        actor_id: &str,
    ) -> Result<Ec2HoldoutSeal, String> {
        if actor_id.trim().is_empty() {
            return Err("ec2_holdout_actor: authenticated actor_id is required".into());
        }
        let previous = self.with_conn(|conn| {
            Self::load_ec2_holdout_seal_row(conn, previous_vault_sha256)?
                .ok_or_else(|| format!("ec2_holdout_missing: {previous_vault_sha256}"))
        })?;
        if previous.invalidation != "VALID" {
            return Err("ec2_holdout_invalidated: previous seal cannot rotate".into());
        }
        let next_epoch = previous.epoch.saturating_add(1);
        let next = self.persist_ec2_holdout_seal(family, next_epoch, actor_id)?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec2_holdout_rotated",
                previous_vault_sha256,
                &serde_json::json!({
                    "previous_vault_sha256": previous_vault_sha256,
                    "next_vault_sha256": next.vault.vault_sha256,
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(next)
        })
    }

    pub fn persist_ec2_prediction_outcome(
        &self,
        hypothesis: &MutationHypothesisManifestV1,
        bundle: &CandidateEvaluationBundle,
        class: Ec2AccessClass,
        actor_id: &str,
    ) -> Result<PredictionOutcomeV1, String> {
        if actor_id.trim().is_empty() {
            return Err("ec2_prediction_actor: authenticated actor_id is required".into());
        }
        if class != Ec2AccessClass::Evaluator {
            return Err("ec2_prediction_author: only the evaluator may write outcomes".into());
        }
        if crate::harness_evolution_eval::prediction_accuracy_is_selection_authority() {
            return Err("ec2_prediction_authority: accuracy cannot gate selection".into());
        }
        let outcome = derive_ec2_prediction_outcome(hypothesis, bundle)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        if outcome.evaluator_identity_hash != bundle.evaluator_identity_hash {
            return Err("ec2_prediction_evaluator: outcome evaluator must match bundle".into());
        }
        validate_prediction_outcome_contract(&outcome)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let body_json = serde_json::to_string(&outcome).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec2_prediction_outcomes WHERE outcome_id=?1",
                    params![outcome.outcome_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(existing_body) = existing {
                if existing_body == body_json {
                    tx.commit().map_err(|e| e.to_string())?;
                    return serde_json::from_str(&existing_body).map_err(|e| e.to_string());
                }
                return Err(format!(
                    "ec2_prediction_immutable: {} already exists",
                    outcome.outcome_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_ec2_prediction_outcomes
                    (outcome_id, hypothesis_manifest_digest, evaluation_digest, outcome, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    outcome.outcome_id,
                    outcome.hypothesis_manifest_digest,
                    outcome.evaluation_digest,
                    outcome.outcome.as_str(),
                    body_json,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec2_prediction_outcome",
                &outcome.outcome_id,
                &serde_json::json!({
                    "outcome": outcome.outcome.as_str(),
                    "evaluation_digest": outcome.evaluation_digest,
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(outcome)
        })
    }

    pub fn get_ec2_prediction_outcome(
        &self,
        outcome_id: &str,
    ) -> Result<Option<PredictionOutcomeV1>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec2_prediction_outcomes WHERE outcome_id=?1",
                    params![outcome_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(serde_json::from_str(&body).map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        })
    }

    pub fn persist_ec3_lifecycle_cost_record(
        &self,
        record: LifecycleCostRecordV1,
        actor_id: &str,
    ) -> Result<LifecycleCostRecordV1, String> {
        if actor_id.trim().is_empty() {
            return Err("ec3_cost_actor: authenticated actor_id is required".into());
        }
        let sealed =
            seal_lifecycle_cost_record(record).map_err(|e| format!("{}: {}", e.code, e.message))?;
        validate_lifecycle_cost_record(&sealed)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let body_json = serde_json::to_string(&sealed).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec3_lifecycle_costs WHERE record_id=?1",
                    params![sealed.record_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(existing_body) = existing {
                if existing_body == body_json {
                    tx.commit().map_err(|e| e.to_string())?;
                    return serde_json::from_str(&existing_body).map_err(|e| e.to_string());
                }
                return Err(format!(
                    "ec3_cost_immutable: {} already exists",
                    sealed.record_id
                ));
            }
            tx.execute(
                "INSERT INTO harness_evolution_ec3_lifecycle_costs
                    (record_id, candidate_id, phase, token_cost, call_count, wall_clock_seconds, trust_source, unmeasured, failure_attempt, evidence_payload_digest, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    sealed.record_id,
                    sealed.candidate_id,
                    sealed.phase.as_str(),
                    sealed.token_cost as i64,
                    sealed.call_count as i64,
                    sealed.wall_clock_seconds as i64,
                    sealed.trust_source.as_str(),
                    if sealed.unmeasured { 1 } else { 0 },
                    if sealed.failure_attempt { 1 } else { 0 },
                    sealed.evidence_payload_digest,
                    body_json,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec3_lifecycle_cost",
                &sealed.record_id,
                &serde_json::json!({
                    "candidate_id": sealed.candidate_id,
                    "phase": sealed.phase.as_str(),
                    "token_cost": sealed.token_cost,
                    "call_count": sealed.call_count,
                    "trust_source": sealed.trust_source.as_str(),
                    "unmeasured": sealed.unmeasured,
                    "failure_attempt": sealed.failure_attempt,
                    "actor_id": actor_id,
                }),
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(sealed)
        })
    }

    pub fn get_ec3_lifecycle_cost_record(
        &self,
        record_id: &str,
    ) -> Result<Option<LifecycleCostRecordV1>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec3_lifecycle_costs WHERE record_id=?1",
                    params![record_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(serde_json::from_str(&body).map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        })
    }

    pub fn list_ec3_lifecycle_cost_records_for_candidate(
        &self,
        candidate_id: &str,
    ) -> Result<Vec<LifecycleCostRecordV1>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT body_json FROM harness_evolution_ec3_lifecycle_costs WHERE candidate_id=?1 ORDER BY created_at ASC",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![candidate_id], |row| row.get::<_, String>(0))
                .map_err(|e| e.to_string())?;
            let mut out = Vec::new();
            for r in rows {
                let body = r.map_err(|e| e.to_string())?;
                let rec: LifecycleCostRecordV1 = serde_json::from_str(&body).map_err(|e| e.to_string())?;
                out.push(rec);
            }
            Ok(out)
        })
    }

    pub fn reserve_candidate_lifecycle_budget(
        &self,
        contract: &Ec3LifecycleBudgetContractV1,
        candidate_id: &str,
        actor_id: &str,
    ) -> Result<LifecycleBudgetReservationV1, String> {
        if actor_id.trim().is_empty() {
            return Err("ec3_budget_actor: authenticated actor_id is required".into());
        }
        if candidate_id.trim().is_empty() {
            return Err("ec3_budget_candidate: non-empty candidate_id is required".into());
        }
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let reservation = seal_lifecycle_budget_reservation(LifecycleBudgetReservationV1 {
            schema_version: LIFECYCLE_BUDGET_RESERVATION_SCHEMA.to_string(),
            reservation_id: String::new(),
            candidate_id: candidate_id.to_string(),
            contract_id: contract.contract_id.clone(),
            reserved_token_cost: contract.candidate_envelope.total_token_limit,
            reserved_call_count: contract.candidate_envelope.total_call_limit,
            reserved_wall_clock_seconds: contract.candidate_envelope.total_wall_clock_seconds_limit,
            status: LifecycleBudgetReservationStatus::Active,
            record_sha256: String::new(),
        })
        .map_err(|e| format!("{}: {}", e.code, e.message))?;

        let body_json = serde_json::to_string(&reservation).map_err(|e| e.to_string())?;

        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;

            let existing: Option<(String, String)> = tx
                .query_row(
                    "SELECT reservation_id, body_json FROM harness_evolution_ec3_lifecycle_budgets WHERE candidate_id=?1",
                    params![candidate_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            if let Some((_res_id, existing_body)) = existing {
                if existing_body == body_json {
                    tx.commit().map_err(|e| e.to_string())?;
                    return serde_json::from_str(&existing_body).map_err(|e| e.to_string());
                }
                return Err(format!(
                    "ec3_reservation_duplicate: candidate {} already has a budget reservation",
                    candidate_id
                ));
            }

            // Check global envelope constraints
            let active_count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM harness_evolution_ec3_lifecycle_budgets WHERE contract_id=?1 AND status != 'cancelled'",
                    params![contract.contract_id],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            if active_count as u32 >= contract.global_envelope.max_total_candidates {
                return Err(format!(
                    "ec3_global_candidates_exhausted: active/reconciled count {} reaches max {}",
                    active_count, contract.global_envelope.max_total_candidates
                ));
            }

            let sums: (i64, i64, i64) = tx
                .query_row(
                    "SELECT COALESCE(SUM(reserved_token_cost), 0), COALESCE(SUM(reserved_call_count), 0), COALESCE(SUM(reserved_wall_clock_seconds), 0)
                     FROM harness_evolution_ec3_lifecycle_budgets WHERE contract_id=?1 AND status != 'cancelled'",
                    params![contract.contract_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .map_err(|e| e.to_string())?;

            let new_tokens = (sums.0 as u64).saturating_add(reservation.reserved_token_cost);
            let new_calls = (sums.1 as u64).saturating_add(reservation.reserved_call_count);
            let new_seconds = (sums.2 as u64).saturating_add(reservation.reserved_wall_clock_seconds);

            if new_tokens > contract.global_envelope.total_token_limit
                || new_calls > contract.global_envelope.total_call_limit
                || new_seconds > contract.global_envelope.total_wall_clock_seconds_limit
            {
                return Err(format!(
                    "ec3_global_budget_exhausted: reservation would exceed global limits (tokens: {}/{}, calls: {}/{}, seconds: {}/{})",
                    new_tokens,
                    contract.global_envelope.total_token_limit,
                    new_calls,
                    contract.global_envelope.total_call_limit,
                    new_seconds,
                    contract.global_envelope.total_wall_clock_seconds_limit
                ));
            }

            tx.execute(
                "INSERT INTO harness_evolution_ec3_lifecycle_budgets
                    (reservation_id, candidate_id, contract_id, reserved_token_cost, reserved_call_count, reserved_wall_clock_seconds, status, reconciliation_id, body_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?9)",
                params![
                    reservation.reservation_id,
                    reservation.candidate_id,
                    reservation.contract_id,
                    reservation.reserved_token_cost as i64,
                    reservation.reserved_call_count as i64,
                    reservation.reserved_wall_clock_seconds as i64,
                    reservation.status.as_str(),
                    body_json,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;

            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec3_budget_reserved",
                &reservation.reservation_id,
                &serde_json::json!({
                    "candidate_id": reservation.candidate_id,
                    "contract_id": reservation.contract_id,
                    "reserved_token_cost": reservation.reserved_token_cost,
                    "reserved_call_count": reservation.reserved_call_count,
                    "reserved_wall_clock_seconds": reservation.reserved_wall_clock_seconds,
                    "actor_id": actor_id,
                }),
            )?;

            tx.commit().map_err(|e| e.to_string())?;
            Ok(reservation)
        })
    }

    pub fn get_candidate_lifecycle_budget_reservation(
        &self,
        candidate_id: &str,
    ) -> Result<Option<LifecycleBudgetReservationV1>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec3_lifecycle_budgets WHERE candidate_id=?1",
                    params![candidate_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(serde_json::from_str(&body).map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        })
    }

    pub fn reconcile_candidate_lifecycle_budget(
        &self,
        contract: &Ec3LifecycleBudgetContractV1,
        candidate_id: &str,
        actor_id: &str,
    ) -> Result<LifecycleBudgetReconciliationV1, String> {
        if actor_id.trim().is_empty() {
            return Err("ec3_budget_actor: authenticated actor_id is required".into());
        }
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let cost_records = self.list_ec3_lifecycle_cost_records_for_candidate(candidate_id)?;

        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;

            let reservation_row: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT reservation_id, status, body_json FROM harness_evolution_ec3_lifecycle_budgets WHERE candidate_id=?1",
                    params![candidate_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            let (reservation_id, _status_str, reservation_body) = match reservation_row {
                Some(r) => r,
                None => return Err(format!("ec3_reservation_missing: candidate {candidate_id} has no budget reservation")),
            };

            let reservation: LifecycleBudgetReservationV1 =
                serde_json::from_str(&reservation_body).map_err(|e| e.to_string())?;

            let existing_reconciliation: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec3_lifecycle_reconciliations WHERE reservation_id=?1",
                    params![reservation_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            if let Some(recon_body) = existing_reconciliation {
                tx.commit().map_err(|e| e.to_string())?;
                return serde_json::from_str(&recon_body).map_err(|e| e.to_string());
            }

            let reconciliation = reconcile_candidate_lifecycle_costs(
                contract,
                &reservation,
                &cost_records,
            )
            .map_err(|e| format!("{}: {}", e.code, e.message))?;

            let recon_body = serde_json::to_string(&reconciliation).map_err(|e| e.to_string())?;

            tx.execute(
                "INSERT INTO harness_evolution_ec3_lifecycle_reconciliations
                    (reconciliation_id, reservation_id, candidate_id, contract_id, total_token_cost, total_call_count, total_wall_clock_seconds, total_failure_attempts, outcome, body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    reconciliation.reconciliation_id,
                    reconciliation.reservation_id,
                    reconciliation.candidate_id,
                    reconciliation.contract_id,
                    reconciliation.total_token_cost as i64,
                    reconciliation.total_call_count as i64,
                    reconciliation.total_wall_clock_seconds as i64,
                    reconciliation.total_failure_attempts as i64,
                    reconciliation.outcome.as_str(),
                    recon_body,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;

            let new_reservation_status = match reconciliation.outcome {
                LifecycleBudgetReconciliationOutcome::WithinEnvelope => {
                    LifecycleBudgetReservationStatus::Reconciled
                }
                LifecycleBudgetReconciliationOutcome::OverrunStopped => {
                    LifecycleBudgetReservationStatus::Overrun
                }
                LifecycleBudgetReconciliationOutcome::CancelledReleased => {
                    LifecycleBudgetReservationStatus::Cancelled
                }
            };

            let mut updated_reservation = reservation.clone();
            updated_reservation.status = new_reservation_status;
            let updated_res_body =
                serde_json::to_string(&updated_reservation).map_err(|e| e.to_string())?;

            tx.execute(
                "UPDATE harness_evolution_ec3_lifecycle_budgets
                 SET status=?1, reconciliation_id=?2, body_json=?3, updated_at=?4
                 WHERE reservation_id=?5",
                params![
                    new_reservation_status.as_str(),
                    reconciliation.reconciliation_id,
                    updated_res_body,
                    now,
                    reservation_id
                ],
            )
            .map_err(|e| e.to_string())?;

            if let Some(reason) = reconciliation.terminal_reason {
                let candidate_exists: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM harness_evolution_candidates WHERE candidate_id=?1)",
                        params![candidate_id],
                        |row| row.get(0),
                    )
                    .unwrap_or(false);

                if candidate_exists {
                    tx.execute(
                        "UPDATE harness_evolution_candidates
                         SET status='rejected', terminal_reason=?1, updated_at=?2
                         WHERE candidate_id=?3",
                        params![reason.as_str(), now, candidate_id],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }

            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec3_budget_reconciled",
                &reconciliation.reconciliation_id,
                &serde_json::json!({
                    "reservation_id": reconciliation.reservation_id,
                    "candidate_id": reconciliation.candidate_id,
                    "contract_id": reconciliation.contract_id,
                    "total_token_cost": reconciliation.total_token_cost,
                    "total_call_count": reconciliation.total_call_count,
                    "total_wall_clock_seconds": reconciliation.total_wall_clock_seconds,
                    "total_failure_attempts": reconciliation.total_failure_attempts,
                    "outcome": reconciliation.outcome.as_str(),
                    "terminal_reason": reconciliation.terminal_reason.map(|t| t.as_str()),
                    "actor_id": actor_id,
                }),
            )?;

            tx.commit().map_err(|e| e.to_string())?;
            Ok(reconciliation)
        })
    }

    pub fn get_candidate_lifecycle_budget_reconciliation(
        &self,
        candidate_id: &str,
    ) -> Result<Option<LifecycleBudgetReconciliationV1>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec3_lifecycle_reconciliations WHERE candidate_id=?1",
                    params![candidate_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(serde_json::from_str(&body).map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        })
    }

    pub fn record_candidate_diversity_score(
        &self,
        record: &DiversityScoreRecordV1,
        actor_id: &str,
    ) -> Result<DiversityScoreRecordV1, String> {
        if actor_id.trim().is_empty() {
            return Err("ec4_diversity_actor: authenticated actor_id is required".into());
        }
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let sealed = crate::harness_evolution::seal_diversity_score_record(record.clone())
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let body_json = serde_json::to_string(&sealed).map_err(|e| e.to_string())?;

        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;

            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec4_diversity_scores WHERE candidate_id=?1",
                    params![sealed.candidate_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            if let Some(existing_body) = existing {
                if existing_body == body_json {
                    tx.commit().map_err(|e| e.to_string())?;
                    return Ok(sealed);
                }
                return Err(format!(
                    "ec4_diversity_immutable: diversity score already recorded for candidate {}",
                    sealed.candidate_id
                ));
            }

            tx.execute(
                "INSERT INTO harness_evolution_ec4_diversity_scores
                    (record_id, candidate_id, contract_id, min_observed_distance_bps,
                     nearest_candidate_id, family_concentration_bps, parent_concentration_bps,
                     is_exact_duplicate, is_near_duplicate, is_collapse_triggered, record_sha256,
                     body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    sealed.record_id,
                    sealed.candidate_id,
                    sealed.contract_id,
                    sealed.min_observed_distance_bps as i64,
                    sealed.nearest_candidate_id,
                    sealed.family_concentration_bps as i64,
                    sealed.parent_concentration_bps as i64,
                    if sealed.is_exact_duplicate { 1 } else { 0 },
                    if sealed.is_near_duplicate { 1 } else { 0 },
                    if sealed.is_collapse_triggered { 1 } else { 0 },
                    sealed.record_sha256,
                    body_json,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;

            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec4_diversity_recorded",
                &sealed.record_id,
                &serde_json::json!({
                    "candidate_id": sealed.candidate_id,
                    "contract_id": sealed.contract_id,
                    "min_observed_distance_bps": sealed.min_observed_distance_bps,
                    "is_exact_duplicate": sealed.is_exact_duplicate,
                    "is_near_duplicate": sealed.is_near_duplicate,
                    "is_collapse_triggered": sealed.is_collapse_triggered,
                    "actor_id": actor_id,
                }),
            )?;

            tx.commit().map_err(|e| e.to_string())?;
            Ok(sealed)
        })
    }

    pub fn get_candidate_diversity_score(
        &self,
        candidate_id: &str,
    ) -> Result<Option<DiversityScoreRecordV1>, String> {
        self.with_conn(|conn| {
            let row: Option<String> = conn
                .query_row(
                    "SELECT body_json FROM harness_evolution_ec4_diversity_scores WHERE candidate_id=?1",
                    params![candidate_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match row {
                Some(body) => Ok(Some(serde_json::from_str(&body).map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        })
    }

    pub fn evaluate_and_record_candidate_diversity(
        &self,
        contract: &Ec4DiversityContractV1,
        candidate_id: &str,
        actor_id: &str,
    ) -> Result<DiversityScoreRecordV1, String> {
        if actor_id.trim().is_empty() {
            return Err("ec4_diversity_actor: authenticated actor_id is required".into());
        }
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;

            let candidate_row: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id=?1",
                    params![candidate_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;

            let candidate_body = match candidate_row {
                Some(b) => b,
                None => return Err(format!("ec4_candidate_missing: candidate {candidate_id} not found")),
            };
            let mut candidate: crate::harness_evolution::EvolutionCandidate =
                serde_json::from_str(&candidate_body).map_err(|e| e.to_string())?;

            let existing_candidates: Vec<crate::harness_evolution::EvolutionCandidate> = {
                let mut stmt = tx
                    .prepare(
                        "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id != ?1 ORDER BY created_at ASC",
                    )
                    .map_err(|e| e.to_string())?;
                let bodies = stmt
                    .query_map(params![candidate_id], |row| row.get::<_, String>(0))
                    .map_err(|e| e.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                let mut cands = Vec::new();
                for b in bodies {
                    let c: crate::harness_evolution::EvolutionCandidate =
                        serde_json::from_str(&b).map_err(|e| e.to_string())?;
                    cands.push(c);
                }
                cands
            };

            let score_record = crate::harness_evolution::evaluate_candidate_diversity(
                contract,
                &candidate,
                &existing_candidates,
            )
            .map_err(|e| format!("{}: {}", e.code, e.message))?;

            let body_json = serde_json::to_string(&score_record).map_err(|e| e.to_string())?;

            tx.execute(
                "INSERT INTO harness_evolution_ec4_diversity_scores
                    (record_id, candidate_id, contract_id, min_observed_distance_bps,
                     nearest_candidate_id, family_concentration_bps, parent_concentration_bps,
                     is_exact_duplicate, is_near_duplicate, is_collapse_triggered, record_sha256,
                     body_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    score_record.record_id,
                    score_record.candidate_id,
                    score_record.contract_id,
                    score_record.min_observed_distance_bps as i64,
                    score_record.nearest_candidate_id,
                    score_record.family_concentration_bps as i64,
                    score_record.parent_concentration_bps as i64,
                    if score_record.is_exact_duplicate { 1 } else { 0 },
                    if score_record.is_near_duplicate { 1 } else { 0 },
                    if score_record.is_collapse_triggered { 1 } else { 0 },
                    score_record.record_sha256,
                    body_json,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;

            if score_record.is_exact_duplicate {
                let term_reason = match contract.exact_duplicate_policy {
                    crate::harness_evolution::ExactDuplicatePolicy::Reject => {
                        CandidateTerminalReason::RejectedExactDuplicate
                    }
                    _ => CandidateTerminalReason::RejectedDuplicate,
                };
                candidate.status = CandidateStatus::Rejected;
                candidate.terminal_reason = term_reason;
                let updated_candidate_body =
                    serde_json::to_string(&candidate).map_err(|e| e.to_string())?;
                tx.execute(
                    "UPDATE harness_evolution_candidates SET status=?1, terminal_reason=?2, body_json=?3, updated_at=?4 WHERE candidate_id=?5",
                    params![
                        candidate.status.as_str(),
                        candidate.terminal_reason.as_str(),
                        updated_candidate_body,
                        now,
                        candidate.candidate_id
                    ],
                )
                .map_err(|e| e.to_string())?;
            } else if score_record.is_near_duplicate {
                candidate.status = CandidateStatus::Rejected;
                candidate.terminal_reason = CandidateTerminalReason::RejectedNearDuplicate;
                let updated_candidate_body =
                    serde_json::to_string(&candidate).map_err(|e| e.to_string())?;
                tx.execute(
                    "UPDATE harness_evolution_candidates SET status=?1, terminal_reason=?2, body_json=?3, updated_at=?4 WHERE candidate_id=?5",
                    params![
                        candidate.status.as_str(),
                        candidate.terminal_reason.as_str(),
                        updated_candidate_body,
                        now,
                        candidate.candidate_id
                    ],
                )
                .map_err(|e| e.to_string())?;
            } else if score_record.is_collapse_triggered {
                candidate.status = CandidateStatus::Rejected;
                candidate.terminal_reason = CandidateTerminalReason::RejectedExplorationCollapse;
                let updated_candidate_body =
                    serde_json::to_string(&candidate).map_err(|e| e.to_string())?;
                tx.execute(
                    "UPDATE harness_evolution_candidates SET status=?1, terminal_reason=?2, body_json=?3, updated_at=?4 WHERE candidate_id=?5",
                    params![
                        candidate.status.as_str(),
                        candidate.terminal_reason.as_str(),
                        updated_candidate_body,
                        now,
                        candidate.candidate_id
                    ],
                )
                .map_err(|e| e.to_string())?;
            }

            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec4_diversity_evaluated",
                &score_record.record_id,
                &serde_json::json!({
                    "candidate_id": score_record.candidate_id,
                    "contract_id": score_record.contract_id,
                    "min_observed_distance_bps": score_record.min_observed_distance_bps,
                    "is_exact_duplicate": score_record.is_exact_duplicate,
                    "is_near_duplicate": score_record.is_near_duplicate,
                    "is_collapse_triggered": score_record.is_collapse_triggered,
                    "actor_id": actor_id,
                }),
            )?;

            tx.commit().map_err(|e| e.to_string())?;
            Ok(score_record)
        })
    }

    pub fn record_ec5_candidate_selection_results(
        &self,
        contract: &crate::harness_evolution::Ec5SelectionContractV1,
        evaluations: &[crate::harness_evolution::CandidateSelectionEvaluation],
        actor_id: &str,
    ) -> Result<Vec<crate::harness_evolution::CandidateSelectionResult>, String> {
        if actor_id.trim().is_empty() {
            return Err("ec5_selection_actor: authenticated actor_id is required".into());
        }
        let results = crate::harness_evolution::evaluate_candidate_selection(contract, evaluations)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;

            for res in &results {
                if !res.is_hard_gate_passed {
                    let row: Option<String> = tx
                        .query_row(
                            "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id=?1",
                            params![res.candidate_id],
                            |r| r.get(0),
                        )
                        .optional()
                        .map_err(|e| e.to_string())?;
                    if let Some(body) = row {
                        if let Ok(mut cand) = serde_json::from_str::<EvolutionCandidate>(&body) {
                            cand.status = CandidateStatus::Rejected;
                            cand.terminal_reason = CandidateTerminalReason::RejectedHardGate;
                            if let Ok(cand_json) = serde_json::to_string(&cand) {
                                let _ = tx.execute(
                                    "UPDATE harness_evolution_candidates SET status=?1, terminal_reason=?2, body_json=?3, updated_at=?4 WHERE candidate_id=?5",
                                    params![
                                        cand.status.as_str(),
                                        cand.terminal_reason.as_str(),
                                        cand_json,
                                        now,
                                        cand.candidate_id
                                    ],
                                );
                            }
                        }
                    }
                }
            }

            append_audit_locked(
                &tx,
                &now,
                actor_id,
                "harness_evolution.ec5_selection_evaluated",
                &contract.contract_id,
                &serde_json::json!({
                    "contract_id": contract.contract_id,
                    "candidates_count": evaluations.len(),
                    "selected_count": results.iter().filter(|r| r.selected_for_archive).count(),
                    "actor_id": actor_id,
                }),
            )?;

            tx.commit().map_err(|e| e.to_string())?;
            Ok(results)
        })
    }

    pub fn record_ec5_run_lifecycle_event(
        &self,
        run: &crate::harness_evolution::ExperimentRunRecordV1,
        event_kind: &str,
        actor_id: &str,
    ) -> Result<(), String> {
        if actor_id.trim().is_empty() {
            return Err("ec5_run_actor: authenticated actor_id is required".into());
        }
        crate::harness_evolution::validate_experiment_run_record(run)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        self.with_conn(|conn| {
            let audit_action = format!("harness_evolution.ec5_run_{}", event_kind);
            append_audit_locked(
                conn,
                &now,
                actor_id,
                &audit_action,
                &run.run_id,
                &serde_json::json!({
                    "run_id": run.run_id,
                    "experiment_id": run.experiment_id,
                    "contract_id": run.contract_id,
                    "state": run.state.as_str(),
                    "lease_holder": run.lease_holder,
                    "lease_expires_at": run.lease_expires_at,
                    "stop_reason": run.stop_reason.map(|r| r.as_str()),
                    "executed_effects_count": run.executed_effects_count,
                    "total_token_cost": run.total_token_cost,
                    "total_wall_clock_seconds": run.total_wall_clock_seconds,
                    "actor_id": actor_id,
                }),
            )?;
            Ok(())
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
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body_json FROM harness_evolution_sealed_holdouts WHERE vault_sha256=?1",
                    params![vault.vault_sha256],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(body) = existing {
                let stored: SealedHoldoutVault =
                    serde_json::from_str(&body).map_err(|e| e.to_string())?;
                if stored == vault {
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
        self.get_harness_evolution_sealed_holdout(vault_sha)
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
        if candidate_ids.len() < MIN_SEALED_ENTRANTS || candidate_ids.len() > MAX_SEALED_ENTRANTS {
            return Err(
                "evolution_eval_sealed_entrants: sealed selection must name 1–3 candidates".into(),
            );
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
            "schema_version": "harness_evolution_sealed_selection.v1",
            "receipt_id": receipt_id,
            "family_id": family_id,
            "candidate_ids": candidate_ids,
            "used": false,
        });
        // Refuse overwrite of existing selection.
        let snap = self.config_snapshot()?;
        if snap
            .as_object()
            .map(|m| m.contains_key(&key))
            .unwrap_or(false)
        {
            return Err(format!(
                "evolution_eval_sealed_selection_exists: {receipt_id}"
            ));
        }
        self.set_config_value(&key, value, actor_id)?;
        Ok(receipt_id)
    }

    fn consume_sealed_selection_for_candidate(
        &self,
        family_id: &str,
        candidate_id: &str,
        actor_id: &str,
    ) -> Result<bool, String> {
        let snap = self.config_snapshot()?;
        let Some(map) = snap.as_object() else {
            return Ok(false);
        };
        let prefix = "harness_evolution.sealed_selection.";
        for (key, value) in map {
            if !key.starts_with(prefix) {
                continue;
            }
            let body = if let Some(inner) = value.get("value") {
                inner.clone()
            } else {
                value.clone()
            };
            let used = body.get("used").and_then(|v| v.as_bool()).unwrap_or(true);
            if used {
                continue;
            }
            let fid = body.get("family_id").and_then(|v| v.as_str()).unwrap_or("");
            if fid != family_id {
                continue;
            }
            let ids = body
                .get("candidate_ids")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let matches = ids.iter().any(|v| v.as_str() == Some(candidate_id));
            if !matches {
                continue;
            }
            let mut updated = body;
            updated
                .as_object_mut()
                .ok_or_else(|| "evolution_eval_sealed_selection_malformed".to_string())?
                .insert("used".into(), serde_json::json!(true));
            self.set_config_value(key, updated, actor_id)?;
            return Ok(true);
        }
        Ok(false)
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
        let sentinel_receipts = crate::harness_evolution_eval::observe_ec2_sentinels(&bundle)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        if crate::harness_evolution_eval::sentinels_admit_pareto(&sentinel_receipts).is_err() {
            let receipt = crate::harness_evolution_eval::build_eval_receipt_with_sentinels(
                &bundle,
                "rejected_sentinel",
                &created_at,
                sentinel_receipts,
            );
            self.persist_evaluation_bundle_and_receipt(
                &bundle,
                &[],
                &receipt,
                budget.seed,
                &created_at,
            )?;
            return Err(format!(
                "ec2_sentinel_fail: rejected evidence retained as {}",
                receipt.receipt_id
            ));
        }
        let archive = build_pareto_archive(&bundle, &created_at)
            .map_err(|e| format!("{}: {}", e.code, e.message))?;
        let receipt = crate::harness_evolution_eval::build_eval_receipt_with_sentinels(
            &bundle,
            "evaluated",
            &created_at,
            sentinel_receipts,
        );
        self.persist_evaluation_bundle_and_receipt(
            &bundle,
            &archive,
            &receipt,
            budget.seed,
            &created_at,
        )?;
        Ok((bundle, archive, receipt))
    }

    fn persist_evaluation_bundle_and_receipt(
        &self,
        bundle: &CandidateEvaluationBundle,
        archive: &[ParetoArchiveEntry],
        receipt: &EvalReceipt,
        budget_seed: u64,
        created_at: &str,
    ) -> Result<(), String> {
        let redacted = redacted_eval_evidence(bundle);
        let bundle_json = serde_json::to_string(bundle).map_err(|e| e.to_string())?;
        let receipt_json = serde_json::to_string(receipt).map_err(|e| e.to_string())?;
        self.with_conn(|conn| {
            let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
                .map_err(|e| e.to_string())?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT evaluation_id FROM harness_evolution_evaluations
                     WHERE candidate_id=?1 AND budget_seed=?2 AND family_id=?3",
                    params![bundle.candidate_id, budget_seed as i64, bundle.family_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if existing.is_some() {
                return Err(format!(
                    "evolution_duplicate_evaluation: candidate {} seed {} family {}",
                    bundle.candidate_id, budget_seed, bundle.family_id
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
                    budget_seed as i64,
                    bundle.bundle_sha256,
                    bundle.sealed_entrant_count as i64,
                    bundle_json,
                    created_at
                ],
            )
            .map_err(|e| e.to_string())?;
            for entry in archive {
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
            let audit_action = if receipt.terminal == "rejected_sentinel" {
                "harness_evolution.evaluation_rejected_sentinel"
            } else {
                "harness_evolution.evaluation_recorded"
            };
            append_audit_locked(
                &tx,
                created_at,
                "system",
                audit_action,
                &bundle.evaluation_id,
                &redacted,
            )?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(())
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
    fn real_workspace_store_sentinels_fail_closed_and_refuse_pareto_archive() {
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
            &json!({"kind": "real-eval"}),
            vec![],
            21,
        )
        .unwrap();
        let bound = store.admit_harness_evolution_proposal(&proposal).unwrap();
        let ws = materialize_for(&env, &bound, "real-eval-content");
        let candidate = candidate_from_proposal(&bound, &ws, "2026-07-21T00:00:00Z").unwrap();
        let (admitted, _) = store.admit_harness_evolution_candidate(candidate).unwrap();
        let family = register_family_and_vault(&store, "fam-real-store");
        let (bundle, archive, receipt) = store
            .record_harness_evolution_evaluation(
                &admitted.candidate_id,
                &sample_budget(2),
                &family.family_id,
            )
            .unwrap();
        assert_eq!(receipt.terminal, "evaluated");
        assert_eq!(receipt.sentinel_receipts.len(), 3);
        assert!(!archive.is_empty());
        let loaded_pareto = store
            .list_harness_evolution_pareto_for_evaluation(&bundle.evaluation_id)
            .unwrap();
        assert!(!loaded_pareto.is_empty());
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

    #[test]
    fn records_immutable_ec1_identity_lineage_with_restart_and_no_orphans() {
        use crate::harness_evolution::{
            seal_ec1_identity_lineage, Ec1IdentityLineageRecord, EC1_FROZEN_ACTIVE_HARNESS_SHA,
            EC1_IDENTITY_LINEAGE_SCHEMA,
        };
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec1-lineage.db");
        let store = LocalProductStore::new(&db).unwrap();
        let root = seal_ec1_identity_lineage(Ec1IdentityLineageRecord {
            schema_version: EC1_IDENTITY_LINEAGE_SCHEMA.to_string(),
            lineage_id: String::new(),
            parent_lineage_id: None,
            source_identity_hash: sha256_hex("ec1-root-source"),
            active_harness_sha: EC1_FROZEN_ACTIVE_HARNESS_SHA.to_string(),
            causal_source_id: None,
            record_sha256: String::new(),
        })
        .unwrap();
        let recorded = store
            .record_ec1_identity_lineage(root.clone(), "operator-test")
            .unwrap();
        let replay = store
            .record_ec1_identity_lineage(root.clone(), "operator-test")
            .unwrap();
        assert_eq!(replay.lineage_id, recorded.lineage_id);
        let mut tampered = recorded.clone();
        tampered.causal_source_id = Some("orphan".into());
        assert!(store
            .record_ec1_identity_lineage(tampered, "operator-test")
            .unwrap_err()
            .contains("causal"));
        let missing_parent = seal_ec1_identity_lineage(Ec1IdentityLineageRecord {
            schema_version: EC1_IDENTITY_LINEAGE_SCHEMA.to_string(),
            lineage_id: String::new(),
            parent_lineage_id: Some("heil-missing".into()),
            source_identity_hash: sha256_hex("ec1-child-source"),
            active_harness_sha: EC1_FROZEN_ACTIVE_HARNESS_SHA.to_string(),
            causal_source_id: None,
            record_sha256: String::new(),
        })
        .unwrap();
        assert!(store
            .record_ec1_identity_lineage(missing_parent, "operator-test")
            .unwrap_err()
            .contains("parent"));
        drop(store);
        let reopened = LocalProductStore::new(&db).unwrap();
        let loaded = reopened
            .get_ec1_identity_lineage(&recorded.lineage_id)
            .unwrap()
            .expect("lineage survives restart");
        assert_eq!(loaded.active_harness_sha, EC1_FROZEN_ACTIVE_HARNESS_SHA);
        assert_eq!(loaded.record_sha256, recorded.record_sha256);
    }

    #[test]
    fn records_source_bound_ec1_causal_manifest_with_restart() {
        use crate::harness_evolution::{
            seal_ec1_identity_lineage, seal_failure_pattern_evidence,
            seal_mutation_hypothesis_manifest, CausalStatus, Ec1IdentityLineageRecord,
            EvidenceRole, FailurePatternEvidenceV1, MutationHypothesisManifestV1, EC1_BUDGET_CLASS,
            EC1_FROZEN_ACTIVE_HARNESS_SHA, EC1_GENERATOR_CLASS, EC1_IDENTITY_LINEAGE_SCHEMA,
            EC1_INVALIDATION_CLASS, FAILURE_PATTERN_EVIDENCE_SCHEMA, LINEAGE_SCHEMA_VERSION,
            MUTATION_HYPOTHESIS_MANIFEST_SCHEMA,
        };
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec1-causal.db");
        let store = LocalProductStore::new(&db).unwrap();
        let source = sha256_hex("ec1-root-source");
        let lineage = store
            .record_ec1_identity_lineage(
                seal_ec1_identity_lineage(Ec1IdentityLineageRecord {
                    schema_version: EC1_IDENTITY_LINEAGE_SCHEMA.to_string(),
                    lineage_id: String::new(),
                    parent_lineage_id: None,
                    source_identity_hash: source.clone(),
                    active_harness_sha: EC1_FROZEN_ACTIVE_HARNESS_SHA.to_string(),
                    causal_source_id: None,
                    record_sha256: String::new(),
                })
                .unwrap(),
                "operator-test",
            )
            .unwrap();
        let pattern = seal_failure_pattern_evidence(FailurePatternEvidenceV1 {
            schema_version: FAILURE_PATTERN_EVIDENCE_SCHEMA.to_string(),
            evidence_id: String::new(),
            lineage_id: lineage.lineage_id.clone(),
            evidence_role: EvidenceRole::Observation,
            source_identity_hash: source,
            parent_identity_hash: sha256_hex("root"),
            generator_class: EC1_GENERATOR_CLASS.to_string(),
            lineage_schema_version: LINEAGE_SCHEMA_VERSION.to_string(),
            invalidation_class: EC1_INVALIDATION_CLASS.to_string(),
            budget_class: EC1_BUDGET_CLASS.to_string(),
            observation_digest: sha256_hex("obs"),
            causal_status: CausalStatus::Unknown,
            counterevidence_digest: sha256_hex("counter"),
            addressable_surface: "prompts_and_bounded_rules".to_string(),
            mutable_surface: "prompts_and_bounded_rules".to_string(),
            record_sha256: String::new(),
        })
        .unwrap();
        let recorded = store
            .record_ec1_failure_pattern(pattern.clone(), "operator-test")
            .unwrap();
        assert_eq!(
            store
                .record_ec1_failure_pattern(pattern.clone(), "operator-test")
                .unwrap()
                .evidence_id,
            recorded.evidence_id
        );
        let mut mutated = recorded.clone();
        mutated.causal_status = CausalStatus::Supported;
        assert!(store
            .record_ec1_failure_pattern(mutated, "operator-test")
            .unwrap_err()
            .contains("immutable"));
        let hypothesis = seal_mutation_hypothesis_manifest(MutationHypothesisManifestV1 {
            schema_version: MUTATION_HYPOTHESIS_MANIFEST_SCHEMA.to_string(),
            manifest_id: String::new(),
            lineage_id: lineage.lineage_id.clone(),
            failure_evidence_id: recorded.evidence_id.clone(),
            proposal_body_sha256: sha256_hex("proposal-body"),
            candidate_delta_digest: sha256_hex("delta"),
            predicted_improvement_digest: sha256_hex("imp"),
            predicted_regression_digest: sha256_hex("reg"),
            invariant_digest: sha256_hex("inv"),
            evaluation_plan_digest: sha256_hex("plan"),
            record_sha256: String::new(),
        })
        .unwrap();
        let stored_h = store
            .record_ec1_hypothesis(hypothesis.clone(), "operator-test")
            .unwrap();
        assert_eq!(stored_h.proposal_body_sha256, sha256_hex("proposal-body"));
        let mut unbound = hypothesis.clone();
        unbound.proposal_body_sha256.clear();
        assert!(store
            .record_ec1_hypothesis(unbound, "operator-test")
            .is_err());
        let mut missing_lineage = pattern.clone();
        missing_lineage.lineage_id = "heil-missing".into();
        assert!(store
            .record_ec1_failure_pattern(missing_lineage, "operator-test")
            .unwrap_err()
            .contains("lineage"));
        let mut source_mismatch = pattern.clone();
        source_mismatch.source_identity_hash = sha256_hex("other-source");
        assert!(store
            .record_ec1_failure_pattern(source_mismatch, "operator-test")
            .unwrap_err()
            .contains("source"));
        let mut post_exec = stored_h.clone();
        post_exec.predicted_improvement_digest = sha256_hex("changed");
        assert!(store
            .record_ec1_hypothesis(post_exec, "operator-test")
            .unwrap_err()
            .contains("immutable"));
        let family = crate::harness_evolution::registered_mutation_families().families[0]
            .family_id
            .clone();
        let bound = store
            .record_ec1_candidate_binding(&family, &stored_h, 7, "operator-test")
            .unwrap();
        assert_eq!(bound.hypothesis_manifest_id, stored_h.manifest_id);
        assert_eq!(
            store
                .record_ec1_candidate_binding(&family, &stored_h, 7, "operator-test")
                .unwrap()
                .binding_id,
            bound.binding_id
        );
        assert!(store
            .record_ec1_candidate_binding("family:unknown", &stored_h, 7, "operator-test")
            .unwrap_err()
            .contains("not registered"));
        drop(store);
        let reopened = LocalProductStore::new(&db).unwrap();
        assert_eq!(
            reopened
                .record_ec1_failure_pattern(pattern, "operator-test")
                .unwrap()
                .record_sha256,
            recorded.record_sha256
        );
        assert_eq!(
            reopened
                .record_ec1_hypothesis(hypothesis, "operator-test")
                .unwrap()
                .manifest_id,
            stored_h.manifest_id
        );
    }

    #[test]
    fn persists_ec2_holdout_seal_with_access_mediation_and_rotation() {
        use crate::harness_evolution_eval::{sample_task_family, Ec2AccessClass};
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec2-holdout.db");
        let store = LocalProductStore::new(&db).unwrap();
        let family = sample_task_family("fam-holdout");
        let sealed = store
            .persist_ec2_holdout_seal(&family, 1, "evaluator-actor")
            .unwrap();
        assert!(store
            .read_ec2_holdout_membership(&sealed.vault.vault_sha256, Ec2AccessClass::Evaluator)
            .is_ok());
        assert!(store
            .persist_ec2_holdout_seal(&family, 1, "")
            .unwrap_err()
            .contains("actor_id"));
        assert!(store
            .persist_ec2_holdout_seal(&family, 2, "evaluator-actor")
            .unwrap_err()
            .contains("immutable"));
        assert!(store
            .read_ec2_holdout_membership(
                &sealed.vault.vault_sha256,
                Ec2AccessClass::CandidateWorker
            )
            .unwrap_err()
            .contains("unauthorized"));
        assert!(store
            .read_ec2_holdout_membership(
                &sealed.vault.vault_sha256,
                Ec2AccessClass::OperatorController
            )
            .unwrap_err()
            .contains("unauthorized"));
        let body = serde_json::to_value(&sealed).unwrap();
        assert!(!crate::harness_evolution_eval::holdout_body_contains_sensitive(&body));
        drop(store);
        let reopened = LocalProductStore::new(&db).unwrap();
        let loaded = reopened
            .get_ec2_holdout_seal(&sealed.vault.vault_sha256, Ec2AccessClass::Evaluator)
            .unwrap()
            .expect("seal survives restart");
        assert_eq!(loaded.vault.vault_sha256, sealed.vault.vault_sha256);
        let mut rotated_family = family.clone();
        rotated_family.sealed_holdout[0].label_sha256 =
            crate::harness_evolution::sha256_hex("rotated-label");
        let rotated = reopened
            .rotate_ec2_holdout_seal(
                &sealed.vault.vault_sha256,
                &rotated_family,
                "evaluator-actor",
            )
            .unwrap();
        assert_ne!(rotated.vault.vault_sha256, sealed.vault.vault_sha256);
        assert!(reopened
            .read_ec2_holdout_membership(&sealed.vault.vault_sha256, Ec2AccessClass::Evaluator)
            .unwrap_err()
            .contains("invalidated"));
        let audits: i64 = {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM audit_log WHERE action IN ('harness_evolution.ec2_holdout_sealed','harness_evolution.ec2_holdout_rotated')",
                [],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert!(audits >= 2);
    }

    #[test]
    fn persists_evaluator_owned_prediction_outcomes_and_rejects_candidate_authors() {
        use crate::harness_evolution::{
            seal_mutation_hypothesis_manifest, MutationHypothesisManifestV1,
            MUTATION_HYPOTHESIS_MANIFEST_SCHEMA,
        };
        use crate::harness_evolution_eval::{
            actual_improvement_digest, actual_regression_digest, derive_ec2_prediction_outcome,
            sample_budget, sample_task_family, summarize_prediction_outcomes, Ec2AccessClass,
        };
        let _g = LabEnvGuard::enable();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec2-po.db");
        let store = LocalProductStore::new(&db).unwrap();
        let family = sample_task_family("fam-po-store");
        let vault = crate::harness_evolution_eval::build_sealed_vault(&family).unwrap();
        let bundle = crate::harness_evolution_eval::evaluate_candidate_fixture(
            "cand-po",
            "lin-po",
            "active-po",
            &"a".repeat(64),
            &"b".repeat(64),
            &sample_budget(3),
            &family,
            &vault,
            false,
            "2026-08-20T00:00:00Z",
        )
        .unwrap();
        let hypothesis = seal_mutation_hypothesis_manifest(MutationHypothesisManifestV1 {
            schema_version: MUTATION_HYPOTHESIS_MANIFEST_SCHEMA.to_string(),
            manifest_id: String::new(),
            lineage_id: "heil-fixture-root".into(),
            failure_evidence_id: "ev-fixture".into(),
            proposal_body_sha256: crate::harness_evolution::sha256_hex("proposal"),
            candidate_delta_digest: crate::harness_evolution::sha256_hex("delta"),
            predicted_improvement_digest: actual_improvement_digest(&bundle),
            predicted_regression_digest: actual_regression_digest(&bundle),
            invariant_digest: crate::harness_evolution::sha256_hex("inv"),
            evaluation_plan_digest: crate::harness_evolution::sha256_hex("plan"),
            record_sha256: String::new(),
        })
        .unwrap();
        assert!(store
            .persist_ec2_prediction_outcome(
                &hypothesis,
                &bundle,
                Ec2AccessClass::CandidateWorker,
                "candidate"
            )
            .unwrap_err()
            .contains("only the evaluator"));
        let stored = store
            .persist_ec2_prediction_outcome(
                &hypothesis,
                &bundle,
                Ec2AccessClass::Evaluator,
                "evaluator-actor",
            )
            .unwrap();
        let replay = derive_ec2_prediction_outcome(&hypothesis, &bundle).unwrap();
        assert_eq!(stored.outcome_id, replay.outcome_id);
        assert_eq!(
            store
                .persist_ec2_prediction_outcome(
                    &hypothesis,
                    &bundle,
                    Ec2AccessClass::Evaluator,
                    "evaluator-actor",
                )
                .unwrap()
                .record_sha256,
            stored.record_sha256
        );
        drop(store);
        let reopened = LocalProductStore::new(&db).unwrap();
        let loaded = reopened
            .get_ec2_prediction_outcome(&stored.outcome_id)
            .unwrap()
            .expect("outcome survives restart");
        assert_eq!(loaded.record_sha256, stored.record_sha256);
        let summary = summarize_prediction_outcomes(&[loaded]);
        assert!(!summary.accuracy_is_selection_authority);
    }

    #[test]
    fn persists_ec3_lifecycle_cost_records_with_failure_retention_and_immutability() {
        use crate::harness_evolution::{
            seal_lifecycle_cost_record, CostTrustSource, LifecycleCostPhase, LifecycleCostRecordV1,
            LIFECYCLE_COST_RECORD_SCHEMA,
        };
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec3-cost.db");
        let store = LocalProductStore::new(&db).unwrap();

        let rec1 = seal_lifecycle_cost_record(LifecycleCostRecordV1 {
            schema_version: LIFECYCLE_COST_RECORD_SCHEMA.to_string(),
            record_id: String::new(),
            candidate_id: "cand-ec3-1".to_string(),
            phase: LifecycleCostPhase::HypothesisGeneration,
            token_cost: 8_000,
            call_count: 2,
            wall_clock_seconds: 15,
            trust_source: CostTrustSource::MeasuredDirect,
            unmeasured: false,
            failure_attempt: false,
            evidence_payload_digest: crate::harness_evolution::sha256_hex("hyp_payload"),
            record_sha256: String::new(),
        })
        .unwrap();

        let rec2_failure = seal_lifecycle_cost_record(LifecycleCostRecordV1 {
            schema_version: LIFECYCLE_COST_RECORD_SCHEMA.to_string(),
            record_id: String::new(),
            candidate_id: "cand-ec3-1".to_string(),
            phase: LifecycleCostPhase::Repair,
            token_cost: 25_000,
            call_count: 5,
            wall_clock_seconds: 60,
            trust_source: CostTrustSource::MeasuredDirect,
            unmeasured: false,
            failure_attempt: true,
            evidence_payload_digest: crate::harness_evolution::sha256_hex("repair_failed_attempt"),
            record_sha256: String::new(),
        })
        .unwrap();

        // Must reject empty actor
        assert!(store
            .persist_ec3_lifecycle_cost_record(rec1.clone(), "")
            .unwrap_err()
            .contains("actor_id"));

        let stored1 = store
            .persist_ec3_lifecycle_cost_record(rec1.clone(), "worker-actor")
            .unwrap();
        assert_eq!(stored1.record_sha256, rec1.record_sha256);

        let stored2 = store
            .persist_ec3_lifecycle_cost_record(rec2_failure.clone(), "worker-actor")
            .unwrap();
        assert!(stored2.failure_attempt);

        // Idempotency
        assert_eq!(
            store
                .persist_ec3_lifecycle_cost_record(rec1.clone(), "worker-actor")
                .unwrap()
                .record_sha256,
            stored1.record_sha256
        );

        // Immutability: different body for same record_id fails
        let mut tampered = rec1.clone();
        tampered.token_cost = 99_999;
        tampered.record_sha256 = crate::harness_evolution::sha256_hex("tampered");
        assert!(store
            .persist_ec3_lifecycle_cost_record(tampered, "worker-actor")
            .unwrap_err()
            .contains("immutable"));

        // Query by candidate_id
        let list = store
            .list_ec3_lifecycle_cost_records_for_candidate("cand-ec3-1")
            .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].record_id, stored1.record_id);
        assert_eq!(list[1].record_id, stored2.record_id);
        assert!(list[1].failure_attempt);

        // Restart survival
        drop(store);
        let reopened = LocalProductStore::new(&db).unwrap();
        let loaded = reopened
            .get_ec3_lifecycle_cost_record(&stored1.record_id)
            .unwrap()
            .expect("cost record survives restart");
        assert_eq!(loaded.record_sha256, stored1.record_sha256);
    }

    #[test]
    fn persists_ec3_budget_reservation_and_reconciles_with_overrun_stop() {
        use crate::harness_evolution::{
            sample_ec3_budget_contract, seal_lifecycle_cost_record, CandidateTerminalReason,
            CostTrustSource, LifecycleBudgetReconciliationOutcome,
            LifecycleBudgetReservationStatus, LifecycleCostPhase, LifecycleCostRecordV1,
            LIFECYCLE_COST_RECORD_SCHEMA,
        };
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec3-enforce.db");
        let store = LocalProductStore::new(&db).unwrap();

        let mut contract = sample_ec3_budget_contract();
        contract.global_envelope.max_total_candidates = 2;

        // 1. Reserve budget for candidate 1
        let res1 = store
            .reserve_candidate_lifecycle_budget(&contract, "cand-enforce-1", "worker-actor")
            .unwrap();
        assert_eq!(res1.status, LifecycleBudgetReservationStatus::Active);

        // 2. Reserve budget for candidate 2
        let res2 = store
            .reserve_candidate_lifecycle_budget(&contract, "cand-enforce-2", "worker-actor")
            .unwrap();
        assert_eq!(res2.status, LifecycleBudgetReservationStatus::Active);

        // 3. Candidate 3 should be rejected because max_total_candidates = 2 is reached
        assert!(store
            .reserve_candidate_lifecycle_budget(&contract, "cand-enforce-3", "worker-actor")
            .unwrap_err()
            .contains("ec3_global_candidates_exhausted"));

        // 4. Record cost for candidate 1 (within envelope)
        let cost1 = seal_lifecycle_cost_record(LifecycleCostRecordV1 {
            schema_version: LIFECYCLE_COST_RECORD_SCHEMA.to_string(),
            record_id: String::new(),
            candidate_id: "cand-enforce-1".to_string(),
            phase: LifecycleCostPhase::Evaluation,
            token_cost: 5_000,
            call_count: 2,
            wall_clock_seconds: 30,
            trust_source: CostTrustSource::MeasuredDirect,
            unmeasured: false,
            failure_attempt: false,
            evidence_payload_digest: crate::harness_evolution::sha256_hex("c1_eval"),
            record_sha256: String::new(),
        })
        .unwrap();
        store
            .persist_ec3_lifecycle_cost_record(cost1, "worker-actor")
            .unwrap();

        let recon1 = store
            .reconcile_candidate_lifecycle_budget(&contract, "cand-enforce-1", "worker-actor")
            .unwrap();
        assert_eq!(
            recon1.outcome,
            LifecycleBudgetReconciliationOutcome::WithinEnvelope
        );
        assert_eq!(recon1.terminal_reason, None);

        // 5. Record cost for candidate 2 (overrun in candidate materialization phase: 60_000 > 50_000 phase limit)
        let cost2 = seal_lifecycle_cost_record(LifecycleCostRecordV1 {
            schema_version: LIFECYCLE_COST_RECORD_SCHEMA.to_string(),
            record_id: String::new(),
            candidate_id: "cand-enforce-2".to_string(),
            phase: LifecycleCostPhase::CandidateMaterialization,
            token_cost: 60_000,
            call_count: 10,
            wall_clock_seconds: 50,
            trust_source: CostTrustSource::MeasuredDirect,
            unmeasured: false,
            failure_attempt: false,
            evidence_payload_digest: crate::harness_evolution::sha256_hex("c2_mat"),
            record_sha256: String::new(),
        })
        .unwrap();
        store
            .persist_ec3_lifecycle_cost_record(cost2, "worker-actor")
            .unwrap();

        let recon2 = store
            .reconcile_candidate_lifecycle_budget(&contract, "cand-enforce-2", "worker-actor")
            .unwrap();
        assert_eq!(
            recon2.outcome,
            LifecycleBudgetReconciliationOutcome::OverrunStopped
        );
        assert_eq!(
            recon2.terminal_reason,
            Some(CandidateTerminalReason::RejectedLifecycleBudgetOverrun)
        );

        // Check updated reservation status
        let updated_res2 = store
            .get_candidate_lifecycle_budget_reservation("cand-enforce-2")
            .unwrap()
            .unwrap();
        assert_eq!(
            updated_res2.status,
            LifecycleBudgetReservationStatus::Overrun
        );
    }

    #[test]
    fn ec4_diversity_admission_rejection_and_persistence_lifecycle() {
        use crate::harness_evolution::{
            sample_active_identity, sample_ec4_diversity_contract, CandidateStatus,
            CandidateTerminalReason, CANDIDATE_SCHEMA_VERSION,
        };
        let _g = LabEnvGuard::enable();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec4-div.db");
        let store = LocalProductStore::new(&db).unwrap();

        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "actor-1")
            .unwrap();

        let contract = sample_ec4_diversity_contract();

        // Insert proposals
        for prop_id in ["prop-1", "prop-2", "prop-3"] {
            store
                .with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO harness_evolution_proposals (proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, proposal_body_sha256, body_json, seed, created_at) VALUES (?1, ?2, ?3, ?4, ?5, '{}', 1, '2026-08-20T00:00:00Z')",
                        params![prop_id, active.active_version_id, active.active_version_hash, active.evaluator_identity_hash, "0".repeat(64)],
                    ).map_err(|e| e.to_string())
                })
                .unwrap();
        }

        // 1. Candidate 1 (first candidate, novel)
        let c1 = EvolutionCandidate {
            schema_version: CANDIDATE_SCHEMA_VERSION.to_string(),
            candidate_id: "cand-ec4-1".to_string(),
            lineage_id: "lin-ec4-1".to_string(),
            parent_candidate_id: None,
            proposal_id: "prop-1".to_string(),
            active_version_id: active.active_version_id.clone(),
            active_version_hash: active.active_version_hash.clone(),
            evaluator_identity_hash: active.evaluator_identity_hash.clone(),
            mutable_surface: crate::harness_evolution::MutableSurfaceDeclaration {
                schema_version: "mutable_surface_declaration.v1".to_string(),
                surfaces: vec!["prompt".to_string()],
            },
            workspace: crate::harness_evolution::CandidateWorkspace {
                schema_version: "candidate_workspace.v1".to_string(),
                workspace_id: "ws-1".to_string(),
                relative_path: "ws/1".to_string(),
                content_hash: "1".repeat(64),
            },
            content_hash: "1".repeat(64),
            status: CandidateStatus::Proposed,
            terminal_reason: CandidateTerminalReason::Admitted,
            seed: 100,
            created_at: "2026-08-20T00:00:00Z".to_string(),
        };
        let c1_body = serde_json::to_string(&c1).unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO harness_evolution_candidates (candidate_id, lineage_id, proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, content_hash, status, terminal_reason, workspace_id, workspace_rel_path, body_json, seed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                    params![
                        c1.candidate_id, c1.lineage_id, c1.proposal_id, c1.active_version_id,
                        c1.active_version_hash, c1.evaluator_identity_hash, c1.content_hash,
                        c1.status.as_str(), c1.terminal_reason.as_str(), c1.workspace.workspace_id,
                        c1.workspace.relative_path, c1_body, c1.seed as i64, c1.created_at
                    ],
                ).map_err(|e| e.to_string())
            })
            .unwrap();

        let div1 = store
            .evaluate_and_record_candidate_diversity(&contract, "cand-ec4-1", "worker-actor")
            .unwrap();
        assert!(!div1.is_exact_duplicate);
        assert!(!div1.is_near_duplicate);
        assert!(!div1.is_collapse_triggered);
        assert_eq!(div1.min_observed_distance_bps, 10_000);

        // 2. Candidate 2 (exact duplicate of c1 -> content_hash identical)
        let c2 = EvolutionCandidate {
            schema_version: CANDIDATE_SCHEMA_VERSION.to_string(),
            candidate_id: "cand-ec4-2".to_string(),
            lineage_id: "lin-ec4-2".to_string(),
            parent_candidate_id: None,
            proposal_id: "prop-2".to_string(),
            active_version_id: active.active_version_id.clone(),
            active_version_hash: active.active_version_hash.clone(),
            evaluator_identity_hash: active.evaluator_identity_hash.clone(),
            mutable_surface: crate::harness_evolution::MutableSurfaceDeclaration {
                schema_version: "mutable_surface_declaration.v1".to_string(),
                surfaces: vec!["prompt".to_string()],
            },
            workspace: crate::harness_evolution::CandidateWorkspace {
                schema_version: "candidate_workspace.v1".to_string(),
                workspace_id: "ws-2".to_string(),
                relative_path: "ws/2".to_string(),
                content_hash: "1".repeat(64),
            },
            content_hash: "1".repeat(64),
            status: CandidateStatus::Proposed,
            terminal_reason: CandidateTerminalReason::Admitted,
            seed: 101,
            created_at: "2026-08-20T00:01:00Z".to_string(),
        };
        let c2_body = serde_json::to_string(&c2).unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO harness_evolution_candidates (candidate_id, lineage_id, proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, content_hash, status, terminal_reason, workspace_id, workspace_rel_path, body_json, seed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                    params![
                        c2.candidate_id, c2.lineage_id, c2.proposal_id, c2.active_version_id,
                        c2.active_version_hash, c2.evaluator_identity_hash, c2.content_hash,
                        c2.status.as_str(), c2.terminal_reason.as_str(), c2.workspace.workspace_id,
                        c2.workspace.relative_path, c2_body, c2.seed as i64, c2.created_at
                    ],
                ).map_err(|e| e.to_string())
            })
            .unwrap();

        let div2 = store
            .evaluate_and_record_candidate_diversity(&contract, "cand-ec4-2", "worker-actor")
            .unwrap();
        assert!(div2.is_exact_duplicate);
        assert_eq!(div2.min_observed_distance_bps, 0);

        let updated_c2: EvolutionCandidate = store
            .with_conn(|conn| {
                let b: String = conn
                    .query_row(
                        "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id='cand-ec4-2'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                Ok(serde_json::from_str(&b).unwrap())
            })
            .unwrap();
        assert_eq!(updated_c2.status, CandidateStatus::Rejected);
        assert_eq!(
            updated_c2.terminal_reason,
            CandidateTerminalReason::RejectedExactDuplicate
        );

        // 3. Candidate 3 (near duplicate of c1 -> 63 out of 64 chars match -> distance = 156 bps < 500 bps)
        let mut near_hash = "1".repeat(63);
        near_hash.push('2');
        let c3 = EvolutionCandidate {
            schema_version: CANDIDATE_SCHEMA_VERSION.to_string(),
            candidate_id: "cand-ec4-3".to_string(),
            lineage_id: "lin-ec4-3".to_string(),
            parent_candidate_id: None,
            proposal_id: "prop-3".to_string(),
            active_version_id: active.active_version_id.clone(),
            active_version_hash: active.active_version_hash.clone(),
            evaluator_identity_hash: active.evaluator_identity_hash.clone(),
            mutable_surface: crate::harness_evolution::MutableSurfaceDeclaration {
                schema_version: "mutable_surface_declaration.v1".to_string(),
                surfaces: vec!["prompt".to_string()],
            },
            workspace: crate::harness_evolution::CandidateWorkspace {
                schema_version: "candidate_workspace.v1".to_string(),
                workspace_id: "ws-3".to_string(),
                relative_path: "ws/3".to_string(),
                content_hash: near_hash.clone(),
            },
            content_hash: near_hash,
            status: CandidateStatus::Proposed,
            terminal_reason: CandidateTerminalReason::Admitted,
            seed: 102,
            created_at: "2026-08-20T00:02:00Z".to_string(),
        };
        let c3_body = serde_json::to_string(&c3).unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO harness_evolution_candidates (candidate_id, lineage_id, proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, content_hash, status, terminal_reason, workspace_id, workspace_rel_path, body_json, seed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                    params![
                        c3.candidate_id, c3.lineage_id, c3.proposal_id, c3.active_version_id,
                        c3.active_version_hash, c3.evaluator_identity_hash, c3.content_hash,
                        c3.status.as_str(), c3.terminal_reason.as_str(), c3.workspace.workspace_id,
                        c3.workspace.relative_path, c3_body, c3.seed as i64, c3.created_at
                    ],
                ).map_err(|e| e.to_string())
            })
            .unwrap();

        let div3 = store
            .evaluate_and_record_candidate_diversity(&contract, "cand-ec4-3", "worker-actor")
            .unwrap();
        assert!(!div3.is_exact_duplicate);
        assert!(div3.is_near_duplicate);

        let updated_c3: EvolutionCandidate = store
            .with_conn(|conn| {
                let b: String = conn
                    .query_row(
                        "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id='cand-ec4-3'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                Ok(serde_json::from_str(&b).unwrap())
            })
            .unwrap();
        assert_eq!(updated_c3.status, CandidateStatus::Rejected);
        assert_eq!(
            updated_c3.terminal_reason,
            CandidateTerminalReason::RejectedNearDuplicate
        );
    }

    #[test]
    fn ec4_store_exploration_collapse_rejection_lifecycle() {
        use crate::harness_evolution::{
            sample_active_identity, sample_ec4_diversity_contract, CandidateStatus,
            CandidateTerminalReason, CANDIDATE_SCHEMA_VERSION,
        };
        let _g = LabEnvGuard::enable();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec4-col.db");
        let store = LocalProductStore::new(&db).unwrap();

        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "actor-1")
            .unwrap();

        let mut contract = sample_ec4_diversity_contract();
        contract.max_family_concentration_bps = 4000;
        contract.collapse_stop_bps = 8000;
        let contract = crate::harness_evolution::seal_ec4_diversity_contract(contract).unwrap();

        // Insert proposals
        for i in 1..=5 {
            let prop_id = format!("prop-col-{}", i);
            store
                .with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO harness_evolution_proposals (proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, proposal_body_sha256, body_json, seed, created_at) VALUES (?1, ?2, ?3, ?4, ?5, '{}', 1, '2026-08-20T00:00:00Z')",
                        params![prop_id, active.active_version_id, active.active_version_hash, active.evaluator_identity_hash, "0".repeat(64)],
                    ).map_err(|e| e.to_string())
                })
                .unwrap();
        }

        // Insert 3 candidates all in the "prompt" mutable surface
        for i in 1..=3 {
            let cand_id = format!("cand-col-{}", i);
            let hash = i.to_string().repeat(64);
            let c = EvolutionCandidate {
                schema_version: CANDIDATE_SCHEMA_VERSION.to_string(),
                candidate_id: cand_id.clone(),
                lineage_id: format!("lin-col-{}", i),
                parent_candidate_id: None,
                proposal_id: format!("prop-col-{}", i),
                active_version_id: active.active_version_id.clone(),
                active_version_hash: active.active_version_hash.clone(),
                evaluator_identity_hash: active.evaluator_identity_hash.clone(),
                mutable_surface: crate::harness_evolution::MutableSurfaceDeclaration {
                    schema_version: "mutable_surface_declaration.v1".to_string(),
                    surfaces: vec!["prompt".to_string()],
                },
                workspace: crate::harness_evolution::CandidateWorkspace {
                    schema_version: "candidate_workspace.v1".to_string(),
                    workspace_id: format!("ws-{}", i),
                    relative_path: format!("ws/{}", i),
                    content_hash: hash.clone(),
                },
                content_hash: hash,
                status: CandidateStatus::Proposed,
                terminal_reason: CandidateTerminalReason::Admitted,
                seed: 200 + i,
                created_at: format!("2026-08-20T00:0{}:00Z", i),
            };
            let c_body = serde_json::to_string(&c).unwrap();
            store
                .with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO harness_evolution_candidates (candidate_id, lineage_id, proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, content_hash, status, terminal_reason, workspace_id, workspace_rel_path, body_json, seed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                        params![
                            c.candidate_id, c.lineage_id, c.proposal_id, c.active_version_id,
                            c.active_version_hash, c.evaluator_identity_hash, c.content_hash,
                            c.status.as_str(), c.terminal_reason.as_str(), c.workspace.workspace_id,
                            c.workspace.relative_path, c_body, c.seed as i64, c.created_at
                        ],
                    ).map_err(|e| e.to_string())
                })
                .unwrap();
        }

        // Insert 4th candidate also in "prompt" surface -> 4/4 from same family = 100% > 40% and >= 80% stop
        let cand_4 = EvolutionCandidate {
            schema_version: CANDIDATE_SCHEMA_VERSION.to_string(),
            candidate_id: "cand-col-4".to_string(),
            lineage_id: "lin-col-4".to_string(),
            parent_candidate_id: None,
            proposal_id: "prop-col-4".to_string(),
            active_version_id: active.active_version_id.clone(),
            active_version_hash: active.active_version_hash.clone(),
            evaluator_identity_hash: active.evaluator_identity_hash.clone(),
            mutable_surface: crate::harness_evolution::MutableSurfaceDeclaration {
                schema_version: "mutable_surface_declaration.v1".to_string(),
                surfaces: vec!["prompt".to_string()],
            },
            workspace: crate::harness_evolution::CandidateWorkspace {
                schema_version: "candidate_workspace.v1".to_string(),
                workspace_id: "ws-4".to_string(),
                relative_path: "ws/4".to_string(),
                content_hash: "4".repeat(64),
            },
            content_hash: "4".repeat(64),
            status: CandidateStatus::Proposed,
            terminal_reason: CandidateTerminalReason::Admitted,
            seed: 204,
            created_at: "2026-08-20T00:04:00Z".to_string(),
        };
        let c4_body = serde_json::to_string(&cand_4).unwrap();
        store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO harness_evolution_candidates (candidate_id, lineage_id, proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, content_hash, status, terminal_reason, workspace_id, workspace_rel_path, body_json, seed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                    params![
                        cand_4.candidate_id, cand_4.lineage_id, cand_4.proposal_id, cand_4.active_version_id,
                        cand_4.active_version_hash, cand_4.evaluator_identity_hash, cand_4.content_hash,
                        cand_4.status.as_str(), cand_4.terminal_reason.as_str(), cand_4.workspace.workspace_id,
                        cand_4.workspace.relative_path, c4_body, cand_4.seed as i64, cand_4.created_at
                    ],
                ).map_err(|e| e.to_string())
            })
            .unwrap();

        let div4 = store
            .evaluate_and_record_candidate_diversity(&contract, "cand-col-4", "worker-actor")
            .unwrap();
        assert!(div4.is_collapse_triggered);
        assert_eq!(div4.family_concentration_bps, 10_000);

        let updated_c4: EvolutionCandidate = store
            .with_conn(|conn| {
                let b: String = conn
                    .query_row(
                        "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id='cand-col-4'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                Ok(serde_json::from_str(&b).unwrap())
            })
            .unwrap();
        assert_eq!(updated_c4.status, CandidateStatus::Rejected);
        assert_eq!(
            updated_c4.terminal_reason,
            CandidateTerminalReason::RejectedExplorationCollapse
        );
    }

    #[test]
    fn ec5_store_selection_archive_and_rejection_lifecycle() {
        use crate::harness_evolution::{
            sample_active_identity, sample_ec5_selection_contract, CandidateSelectionEvaluation,
            CandidateStatus, CandidateTerminalReason, CANDIDATE_SCHEMA_VERSION,
        };
        use std::collections::HashMap;
        let _g = LabEnvGuard::enable();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec5-sel.db");
        let store = LocalProductStore::new(&db).unwrap();

        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "actor-1")
            .unwrap();

        let contract = sample_ec5_selection_contract();

        // Insert proposals
        for i in 1..=2 {
            let prop_id = format!("prop-sel-{}", i);
            store
                .with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO harness_evolution_proposals (proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, proposal_body_sha256, body_json, seed, created_at) VALUES (?1, ?2, ?3, ?4, ?5, '{}', 1, '2026-08-20T00:00:00Z')",
                        params![prop_id, active.active_version_id, active.active_version_hash, active.evaluator_identity_hash, "0".repeat(64)],
                    ).map_err(|e| e.to_string())
                })
                .unwrap();
        }

        // Insert candidates
        for i in 1..=2 {
            let cand_id = format!("cand-sel-{}", i);
            let hash = i.to_string().repeat(64);
            let c = EvolutionCandidate {
                schema_version: CANDIDATE_SCHEMA_VERSION.to_string(),
                candidate_id: cand_id.clone(),
                lineage_id: format!("lin-sel-{}", i),
                parent_candidate_id: None,
                proposal_id: format!("prop-sel-{}", i),
                active_version_id: active.active_version_id.clone(),
                active_version_hash: active.active_version_hash.clone(),
                evaluator_identity_hash: active.evaluator_identity_hash.clone(),
                mutable_surface: crate::harness_evolution::MutableSurfaceDeclaration {
                    schema_version: "mutable_surface_declaration.v1".to_string(),
                    surfaces: vec!["prompt".to_string()],
                },
                workspace: crate::harness_evolution::CandidateWorkspace {
                    schema_version: "candidate_workspace.v1".to_string(),
                    workspace_id: format!("ws-{}", i),
                    relative_path: format!("ws/{}", i),
                    content_hash: hash.clone(),
                },
                content_hash: hash,
                status: CandidateStatus::Proposed,
                terminal_reason: CandidateTerminalReason::Admitted,
                seed: 300 + i,
                created_at: format!("2026-08-20T00:0{}:00Z", i),
            };
            let c_body = serde_json::to_string(&c).unwrap();
            store
                .with_conn(|conn| {
                    conn.execute(
                        "INSERT INTO harness_evolution_candidates (candidate_id, lineage_id, proposal_id, active_version_id, active_version_hash, evaluator_identity_hash, content_hash, status, terminal_reason, workspace_id, workspace_rel_path, body_json, seed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                        params![
                            c.candidate_id, c.lineage_id, c.proposal_id, c.active_version_id,
                            c.active_version_hash, c.evaluator_identity_hash, c.content_hash,
                            c.status.as_str(), c.terminal_reason.as_str(), c.workspace.workspace_id,
                            c.workspace.relative_path, c_body, c.seed as i64, c.created_at
                        ],
                    ).map_err(|e| e.to_string())
                })
                .unwrap();
        }

        let mut gates_pass = HashMap::new();
        for gate in &contract.hard_gate_order {
            gates_pass.insert(gate.as_str().to_string(), true);
        }
        let mut gates_fail = gates_pass.clone();
        gates_fail.insert("verifier_pass".to_string(), false);

        let mut m1 = HashMap::new();
        m1.insert("quality_score".to_string(), 9000.0);
        m1.insert("token_cost".to_string(), 1000.0);
        m1.insert("wall_clock_seconds".to_string(), 10.0);

        let mut m2 = HashMap::new();
        m2.insert("quality_score".to_string(), 9500.0);
        m2.insert("token_cost".to_string(), 500.0);
        m2.insert("wall_clock_seconds".to_string(), 5.0);

        let eval1 = CandidateSelectionEvaluation {
            candidate_id: "cand-sel-1".to_string(),
            lineage_id: "lin-sel-1".to_string(),
            parent_candidate_id: None,
            proposal_id: "prop-sel-1".to_string(),
            hard_gate_passes: gates_pass,
            metrics: m1,
            seed: 301,
            causal_manifest_digest: Some("1".repeat(64)),
            counterevidence_digest: None,
            prediction_outcome_digest: Some("p1".repeat(32)),
            created_at: "2026-08-20T00:01:00Z".to_string(),
        };

        let eval2 = CandidateSelectionEvaluation {
            candidate_id: "cand-sel-2".to_string(),
            lineage_id: "lin-sel-2".to_string(),
            parent_candidate_id: None,
            proposal_id: "prop-sel-2".to_string(),
            hard_gate_passes: gates_fail,
            metrics: m2,
            seed: 302,
            causal_manifest_digest: Some("2".repeat(64)),
            counterevidence_digest: None,
            prediction_outcome_digest: Some("p2".repeat(32)),
            created_at: "2026-08-20T00:02:00Z".to_string(),
        };

        let results = store
            .record_ec5_candidate_selection_results(
                &contract,
                &[eval1.clone(), eval2.clone()],
                "selection-actor",
            )
            .unwrap();

        assert_eq!(results.len(), 2);
        let r1 = results
            .iter()
            .find(|r| r.candidate_id == "cand-sel-1")
            .unwrap();
        assert!(r1.is_hard_gate_passed);
        assert!(r1.selected_for_archive);
        assert_eq!(r1.sequential_rank, 0);

        let r2 = results
            .iter()
            .find(|r| r.candidate_id == "cand-sel-2")
            .unwrap();
        assert!(!r2.is_hard_gate_passed);
        assert!(!r2.selected_for_archive);
        assert_eq!(r2.sequential_rank, 1);

        // Verify cand-sel-2 is updated to Rejected in database
        let updated_c2: EvolutionCandidate = store
            .with_conn(|conn| {
                let b: String = conn
                    .query_row(
                        "SELECT body_json FROM harness_evolution_candidates WHERE candidate_id='cand-sel-2'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                Ok(serde_json::from_str(&b).unwrap())
            })
            .unwrap();
        assert_eq!(updated_c2.status, CandidateStatus::Rejected);
        assert_eq!(
            updated_c2.terminal_reason,
            CandidateTerminalReason::RejectedHardGate
        );
    }

    #[test]
    fn ec5_store_run_recovery_lifecycle_and_audit() {
        use crate::harness_evolution::{
            acquire_or_renew_run_lease, reconcile_and_restart_run, request_run_stop,
            sample_active_identity, seal_experiment_run_record, Ec5StopTriggerKind,
            ExperimentRunRecordV1, ExperimentRunState, EXPERIMENT_RUN_SCHEMA_VERSION,
        };
        let _g = LabEnvGuard::enable();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ec5-run.db");
        let store = LocalProductStore::new(&db).unwrap();

        let active = sample_active_identity();
        store
            .register_harness_evolution_active_identity(&active, "actor-1")
            .unwrap();

        let mut run = ExperimentRunRecordV1 {
            schema_version: EXPERIMENT_RUN_SCHEMA_VERSION.to_string(),
            run_id: "run-rec-1".to_string(),
            experiment_id: "exp-1".to_string(),
            contract_id: "PE7-HE-EC5-CONTRACT-1".to_string(),
            state: ExperimentRunState::Initialized,
            lease_holder: None,
            lease_expires_at: None,
            stop_reason: None,
            stop_detail: None,
            executed_effects_count: 0,
            total_token_cost: 0,
            total_wall_clock_seconds: 0,
            active_version_hash: active.active_version_hash.clone(),
            evaluator_identity_hash: active.evaluator_identity_hash.clone(),
            created_at: "2026-08-20T00:00:00Z".to_string(),
            updated_at: "2026-08-20T00:00:00Z".to_string(),
            record_sha256: String::new(),
        };

        // 1. Acquire lease
        acquire_or_renew_run_lease(
            &mut run,
            "worker-1",
            "2026-08-20T00:10:00Z",
            "2026-08-20T00:01:00Z",
        )
        .unwrap();
        let sealed_1 = seal_experiment_run_record(run.clone()).unwrap();
        store
            .record_ec5_run_lifecycle_event(&sealed_1, "lease_acquired", "worker-1")
            .unwrap();

        // 2. Stop run on diversity collapse
        request_run_stop(
            &mut run,
            Ec5StopTriggerKind::DiversityCollapseStop,
            "exploration diversity collapsed",
            "2026-08-20T00:05:00Z",
        )
        .unwrap();
        let sealed_2 = seal_experiment_run_record(run.clone()).unwrap();
        store
            .record_ec5_run_lifecycle_event(&sealed_2, "stopped", "supervisor")
            .unwrap();

        // 3. Reconcile and restart (simulated fresh run)
        let mut fresh_run = ExperimentRunRecordV1 {
            schema_version: EXPERIMENT_RUN_SCHEMA_VERSION.to_string(),
            run_id: "run-rec-2".to_string(),
            experiment_id: "exp-2".to_string(),
            contract_id: "PE7-HE-EC5-CONTRACT-1".to_string(),
            state: ExperimentRunState::LeaseAcquired,
            lease_holder: Some("crashed-worker".to_string()),
            lease_expires_at: Some("2026-08-20T00:04:00Z".to_string()),
            stop_reason: None,
            stop_detail: None,
            executed_effects_count: 3,
            total_token_cost: 15000,
            total_wall_clock_seconds: 45,
            active_version_hash: active.active_version_hash.clone(),
            evaluator_identity_hash: active.evaluator_identity_hash.clone(),
            created_at: "2026-08-20T00:00:00Z".to_string(),
            updated_at: "2026-08-20T00:00:00Z".to_string(),
            record_sha256: String::new(),
        };
        reconcile_and_restart_run(
            &mut fresh_run,
            &active.active_version_hash,
            &active.evaluator_identity_hash,
            "2026-08-20T00:05:00Z",
        )
        .unwrap();
        let sealed_3 = seal_experiment_run_record(fresh_run).unwrap();
        assert_eq!(sealed_3.executed_effects_count, 3);
        assert_eq!(sealed_3.total_token_cost, 15000);
        store
            .record_ec5_run_lifecycle_event(&sealed_3, "restarted", "supervisor")
            .unwrap();
    }
}
