//! PE7 Harness Evolution B3 — PR_READY candidate bundle (default-off).
//!
//! Produces a bounded, independently finalizer-validated bundle only. Does not
//! write active main, create/merge a PR, enable auto-merge, or change evaluator,
//! permissions, budgets, audit, promotion thresholds, release, deployment, or
//! rollback authority.

use crate::harness_evolution::{
    sha256_hex, validate_sha256_hex, ActiveHarnessIdentity, CandidateStatus,
    EvolutionAdmissionError, EvolutionCandidate, ENABLE_ENV, KILL_SWITCH_ENV,
};
use crate::harness_evolution_eval::{
    CandidateEvaluationBundle, HardGateResult, EVAL_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PR_READY_SCHEMA_VERSION: &str = "harness_evolution_pr_ready.v1";
pub const PR_READY_RECEIPT_SCHEMA: &str = "harness_evolution_pr_ready_receipt.v1";
pub const MAX_PATCH_BYTES: usize = 256 * 1024;
pub const MAX_ALLOWED_PATHS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrReadyTerminal {
    PrReady,
    RejectedStaleCandidate,
    RejectedChangedActiveVersion,
    RejectedChangedBase,
    RejectedTamperedEvaluation,
    RejectedScopeEscape,
    RejectedSecretScan,
    RejectedMissingTestEvidence,
    RejectedMissingEval,
    RejectedHardGate,
    RejectedDuplicateDelivery,
    RejectedKillSwitch,
    RejectedLabDisabled,
    RejectedMalformed,
    RejectedRollbackEvidence,
}

impl PrReadyTerminal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrReady => "pr_ready",
            Self::RejectedStaleCandidate => "rejected_stale_candidate",
            Self::RejectedChangedActiveVersion => "rejected_changed_active_version",
            Self::RejectedChangedBase => "rejected_changed_base",
            Self::RejectedTamperedEvaluation => "rejected_tampered_evaluation",
            Self::RejectedScopeEscape => "rejected_scope_escape",
            Self::RejectedSecretScan => "rejected_secret_scan",
            Self::RejectedMissingTestEvidence => "rejected_missing_test_evidence",
            Self::RejectedMissingEval => "rejected_missing_eval",
            Self::RejectedHardGate => "rejected_hard_gate",
            Self::RejectedDuplicateDelivery => "rejected_duplicate_delivery",
            Self::RejectedKillSwitch => "rejected_kill_switch",
            Self::RejectedLabDisabled => "rejected_lab_disabled",
            Self::RejectedMalformed => "rejected_malformed",
            Self::RejectedRollbackEvidence => "rejected_rollback_evidence",
        }
    }

    pub fn is_ready(self) -> bool {
        matches!(self, Self::PrReady)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReadyPatchBundle {
    pub schema_version: String,
    /// Unified-diff style patch text (fixture grammar only in laboratory).
    pub patch_text: String,
    pub patch_sha256: String,
    pub allowed_paths: Vec<String>,
    pub changed_paths: Vec<String>,
    pub base_commit_sha: String,
    pub head_commit_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReadyEvidence {
    pub static_check_sha256: String,
    pub test_evidence_sha256: String,
    pub secret_scan_sha256: String,
    pub rollback_evidence_sha256: String,
    pub evaluation_id: String,
    pub evaluation_bundle_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReadyCandidateBundle {
    pub schema_version: String,
    pub bundle_id: String,
    pub candidate_id: String,
    pub lineage_id: String,
    pub active_version_id: String,
    pub active_version_hash: String,
    pub evaluator_identity_hash: String,
    pub patch: PrReadyPatchBundle,
    pub evidence: PrReadyEvidence,
    pub operator_decision: String,
    pub terminal: PrReadyTerminal,
    pub bundle_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrReadyReceipt {
    pub schema_version: String,
    pub receipt_id: String,
    pub bundle_id: String,
    pub candidate_id: String,
    pub terminal: PrReadyTerminal,
    pub bundle_sha256: String,
    pub created_at: String,
}

pub fn lab_enabled() -> bool {
    std::env::var(ENABLE_ENV).as_deref() == Ok("1")
}

pub fn kill_switch_active() -> bool {
    std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1")
}

pub fn derive_bundle_id(candidate_id: &str, patch_sha256: &str, evaluation_id: &str) -> String {
    format!(
        "epr_{}",
        &sha256_hex(&format!(
            "pr_ready|{}|{}|{}",
            candidate_id, patch_sha256, evaluation_id
        ))[..32]
    )
}

pub fn derive_pr_ready_receipt_id(bundle_id: &str, terminal: PrReadyTerminal) -> String {
    format!(
        "eprr_{}",
        &sha256_hex(&format!(
            "pr_ready_receipt|{}|{}",
            bundle_id,
            terminal.as_str()
        ))[..32]
    )
}

fn looks_like_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("api_key=")
        || lower.contains("begin private key")
        || lower.contains("secret_token")
        || lower.contains("password=")
}

fn validate_relative_path(path: &str) -> Result<(), EvolutionAdmissionError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains("..")
        || path.contains('\0')
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_scope",
            "path must be a safe relative workspace path",
        ));
    }
    Ok(())
}

/// Independently validate a PR_READY candidate against active identity, evaluation, and patch contracts.
pub fn finalize_pr_ready_bundle(
    candidate: &EvolutionCandidate,
    current_active: &ActiveHarnessIdentity,
    evaluation: &CandidateEvaluationBundle,
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
    created_at: &str,
) -> Result<(PrReadyCandidateBundle, PrReadyReceipt), EvolutionAdmissionError> {
    if !lab_enabled() {
        return Err(EvolutionAdmissionError::new(
            "evolution_lab_disabled",
            "Harness evolution laboratory is default-off",
        ));
    }
    if kill_switch_active() {
        return Err(EvolutionAdmissionError::new(
            "evolution_kill_switch",
            "Harness evolution kill switch is active",
        ));
    }
    if candidate.status != CandidateStatus::Admitted {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_stale_candidate",
            "only admitted candidates may become PR_READY",
        ));
    }
    if candidate.active_version_id != current_active.active_version_id
        || candidate.active_version_hash != current_active.active_version_hash
        || candidate.evaluator_identity_hash != current_active.evaluator_identity_hash
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_changed_active_version",
            "active Harness or evaluator identity changed",
        ));
    }
    if evaluation.schema_version != EVAL_SCHEMA_VERSION
        || evaluation.candidate_id != candidate.candidate_id
        || evaluation.bundle_sha256.is_empty()
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_missing_eval",
            "equal-budget evaluation receipt is required",
        ));
    }
    // Recompute evaluation hash integrity (tamper detection).
    let mut eval_for_hash = evaluation.clone();
    let claimed = eval_for_hash.bundle_sha256.clone();
    eval_for_hash.bundle_sha256.clear();
    let encoded = serde_json::to_string(&eval_for_hash).map_err(|e| {
        EvolutionAdmissionError::new("evolution_pr_ready_eval_encode", e.to_string())
    })?;
    let recomputed = sha256_hex(&encoded);
    if recomputed != claimed {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_tampered_eval",
            "evaluation bundle hash mismatch",
        ));
    }
    if evaluation.claims_improvement || evaluation.sealed_feedback_into_mutation {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_eval_contract",
            "evaluation violates laboratory claims contract",
        ));
    }
    if !evaluation
        .baselines
        .iter()
        .any(|b| !b.used_sealed_holdout && b.hard_gate == HardGateResult::Passed)
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_hard_gate",
            "no validation baseline passed hard gates",
        ));
    }
    if base_commit_sha != expected_base_commit_sha {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_changed_base",
            "base commit identity changed",
        ));
    }
    for h in [
        base_commit_sha,
        head_commit_sha,
        static_check_sha256,
        test_evidence_sha256,
        secret_scan_sha256,
        rollback_evidence_sha256,
    ] {
        validate_sha256_hex(h).map_err(|_| {
            EvolutionAdmissionError::new(
                "evolution_pr_ready_hash",
                "identity hashes must be 64 lowercase hex",
            )
        })?;
    }
    if test_evidence_sha256 == "0".repeat(64) {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_missing_test",
            "test evidence hash is required",
        ));
    }
    if rollback_evidence_sha256 == "0".repeat(64) {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_rollback",
            "rollback evidence hash is required",
        ));
    }
    if patch_text.is_empty() || patch_text.len() > MAX_PATCH_BYTES {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_patch",
            "patch size out of bounds",
        ));
    }
    if !patch_text.starts_with("diff --git ") && !patch_text.contains("\n+++ ") {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_patch_grammar",
            "patch must use versioned unified-diff grammar",
        ));
    }
    if looks_like_secret(patch_text) {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_secret",
            "secret scan refused patch contents",
        ));
    }
    if allowed_paths.is_empty() || allowed_paths.len() > MAX_ALLOWED_PATHS {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_paths",
            "allowed_paths bound violated",
        ));
    }
    for path in allowed_paths {
        validate_relative_path(path)?;
    }
    // Derive changed paths from simple "+++ b/<path>" lines.
    let mut changed = Vec::new();
    for line in patch_text.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            let path = rest.trim();
            validate_relative_path(path)?;
            if !allowed_paths.iter().any(|p| p == path) {
                return Err(EvolutionAdmissionError::new(
                    "evolution_pr_ready_scope",
                    "changed path escapes allowed mutable surface",
                ));
            }
            changed.push(path.to_string());
        }
    }
    if changed.is_empty() {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_patch",
            "patch declares no changed paths",
        ));
    }
    if operator_decision.trim() != "approve_pr_ready" {
        return Err(EvolutionAdmissionError::new(
            "evolution_pr_ready_operator",
            "explicit operator decision approve_pr_ready is required",
        ));
    }

    let patch_sha256 = sha256_hex(patch_text);
    let patch = PrReadyPatchBundle {
        schema_version: PR_READY_SCHEMA_VERSION.to_string(),
        patch_text: patch_text.to_string(),
        patch_sha256: patch_sha256.clone(),
        allowed_paths: allowed_paths.to_vec(),
        changed_paths: changed,
        base_commit_sha: base_commit_sha.to_string(),
        head_commit_sha: head_commit_sha.to_string(),
    };
    let evidence = PrReadyEvidence {
        static_check_sha256: static_check_sha256.to_string(),
        test_evidence_sha256: test_evidence_sha256.to_string(),
        secret_scan_sha256: secret_scan_sha256.to_string(),
        rollback_evidence_sha256: rollback_evidence_sha256.to_string(),
        evaluation_id: evaluation.evaluation_id.clone(),
        evaluation_bundle_sha256: evaluation.bundle_sha256.clone(),
    };
    let bundle_id = derive_bundle_id(
        &candidate.candidate_id,
        &patch_sha256,
        &evaluation.evaluation_id,
    );
    let mut bundle = PrReadyCandidateBundle {
        schema_version: PR_READY_SCHEMA_VERSION.to_string(),
        bundle_id: bundle_id.clone(),
        candidate_id: candidate.candidate_id.clone(),
        lineage_id: candidate.lineage_id.clone(),
        active_version_id: candidate.active_version_id.clone(),
        active_version_hash: candidate.active_version_hash.clone(),
        evaluator_identity_hash: candidate.evaluator_identity_hash.clone(),
        patch,
        evidence,
        operator_decision: operator_decision.to_string(),
        terminal: PrReadyTerminal::PrReady,
        bundle_sha256: String::new(),
        created_at: created_at.to_string(),
    };
    bundle.bundle_sha256 = hash_bundle(&bundle)?;
    let receipt = PrReadyReceipt {
        schema_version: PR_READY_RECEIPT_SCHEMA.to_string(),
        receipt_id: derive_pr_ready_receipt_id(&bundle_id, PrReadyTerminal::PrReady),
        bundle_id,
        candidate_id: candidate.candidate_id.clone(),
        terminal: PrReadyTerminal::PrReady,
        bundle_sha256: bundle.bundle_sha256.clone(),
        created_at: created_at.to_string(),
    };
    Ok((bundle, receipt))
}

fn hash_bundle(bundle: &PrReadyCandidateBundle) -> Result<String, EvolutionAdmissionError> {
    let mut for_hash = bundle.clone();
    for_hash.bundle_sha256.clear();
    // Never hash raw patch into durable redacted logs; hash includes patch_sha256 only.
    for_hash.patch.patch_text.clear();
    let encoded = serde_json::to_string(&for_hash)
        .map_err(|e| EvolutionAdmissionError::new("evolution_pr_ready_encode", e.to_string()))?;
    Ok(sha256_hex(&encoded))
}

pub fn redacted_pr_ready_evidence(bundle: &PrReadyCandidateBundle) -> Value {
    json!({
        "schema_version": PR_READY_SCHEMA_VERSION,
        "bundle_id": bundle.bundle_id,
        "candidate_id": bundle.candidate_id,
        "lineage_id": bundle.lineage_id,
        "active_version_id": bundle.active_version_id,
        "terminal": bundle.terminal.as_str(),
        "patch_sha256": bundle.patch.patch_sha256,
        "base_commit_sha": bundle.patch.base_commit_sha,
        "head_commit_sha": bundle.patch.head_commit_sha,
        "allowed_paths": bundle.patch.allowed_paths,
        "changed_paths": bundle.patch.changed_paths,
        "evaluation_id": bundle.evidence.evaluation_id,
        "evaluation_bundle_sha256": bundle.evidence.evaluation_bundle_sha256,
        "static_check_sha256": bundle.evidence.static_check_sha256,
        "test_evidence_sha256": bundle.evidence.test_evidence_sha256,
        "secret_scan_sha256": bundle.evidence.secret_scan_sha256,
        "rollback_evidence_sha256": bundle.evidence.rollback_evidence_sha256,
        "operator_decision": bundle.operator_decision,
        "bundle_sha256": bundle.bundle_sha256,
        // Explicit non-claims for honesty.
        "writes_main": false,
        "creates_pr": false,
        "merges_pr": false,
        "auto_merge": false,
    })
}

pub fn map_finalize_error(err: &EvolutionAdmissionError) -> PrReadyTerminal {
    match err.code.as_str() {
        "evolution_lab_disabled" => PrReadyTerminal::RejectedLabDisabled,
        "evolution_kill_switch" => PrReadyTerminal::RejectedKillSwitch,
        "evolution_pr_ready_stale_candidate" => PrReadyTerminal::RejectedStaleCandidate,
        "evolution_pr_ready_changed_active_version" => {
            PrReadyTerminal::RejectedChangedActiveVersion
        }
        "evolution_pr_ready_changed_base" => PrReadyTerminal::RejectedChangedBase,
        "evolution_pr_ready_tampered_eval" => PrReadyTerminal::RejectedTamperedEvaluation,
        "evolution_pr_ready_scope" => PrReadyTerminal::RejectedScopeEscape,
        "evolution_pr_ready_secret" => PrReadyTerminal::RejectedSecretScan,
        "evolution_pr_ready_missing_test" => PrReadyTerminal::RejectedMissingTestEvidence,
        "evolution_pr_ready_missing_eval" | "evolution_pr_ready_eval_contract" => {
            PrReadyTerminal::RejectedMissingEval
        }
        "evolution_pr_ready_hard_gate" => PrReadyTerminal::RejectedHardGate,
        "evolution_pr_ready_rollback" => PrReadyTerminal::RejectedRollbackEvidence,
        "evolution_duplicate_pr_ready" => PrReadyTerminal::RejectedDuplicateDelivery,
        _ => PrReadyTerminal::RejectedMalformed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness_evolution::{
        candidate_from_proposal, proposal_from_body, sample_active_identity,
    };
    use crate::harness_evolution_eval::{
        build_sealed_vault, evaluate_candidate_fixture, sample_budget, sample_task_family,
    };

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_e: Option<String>,
        prev_k: Option<String>,
    }
    impl EnvGuard {
        fn enable() -> Self {
            let lock = crate::harness_evolution::EVOLUTION_LAB_TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let prev_e = std::env::var(ENABLE_ENV).ok();
            let prev_k = std::env::var(KILL_SWITCH_ENV).ok();
            std::env::set_var(ENABLE_ENV, "1");
            std::env::remove_var(KILL_SWITCH_ENV);
            Self {
                _lock: lock,
                prev_e,
                prev_k,
            }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.prev_e {
                Some(v) => std::env::set_var(ENABLE_ENV, v),
                None => std::env::remove_var(ENABLE_ENV),
            }
            match &self.prev_k {
                Some(v) => std::env::set_var(KILL_SWITCH_ENV, v),
                None => std::env::remove_var(KILL_SWITCH_ENV),
            }
        }
    }

    fn admitted_candidate(active: &ActiveHarnessIdentity) -> EvolutionCandidate {
        let proposal = proposal_from_body(
            active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"kind": "pr"}),
            vec![],
            3,
        )
        .unwrap();
        let mut c = candidate_from_proposal(
            &proposal,
            &sha256_hex("pr-content"),
            "candidates/pr",
            &sha256_hex("ws"),
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        c.status = CandidateStatus::Admitted;
        c
    }

    fn eval_for(candidate: &EvolutionCandidate) -> CandidateEvaluationBundle {
        let family = sample_task_family("fam-pr");
        let vault = build_sealed_vault(&family).unwrap();
        evaluate_candidate_fixture(
            &candidate.candidate_id,
            &candidate.lineage_id,
            &candidate.active_version_id,
            &candidate.active_version_hash,
            &candidate.evaluator_identity_hash,
            &sample_budget(1),
            &family,
            &vault,
            false,
            "2026-07-21T00:00:00Z",
        )
        .unwrap()
    }

    fn good_patch() -> &'static str {
        "diff --git a/prompts/rules.md b/prompts/rules.md\n--- a/prompts/rules.md\n+++ b/prompts/rules.md\n@@ -1 +1 @@\n-old\n+new\n"
    }

    #[test]
    fn accepts_valid_pr_ready_bundle() {
        let _g = EnvGuard::enable();
        let active = sample_active_identity();
        let candidate = admitted_candidate(&active);
        let evaluation = eval_for(&candidate);
        let base = sha256_hex("base");
        let (bundle, receipt) = finalize_pr_ready_bundle(
            &candidate,
            &active,
            &evaluation,
            good_patch(),
            &["prompts/rules.md".into()],
            &base,
            &sha256_hex("head"),
            &base,
            &sha256_hex("static"),
            &sha256_hex("tests"),
            &sha256_hex("secret-scan-clean"),
            &sha256_hex("rollback"),
            "approve_pr_ready",
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        assert_eq!(bundle.terminal, PrReadyTerminal::PrReady);
        assert_eq!(receipt.terminal, PrReadyTerminal::PrReady);
        let redacted = redacted_pr_ready_evidence(&bundle).to_string();
        assert!(redacted.contains("\"creates_pr\":false"));
        assert!(!redacted.contains("old\n+new"));
    }

    #[test]
    fn refuses_secret_scope_stale_tamper_and_missing_tests() {
        let _g = EnvGuard::enable();
        let active = sample_active_identity();
        let candidate = admitted_candidate(&active);
        let evaluation = eval_for(&candidate);
        let base = sha256_hex("base");
        let err = finalize_pr_ready_bundle(
            &candidate,
            &active,
            &evaluation,
            &format!(
                "diff --git a/x b/x\n+++ b/x\n+{}={}\n",
                ["api", "key"].join("_"),
                "supersecret"
            ),
            &["x".into()],
            &base,
            &sha256_hex("head"),
            &base,
            &sha256_hex("s"),
            &sha256_hex("t"),
            &sha256_hex("sec"),
            &sha256_hex("r"),
            "approve_pr_ready",
            "t",
        )
        .unwrap_err();
        assert_eq!(
            map_finalize_error(&err),
            PrReadyTerminal::RejectedSecretScan
        );

        let err = finalize_pr_ready_bundle(
            &candidate,
            &active,
            &evaluation,
            "diff --git a/x b/x\n+++ b/../escape\n+x\n",
            &["x".into()],
            &base,
            &sha256_hex("head"),
            &base,
            &sha256_hex("s"),
            &sha256_hex("t"),
            &sha256_hex("sec"),
            &sha256_hex("r"),
            "approve_pr_ready",
            "t",
        )
        .unwrap_err();
        assert_eq!(
            map_finalize_error(&err),
            PrReadyTerminal::RejectedScopeEscape
        );

        let err = finalize_pr_ready_bundle(
            &candidate,
            &active,
            &evaluation,
            good_patch(),
            &["prompts/rules.md".into()],
            &base,
            &sha256_hex("head"),
            &sha256_hex("other-base"),
            &sha256_hex("s"),
            &sha256_hex("t"),
            &sha256_hex("sec"),
            &sha256_hex("r"),
            "approve_pr_ready",
            "t",
        )
        .unwrap_err();
        assert_eq!(
            map_finalize_error(&err),
            PrReadyTerminal::RejectedChangedBase
        );

        let mut bad_eval = evaluation.clone();
        bad_eval.bundle_sha256 = "a".repeat(64);
        let err = finalize_pr_ready_bundle(
            &candidate,
            &active,
            &bad_eval,
            good_patch(),
            &["prompts/rules.md".into()],
            &base,
            &sha256_hex("head"),
            &base,
            &sha256_hex("s"),
            &sha256_hex("t"),
            &sha256_hex("sec"),
            &sha256_hex("r"),
            "approve_pr_ready",
            "t",
        )
        .unwrap_err();
        assert_eq!(
            map_finalize_error(&err),
            PrReadyTerminal::RejectedTamperedEvaluation
        );

        let err = finalize_pr_ready_bundle(
            &candidate,
            &active,
            &evaluation,
            good_patch(),
            &["prompts/rules.md".into()],
            &base,
            &sha256_hex("head"),
            &base,
            &sha256_hex("s"),
            &"0".repeat(64),
            &sha256_hex("sec"),
            &sha256_hex("r"),
            "approve_pr_ready",
            "t",
        )
        .unwrap_err();
        assert_eq!(
            map_finalize_error(&err),
            PrReadyTerminal::RejectedMissingTestEvidence
        );
    }
}
