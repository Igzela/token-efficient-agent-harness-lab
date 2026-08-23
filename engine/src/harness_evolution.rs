//! PE7 Harness Evolution laboratory — B1 evidence foundation (default-off).
//!
//! Owns versioned candidate/proposal/lineage identity and validation only.
//! Persistence is through `LocalProductStore`. Evaluation (B2) and PR_READY
//! finalization (B3) are separate packets. The active Harness, evaluator,
//! permissions, budgets, audit, target-output, merge, release, and rollback
//! owners remain immutable to candidates.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

pub const EVOLUTION_LAB_SCHEMA_VERSION: &str = "harness_evolution_lab.v1";
pub const CANDIDATE_SCHEMA_VERSION: &str = "harness_evolution_candidate.v1";
pub const PROPOSAL_SCHEMA_VERSION: &str = "harness_evolution_proposal.v1";
pub const LINEAGE_SCHEMA_VERSION: &str = "harness_evolution_lineage.v1";
pub const RECEIPT_SCHEMA_VERSION: &str = "harness_evolution_receipt.v1";
pub const WORKSPACE_SCHEMA_VERSION: &str = "harness_evolution_workspace.v1";
pub const ACTIVE_VERSION_SCHEMA: &str = "harness_active_version.v1";
pub const MUTABLE_SURFACE_SCHEMA: &str = "harness_mutable_surface.v1";
pub const EC1_CONTRACT_SCHEMA_VERSION: &str = "harness_evolution_ec1_contract.v1";
pub const EC1_CONTRACT_ID: &str = "PE7-HE-EC1-CONTRACT-1";
pub const FAILURE_PATTERN_EVIDENCE_SCHEMA: &str = "failure_pattern_evidence.v1";
pub const MUTATION_HYPOTHESIS_MANIFEST_SCHEMA: &str = "mutation_hypothesis_manifest.v1";
pub const PREDICTION_OUTCOME_SCHEMA: &str = "prediction_outcome.v1";
pub const MUTATION_FAMILY_REGISTRY_SCHEMA: &str = "mutation_family_registry.v1";
pub const EC1_IDENTITY_LINEAGE_SCHEMA: &str = "harness_evolution_ec1_identity_lineage.v1";
pub const EC1_CANDIDATE_BINDING_SCHEMA: &str = "harness_evolution_ec1_candidate_binding.v1";
pub const EC3_LIFECYCLE_BUDGET_SCHEMA: &str = "harness_evolution_ec3_lifecycle_budget.v1";

/// CWS analysis bound this SHA as the default-off active Harness. EC1 freezes it;
/// it is not a live ENABLE and does not authorize candidate generation.
pub const EC1_FROZEN_ACTIVE_HARNESS_SHA: &str =
    crate::context_working_set::CWS_ACTIVE_HARNESS_DEFAULT_OFF_SHA;
pub const EC1_GENERATOR_CLASS: &str = "unissued_bounded_generator.v1";
pub const EC1_INVALIDATION_CLASS: &str = "harness_evolution_invalidation.v1";
pub const EC1_BUDGET_CLASS: &str = "existing_budget_owner_non_authoritative.v1";

pub const ENABLE_ENV: &str = "ACP_ENABLE_HARNESS_EVOLUTION_LAB";
pub const KILL_SWITCH_ENV: &str = "ACP_HARNESS_EVOLUTION_KILL_SWITCH";
/// Canonical app-owned root for evolution candidate workspaces (must be set for real workspace ops).
pub const WORKSPACE_ROOT_ENV: &str = "ACP_HARNESS_EVOLUTION_WORKSPACE_ROOT";

/// Serializes process-wide evolution lab env mutations across unit tests.
#[cfg(test)]
pub(crate) static EVOLUTION_LAB_TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub const MAX_PROPOSAL_BYTES: usize = 16 * 1024;
pub const MAX_EVIDENCE_HASHES: usize = 32;
pub const MAX_MUTABLE_SURFACES: usize = 8;
pub const MAX_SCOPE_PATHS: usize = 32;
pub const MAX_WORKSPACE_REL_DEPTH: usize = 8;
pub const MAX_WORKSPACE_FILES: usize = 64;
pub const MAX_WORKSPACE_FILE_BYTES: usize = 64 * 1024;

/// Documented component-level mutable surfaces for the initial laboratory.
pub const ADMITTED_MUTABLE_SURFACES: &[&str] = &[
    "prompts_and_bounded_rules",
    "context_selection_and_summarization",
    "tool_descriptions_and_selection_policy",
    "retry_and_stop_policy",
    "model_routing_within_admitted_set",
    "recursive_decomposition_policy",
];

/// Surfaces that must never be declared mutable by a candidate/evolver.
pub const FORBIDDEN_MUTABLE_SURFACES: &[&str] = &[
    "evaluator",
    "sealed_labels",
    "permissions",
    "credentials",
    "budgets",
    "audit",
    "promotion_thresholds",
    "target_output",
    "merge",
    "release",
    "deployment",
    "rollback",
    "active_harness_source",
    "scheduler",
    "auth",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateTerminalReason {
    Admitted,
    RejectedStaleParent,
    RejectedChangedActiveVersion,
    RejectedDuplicate,
    RejectedTamper,
    RejectedWorkspaceEscape,
    RejectedMalformed,
    RejectedKillSwitch,
    RejectedPaused,
    RejectedLateWrite,
    RejectedForbiddenSurface,
    WorkspaceDiscarded,
}

impl CandidateTerminalReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::RejectedStaleParent => "rejected_stale_parent",
            Self::RejectedChangedActiveVersion => "rejected_changed_active_version",
            Self::RejectedDuplicate => "rejected_duplicate",
            Self::RejectedTamper => "rejected_tamper",
            Self::RejectedWorkspaceEscape => "rejected_workspace_escape",
            Self::RejectedMalformed => "rejected_malformed",
            Self::RejectedKillSwitch => "rejected_kill_switch",
            Self::RejectedPaused => "rejected_paused",
            Self::RejectedLateWrite => "rejected_late_write",
            Self::RejectedForbiddenSurface => "rejected_forbidden_surface",
            Self::WorkspaceDiscarded => "workspace_discarded",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Proposed,
    Admitted,
    Rejected,
    Discarded,
}

impl CandidateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Admitted => "admitted",
            Self::Rejected => "rejected",
            Self::Discarded => "discarded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveHarnessIdentity {
    pub schema_version: String,
    pub active_version_id: String,
    pub active_version_hash: String,
    pub evaluator_identity_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutableSurfaceDeclaration {
    pub schema_version: String,
    pub surfaces: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CausalStatus {
    Unknown,
    Hypothesized,
    Supported,
    Disputed,
}

impl CausalStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Hypothesized => "hypothesized",
            Self::Supported => "supported",
            Self::Disputed => "disputed",
        }
    }

    pub fn is_causal_proof(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRole {
    Observation,
    Inference,
}

impl EvidenceRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Inference => "inference",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredictionOutcomeKind {
    Correct,
    Incorrect,
    PartiallySupported,
    Contradicted,
    Unavailable,
}

impl PredictionOutcomeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Correct => "correct",
            Self::Incorrect => "incorrect",
            Self::PartiallySupported => "partially_supported",
            Self::Contradicted => "contradicted",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn is_evaluator_authority(self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailurePatternEvidenceV1 {
    pub schema_version: String,
    pub evidence_id: String,
    pub lineage_id: String,
    pub evidence_role: EvidenceRole,
    pub source_identity_hash: String,
    pub parent_identity_hash: String,
    pub generator_class: String,
    pub lineage_schema_version: String,
    pub invalidation_class: String,
    pub budget_class: String,
    pub observation_digest: String,
    pub causal_status: CausalStatus,
    pub counterevidence_digest: String,
    pub addressable_surface: String,
    pub mutable_surface: String,
    pub record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationHypothesisManifestV1 {
    pub schema_version: String,
    pub manifest_id: String,
    pub lineage_id: String,
    pub failure_evidence_id: String,
    pub proposal_body_sha256: String,
    pub candidate_delta_digest: String,
    pub predicted_improvement_digest: String,
    pub predicted_regression_digest: String,
    pub invariant_digest: String,
    pub evaluation_plan_digest: String,
    pub record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PredictionOutcomeV1 {
    pub schema_version: String,
    pub outcome_id: String,
    pub hypothesis_manifest_digest: String,
    pub evaluation_digest: String,
    pub evaluator_identity_hash: String,
    /// Evaluator-owned digest of development/validation counterevidence. Older
    /// v1 rows may omit this field; every newly derived row records it.
    #[serde(default)]
    pub counterevidence_digest: String,
    pub outcome: PredictionOutcomeKind,
    pub record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationFamilyRecord {
    pub family_id: String,
    pub admitted_surface: String,
    pub owner: String,
    pub non_authority: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationFamilyRegistry {
    pub schema_version: String,
    pub families: Vec<MutationFamilyRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleCostPhase {
    Diagnosis,
    HypothesisConstruction,
    Prediction,
    CandidateMaterialization,
    Evaluation,
    Review,
    Repair,
    Ci,
    Recovery,
    HumanEffort,
    OutcomeReconciliation,
}

pub const REQUIRED_LIFECYCLE_COST_PHASES: &[LifecycleCostPhase] = &[
    LifecycleCostPhase::Diagnosis,
    LifecycleCostPhase::HypothesisConstruction,
    LifecycleCostPhase::Prediction,
    LifecycleCostPhase::CandidateMaterialization,
    LifecycleCostPhase::Evaluation,
    LifecycleCostPhase::Review,
    LifecycleCostPhase::Repair,
    LifecycleCostPhase::Ci,
    LifecycleCostPhase::Recovery,
    LifecycleCostPhase::HumanEffort,
    LifecycleCostPhase::OutcomeReconciliation,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleCostDimension {
    ModelTokens,
    ProviderCalls,
    ProviderCostMicrounits,
    WallClockMilliseconds,
    ComputeMilliseconds,
    HumanEffortMilliseconds,
}

pub const REQUIRED_LIFECYCLE_COST_DIMENSIONS: &[LifecycleCostDimension] = &[
    LifecycleCostDimension::ModelTokens,
    LifecycleCostDimension::ProviderCalls,
    LifecycleCostDimension::ProviderCostMicrounits,
    LifecycleCostDimension::WallClockMilliseconds,
    LifecycleCostDimension::ComputeMilliseconds,
    LifecycleCostDimension::HumanEffortMilliseconds,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostTrustSource {
    MeasuredDirect,
    DerivedDeterministic,
    Unavailable,
    CallerEstimate,
}

pub const REQUIRED_COST_SOURCE_SEMANTICS: &[CostTrustSource] = &[
    CostTrustSource::MeasuredDirect,
    CostTrustSource::DerivedDeterministic,
    CostTrustSource::Unavailable,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteCostDisposition {
    CandidateIneligible,
    TreatAsZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZeroCostRule {
    ExplicitEvidenceRequired,
    ImplicitZeroAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReservationRule {
    RequiredBeforeExecution,
    BestEffort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationRule {
    ExactOnceAfterTerminal,
    EstimateAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureAccountingRule {
    ChargeAllAttempts,
    ChargeSuccessfulOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeScope {
    PerCandidate,
    AggregateAcrossCandidates,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeExhaustionRule {
    RejectReservationBeforeExecution,
    AllowAndEstimateAfterExecution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecyclePhasePolicy {
    pub phase: LifecycleCostPhase,
    pub required_dimensions: Vec<LifecycleCostDimension>,
    /// Complete values may be direct or deterministic. `Unavailable` is an
    /// explicit evidence state, not a zero or an eligible value.
    pub source_semantics: Vec<CostTrustSource>,
    /// Applies to every required dimension in this phase; both partial and
    /// unavailable evidence are incomplete.
    pub incomplete_cost_disposition: IncompleteCostDisposition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleResourceLimit {
    pub dimension: LifecycleCostDimension,
    /// A zero limit explicitly forbids use of this resource; it never means
    /// that an observed cost may be omitted or silently recorded as zero.
    pub limit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateLifecycleEnvelope {
    pub scope: EnvelopeScope,
    pub resource_limits: Vec<LifecycleResourceLimit>,
    pub max_repair_attempts: u32,
    pub max_ci_runs: u32,
    pub max_recovery_attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalLifecycleEnvelope {
    pub scope: EnvelopeScope,
    pub resource_limits: Vec<LifecycleResourceLimit>,
    pub max_candidates: u32,
    pub max_failed_candidates: u32,
}

/// Versioned EC3 accounting semantics only. This contract neither reserves nor
/// spends resources and does not own persistence, admission, or execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec3LifecycleBudgetContractV1 {
    pub schema_version: String,
    pub contract_id: String,
    pub phase_policies: Vec<LifecyclePhasePolicy>,
    pub candidate_envelope: CandidateLifecycleEnvelope,
    pub global_envelope: GlobalLifecycleEnvelope,
    pub zero_cost_rule: ZeroCostRule,
    pub reservation_rule: ReservationRule,
    pub reconciliation_rule: ReconciliationRule,
    pub failure_accounting_rule: FailureAccountingRule,
    pub envelope_exhaustion_rule: EnvelopeExhaustionRule,
    pub grants_spend_authority: bool,
    pub record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec1CandidateCausalBinding {
    pub schema_version: String,
    pub binding_id: String,
    pub family_id: String,
    pub hypothesis_manifest_id: String,
    pub candidate_delta_digest: String,
    pub lineage_id: String,
    pub seed: u64,
    pub record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec1IdentityLineageRecord {
    pub schema_version: String,
    pub lineage_id: String,
    pub parent_lineage_id: Option<String>,
    pub source_identity_hash: String,
    pub active_harness_sha: String,
    pub causal_source_id: Option<String>,
    pub record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionProposal {
    pub schema_version: String,
    pub proposal_id: String,
    pub parent_candidate_id: Option<String>,
    pub active_version_id: String,
    pub active_version_hash: String,
    pub evaluator_identity_hash: String,
    pub mutable_surface: MutableSurfaceDeclaration,
    /// Hash of redacted structured proposal body only — never raw prompts/outputs.
    pub proposal_body_sha256: String,
    pub evidence_hashes: Vec<String>,
    pub seed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateWorkspace {
    pub schema_version: String,
    pub workspace_id: String,
    /// Relative path under the app-owned evolution root (never absolute).
    pub relative_path: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionCandidate {
    pub schema_version: String,
    pub candidate_id: String,
    pub lineage_id: String,
    pub parent_candidate_id: Option<String>,
    pub proposal_id: String,
    pub active_version_id: String,
    pub active_version_hash: String,
    pub evaluator_identity_hash: String,
    pub mutable_surface: MutableSurfaceDeclaration,
    pub workspace: CandidateWorkspace,
    pub content_hash: String,
    pub status: CandidateStatus,
    pub terminal_reason: CandidateTerminalReason,
    pub seed: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvolutionReceipt {
    pub schema_version: String,
    pub receipt_id: String,
    pub candidate_id: String,
    pub proposal_id: String,
    pub lineage_id: String,
    pub active_version_id: String,
    pub terminal_reason: CandidateTerminalReason,
    pub content_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvolutionAdmissionError {
    pub code: String,
    pub message: String,
}

impl EvolutionAdmissionError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

pub fn lab_enabled() -> bool {
    std::env::var(ENABLE_ENV).as_deref() == Ok("1")
}

pub fn kill_switch_active() -> bool {
    std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1")
}

pub fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

pub fn canonical_json_sha256(value: &Value) -> Result<String, String> {
    let canonical =
        crate::event_schema::canonical_event_json(value).map_err(|error| error.to_string())?;
    Ok(sha256_hex(&canonical))
}

pub fn derive_candidate_id(proposal_id: &str, content_hash: &str, seed: u64) -> String {
    let material = format!("candidate.v1|{proposal_id}|{content_hash}|{seed}");
    format!("hevc-{}", &sha256_hex(&material)[..32])
}

pub fn derive_lineage_id(parent_candidate_id: Option<&str>, proposal_id: &str) -> String {
    let parent = parent_candidate_id.unwrap_or("root");
    let material = format!("lineage.v1|{parent}|{proposal_id}");
    format!("heln-{}", &sha256_hex(&material)[..32])
}

pub fn derive_proposal_id(
    active_version_id: &str,
    proposal_body_sha256: &str,
    seed: u64,
) -> String {
    let material = format!("proposal.v1|{active_version_id}|{proposal_body_sha256}|{seed}");
    format!("hepr-{}", &sha256_hex(&material)[..32])
}

pub fn derive_receipt_id(candidate_id: &str, terminal_reason: CandidateTerminalReason) -> String {
    let material = format!("receipt.v1|{candidate_id}|{}", terminal_reason.as_str());
    format!("herc-{}", &sha256_hex(&material)[..32])
}

pub fn validate_sha256_hex(value: &str) -> Result<(), EvolutionAdmissionError> {
    if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(EvolutionAdmissionError::new(
            "evolution_hash_invalid",
            "expected 64-char lowercase hex sha256",
        ));
    }
    if value.chars().any(|c| c.is_ascii_uppercase()) {
        return Err(EvolutionAdmissionError::new(
            "evolution_hash_invalid",
            "sha256 must be lowercase hex",
        ));
    }
    Ok(())
}

pub fn registered_mutation_families() -> MutationFamilyRegistry {
    MutationFamilyRegistry {
        schema_version: MUTATION_FAMILY_REGISTRY_SCHEMA.to_string(),
        families: ADMITTED_MUTABLE_SURFACES
            .iter()
            .map(|surface| MutationFamilyRecord {
                family_id: format!("family:{surface}"),
                admitted_surface: (*surface).to_string(),
                owner: "engine/src/harness_evolution.rs".to_string(),
                non_authority: true,
            })
            .collect(),
    }
}

fn require_nonempty_id(value: &str, code: &str) -> Result<(), EvolutionAdmissionError> {
    if value.trim().is_empty() {
        return Err(EvolutionAdmissionError::new(
            code,
            "identity cannot be caller-asserted empty",
        ));
    }
    Ok(())
}

fn record_digest_excluding_sha256(value: &Value) -> Result<String, EvolutionAdmissionError> {
    refuse_sensitive_payload_fields(value)?;
    let mut copy = value.clone();
    if let Value::Object(map) = &mut copy {
        map.remove("record_sha256");
    }
    canonical_json_sha256(&copy)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error))
}

pub fn derive_failure_evidence_id(
    lineage_id: &str,
    source_identity_hash: &str,
    observation_digest: &str,
) -> String {
    format!(
        "fpe-{}",
        &sha256_hex(&format!(
            "fpe.v1|{lineage_id}|{source_identity_hash}|{observation_digest}"
        ))[..32]
    )
}

pub fn derive_hypothesis_manifest_id(
    lineage_id: &str,
    failure_evidence_id: &str,
    candidate_delta_digest: &str,
) -> String {
    format!(
        "mh-{}",
        &sha256_hex(&format!(
            "mh.v1|{lineage_id}|{failure_evidence_id}|{candidate_delta_digest}"
        ))[..32]
    )
}

pub fn derive_prediction_outcome_id(
    hypothesis_manifest_digest: &str,
    evaluation_digest: &str,
) -> String {
    format!(
        "po-{}",
        &sha256_hex(&format!(
            "po.v1|{hypothesis_manifest_digest}|{evaluation_digest}"
        ))[..32]
    )
}

fn require_derived_id(observed: &str, expected: &str) -> Result<(), EvolutionAdmissionError> {
    if observed != expected {
        return Err(EvolutionAdmissionError::new(
            "ec1_identity_asserted",
            "identity must be derived from source hashes, not caller text",
        ));
    }
    Ok(())
}

fn require_record_sha256(value: &Value, observed: &str) -> Result<(), EvolutionAdmissionError> {
    let expected = record_digest_excluding_sha256(value)?;
    if observed != expected {
        return Err(EvolutionAdmissionError::new(
            "ec1_record_tamper",
            "record_sha256 must match canonical content excluding itself",
        ));
    }
    Ok(())
}

fn require_ec1_identity_classes(
    generator_class: &str,
    lineage_schema_version: &str,
    invalidation_class: &str,
    budget_class: &str,
) -> Result<(), EvolutionAdmissionError> {
    if generator_class != EC1_GENERATOR_CLASS
        || lineage_schema_version != LINEAGE_SCHEMA_VERSION
        || invalidation_class != EC1_INVALIDATION_CLASS
        || budget_class != EC1_BUDGET_CLASS
    {
        return Err(EvolutionAdmissionError::new(
            "ec1_identity_class_mismatch",
            "parent/generator/lineage/invalidation/budget classes are frozen",
        ));
    }
    Ok(())
}

pub fn validate_failure_pattern_evidence(
    evidence: &FailurePatternEvidenceV1,
) -> Result<(), EvolutionAdmissionError> {
    if evidence.schema_version != FAILURE_PATTERN_EVIDENCE_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec1_schema_invalid",
            "failure pattern schema mismatch",
        ));
    }
    require_nonempty_id(&evidence.lineage_id, "ec1_identity_empty")?;
    validate_sha256_hex(&evidence.source_identity_hash)?;
    validate_sha256_hex(&evidence.parent_identity_hash)?;
    validate_sha256_hex(&evidence.observation_digest)?;
    validate_sha256_hex(&evidence.counterevidence_digest)?;
    require_ec1_identity_classes(
        &evidence.generator_class,
        &evidence.lineage_schema_version,
        &evidence.invalidation_class,
        &evidence.budget_class,
    )?;
    if evidence.causal_status.is_causal_proof() {
        return Err(EvolutionAdmissionError::new(
            "ec1_causal_proof",
            "causal status is not causal proof",
        ));
    }
    if evidence.evidence_role == EvidenceRole::Inference
        && evidence.causal_status == CausalStatus::Supported
    {
        return Err(EvolutionAdmissionError::new(
            "ec1_inference_as_proof",
            "inference cannot be recorded as supported causal proof",
        ));
    }
    require_derived_id(
        &evidence.evidence_id,
        &derive_failure_evidence_id(
            &evidence.lineage_id,
            &evidence.source_identity_hash,
            &evidence.observation_digest,
        ),
    )?;
    if !ADMITTED_MUTABLE_SURFACES.contains(&evidence.mutable_surface.as_str())
        || !ADMITTED_MUTABLE_SURFACES.contains(&evidence.addressable_surface.as_str())
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_forbidden_surface",
            "failure pattern surface is not admitted",
        ));
    }
    let value = serde_json::to_value(evidence)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    require_record_sha256(&value, &evidence.record_sha256)?;
    Ok(())
}

pub fn validate_mutation_hypothesis_manifest(
    manifest: &MutationHypothesisManifestV1,
) -> Result<(), EvolutionAdmissionError> {
    if manifest.schema_version != MUTATION_HYPOTHESIS_MANIFEST_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec1_schema_invalid",
            "hypothesis manifest schema mismatch",
        ));
    }
    require_nonempty_id(&manifest.lineage_id, "ec1_identity_empty")?;
    require_nonempty_id(&manifest.failure_evidence_id, "ec1_identity_empty")?;
    validate_sha256_hex(&manifest.proposal_body_sha256)?;
    validate_sha256_hex(&manifest.candidate_delta_digest)?;
    validate_sha256_hex(&manifest.predicted_improvement_digest)?;
    validate_sha256_hex(&manifest.predicted_regression_digest)?;
    validate_sha256_hex(&manifest.invariant_digest)?;
    validate_sha256_hex(&manifest.evaluation_plan_digest)?;
    require_derived_id(
        &manifest.manifest_id,
        &derive_hypothesis_manifest_id(
            &manifest.lineage_id,
            &manifest.failure_evidence_id,
            &manifest.candidate_delta_digest,
        ),
    )?;
    let value = serde_json::to_value(manifest)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    require_record_sha256(&value, &manifest.record_sha256)?;
    Ok(())
}

pub fn validate_prediction_outcome_contract(
    outcome: &PredictionOutcomeV1,
) -> Result<(), EvolutionAdmissionError> {
    if outcome.schema_version != PREDICTION_OUTCOME_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec1_schema_invalid",
            "prediction outcome schema mismatch",
        ));
    }
    validate_sha256_hex(&outcome.hypothesis_manifest_digest)?;
    validate_sha256_hex(&outcome.evaluation_digest)?;
    validate_sha256_hex(&outcome.evaluator_identity_hash)?;
    if !outcome.counterevidence_digest.is_empty() {
        validate_sha256_hex(&outcome.counterevidence_digest)?;
    }
    require_derived_id(
        &outcome.outcome_id,
        &derive_prediction_outcome_id(
            &outcome.hypothesis_manifest_digest,
            &outcome.evaluation_digest,
        ),
    )?;
    if outcome.outcome.is_evaluator_authority() {
        return Err(EvolutionAdmissionError::new(
            "ec1_prediction_authority",
            "prediction outcome cannot grant evaluator authority",
        ));
    }
    let value = serde_json::to_value(outcome)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    require_record_sha256(&value, &outcome.record_sha256)?;
    Ok(())
}

pub fn seal_prediction_outcome(
    mut outcome: PredictionOutcomeV1,
) -> Result<PredictionOutcomeV1, EvolutionAdmissionError> {
    outcome.schema_version = PREDICTION_OUTCOME_SCHEMA.to_string();
    outcome.outcome_id = derive_prediction_outcome_id(
        &outcome.hypothesis_manifest_digest,
        &outcome.evaluation_digest,
    );
    let mut value = serde_json::to_value(&outcome)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    outcome.record_sha256 = record_digest_excluding_sha256(&value)?;
    validate_prediction_outcome_contract(&outcome)?;
    Ok(outcome)
}

pub fn validate_mutation_family_registry(
    registry: &MutationFamilyRegistry,
) -> Result<(), EvolutionAdmissionError> {
    if registry.schema_version != MUTATION_FAMILY_REGISTRY_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec1_schema_invalid",
            "mutation family registry schema mismatch",
        ));
    }
    if registry.families.is_empty() {
        return Err(EvolutionAdmissionError::new(
            "ec1_registry_empty",
            "mutation family registry must be pre-registered",
        ));
    }
    for family in &registry.families {
        if !ADMITTED_MUTABLE_SURFACES.contains(&family.admitted_surface.as_str()) {
            return Err(EvolutionAdmissionError::new(
                "evolution_forbidden_surface",
                format!("unregistered mutation family {}", family.family_id),
            ));
        }
        if FORBIDDEN_MUTABLE_SURFACES.contains(&family.admitted_surface.as_str())
            || !family.non_authority
            || family.owner != "engine/src/harness_evolution.rs"
        {
            return Err(EvolutionAdmissionError::new(
                "evolution_forbidden_surface",
                "mutation family cannot reach evaluator or authority policy",
            ));
        }
    }
    Ok(())
}

fn ec3_identity_value(
    contract: &Ec3LifecycleBudgetContractV1,
) -> Result<Value, EvolutionAdmissionError> {
    let mut value = serde_json::to_value(contract)
        .map_err(|error| EvolutionAdmissionError::new("ec3_record_digest", error.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("contract_id".into(), Value::String(String::new()));
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    Ok(value)
}

pub fn derive_ec3_lifecycle_budget_contract_id(
    contract: &Ec3LifecycleBudgetContractV1,
) -> Result<String, EvolutionAdmissionError> {
    let value = ec3_identity_value(contract)?;
    let digest = canonical_json_sha256(&value)
        .map_err(|error| EvolutionAdmissionError::new("ec3_record_digest", error))?;
    Ok(format!("hebc-{}", &digest[..32]))
}

fn validate_ec3_phase_policies(
    policies: &[LifecyclePhasePolicy],
) -> Result<(), EvolutionAdmissionError> {
    let mut phases = BTreeSet::new();
    let mut covered_dimensions = BTreeSet::new();
    for policy in policies {
        if !phases.insert(policy.phase) {
            return Err(EvolutionAdmissionError::new(
                "ec3_phase_duplicate",
                "each lifecycle phase must have exactly one policy",
            ));
        }
        if policy.required_dimensions.is_empty() {
            return Err(EvolutionAdmissionError::new(
                "ec3_phase_cost_missing",
                "each lifecycle phase must require at least one cost dimension",
            ));
        }
        let mut dimensions = BTreeSet::new();
        for dimension in &policy.required_dimensions {
            if !dimensions.insert(*dimension) {
                return Err(EvolutionAdmissionError::new(
                    "ec3_phase_dimension_duplicate",
                    "a phase cannot repeat a required cost dimension",
                ));
            }
            covered_dimensions.insert(*dimension);
        }
        if policy.source_semantics.is_empty() {
            return Err(EvolutionAdmissionError::new(
                "ec3_source_missing",
                "each lifecycle phase must define complete and unavailable source semantics",
            ));
        }
        let mut sources = BTreeSet::new();
        for source in &policy.source_semantics {
            if *source == CostTrustSource::CallerEstimate {
                return Err(EvolutionAdmissionError::new(
                    "ec3_source_untrusted",
                    "caller estimates are not lifecycle-cost evidence",
                ));
            }
            if !sources.insert(*source) {
                return Err(EvolutionAdmissionError::new(
                    "ec3_source_duplicate",
                    "a phase cannot repeat a cost source",
                ));
            }
        }
        for source in REQUIRED_COST_SOURCE_SEMANTICS {
            if !sources.contains(source) {
                return Err(EvolutionAdmissionError::new(
                    "ec3_source_semantics_incomplete",
                    format!(
                        "phase {:?} is missing source state {source:?}",
                        policy.phase
                    ),
                ));
            }
        }
        if sources.len() != REQUIRED_COST_SOURCE_SEMANTICS.len() {
            return Err(EvolutionAdmissionError::new(
                "ec3_source_semantics_unknown",
                "phase source semantics must match the frozen set exactly",
            ));
        }
        if policy.incomplete_cost_disposition != IncompleteCostDisposition::CandidateIneligible {
            return Err(EvolutionAdmissionError::new(
                "ec3_missing_cost_must_fail_closed",
                "partial or unavailable required phase cost makes the candidate ineligible",
            ));
        }
    }
    for phase in REQUIRED_LIFECYCLE_COST_PHASES {
        if !phases.contains(phase) {
            return Err(EvolutionAdmissionError::new(
                "ec3_phase_missing",
                format!("missing lifecycle cost phase {phase:?}"),
            ));
        }
    }
    if phases.len() != REQUIRED_LIFECYCLE_COST_PHASES.len() {
        return Err(EvolutionAdmissionError::new(
            "ec3_phase_unknown",
            "phase policies must match the frozen lifecycle ontology exactly",
        ));
    }
    for dimension in REQUIRED_LIFECYCLE_COST_DIMENSIONS {
        if !covered_dimensions.contains(dimension) {
            return Err(EvolutionAdmissionError::new(
                "ec3_dimension_uncovered",
                format!("lifecycle cost dimension {dimension:?} is not covered by any phase"),
            ));
        }
    }
    Ok(())
}

fn validate_ec3_resource_limits(
    limits: &[LifecycleResourceLimit],
    envelope: &str,
) -> Result<BTreeMap<LifecycleCostDimension, u64>, EvolutionAdmissionError> {
    let mut indexed = BTreeMap::new();
    for resource in limits {
        if indexed.insert(resource.dimension, resource.limit).is_some() {
            return Err(EvolutionAdmissionError::new(
                "ec3_resource_limit_duplicate",
                format!("{envelope} envelope repeats a resource limit"),
            ));
        }
    }
    for dimension in REQUIRED_LIFECYCLE_COST_DIMENSIONS {
        if !indexed.contains_key(dimension) {
            return Err(EvolutionAdmissionError::new(
                "ec3_resource_limit_missing",
                format!("{envelope} envelope is missing {dimension:?}"),
            ));
        }
    }
    if indexed.len() != REQUIRED_LIFECYCLE_COST_DIMENSIONS.len() {
        return Err(EvolutionAdmissionError::new(
            "ec3_resource_limit_unknown",
            format!("{envelope} envelope does not match the frozen cost dimensions"),
        ));
    }
    if indexed.values().all(|limit| *limit == 0) {
        return Err(EvolutionAdmissionError::new(
            "ec3_envelope_empty",
            format!("{envelope} envelope cannot forbid every resource"),
        ));
    }
    Ok(indexed)
}

pub fn validate_ec3_lifecycle_budget_contract(
    contract: &Ec3LifecycleBudgetContractV1,
) -> Result<(), EvolutionAdmissionError> {
    if contract.schema_version != EC3_LIFECYCLE_BUDGET_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec3_schema_invalid",
            "lifecycle budget contract schema mismatch",
        ));
    }
    if contract.contract_id != derive_ec3_lifecycle_budget_contract_id(contract)? {
        return Err(EvolutionAdmissionError::new(
            "ec3_contract_id_mismatch",
            "contract_id must be derived from the complete accounting contract",
        ));
    }
    if contract.zero_cost_rule != ZeroCostRule::ExplicitEvidenceRequired {
        return Err(EvolutionAdmissionError::new(
            "ec3_implicit_zero_forbidden",
            "zero cost requires measured or deterministic evidence",
        ));
    }
    if contract.reservation_rule != ReservationRule::RequiredBeforeExecution {
        return Err(EvolutionAdmissionError::new(
            "ec3_reservation_required",
            "the full candidate envelope must be reserved before execution",
        ));
    }
    if contract.reconciliation_rule != ReconciliationRule::ExactOnceAfterTerminal {
        return Err(EvolutionAdmissionError::new(
            "ec3_exact_reconciliation_required",
            "terminal actual cost must reconcile exactly once",
        ));
    }
    if contract.failure_accounting_rule != FailureAccountingRule::ChargeAllAttempts {
        return Err(EvolutionAdmissionError::new(
            "ec3_failure_accounting_required",
            "rejected, failed, cancelled, and recovery attempts remain charged",
        ));
    }
    if contract.grants_spend_authority {
        return Err(EvolutionAdmissionError::new(
            "ec3_spend_authority_forbidden",
            "this accounting contract cannot grant spend authority",
        ));
    }
    validate_ec3_phase_policies(&contract.phase_policies)?;
    if contract.candidate_envelope.scope != EnvelopeScope::PerCandidate
        || contract.global_envelope.scope != EnvelopeScope::AggregateAcrossCandidates
    {
        return Err(EvolutionAdmissionError::new(
            "ec3_envelope_scope_invalid",
            "candidate limits are per-candidate and global limits aggregate every candidate",
        ));
    }
    if contract.envelope_exhaustion_rule != EnvelopeExhaustionRule::RejectReservationBeforeExecution
    {
        return Err(EvolutionAdmissionError::new(
            "ec3_envelope_overflow_forbidden",
            "the first exhausted resource or count cap rejects reservation before execution",
        ));
    }
    let candidate =
        validate_ec3_resource_limits(&contract.candidate_envelope.resource_limits, "candidate")?;
    let global = validate_ec3_resource_limits(&contract.global_envelope.resource_limits, "global")?;
    if contract.global_envelope.max_candidates == 0 {
        return Err(EvolutionAdmissionError::new(
            "ec3_global_candidates_zero",
            "the global envelope must admit a finite positive candidate count",
        ));
    }
    if contract.global_envelope.max_failed_candidates > contract.global_envelope.max_candidates {
        return Err(EvolutionAdmissionError::new(
            "ec3_failed_candidates_overflow",
            "failed candidate bound cannot exceed the total candidate bound",
        ));
    }
    for dimension in REQUIRED_LIFECYCLE_COST_DIMENSIONS {
        if global[dimension] < candidate[dimension] {
            return Err(EvolutionAdmissionError::new(
                "ec3_global_limit_smaller",
                format!("global {dimension:?} limit is smaller than one candidate envelope"),
            ));
        }
    }
    let value = serde_json::to_value(contract)
        .map_err(|error| EvolutionAdmissionError::new("ec3_record_digest", error.to_string()))?;
    if contract.record_sha256 != record_digest_excluding_sha256(&value)? {
        return Err(EvolutionAdmissionError::new(
            "ec3_record_tamper",
            "record_sha256 must bind the complete EC3 contract",
        ));
    }
    Ok(())
}

pub fn seal_ec3_lifecycle_budget_contract(
    mut contract: Ec3LifecycleBudgetContractV1,
) -> Result<Ec3LifecycleBudgetContractV1, EvolutionAdmissionError> {
    contract.schema_version = EC3_LIFECYCLE_BUDGET_SCHEMA.to_string();
    contract.contract_id = derive_ec3_lifecycle_budget_contract_id(&contract)?;
    let value = serde_json::to_value(&contract)
        .map_err(|error| EvolutionAdmissionError::new("ec3_record_digest", error.to_string()))?;
    contract.record_sha256 = record_digest_excluding_sha256(&value)?;
    validate_ec3_lifecycle_budget_contract(&contract)?;
    Ok(contract)
}

pub fn frozen_ec1_active_harness_sha() -> &'static str {
    EC1_FROZEN_ACTIVE_HARNESS_SHA
}

pub fn derive_ec1_candidate_binding_id(
    family_id: &str,
    hypothesis_manifest_id: &str,
    candidate_delta_digest: &str,
    seed: u64,
) -> String {
    format!(
        "hecb-{}",
        &sha256_hex(&format!(
            "ec1-bind.v1|{family_id}|{hypothesis_manifest_id}|{candidate_delta_digest}|{seed}"
        ))[..32]
    )
}

fn lookup_registered_family(
    family_id: &str,
) -> Result<MutationFamilyRecord, EvolutionAdmissionError> {
    registered_mutation_families()
        .families
        .into_iter()
        .find(|family| family.family_id == family_id)
        .ok_or_else(|| {
            EvolutionAdmissionError::new("ec1_unknown_family", "mutation family is not registered")
        })
}

pub fn generate_ec1_candidate_binding(
    family_id: &str,
    hypothesis: &MutationHypothesisManifestV1,
    seed: u64,
) -> Result<Ec1CandidateCausalBinding, EvolutionAdmissionError> {
    validate_mutation_hypothesis_manifest(hypothesis)?;
    let family = lookup_registered_family(family_id)?;
    if !ADMITTED_MUTABLE_SURFACES.contains(&family.admitted_surface.as_str())
        || FORBIDDEN_MUTABLE_SURFACES.contains(&family.admitted_surface.as_str())
        || !family.non_authority
    {
        return Err(EvolutionAdmissionError::new(
            "ec1_unaddressable_pattern",
            "mutation family is not an admitted addressable surface",
        ));
    }
    if hypothesis.candidate_delta_digest.is_empty() {
        return Err(EvolutionAdmissionError::new(
            "ec1_unaddressable_pattern",
            "hypothesis delta digest is not addressable",
        ));
    }
    require_nonempty_id(&hypothesis.lineage_id, "ec1_identity_empty")?;
    let binding = Ec1CandidateCausalBinding {
        schema_version: EC1_CANDIDATE_BINDING_SCHEMA.to_string(),
        binding_id: derive_ec1_candidate_binding_id(
            family_id,
            &hypothesis.manifest_id,
            &hypothesis.candidate_delta_digest,
            seed,
        ),
        family_id: family.family_id,
        hypothesis_manifest_id: hypothesis.manifest_id.clone(),
        candidate_delta_digest: hypothesis.candidate_delta_digest.clone(),
        lineage_id: hypothesis.lineage_id.clone(),
        seed,
        record_sha256: String::new(),
    };
    seal_ec1_candidate_binding(binding)
}

pub fn validate_ec1_candidate_binding(
    binding: &Ec1CandidateCausalBinding,
) -> Result<(), EvolutionAdmissionError> {
    if binding.schema_version != EC1_CANDIDATE_BINDING_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec1_schema_invalid",
            "candidate binding schema mismatch",
        ));
    }
    lookup_registered_family(&binding.family_id)?;
    validate_sha256_hex(&binding.candidate_delta_digest)?;
    require_nonempty_id(&binding.hypothesis_manifest_id, "ec1_identity_empty")?;
    require_nonempty_id(&binding.lineage_id, "ec1_identity_empty")?;
    require_derived_id(
        &binding.binding_id,
        &derive_ec1_candidate_binding_id(
            &binding.family_id,
            &binding.hypothesis_manifest_id,
            &binding.candidate_delta_digest,
            binding.seed,
        ),
    )?;
    let value = serde_json::to_value(binding)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    require_record_sha256(&value, &binding.record_sha256)?;
    Ok(())
}

pub fn seal_ec1_candidate_binding(
    mut binding: Ec1CandidateCausalBinding,
) -> Result<Ec1CandidateCausalBinding, EvolutionAdmissionError> {
    binding.binding_id = derive_ec1_candidate_binding_id(
        &binding.family_id,
        &binding.hypothesis_manifest_id,
        &binding.candidate_delta_digest,
        binding.seed,
    );
    let mut value = serde_json::to_value(&binding)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    binding.record_sha256 = record_digest_excluding_sha256(&value)?;
    validate_ec1_candidate_binding(&binding)?;
    Ok(binding)
}

pub fn derive_ec1_identity_lineage_id(
    parent_lineage_id: Option<&str>,
    source_identity_hash: &str,
    active_harness_sha: &str,
) -> String {
    let parent = parent_lineage_id.unwrap_or("root");
    format!(
        "heil-{}",
        &sha256_hex(&format!(
            "ec1-lineage.v1|{parent}|{source_identity_hash}|{active_harness_sha}"
        ))[..32]
    )
}

pub fn validate_ec1_identity_lineage(
    record: &Ec1IdentityLineageRecord,
) -> Result<(), EvolutionAdmissionError> {
    if record.schema_version != EC1_IDENTITY_LINEAGE_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec1_schema_invalid",
            "identity lineage schema mismatch",
        ));
    }
    if record.active_harness_sha != EC1_FROZEN_ACTIVE_HARNESS_SHA {
        return Err(EvolutionAdmissionError::new(
            "ec1_active_harness_mismatch",
            "identity lineage must bind the CWS default-off Harness SHA",
        ));
    }
    validate_sha256_hex(&record.source_identity_hash)?;
    require_derived_id(
        &record.lineage_id,
        &derive_ec1_identity_lineage_id(
            record.parent_lineage_id.as_deref(),
            &record.source_identity_hash,
            &record.active_harness_sha,
        ),
    )?;
    if let Some(parent) = record.parent_lineage_id.as_deref() {
        require_nonempty_id(parent, "ec1_identity_empty")?;
        if parent == record.lineage_id {
            return Err(EvolutionAdmissionError::new(
                "ec1_lineage_cycle",
                "identity lineage cannot parent itself",
            ));
        }
    }
    if let Some(causal) = record.causal_source_id.as_deref() {
        if causal != record.lineage_id {
            return Err(EvolutionAdmissionError::new(
                "ec1_orphan_causal_source",
                "causal source must reference this lineage, not an unknown identity",
            ));
        }
    }
    let value = serde_json::to_value(record)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    require_record_sha256(&value, &record.record_sha256)?;
    Ok(())
}

pub fn seal_ec1_identity_lineage(
    mut record: Ec1IdentityLineageRecord,
) -> Result<Ec1IdentityLineageRecord, EvolutionAdmissionError> {
    record.lineage_id = derive_ec1_identity_lineage_id(
        record.parent_lineage_id.as_deref(),
        &record.source_identity_hash,
        &record.active_harness_sha,
    );
    let mut value = serde_json::to_value(&record)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    let digest = record_digest_excluding_sha256(&value)?;
    record.record_sha256 = digest;
    validate_ec1_identity_lineage(&record)?;
    Ok(record)
}

pub fn seal_failure_pattern_evidence(
    mut evidence: FailurePatternEvidenceV1,
) -> Result<FailurePatternEvidenceV1, EvolutionAdmissionError> {
    evidence.evidence_id = derive_failure_evidence_id(
        &evidence.lineage_id,
        &evidence.source_identity_hash,
        &evidence.observation_digest,
    );
    let mut value = serde_json::to_value(&evidence)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    evidence.record_sha256 = record_digest_excluding_sha256(&value)?;
    validate_failure_pattern_evidence(&evidence)?;
    Ok(evidence)
}

pub fn seal_mutation_hypothesis_manifest(
    mut manifest: MutationHypothesisManifestV1,
) -> Result<MutationHypothesisManifestV1, EvolutionAdmissionError> {
    manifest.manifest_id = derive_hypothesis_manifest_id(
        &manifest.lineage_id,
        &manifest.failure_evidence_id,
        &manifest.candidate_delta_digest,
    );
    let mut value = serde_json::to_value(&manifest)
        .map_err(|error| EvolutionAdmissionError::new("ec1_record_digest", error.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    manifest.record_sha256 = record_digest_excluding_sha256(&value)?;
    validate_mutation_hypothesis_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_mutable_surface(
    surface: &MutableSurfaceDeclaration,
) -> Result<(), EvolutionAdmissionError> {
    if surface.schema_version != MUTABLE_SURFACE_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "evolution_mutable_surface_schema",
            "mutable surface schema_version mismatch",
        ));
    }
    if surface.surfaces.is_empty() || surface.surfaces.len() > MAX_MUTABLE_SURFACES {
        return Err(EvolutionAdmissionError::new(
            "evolution_mutable_surface_bound",
            "mutable surface count out of bound",
        ));
    }
    let mut seen = BTreeSet::new();
    for name in &surface.surfaces {
        if !seen.insert(name.clone()) {
            return Err(EvolutionAdmissionError::new(
                "evolution_mutable_surface_duplicate",
                format!("duplicate mutable surface: {name}"),
            ));
        }
        if FORBIDDEN_MUTABLE_SURFACES.contains(&name.as_str()) {
            return Err(EvolutionAdmissionError::new(
                "evolution_forbidden_surface",
                format!("forbidden mutable surface: {name}"),
            ));
        }
        if !ADMITTED_MUTABLE_SURFACES.contains(&name.as_str()) {
            return Err(EvolutionAdmissionError::new(
                "evolution_unknown_surface",
                format!("mutable surface not in admitted set: {name}"),
            ));
        }
    }
    Ok(())
}

pub fn validate_workspace_relative_path(path: &str) -> Result<(), EvolutionAdmissionError> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_escape",
            "workspace path must be relative and non-empty",
        ));
    }
    if path.contains('\0') || path.contains("..") {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_escape",
            "workspace path traversal is forbidden",
        ));
    }
    let pb = PathBuf::from(path);
    let mut depth = 0usize;
    for component in pb.components() {
        match component {
            Component::Normal(_) => depth = depth.saturating_add(1),
            Component::CurDir => {}
            _ => {
                return Err(EvolutionAdmissionError::new(
                    "evolution_workspace_escape",
                    "workspace path contains forbidden component",
                ));
            }
        }
    }
    if depth == 0 || depth > MAX_WORKSPACE_REL_DEPTH {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_escape",
            "workspace path depth out of bound",
        ));
    }
    Ok(())
}

/// Resolve the configured app-owned evolution workspace root (canonicalized).
pub fn configured_workspace_root() -> Result<PathBuf, EvolutionAdmissionError> {
    let raw = std::env::var(WORKSPACE_ROOT_ENV).map_err(|_| {
        EvolutionAdmissionError::new(
            "evolution_workspace_root_unset",
            "ACP_HARNESS_EVOLUTION_WORKSPACE_ROOT must be set to an app-owned directory",
        )
    })?;
    if raw.trim().is_empty() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_root_unset",
            "ACP_HARNESS_EVOLUTION_WORKSPACE_ROOT must not be empty",
        ));
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_root",
            "workspace root must be an absolute path",
        ));
    }
    if !path.exists() || !path.is_dir() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_root",
            "workspace root must be an existing directory",
        ));
    }
    path.canonicalize()
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_root", e.to_string()))
}

/// Resolve a candidate workspace under an app-owned root; refuse escape and symlink ownership.
pub fn resolve_workspace_under_root(
    root: &Path,
    relative: &str,
) -> Result<PathBuf, EvolutionAdmissionError> {
    validate_workspace_relative_path(relative)?;
    let root_canon = root
        .canonicalize()
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_root", e.to_string()))?;
    let mut cursor = root_canon.clone();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(name) => {
                cursor = cursor.join(name);
                if cursor.exists() {
                    let canon = cursor.canonicalize().map_err(|e| {
                        EvolutionAdmissionError::new("evolution_workspace_escape", e.to_string())
                    })?;
                    if !canon.starts_with(&root_canon) {
                        return Err(EvolutionAdmissionError::new(
                            "evolution_workspace_escape",
                            "workspace escapes app-owned root",
                        ));
                    }
                    cursor = canon;
                }
            }
            _ => {
                return Err(EvolutionAdmissionError::new(
                    "evolution_workspace_escape",
                    "workspace path contains forbidden component",
                ));
            }
        }
    }
    Ok(cursor)
}

/// Deterministic content hash of the bounded workspace surface (sorted relative paths + contents).
pub fn hash_workspace_directory(workspace_dir: &Path) -> Result<String, EvolutionAdmissionError> {
    if !workspace_dir.is_dir() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_missing",
            "candidate workspace directory is missing",
        ));
    }
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    collect_workspace_files(workspace_dir, workspace_dir, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    if entries.len() > MAX_WORKSPACE_FILES {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_bound",
            "workspace file count exceeds bound",
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"harness_evolution_workspace_surface.v1\n");
    for (rel, bytes) in &entries {
        hasher.update(rel.as_bytes());
        hasher.update(b"\0");
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
        hasher.update(b"\n");
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect_workspace_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), EvolutionAdmissionError> {
    let read = std::fs::read_dir(current)
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_read", e.to_string()))?;
    for entry in read {
        let entry = entry
            .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_read", e.to_string()))?;
        let path = entry.path();
        let meta = entry
            .metadata()
            .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_read", e.to_string()))?;
        if meta.file_type().is_symlink() {
            return Err(EvolutionAdmissionError::new(
                "evolution_workspace_escape",
                "symlinks are forbidden inside candidate workspaces",
            ));
        }
        if meta.is_dir() {
            collect_workspace_files(root, &path, out)?;
            continue;
        }
        if !meta.is_file() {
            return Err(EvolutionAdmissionError::new(
                "evolution_workspace_escape",
                "non-file workspace entries are forbidden",
            ));
        }
        if meta.len() as usize > MAX_WORKSPACE_FILE_BYTES {
            return Err(EvolutionAdmissionError::new(
                "evolution_workspace_bound",
                "workspace file exceeds size bound",
            ));
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|_| {
                EvolutionAdmissionError::new(
                    "evolution_workspace_escape",
                    "workspace file escaped root during collection",
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        validate_workspace_relative_path(&rel)?;
        let bytes = std::fs::read(&path)
            .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_read", e.to_string()))?;
        out.push((rel, bytes));
    }
    Ok(())
}

/// Materialize bounded fixture files under the app-owned root and return a workspace descriptor.
pub fn materialize_candidate_workspace(
    root: &Path,
    workspace_id: &str,
    files: &[(String, Vec<u8>)],
) -> Result<CandidateWorkspace, EvolutionAdmissionError> {
    if workspace_id.is_empty()
        || workspace_id.contains('/')
        || workspace_id.contains('\\')
        || workspace_id.contains("..")
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_id",
            "workspace_id must be a single path segment",
        ));
    }
    if files.is_empty() || files.len() > MAX_WORKSPACE_FILES {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_bound",
            "workspace file count out of bound",
        ));
    }
    let relative_path = format!("candidates/{workspace_id}");
    validate_workspace_relative_path(&relative_path)?;
    let dir = resolve_workspace_under_root(root, &relative_path)?;
    if dir.exists() {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_exists",
            "workspace directory already exists",
        ));
    }
    std::fs::create_dir_all(&dir)
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_create", e.to_string()))?;
    for (rel, bytes) in files {
        validate_workspace_relative_path(rel)?;
        if bytes.len() > MAX_WORKSPACE_FILE_BYTES {
            return Err(EvolutionAdmissionError::new(
                "evolution_workspace_bound",
                "workspace file exceeds size bound",
            ));
        }
        let target = resolve_workspace_under_root(&dir, rel)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                EvolutionAdmissionError::new("evolution_workspace_create", e.to_string())
            })?;
        }
        std::fs::write(&target, bytes).map_err(|e| {
            EvolutionAdmissionError::new("evolution_workspace_write", e.to_string())
        })?;
    }
    let content_hash = hash_workspace_directory(&dir)?;
    Ok(CandidateWorkspace {
        schema_version: WORKSPACE_SCHEMA_VERSION.to_string(),
        workspace_id: workspace_id.to_string(),
        relative_path,
        content_hash,
    })
}

/// Recompute the workspace surface hash and refuse if it no longer matches the admitted hash.
pub fn revalidate_workspace_content(
    root: &Path,
    workspace: &CandidateWorkspace,
) -> Result<String, EvolutionAdmissionError> {
    let dir = resolve_workspace_under_root(root, &workspace.relative_path)?;
    let actual = hash_workspace_directory(&dir)?;
    if actual != workspace.content_hash {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_tamper",
            "workspace content hash no longer matches admitted surface",
        ));
    }
    Ok(actual)
}

/// Discard an unpromoted candidate workspace directory without touching active main.
pub fn discard_candidate_workspace(
    root: &Path,
    workspace: &CandidateWorkspace,
) -> Result<(), EvolutionAdmissionError> {
    let dir = resolve_workspace_under_root(root, &workspace.relative_path)?;
    if !dir.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&dir)
        .map_err(|e| EvolutionAdmissionError::new("evolution_workspace_discard", e.to_string()))?;
    Ok(())
}

/// Derive a stable workspace_id from proposal and seed material.
pub fn derive_workspace_id(proposal_id: &str, seed: u64) -> String {
    let material = format!("workspace.v1|{proposal_id}|{seed}");
    format!("hews-{}", &sha256_hex(&material)[..16])
}

pub fn validate_proposal(proposal: &EvolutionProposal) -> Result<(), EvolutionAdmissionError> {
    if proposal.schema_version != PROPOSAL_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_proposal_schema",
            "proposal schema_version mismatch",
        ));
    }
    if proposal.proposal_id.is_empty()
        || proposal.active_version_id.is_empty()
        || proposal.active_version_hash.is_empty()
        || proposal.evaluator_identity_hash.is_empty()
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_proposal_identity",
            "proposal identity fields required",
        ));
    }
    validate_sha256_hex(&proposal.active_version_hash)?;
    validate_sha256_hex(&proposal.evaluator_identity_hash)?;
    validate_sha256_hex(&proposal.proposal_body_sha256)?;
    if proposal.evidence_hashes.len() > MAX_EVIDENCE_HASHES {
        return Err(EvolutionAdmissionError::new(
            "evolution_evidence_bound",
            "too many evidence hashes",
        ));
    }
    for hash in &proposal.evidence_hashes {
        validate_sha256_hex(hash)?;
    }
    validate_mutable_surface(&proposal.mutable_surface)?;
    let expected = derive_proposal_id(
        &proposal.active_version_id,
        &proposal.proposal_body_sha256,
        proposal.seed,
    );
    if proposal.proposal_id != expected {
        return Err(EvolutionAdmissionError::new(
            "evolution_proposal_id_mismatch",
            "proposal_id is not deterministically derived",
        ));
    }
    Ok(())
}

pub fn validate_candidate_for_admission(
    candidate: &EvolutionCandidate,
    current_active: &ActiveHarnessIdentity,
    parent_still_valid: bool,
) -> Result<(), EvolutionAdmissionError> {
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
    if candidate.schema_version != CANDIDATE_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_candidate_schema",
            "candidate schema_version mismatch",
        ));
    }
    if current_active.schema_version != ACTIVE_VERSION_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "evolution_active_version_schema",
            "active version schema mismatch",
        ));
    }
    validate_sha256_hex(&candidate.active_version_hash)?;
    validate_sha256_hex(&candidate.evaluator_identity_hash)?;
    validate_sha256_hex(&candidate.content_hash)?;
    validate_mutable_surface(&candidate.mutable_surface)?;
    validate_workspace_relative_path(&candidate.workspace.relative_path)?;
    if candidate.workspace.schema_version != WORKSPACE_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_schema",
            "workspace schema_version mismatch",
        ));
    }
    validate_sha256_hex(&candidate.workspace.content_hash)?;
    if candidate.content_hash != candidate.workspace.content_hash {
        return Err(EvolutionAdmissionError::new(
            "evolution_content_workspace_mismatch",
            "candidate content_hash must equal workspace surface hash",
        ));
    }

    if candidate.active_version_id != current_active.active_version_id
        || candidate.active_version_hash != current_active.active_version_hash
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_changed_active_version",
            "candidate active-version binding does not match immutable current active Harness",
        ));
    }
    if candidate.evaluator_identity_hash != current_active.evaluator_identity_hash {
        return Err(EvolutionAdmissionError::new(
            "evolution_evaluator_immutable",
            "evaluator identity must remain immutable to the candidate",
        ));
    }
    if candidate.parent_candidate_id.is_some() && !parent_still_valid {
        return Err(EvolutionAdmissionError::new(
            "evolution_stale_parent",
            "parent candidate is stale or missing",
        ));
    }
    let expected_id = derive_candidate_id(
        &candidate.proposal_id,
        &candidate.content_hash,
        candidate.seed,
    );
    if candidate.candidate_id != expected_id {
        return Err(EvolutionAdmissionError::new(
            "evolution_candidate_id_mismatch",
            "candidate_id is not deterministically derived",
        ));
    }
    let expected_lineage = derive_lineage_id(
        candidate.parent_candidate_id.as_deref(),
        &candidate.proposal_id,
    );
    if candidate.lineage_id != expected_lineage {
        return Err(EvolutionAdmissionError::new(
            "evolution_lineage_id_mismatch",
            "lineage_id is not deterministically derived",
        ));
    }
    // Fail closed on forbidden fields that must never appear in durable evidence.
    let as_json = serde_json::to_value(candidate)
        .map_err(|e| EvolutionAdmissionError::new("evolution_candidate_encode", e.to_string()))?;
    refuse_sensitive_payload_fields(&as_json)?;
    Ok(())
}

fn refuse_sensitive_payload_fields(value: &Value) -> Result<(), EvolutionAdmissionError> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let lower = key.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "raw_prompt"
                        | "prompt_text"
                        | "model_output"
                        | "transcript"
                        | "repository_contents"
                        | "secret"
                        | "credential"
                        | "private_path"
                        | "api_key"
                        | "authorization"
                ) {
                    return Err(EvolutionAdmissionError::new(
                        "evolution_sensitive_payload",
                        format!("forbidden durable evidence field: {key}"),
                    ));
                }
                refuse_sensitive_payload_fields(child)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                refuse_sensitive_payload_fields(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn build_admission_receipt(
    candidate: &EvolutionCandidate,
    created_at: impl Into<String>,
) -> EvolutionReceipt {
    let created_at = created_at.into();
    EvolutionReceipt {
        schema_version: RECEIPT_SCHEMA_VERSION.to_string(),
        receipt_id: derive_receipt_id(&candidate.candidate_id, candidate.terminal_reason),
        candidate_id: candidate.candidate_id.clone(),
        proposal_id: candidate.proposal_id.clone(),
        lineage_id: candidate.lineage_id.clone(),
        active_version_id: candidate.active_version_id.clone(),
        terminal_reason: candidate.terminal_reason,
        content_hash: candidate.content_hash.clone(),
        created_at,
    }
}

pub fn sample_active_identity() -> ActiveHarnessIdentity {
    ActiveHarnessIdentity {
        schema_version: ACTIVE_VERSION_SCHEMA.to_string(),
        active_version_id: "active-harness-v0".to_string(),
        active_version_hash: sha256_hex("active-harness-fixture-body"),
        evaluator_identity_hash: sha256_hex("evaluator-fixture-identity"),
    }
}

pub fn proposal_from_body(
    active: &ActiveHarnessIdentity,
    parent_candidate_id: Option<String>,
    mutable_surfaces: &[&str],
    body: &Value,
    evidence_hashes: Vec<String>,
    seed: u64,
) -> Result<EvolutionProposal, EvolutionAdmissionError> {
    let proposal_body_sha256 = canonical_json_sha256(body)
        .map_err(|e| EvolutionAdmissionError::new("evolution_proposal_body", e))?;
    let proposal_id = derive_proposal_id(&active.active_version_id, &proposal_body_sha256, seed);
    let proposal = EvolutionProposal {
        schema_version: PROPOSAL_SCHEMA_VERSION.to_string(),
        proposal_id,
        parent_candidate_id,
        active_version_id: active.active_version_id.clone(),
        active_version_hash: active.active_version_hash.clone(),
        evaluator_identity_hash: active.evaluator_identity_hash.clone(),
        mutable_surface: MutableSurfaceDeclaration {
            schema_version: MUTABLE_SURFACE_SCHEMA.to_string(),
            surfaces: mutable_surfaces.iter().map(|s| (*s).to_string()).collect(),
        },
        proposal_body_sha256,
        evidence_hashes,
        seed,
    };
    validate_proposal(&proposal)?;
    Ok(proposal)
}

/// Build a candidate bound to an already-materialized app-owned workspace.
///
/// `content_hash` must equal the workspace surface hash (no independent caller authority).
pub fn candidate_from_proposal(
    proposal: &EvolutionProposal,
    workspace: &CandidateWorkspace,
    created_at: impl Into<String>,
) -> Result<EvolutionCandidate, EvolutionAdmissionError> {
    validate_sha256_hex(&workspace.content_hash)?;
    validate_workspace_relative_path(&workspace.relative_path)?;
    if workspace.schema_version != WORKSPACE_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_workspace_schema",
            "workspace schema_version mismatch",
        ));
    }
    let content_hash = workspace.content_hash.clone();
    let candidate_id = derive_candidate_id(&proposal.proposal_id, &content_hash, proposal.seed);
    let lineage_id = derive_lineage_id(
        proposal.parent_candidate_id.as_deref(),
        &proposal.proposal_id,
    );
    let candidate = EvolutionCandidate {
        schema_version: CANDIDATE_SCHEMA_VERSION.to_string(),
        candidate_id,
        lineage_id,
        parent_candidate_id: proposal.parent_candidate_id.clone(),
        proposal_id: proposal.proposal_id.clone(),
        active_version_id: proposal.active_version_id.clone(),
        active_version_hash: proposal.active_version_hash.clone(),
        evaluator_identity_hash: proposal.evaluator_identity_hash.clone(),
        mutable_surface: proposal.mutable_surface.clone(),
        workspace: workspace.clone(),
        content_hash,
        status: CandidateStatus::Proposed,
        terminal_reason: CandidateTerminalReason::Admitted,
        seed: proposal.seed,
        created_at: created_at.into(),
    };
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn digest(label: &str) -> String {
        sha256_hex(label)
    }

    fn seal_record_sha256(value: &mut Value) {
        let expected = record_digest_excluding_sha256(value).unwrap();
        value["record_sha256"] = Value::String(expected);
    }

    fn sample_ec3_lifecycle_budget_contract() -> Ec3LifecycleBudgetContractV1 {
        let source_semantics = vec![
            CostTrustSource::MeasuredDirect,
            CostTrustSource::DerivedDeterministic,
            CostTrustSource::Unavailable,
        ];
        let policy = |phase, required_dimensions| LifecyclePhasePolicy {
            phase,
            required_dimensions,
            source_semantics: source_semantics.clone(),
            incomplete_cost_disposition: IncompleteCostDisposition::CandidateIneligible,
        };
        let phase_policies = vec![
            policy(
                LifecycleCostPhase::Diagnosis,
                vec![
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::HumanEffortMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::HypothesisConstruction,
                vec![
                    LifecycleCostDimension::ModelTokens,
                    LifecycleCostDimension::ProviderCalls,
                    LifecycleCostDimension::ProviderCostMicrounits,
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::HumanEffortMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::Prediction,
                vec![
                    LifecycleCostDimension::ModelTokens,
                    LifecycleCostDimension::ProviderCalls,
                    LifecycleCostDimension::ProviderCostMicrounits,
                    LifecycleCostDimension::WallClockMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::CandidateMaterialization,
                vec![
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::ComputeMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::Evaluation,
                vec![
                    LifecycleCostDimension::ModelTokens,
                    LifecycleCostDimension::ProviderCalls,
                    LifecycleCostDimension::ProviderCostMicrounits,
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::ComputeMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::Review,
                vec![
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::HumanEffortMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::Repair,
                vec![
                    LifecycleCostDimension::ModelTokens,
                    LifecycleCostDimension::ProviderCalls,
                    LifecycleCostDimension::ProviderCostMicrounits,
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::ComputeMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::Ci,
                vec![
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::ComputeMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::Recovery,
                vec![
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::ComputeMilliseconds,
                    LifecycleCostDimension::HumanEffortMilliseconds,
                ],
            ),
            policy(
                LifecycleCostPhase::HumanEffort,
                vec![LifecycleCostDimension::HumanEffortMilliseconds],
            ),
            policy(
                LifecycleCostPhase::OutcomeReconciliation,
                vec![
                    LifecycleCostDimension::WallClockMilliseconds,
                    LifecycleCostDimension::ComputeMilliseconds,
                    LifecycleCostDimension::HumanEffortMilliseconds,
                ],
            ),
        ];
        let limits = |multiplier: u64| {
            REQUIRED_LIFECYCLE_COST_DIMENSIONS
                .iter()
                .enumerate()
                .map(|(index, dimension)| LifecycleResourceLimit {
                    dimension: *dimension,
                    limit: multiplier * (index as u64 + 1),
                })
                .collect()
        };
        seal_ec3_lifecycle_budget_contract(Ec3LifecycleBudgetContractV1 {
            schema_version: String::new(),
            contract_id: String::new(),
            phase_policies,
            candidate_envelope: CandidateLifecycleEnvelope {
                scope: EnvelopeScope::PerCandidate,
                resource_limits: limits(100),
                max_repair_attempts: 2,
                max_ci_runs: 2,
                max_recovery_attempts: 1,
            },
            global_envelope: GlobalLifecycleEnvelope {
                scope: EnvelopeScope::AggregateAcrossCandidates,
                resource_limits: limits(1_000),
                max_candidates: 10,
                max_failed_candidates: 5,
            },
            zero_cost_rule: ZeroCostRule::ExplicitEvidenceRequired,
            reservation_rule: ReservationRule::RequiredBeforeExecution,
            reconciliation_rule: ReconciliationRule::ExactOnceAfterTerminal,
            failure_accounting_rule: FailureAccountingRule::ChargeAllAttempts,
            envelope_exhaustion_rule: EnvelopeExhaustionRule::RejectReservationBeforeExecution,
            grants_spend_authority: false,
            record_sha256: String::new(),
        })
        .unwrap()
    }

    #[test]
    fn ec3_lifecycle_budget_seals_complete_fail_closed_contract() {
        let contract = sample_ec3_lifecycle_budget_contract();
        validate_ec3_lifecycle_budget_contract(&contract).unwrap();
        assert_eq!(contract.schema_version, EC3_LIFECYCLE_BUDGET_SCHEMA);
        assert!(contract.contract_id.starts_with("hebc-"));
        assert_eq!(
            contract.phase_policies.len(),
            REQUIRED_LIFECYCLE_COST_PHASES.len()
        );
        assert!(contract
            .phase_policies
            .iter()
            .any(|policy| policy.phase == LifecycleCostPhase::Prediction));
        assert!(contract.phase_policies.iter().all(|policy| policy
            .source_semantics
            .contains(&CostTrustSource::Unavailable)));
    }

    #[test]
    fn ec3_lifecycle_budget_rejects_missing_or_duplicate_phase() {
        let mut missing = sample_ec3_lifecycle_budget_contract();
        missing.phase_policies.pop();
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(missing)
                .unwrap_err()
                .code,
            "ec3_phase_missing"
        );

        let mut duplicate = sample_ec3_lifecycle_budget_contract();
        duplicate.phase_policies[1].phase = duplicate.phase_policies[0].phase;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(duplicate)
                .unwrap_err()
                .code,
            "ec3_phase_duplicate"
        );
    }

    #[test]
    fn ec3_lifecycle_budget_rejects_untrusted_or_missing_cost() {
        let mut untrusted = sample_ec3_lifecycle_budget_contract();
        untrusted.phase_policies[0].source_semantics = vec![CostTrustSource::CallerEstimate];
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(untrusted)
                .unwrap_err()
                .code,
            "ec3_source_untrusted"
        );

        let mut missing = sample_ec3_lifecycle_budget_contract();
        missing.phase_policies[0].incomplete_cost_disposition =
            IncompleteCostDisposition::TreatAsZero;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(missing)
                .unwrap_err()
                .code,
            "ec3_missing_cost_must_fail_closed"
        );

        let mut implicit_zero = sample_ec3_lifecycle_budget_contract();
        implicit_zero.zero_cost_rule = ZeroCostRule::ImplicitZeroAllowed;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(implicit_zero)
                .unwrap_err()
                .code,
            "ec3_implicit_zero_forbidden"
        );
    }

    #[test]
    fn ec3_lifecycle_budget_rejects_weak_accounting_or_authority() {
        let mut best_effort = sample_ec3_lifecycle_budget_contract();
        best_effort.reservation_rule = ReservationRule::BestEffort;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(best_effort)
                .unwrap_err()
                .code,
            "ec3_reservation_required"
        );

        let mut estimated = sample_ec3_lifecycle_budget_contract();
        estimated.reconciliation_rule = ReconciliationRule::EstimateAllowed;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(estimated)
                .unwrap_err()
                .code,
            "ec3_exact_reconciliation_required"
        );

        let mut successful_only = sample_ec3_lifecycle_budget_contract();
        successful_only.failure_accounting_rule = FailureAccountingRule::ChargeSuccessfulOnly;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(successful_only)
                .unwrap_err()
                .code,
            "ec3_failure_accounting_required"
        );

        let mut authority = sample_ec3_lifecycle_budget_contract();
        authority.grants_spend_authority = true;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(authority)
                .unwrap_err()
                .code,
            "ec3_spend_authority_forbidden"
        );
    }

    #[test]
    fn ec3_lifecycle_budget_rejects_incomplete_or_incoherent_envelopes() {
        let mut missing = sample_ec3_lifecycle_budget_contract();
        missing.candidate_envelope.resource_limits.pop();
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(missing)
                .unwrap_err()
                .code,
            "ec3_resource_limit_missing"
        );

        let mut duplicate = sample_ec3_lifecycle_budget_contract();
        duplicate.candidate_envelope.resource_limits[1].dimension =
            duplicate.candidate_envelope.resource_limits[0].dimension;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(duplicate)
                .unwrap_err()
                .code,
            "ec3_resource_limit_duplicate"
        );

        let mut too_small = sample_ec3_lifecycle_budget_contract();
        too_small.global_envelope.resource_limits[0].limit = 1;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(too_small)
                .unwrap_err()
                .code,
            "ec3_global_limit_smaller"
        );

        let mut failed_overflow = sample_ec3_lifecycle_budget_contract();
        failed_overflow.global_envelope.max_failed_candidates = 11;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(failed_overflow)
                .unwrap_err()
                .code,
            "ec3_failed_candidates_overflow"
        );

        let mut wrong_scope = sample_ec3_lifecycle_budget_contract();
        wrong_scope.global_envelope.scope = EnvelopeScope::PerCandidate;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(wrong_scope)
                .unwrap_err()
                .code,
            "ec3_envelope_scope_invalid"
        );

        let mut late_overflow = sample_ec3_lifecycle_budget_contract();
        late_overflow.envelope_exhaustion_rule =
            EnvelopeExhaustionRule::AllowAndEstimateAfterExecution;
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(late_overflow)
                .unwrap_err()
                .code,
            "ec3_envelope_overflow_forbidden"
        );
    }

    #[test]
    fn ec3_lifecycle_budget_rejects_contract_id_and_record_tamper() {
        let mut identity_tamper = sample_ec3_lifecycle_budget_contract();
        identity_tamper.contract_id = "hebc-tampered".to_string();
        assert_eq!(
            validate_ec3_lifecycle_budget_contract(&identity_tamper)
                .unwrap_err()
                .code,
            "ec3_contract_id_mismatch"
        );

        let mut record_tamper = sample_ec3_lifecycle_budget_contract();
        record_tamper.record_sha256 = sha256_hex("tampered");
        assert_eq!(
            validate_ec3_lifecycle_budget_contract(&record_tamper)
                .unwrap_err()
                .code,
            "ec3_record_tamper"
        );
    }

    fn sample_failure_pattern(causal: CausalStatus) -> FailurePatternEvidenceV1 {
        let source = digest("source");
        let observation = digest("obs");
        let lineage_id = "heil-fixture-root".to_string();
        seal_failure_pattern_evidence(FailurePatternEvidenceV1 {
            schema_version: FAILURE_PATTERN_EVIDENCE_SCHEMA.to_string(),
            evidence_id: String::new(),
            lineage_id,
            evidence_role: EvidenceRole::Observation,
            source_identity_hash: source,
            parent_identity_hash: digest("root"),
            generator_class: EC1_GENERATOR_CLASS.to_string(),
            lineage_schema_version: LINEAGE_SCHEMA_VERSION.to_string(),
            invalidation_class: EC1_INVALIDATION_CLASS.to_string(),
            budget_class: EC1_BUDGET_CLASS.to_string(),
            observation_digest: observation,
            causal_status: causal,
            counterevidence_digest: digest("counter"),
            addressable_surface: "prompts_and_bounded_rules".to_string(),
            mutable_surface: "prompts_and_bounded_rules".to_string(),
            record_sha256: String::new(),
        })
        .unwrap()
    }

    #[test]
    fn ec1_contract_freezes_default_off_harness_and_registry() {
        assert_eq!(
            frozen_ec1_active_harness_sha(),
            crate::context_working_set::CWS_ACTIVE_HARNESS_DEFAULT_OFF_SHA
        );
        let registry = registered_mutation_families();
        validate_mutation_family_registry(&registry).unwrap();
        assert_eq!(registry.families.len(), ADMITTED_MUTABLE_SURFACES.len());
        assert!(registry.families.iter().all(|family| family.non_authority));
    }

    #[test]
    fn ec1_identity_lineage_binds_default_off_sha_and_rejects_orphans() {
        let sealed = seal_ec1_identity_lineage(Ec1IdentityLineageRecord {
            schema_version: EC1_IDENTITY_LINEAGE_SCHEMA.to_string(),
            lineage_id: String::new(),
            parent_lineage_id: None,
            source_identity_hash: digest("source-root"),
            active_harness_sha: EC1_FROZEN_ACTIVE_HARNESS_SHA.to_string(),
            causal_source_id: None,
            record_sha256: String::new(),
        })
        .unwrap();
        validate_ec1_identity_lineage(&sealed).unwrap();
        let mut child = seal_ec1_identity_lineage(Ec1IdentityLineageRecord {
            schema_version: EC1_IDENTITY_LINEAGE_SCHEMA.to_string(),
            lineage_id: String::new(),
            parent_lineage_id: Some(sealed.lineage_id.clone()),
            source_identity_hash: digest("source-child"),
            active_harness_sha: EC1_FROZEN_ACTIVE_HARNESS_SHA.to_string(),
            causal_source_id: None,
            record_sha256: String::new(),
        })
        .unwrap();
        child.causal_source_id = Some(child.lineage_id.clone());
        child = seal_ec1_identity_lineage(child).unwrap();
        validate_ec1_identity_lineage(&child).unwrap();
        let mut orphan = child.clone();
        orphan.causal_source_id = Some("unknown-causal".into());
        assert_eq!(
            validate_ec1_identity_lineage(&orphan).unwrap_err().code,
            "ec1_orphan_causal_source"
        );
        let mut wrong_harness = sealed.clone();
        wrong_harness.active_harness_sha = "0".repeat(40);
        assert_eq!(
            validate_ec1_identity_lineage(&wrong_harness)
                .unwrap_err()
                .code,
            "ec1_active_harness_mismatch"
        );
        let mut rewritten = sealed;
        rewritten.parent_lineage_id = Some(child.lineage_id);
        assert_eq!(
            validate_ec1_identity_lineage(&rewritten).unwrap_err().code,
            "ec1_identity_asserted"
        );
    }

    #[test]
    fn causal_manifest_keeps_unknown_and_rejects_inference_as_proof() {
        let observed = sample_failure_pattern(CausalStatus::Unknown);
        assert_eq!(observed.evidence_role, EvidenceRole::Observation);
        assert!(!observed.causal_status.is_causal_proof());
        validate_failure_pattern_evidence(&sample_failure_pattern(CausalStatus::Disputed)).unwrap();
        let mut inferred = observed;
        inferred.evidence_role = EvidenceRole::Inference;
        inferred.causal_status = CausalStatus::Supported;
        assert_eq!(
            seal_failure_pattern_evidence(inferred).unwrap_err().code,
            "ec1_inference_as_proof"
        );
        let err = refuse_sensitive_payload_fields(&json!({"raw_prompt": "secret"}));
        assert_eq!(err.unwrap_err().code, "evolution_sensitive_payload");
    }

    #[test]
    fn mutation_registry_rejects_unknown_family_and_binds_seed() {
        let pattern = sample_failure_pattern(CausalStatus::Unknown);
        let hypothesis = seal_mutation_hypothesis_manifest(MutationHypothesisManifestV1 {
            schema_version: MUTATION_HYPOTHESIS_MANIFEST_SCHEMA.to_string(),
            manifest_id: String::new(),
            lineage_id: pattern.lineage_id.clone(),
            failure_evidence_id: pattern.evidence_id.clone(),
            proposal_body_sha256: digest("proposal-body"),
            candidate_delta_digest: digest("delta"),
            predicted_improvement_digest: digest("imp"),
            predicted_regression_digest: digest("reg"),
            invariant_digest: digest("inv"),
            evaluation_plan_digest: digest("plan"),
            record_sha256: String::new(),
        })
        .unwrap();
        let family = &registered_mutation_families().families[0].family_id;
        let a = generate_ec1_candidate_binding(family, &hypothesis, 7).unwrap();
        let b = generate_ec1_candidate_binding(family, &hypothesis, 7).unwrap();
        assert_eq!(a.binding_id, b.binding_id);
        assert_eq!(a.candidate_delta_digest, hypothesis.candidate_delta_digest);
        assert_eq!(a.lineage_id, hypothesis.lineage_id);
        assert_eq!(
            generate_ec1_candidate_binding("family:unknown", &hypothesis, 7)
                .unwrap_err()
                .code,
            "ec1_unknown_family"
        );
        assert_eq!(
            generate_ec1_candidate_binding("family:evaluator", &hypothesis, 7)
                .unwrap_err()
                .code,
            "ec1_unknown_family"
        );
        let other = generate_ec1_candidate_binding(family, &hypothesis, 8).unwrap();
        assert_ne!(a.binding_id, other.binding_id);
    }

    #[test]
    fn failure_pattern_keeps_unknown_and_rejects_empty_identity() {
        let ok = sample_failure_pattern(CausalStatus::Unknown);
        validate_failure_pattern_evidence(&ok).unwrap();
        let disputed = sample_failure_pattern(CausalStatus::Disputed);
        validate_failure_pattern_evidence(&disputed).unwrap();
        let mut asserted = ok.clone();
        asserted.evidence_id = "caller-id".to_string();
        assert_eq!(
            validate_failure_pattern_evidence(&asserted)
                .unwrap_err()
                .code,
            "ec1_identity_asserted"
        );
        let mut rewritten = ok.clone();
        rewritten.parent_identity_hash = digest("rewritten-parent");
        assert_eq!(
            validate_failure_pattern_evidence(&rewritten)
                .unwrap_err()
                .code,
            "ec1_record_tamper"
        );
        let mut forbidden = ok;
        forbidden.addressable_surface = "evaluator".to_string();
        assert_eq!(
            validate_failure_pattern_evidence(&forbidden)
                .unwrap_err()
                .code,
            "evolution_forbidden_surface"
        );
    }

    #[test]
    fn prediction_outcome_is_not_evaluator_authority() {
        let hyp = digest("hyp");
        let evaluation = digest("eval");
        let mut outcome_value = serde_json::to_value(PredictionOutcomeV1 {
            schema_version: PREDICTION_OUTCOME_SCHEMA.to_string(),
            outcome_id: derive_prediction_outcome_id(&hyp, &evaluation),
            hypothesis_manifest_digest: hyp,
            evaluation_digest: evaluation,
            evaluator_identity_hash: digest("evaluator"),
            counterevidence_digest: digest("counterevidence"),
            outcome: PredictionOutcomeKind::Unavailable,
            record_sha256: String::new(),
        })
        .unwrap();
        seal_record_sha256(&mut outcome_value);
        let outcome: PredictionOutcomeV1 = serde_json::from_value(outcome_value).unwrap();
        validate_prediction_outcome_contract(&outcome).unwrap();
        assert!(!PredictionOutcomeKind::Correct.is_evaluator_authority());
        let pattern = sample_failure_pattern(CausalStatus::Unknown);
        let delta = digest("delta");
        let manifest = seal_mutation_hypothesis_manifest(MutationHypothesisManifestV1 {
            schema_version: MUTATION_HYPOTHESIS_MANIFEST_SCHEMA.to_string(),
            manifest_id: String::new(),
            lineage_id: pattern.lineage_id.clone(),
            failure_evidence_id: pattern.evidence_id.clone(),
            proposal_body_sha256: digest("proposal-body"),
            candidate_delta_digest: delta,
            predicted_improvement_digest: digest("imp"),
            predicted_regression_digest: digest("reg"),
            invariant_digest: digest("inv"),
            evaluation_plan_digest: digest("plan"),
            record_sha256: String::new(),
        })
        .unwrap();
        validate_mutation_hypothesis_manifest(&manifest).unwrap();
        let mut unregistered = registered_mutation_families();
        unregistered.families.push(MutationFamilyRecord {
            family_id: "family:evaluator".to_string(),
            admitted_surface: "evaluator".to_string(),
            owner: "caller".to_string(),
            non_authority: false,
        });
        assert_eq!(
            validate_mutation_family_registry(&unregistered)
                .unwrap_err()
                .code,
            "evolution_forbidden_surface"
        );
    }

    #[test]
    fn derives_stable_identities() {
        let active = sample_active_identity();
        let body = json!({"kind":"prompt_tweak","digest":"abc"});
        let p1 = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &body,
            vec![sha256_hex("evidence-1")],
            7,
        )
        .unwrap();
        let p2 = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &body,
            vec![sha256_hex("evidence-1")],
            7,
        )
        .unwrap();
        assert_eq!(p1.proposal_id, p2.proposal_id);
        let ws = CandidateWorkspace {
            schema_version: WORKSPACE_SCHEMA_VERSION.to_string(),
            workspace_id: derive_workspace_id(&p1.proposal_id, p1.seed),
            relative_path: "candidates/c1".to_string(),
            content_hash: sha256_hex("content"),
        };
        let c = candidate_from_proposal(&p1, &ws, "2026-07-21T00:00:00Z").unwrap();
        assert!(c.candidate_id.starts_with("hevc-"));
        assert!(c.lineage_id.starts_with("heln-"));
        assert_eq!(c.content_hash, ws.content_hash);
    }

    #[test]
    fn materializes_and_revalidates_workspace_surface() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let files = vec![("manifest.json".to_string(), b"{\"k\":1}".to_vec())];
        let ws = materialize_candidate_workspace(root, "ws-unit-1", &files).unwrap();
        assert!(ws.relative_path.starts_with("candidates/"));
        revalidate_workspace_content(root, &ws).unwrap();
        // Tamper after materialization is detected.
        let path = resolve_workspace_under_root(root, &ws.relative_path).unwrap();
        std::fs::write(path.join("extra.txt"), b"tamper").unwrap();
        let err = revalidate_workspace_content(root, &ws).unwrap_err();
        assert_eq!(err.code, "evolution_workspace_tamper");
        discard_candidate_workspace(root, &ws).unwrap();
        assert!(!resolve_workspace_under_root(root, &ws.relative_path)
            .unwrap()
            .exists());
    }

    #[test]
    fn rejects_forbidden_mutable_surface() {
        let active = sample_active_identity();
        let err = proposal_from_body(&active, None, &["evaluator"], &json!({"x":1}), vec![], 1)
            .unwrap_err();
        assert_eq!(err.code, "evolution_forbidden_surface");
    }

    #[test]
    fn rejects_workspace_escape() {
        assert!(validate_workspace_relative_path("../etc").is_err());
        assert!(validate_workspace_relative_path("/abs").is_err());
        assert!(validate_workspace_relative_path("ok/path").is_ok());
    }

    struct UnitLabEnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_enable: Option<String>,
        prev_kill: Option<String>,
    }

    impl UnitLabEnvGuard {
        fn set(enable: bool, kill: bool) -> Self {
            let lock = EVOLUTION_LAB_TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let prev_enable = std::env::var(ENABLE_ENV).ok();
            let prev_kill = std::env::var(KILL_SWITCH_ENV).ok();
            if enable {
                std::env::set_var(ENABLE_ENV, "1");
            } else {
                std::env::remove_var(ENABLE_ENV);
            }
            if kill {
                std::env::set_var(KILL_SWITCH_ENV, "1");
            } else {
                std::env::remove_var(KILL_SWITCH_ENV);
            }
            Self {
                _lock: lock,
                prev_enable,
                prev_kill,
            }
        }
    }

    impl Drop for UnitLabEnvGuard {
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
    fn rejects_changed_active_version() {
        let _env = UnitLabEnvGuard::set(true, false);
        let active = sample_active_identity();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"k":"v"}),
            vec![],
            3,
        )
        .unwrap();
        let ws = CandidateWorkspace {
            schema_version: WORKSPACE_SCHEMA_VERSION.to_string(),
            workspace_id: "hews-c2".to_string(),
            relative_path: "candidates/c2".to_string(),
            content_hash: sha256_hex("content"),
        };
        let mut candidate =
            candidate_from_proposal(&proposal, &ws, "2026-07-21T00:00:00Z").unwrap();
        candidate.active_version_hash = sha256_hex("different");
        let err = validate_candidate_for_admission(&candidate, &active, true).unwrap_err();
        assert_eq!(err.code, "evolution_changed_active_version");
    }

    #[test]
    fn rejects_sensitive_payload_fields() {
        let _env = UnitLabEnvGuard::set(true, false);
        let poisoned = json!({"raw_prompt":"secret text","ok":true});
        let err = refuse_sensitive_payload_fields(&poisoned).unwrap_err();
        assert_eq!(err.code, "evolution_sensitive_payload");
    }

    #[test]
    fn kill_switch_fails_closed() {
        let _env = UnitLabEnvGuard::set(true, true);
        let active = sample_active_identity();
        let proposal = proposal_from_body(
            &active,
            None,
            &["prompts_and_bounded_rules"],
            &json!({"k":"v"}),
            vec![],
            4,
        )
        .unwrap();
        let ws = CandidateWorkspace {
            schema_version: WORKSPACE_SCHEMA_VERSION.to_string(),
            workspace_id: "hews-c4".to_string(),
            relative_path: "candidates/c4".to_string(),
            content_hash: sha256_hex("content"),
        };
        let candidate = candidate_from_proposal(&proposal, &ws, "2026-07-21T00:00:00Z").unwrap();
        let err = validate_candidate_for_admission(&candidate, &active, true).unwrap_err();
        assert_eq!(err.code, "evolution_kill_switch");
    }
}
