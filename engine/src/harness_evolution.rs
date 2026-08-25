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

use crate::product_golden_path::{
    ProductHarnessEvidenceState, ProductHarnessRunEvidence, ProductTaskStatus,
    PRODUCT_HARNESS_RUN_SEAM_SCHEMA_VERSION,
};

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
pub const EC3_LIFECYCLE_COST_OBSERVATION_SCHEMA: &str =
    "harness_evolution_ec3_lifecycle_cost_observation.v1";
pub const EC3_LIFECYCLE_COST_BUNDLE_SCHEMA: &str = "harness_evolution_ec3_lifecycle_cost_bundle.v1";
pub const EC3_LIFECYCLE_COST_READ_MODEL_SCHEMA: &str =
    "harness_evolution_ec3_lifecycle_cost_read_model.v1";
pub const MX1_DESCRIPTOR_SCHEMA_VERSION: &str = "harness_evolution_mx1_descriptor.v1";
pub const MX1_DESCRIPTOR_MANIFEST_SCHEMA_VERSION: &str =
    "harness_evolution_mx1_descriptor_manifest.v1";
pub const MX1_MATRIX_PLAN_SCHEMA_VERSION: &str = "harness_evolution_mx1_matrix_plan.v1";
pub const MX1_MATRIX_PROJECTION_SCHEMA_VERSION: &str = "harness_evolution_mx1_matrix_projection.v1";
pub const MX1_NORMALIZED_RUN_SCHEMA_VERSION: &str = "harness_evolution_mx1_run.v1";
pub const MX1_CONTRACT_ID: &str = "PE7-HE-MX1-CONTRACT-1";
pub const MX1_ARM_ZERO_HARNESS_ID: &str = "engine-managed@075f995b574fb8a28f08986291751152bf158dd5";
pub const MX1_ARM_ZERO_MODEL_ID: &str = "deepseek-v4-pro:single-model-three-role:v1";
pub const MX1_SECOND_HARNESS_ID: &str = "confined-subprocess-adapter:provider-free:v1";
pub const MX1_SECOND_MODEL_ID: &str = "deepseek-v4-flash:single-model-three-role:v1";
pub const MX1_NO_PROJECTION_STRATEGY_ID: &str =
    "single-pass-plan-implement-review:no-projection:v1";
pub const MX1_MEMORY_ONLY_STRATEGY_ID: &str = "single-pass-plan-implement-review:memory-only:v1";
pub const MX1_SKILL_ONLY_STRATEGY_ID: &str = "single-pass-plan-implement-review:skill-only:v1";
const MX1_DEEPSEEK_CHAT_COMPLETIONS_ENDPOINT: &str = "https://api.deepseek.com/chat/completions";
const MX1_DEEPSEEK_CREDENTIAL_REFERENCE: &str = "DEEPSEEK_API_KEY";

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

/// A C1 Harness candidate is visible to the matrix only through this bounded
/// admission disposition. Neither variant transfers execution, scheduler,
/// persistence, evaluator, budget, approval, output, audit, recovery, or
/// rollback authority away from the Rust engine and `LocalProductStore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mx1HarnessAdmissionDisposition {
    EmbedBehindSeam,
    ConfinedSubprocess,
    PolicyBlocked,
    CapabilityBlocked,
    Incomparable,
    Reject,
}

impl Mx1HarnessAdmissionDisposition {
    pub fn is_admitted(self) -> bool {
        matches!(self, Self::EmbedBehindSeam | Self::ConfinedSubprocess)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mx1ProjectionKind {
    Memory,
    Skill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mx1ProjectionSourceKind {
    GitBlob,
    ArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1ProjectionDescriptor {
    pub kind: Mx1ProjectionKind,
    pub source_kind: Mx1ProjectionSourceKind,
    pub source_handle: String,
    pub content_sha256: String,
    pub expires_at_unix_ms: u64,
    pub deletion_recipe_sha256: String,
    pub rebuild_recipe_sha256: String,
}

/// Complete immutable identity for one Harness factor. `source_identity` is
/// either a frozen Git commit or an immutable adapter-package digest; no display
/// name can stand in for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1HarnessImplementationDescriptor {
    pub schema_version: String,
    pub descriptor_id: String,
    pub source_owner: String,
    pub source_identity: String,
    pub version: String,
    pub build_identity_sha256: String,
    pub executable_identity_sha256: String,
    pub capability_probe_sha256: String,
    pub shared_run_seam_version: String,
    pub supported_task_capabilities: Vec<String>,
    pub supported_tool_capabilities: Vec<String>,
    pub process_confinement: String,
    pub workspace_confinement: String,
    pub terminal_outcome_mapping: String,
    pub verified_deliverable_mapping: String,
    pub usage_cost_mapping: String,
    pub cancellation_mapping: String,
    pub cleanup_mapping: String,
    pub restart_mapping: String,
    pub retry_mapping: String,
    pub failure_mapping: String,
    pub outcome_unknown_mapping: String,
    pub license_id: String,
    pub sbom_sha256: String,
    pub provenance_sha256: String,
    pub supported_model_ids: Vec<String>,
    pub supported_strategy_ids: Vec<String>,
    pub default_off: bool,
    pub rollback_binding_sha256: String,
    pub admission_disposition: Mx1HarnessAdmissionDisposition,
}

/// Complete immutable identity for one Model factor. Values here are admission
/// metadata only; this type contains no callable client, secret, or spend
/// authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1ModelPlanDescriptor {
    pub schema_version: String,
    pub descriptor_id: String,
    pub requested_model_id: String,
    pub resolved_model_id: String,
    pub provider: String,
    pub protocol: String,
    pub endpoint: String,
    pub endpoint_allowlist: Vec<String>,
    pub admitted_profile_sha256: String,
    pub credential_reference_name: String,
    pub role_order: Vec<String>,
    pub role_assignments: BTreeMap<String, String>,
    pub max_provider_requests: u32,
    pub max_input_tokens: u32,
    pub max_output_tokens: u32,
    pub max_total_tokens: u32,
    pub max_retries: u32,
    pub max_wall_time_ms: u64,
    pub tokenizer_identity: String,
    pub usage_mapping: String,
    pub pricing_currency: String,
    pub pricing_unit: String,
    pub pricing_source_sha256: String,
    pub pricing_effective_date: String,
    pub lifecycle_cost_mapping: String,
    pub supported_harness_ids: Vec<String>,
    pub supported_strategy_ids: Vec<String>,
    pub missing_identity_disposition: String,
    pub missing_usage_disposition: String,
}

/// Complete immutable identity for one Strategy factor. Projection contents are
/// represented by a source handle plus digest only; the descriptor cannot carry
/// prompts, transcripts, credentials, private paths, or durable authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1StrategyPlanDescriptor {
    pub schema_version: String,
    pub descriptor_id: String,
    pub strategy_kind: String,
    pub composition_order: Vec<String>,
    pub source_identity: String,
    pub source_identity_sha256: String,
    pub projection: Option<Mx1ProjectionDescriptor>,
    pub admitted_input_class: String,
    pub redaction_class: String,
    pub cross_task_isolation: bool,
    pub cross_arm_isolation: bool,
    pub leakage_scan_sha256: String,
    pub prompt_policy_sha256: String,
    pub tool_policy_sha256: String,
    pub retry_policy_sha256: String,
    pub compression_policy_sha256: String,
    pub no_authority: bool,
    pub supported_harness_ids: Vec<String>,
    pub supported_model_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1DescriptorManifest {
    pub schema_version: String,
    pub contract_id: String,
    pub harnesses: Vec<Mx1HarnessImplementationDescriptor>,
    pub models: Vec<Mx1ModelPlanDescriptor>,
    pub strategies: Vec<Mx1StrategyPlanDescriptor>,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1CellIdentity {
    pub harness_id: String,
    pub model_id: String,
    pub strategy_id: String,
    pub task_id: String,
}

/// A transient lease is the only projection material handled by CORE. It is
/// per-cell, contains no content, and can only be rebuilt from the immutable
/// descriptor source identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1ProjectionLease {
    pub schema_version: String,
    pub strategy_id: String,
    pub cell_binding_sha256: String,
    pub source_handle: String,
    pub content_sha256: String,
    pub expires_at_unix_ms: u64,
    pub deleted: bool,
}

#[derive(Debug, Clone)]
pub struct Mx1StrategyAdapter {
    descriptor: Mx1StrategyPlanDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1NormalizedHarnessRun {
    pub schema_version: String,
    pub matrix_plan_id: String,
    pub matrix_manifest_sha256: String,
    pub matrix_rung: Mx1MatrixRung,
    pub matrix_repetition: u32,
    pub common_basis_sha256: String,
    pub cell_id: String,
    pub cell_identity: Mx1CellIdentity,
    pub cell_descriptor_sha256: String,
    pub harness_id: String,
    pub harness_descriptor_sha256: String,
    pub product_task_id: String,
    pub workspace_id: String,
    pub workspace_binding_sha256: String,
    pub source_revision_sha256: String,
    pub terminal_outcome: ProductTaskStatus,
    pub verified_deliverable: ProductHarnessEvidenceState,
    pub usage: ProductHarnessEvidenceState,
    pub usage_evidence_sha256: Option<String>,
    pub cost: ProductHarnessEvidenceState,
    pub cost_evidence_sha256: Option<String>,
    pub cancellation: ProductHarnessEvidenceState,
    pub cleanup: ProductHarnessEvidenceState,
    pub restart: ProductHarnessEvidenceState,
    pub recovery: ProductHarnessEvidenceState,
    pub failure: ProductHarnessEvidenceState,
    pub failure_code: Option<String>,
    pub failure_detail_sha256: Option<String>,
    pub terminal_evidence_sha256: Option<String>,
}

/// A common, provider-free adapter boundary. The adapters normalize already
/// owned Golden Path evidence; they do not execute CLI commands or subprocesses.
pub trait Mx1HarnessRunAdapter {
    fn descriptor(&self) -> &Mx1HarnessImplementationDescriptor;
    fn normalize_run(
        &self,
        plan: &Mx1MatrixPlan,
        cell: &Mx1MatrixCell,
        evidence: &ProductHarnessRunEvidence,
    ) -> Result<Mx1NormalizedHarnessRun, EvolutionAdmissionError>;
}

#[derive(Debug, Clone)]
pub struct Mx1EngineManagedHarnessAdapter {
    descriptor: Mx1HarnessImplementationDescriptor,
}

#[derive(Debug, Clone)]
pub struct Mx1ConfinedSubprocessHarnessAdapter {
    descriptor: Mx1HarnessImplementationDescriptor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mx1MatrixRung {
    OneByTwoByOne,
    OneByTwoByThree,
    TwoByTwoByThree,
}

impl Mx1MatrixRung {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OneByTwoByOne => "1x2x1",
            Self::OneByTwoByThree => "1x2x3",
            Self::TwoByTwoByThree => "2x2x3",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "reason")]
pub enum Mx1MatrixCellDisposition {
    Admitted,
    Incomparable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1MatrixCell {
    pub cell_id: String,
    pub identity: Mx1CellIdentity,
    pub descriptor_digest: String,
    pub disposition: Mx1MatrixCellDisposition,
    pub order_key_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1MatrixPlan {
    pub schema_version: String,
    pub plan_id: String,
    pub manifest_sha256: String,
    pub common_basis_sha256: String,
    pub rung: Mx1MatrixRung,
    pub task_id: String,
    pub repetition: u32,
    pub cells: Vec<Mx1MatrixCell>,
}

/// An observation is bound to the full three-axis cell and task, rather than
/// merely to a Harness label. This prevents one arm's result from being reused
/// across a different Model or Strategy cell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1MatrixObservation {
    pub cell_id: String,
    pub normalized_run: Mx1NormalizedHarnessRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "reason")]
pub enum Mx1ReadOnlyCellDisposition {
    Observed,
    Incomparable(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1ReadOnlyCellProjection {
    pub cell_id: String,
    pub disposition: Mx1ReadOnlyCellDisposition,
    pub normalized_run: Option<Mx1NormalizedHarnessRun>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mx1MatrixReadOnlyProjection {
    pub schema_version: String,
    pub plan_id: String,
    pub manifest_sha256: String,
    pub cells: Vec<Mx1ReadOnlyCellProjection>,
}

fn mx1_error(code: &str, message: impl Into<String>) -> EvolutionAdmissionError {
    EvolutionAdmissionError::new(code, message.into())
}

fn mx1_require_id(field: &str, value: &str) -> Result<(), EvolutionAdmissionError> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '@'))
    {
        return Err(mx1_error(
            "mx1_descriptor_identity",
            format!("{field} is missing or malformed"),
        ));
    }
    Ok(())
}

fn mx1_require_text(field: &str, value: &str) -> Result<(), EvolutionAdmissionError> {
    if value.trim().is_empty() || value.len() > 512 || value.contains('\0') {
        return Err(mx1_error(
            "mx1_descriptor_field",
            format!("{field} is missing or malformed"),
        ));
    }
    Ok(())
}

fn mx1_require_sha(field: &str, value: &str) -> Result<(), EvolutionAdmissionError> {
    validate_sha256_hex(value).map_err(|_| {
        mx1_error(
            "mx1_descriptor_digest",
            format!("{field} must be a 64-hex digest"),
        )
    })
}

fn mx1_require_commit_or_digest(field: &str, value: &str) -> Result<(), EvolutionAdmissionError> {
    let is_commit = value.len() == 40 && value.chars().all(|ch| ch.is_ascii_hexdigit());
    if is_commit {
        Ok(())
    } else {
        mx1_require_sha(field, value)
    }
}

fn mx1_validate_sorted_ids(field: &str, values: &[String]) -> Result<(), EvolutionAdmissionError> {
    if values.is_empty() || values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(mx1_error(
            "mx1_descriptor_support",
            format!("{field} must be nonempty, sorted, and unique"),
        ));
    }
    for value in values {
        mx1_require_id(field, value)?;
    }
    Ok(())
}

fn mx1_descriptor_digest<T: Serialize>(value: &T) -> Result<String, EvolutionAdmissionError> {
    let value = serde_json::to_value(value)
        .map_err(|error| mx1_error("mx1_descriptor_encode", error.to_string()))?;
    canonical_json_sha256(&value).map_err(|error| mx1_error("mx1_descriptor_encode", error))
}

fn mx1_expected_harness_evidence_digest(
    evidence_kind: &str,
    descriptor_id: &str,
    source_identity: &str,
) -> String {
    sha256_hex(&format!(
        "{MX1_CONTRACT_ID}/harness-evidence:v1|{evidence_kind}|{descriptor_id}|{source_identity}"
    ))
}

/// H1 is an in-tree, provider-free adapter package. Its immutable package
/// identity is the compiled source unit itself, rather than a display label or
/// a caller-supplied digest. CORE never spawns it; the package identity still
/// gives a later confined execution preflight an exact source boundary to bind.
fn mx1_confined_adapter_package_sha256() -> String {
    hex::encode(Sha256::digest(include_bytes!("harness_evolution.rs")))
}

fn mx1_validate_frozen_harness_identity(
    descriptor: &Mx1HarnessImplementationDescriptor,
) -> Result<(), EvolutionAdmissionError> {
    let (expected_owner, expected_source, expected_version, expected_disposition, expected_process) =
        match descriptor.descriptor_id.as_str() {
            MX1_ARM_ZERO_HARNESS_ID => (
                "rust-engine-product-golden-path",
                "075f995b574fb8a28f08986291751152bf158dd5".to_string(),
                "v1",
                Mx1HarnessAdmissionDisposition::EmbedBehindSeam,
                "engine-owned-no-exec-in-core",
            ),
            MX1_SECOND_HARNESS_ID => (
                "rust-engine-harness-evolution",
                mx1_confined_adapter_package_sha256(),
                "provider-free-adapter-v1",
                Mx1HarnessAdmissionDisposition::ConfinedSubprocess,
                "product-owned-confined-subprocess;core-no-spawn",
            ),
            _ => {
                return Err(mx1_error(
                    "mx1_harness_identity",
                    "Harness descriptor is not one of the two frozen MX1 implementations",
                ));
            }
        };
    if descriptor.source_owner != expected_owner
        || descriptor.source_identity != expected_source
        || descriptor.version != expected_version
        || descriptor.admission_disposition != expected_disposition
        || descriptor.process_confinement != expected_process
        || descriptor.workspace_confinement != "product-workspace-binding-digest"
        || descriptor.terminal_outcome_mapping != "product-task-status"
        || descriptor.verified_deliverable_mapping != "product-terminal-evidence"
        || descriptor.usage_cost_mapping != "existing-product-usage-and-cost-owners"
        || descriptor.cancellation_mapping != "product-task-killed-state"
        || descriptor.cleanup_mapping != "product-terminal-cleanup-evidence"
        || descriptor.restart_mapping != "product-terminal-restart-evidence"
        || descriptor.retry_mapping != "model-plan-max-retries"
        || descriptor.failure_mapping != "product-task-failure-code-digest"
        || descriptor.outcome_unknown_mapping != "product-task-outcome-unknown"
        || descriptor.license_id != "Apache-2.0"
    {
        return Err(mx1_error(
            "mx1_harness_identity",
            "Harness descriptor drifted from the frozen source, confinement, or mapping contract",
        ));
    }
    for (field, observed) in [
        ("build", &descriptor.build_identity_sha256),
        ("executable", &descriptor.executable_identity_sha256),
        ("capability-probe", &descriptor.capability_probe_sha256),
        ("sbom", &descriptor.sbom_sha256),
        ("provenance", &descriptor.provenance_sha256),
        ("rollback", &descriptor.rollback_binding_sha256),
    ] {
        if observed
            != &mx1_expected_harness_evidence_digest(
                field,
                &descriptor.descriptor_id,
                &descriptor.source_identity,
            )
        {
            return Err(mx1_error(
                "mx1_harness_evidence_drift",
                "Harness build, executable, probe, SBOM, provenance, or rollback evidence drifted",
            ));
        }
    }
    Ok(())
}

fn mx1_confined_adapter_capability_probe(
    descriptor: &Mx1HarnessImplementationDescriptor,
) -> Result<(), EvolutionAdmissionError> {
    mx1_validate_frozen_harness_identity(descriptor)?;
    if descriptor.descriptor_id != MX1_SECOND_HARNESS_ID {
        return Err(mx1_error(
            "mx1_second_harness_probe",
            "confined adapter probe can only attest the frozen H1 package",
        ));
    }
    Ok(())
}

fn mx1_expected_model_evidence_digest(evidence_kind: &str, descriptor_id: &str) -> String {
    sha256_hex(&format!(
        "{MX1_CONTRACT_ID}/model-evidence:v1|{evidence_kind}|{descriptor_id}"
    ))
}

fn mx1_validate_frozen_model_identity(
    descriptor: &Mx1ModelPlanDescriptor,
) -> Result<(), EvolutionAdmissionError> {
    let expected_model = match descriptor.descriptor_id.as_str() {
        MX1_ARM_ZERO_MODEL_ID => "deepseek-v4-pro",
        MX1_SECOND_MODEL_ID => "deepseek-v4-flash",
        _ => {
            return Err(mx1_error(
                "mx1_model_identity",
                "Model descriptor is not one of the two frozen MX1 plans",
            ));
        }
    };
    if descriptor.requested_model_id != expected_model
        || descriptor.resolved_model_id != expected_model
        || descriptor.provider != "deepseek"
        || descriptor.protocol != "openai_compatible"
        || descriptor.endpoint != MX1_DEEPSEEK_CHAT_COMPLETIONS_ENDPOINT
        || descriptor.endpoint_allowlist != vec![MX1_DEEPSEEK_CHAT_COMPLETIONS_ENDPOINT.to_string()]
        || descriptor.credential_reference_name != MX1_DEEPSEEK_CREDENTIAL_REFERENCE
        || descriptor.tokenizer_identity != "deepseek-tokenizer-current"
        || descriptor.usage_mapping != "existing-execution-usage-owner"
        || descriptor.pricing_currency != "USD"
        || descriptor.pricing_unit != "per-token"
        || descriptor.pricing_effective_date != "2026-08-24"
        || descriptor.lifecycle_cost_mapping != "existing-product-usage-and-cost-owners"
        || descriptor.missing_identity_disposition != "incomparable"
        || descriptor.missing_usage_disposition != "incomparable"
        || descriptor.admitted_profile_sha256
            != mx1_expected_model_evidence_digest("admitted-profile", &descriptor.descriptor_id)
        || descriptor.pricing_source_sha256
            != mx1_expected_model_evidence_digest("pricing-source", &descriptor.descriptor_id)
    {
        return Err(mx1_error(
            "mx1_model_identity",
            "Model descriptor drifted from the frozen endpoint, credential, or evidence identity",
        ));
    }
    Ok(())
}

fn mx1_expected_strategy_evidence_digest(evidence_kind: &str, descriptor_id: &str) -> String {
    sha256_hex(&format!(
        "{MX1_CONTRACT_ID}/strategy-evidence:v1|{evidence_kind}|{descriptor_id}"
    ))
}

fn mx1_validate_frozen_strategy_identity(
    descriptor: &Mx1StrategyPlanDescriptor,
) -> Result<(), EvolutionAdmissionError> {
    if !matches!(
        descriptor.descriptor_id.as_str(),
        MX1_NO_PROJECTION_STRATEGY_ID | MX1_MEMORY_ONLY_STRATEGY_ID | MX1_SKILL_ONLY_STRATEGY_ID
    ) || descriptor.strategy_kind != "single-pass-plan-implement-review"
        || descriptor.composition_order
            != vec![
                "plan".to_string(),
                "implement".to_string(),
                "review".to_string(),
            ]
        || descriptor.source_identity != mx1_confined_adapter_package_sha256()
        || descriptor.source_identity_sha256 != sha256_hex(&descriptor.source_identity)
        || descriptor.admitted_input_class != "redacted-digest-reference"
        || descriptor.redaction_class != "digest-only"
    {
        return Err(mx1_error(
            "mx1_strategy_identity",
            "Strategy descriptor drifted from the frozen MX1 strategy contract",
        ));
    }
    for (kind, observed) in [
        ("leakage", &descriptor.leakage_scan_sha256),
        ("prompt-policy", &descriptor.prompt_policy_sha256),
        ("tool-policy", &descriptor.tool_policy_sha256),
        ("retry-policy", &descriptor.retry_policy_sha256),
        ("compression-policy", &descriptor.compression_policy_sha256),
    ] {
        if observed != &mx1_expected_strategy_evidence_digest(kind, &descriptor.descriptor_id) {
            return Err(mx1_error(
                "mx1_strategy_evidence_drift",
                "Strategy leakage or policy evidence drifted",
            ));
        }
    }
    let expected_projection = match descriptor.descriptor_id.as_str() {
        MX1_NO_PROJECTION_STRATEGY_ID => None,
        MX1_MEMORY_ONLY_STRATEGY_ID => Some((
            Mx1ProjectionKind::Memory,
            Mx1ProjectionSourceKind::ArtifactRef,
            "artifact:mx1-memory-projection-v1",
        )),
        MX1_SKILL_ONLY_STRATEGY_ID => Some((
            Mx1ProjectionKind::Skill,
            Mx1ProjectionSourceKind::GitBlob,
            "git:mx1-skill-projection-v1",
        )),
        _ => unreachable!("the frozen strategy identity check above is exhaustive"),
    };
    match (expected_projection, &descriptor.projection) {
        (None, None) => {}
        (Some((kind, source_kind, source_handle)), Some(projection))
            if projection.kind == kind
                && projection.source_kind == source_kind
                && projection.source_handle == source_handle
                && projection.expires_at_unix_ms == 4_102_444_800_000
                && projection.content_sha256
                    == mx1_expected_strategy_evidence_digest(
                        "projection-content",
                        &descriptor.descriptor_id,
                    )
                && projection.deletion_recipe_sha256
                    == mx1_expected_strategy_evidence_digest(
                        "projection-delete",
                        &descriptor.descriptor_id,
                    )
                && projection.rebuild_recipe_sha256
                    == mx1_expected_strategy_evidence_digest(
                        "projection-rebuild",
                        &descriptor.descriptor_id,
                    ) => {}
        _ => {
            return Err(mx1_error(
                "mx1_strategy_projection_drift",
                "Strategy projection identity, expiry, or lifecycle recipe drifted",
            ));
        }
    }
    Ok(())
}

fn mx1_validate_harness_descriptor(
    descriptor: &Mx1HarnessImplementationDescriptor,
) -> Result<(), EvolutionAdmissionError> {
    if descriptor.schema_version != MX1_DESCRIPTOR_SCHEMA_VERSION {
        return Err(mx1_error(
            "mx1_harness_schema",
            "unsupported Harness descriptor schema",
        ));
    }
    mx1_require_id("harness descriptor_id", &descriptor.descriptor_id)?;
    mx1_require_text("harness source_owner", &descriptor.source_owner)?;
    mx1_require_commit_or_digest("harness source_identity", &descriptor.source_identity)?;
    mx1_require_id("harness version", &descriptor.version)?;
    mx1_require_sha(
        "harness build_identity_sha256",
        &descriptor.build_identity_sha256,
    )?;
    mx1_require_sha(
        "harness executable_identity_sha256",
        &descriptor.executable_identity_sha256,
    )?;
    mx1_require_sha(
        "harness capability_probe_sha256",
        &descriptor.capability_probe_sha256,
    )?;
    if descriptor.shared_run_seam_version != PRODUCT_HARNESS_RUN_SEAM_SCHEMA_VERSION {
        return Err(mx1_error(
            "mx1_harness_seam",
            "Harness descriptor does not bind the Product Golden Path run seam",
        ));
    }
    mx1_validate_sorted_ids(
        "harness supported_task_capabilities",
        &descriptor.supported_task_capabilities,
    )?;
    mx1_validate_sorted_ids(
        "harness supported_tool_capabilities",
        &descriptor.supported_tool_capabilities,
    )?;
    for (field, value) in [
        (
            "process_confinement",
            descriptor.process_confinement.as_str(),
        ),
        (
            "workspace_confinement",
            descriptor.workspace_confinement.as_str(),
        ),
        (
            "terminal_outcome_mapping",
            descriptor.terminal_outcome_mapping.as_str(),
        ),
        (
            "verified_deliverable_mapping",
            descriptor.verified_deliverable_mapping.as_str(),
        ),
        ("usage_cost_mapping", descriptor.usage_cost_mapping.as_str()),
        (
            "cancellation_mapping",
            descriptor.cancellation_mapping.as_str(),
        ),
        ("cleanup_mapping", descriptor.cleanup_mapping.as_str()),
        ("restart_mapping", descriptor.restart_mapping.as_str()),
        ("retry_mapping", descriptor.retry_mapping.as_str()),
        ("failure_mapping", descriptor.failure_mapping.as_str()),
        (
            "outcome_unknown_mapping",
            descriptor.outcome_unknown_mapping.as_str(),
        ),
        ("license_id", descriptor.license_id.as_str()),
    ] {
        mx1_require_text(field, value)?;
    }
    mx1_require_sha("harness sbom_sha256", &descriptor.sbom_sha256)?;
    mx1_require_sha("harness provenance_sha256", &descriptor.provenance_sha256)?;
    mx1_validate_sorted_ids(
        "harness supported_model_ids",
        &descriptor.supported_model_ids,
    )?;
    mx1_validate_sorted_ids(
        "harness supported_strategy_ids",
        &descriptor.supported_strategy_ids,
    )?;
    mx1_require_sha(
        "harness rollback_binding_sha256",
        &descriptor.rollback_binding_sha256,
    )?;
    if !descriptor.default_off || !descriptor.admission_disposition.is_admitted() {
        return Err(mx1_error(
            "mx1_harness_admission",
            "Harness must be default-off and admitted behind the common seam",
        ));
    }
    mx1_validate_frozen_harness_identity(descriptor)?;
    Ok(())
}

fn mx1_validate_model_descriptor(
    descriptor: &Mx1ModelPlanDescriptor,
) -> Result<(), EvolutionAdmissionError> {
    if descriptor.schema_version != MX1_DESCRIPTOR_SCHEMA_VERSION {
        return Err(mx1_error(
            "mx1_model_schema",
            "unsupported Model descriptor schema",
        ));
    }
    for (field, value) in [
        ("model descriptor_id", descriptor.descriptor_id.as_str()),
        ("requested_model_id", descriptor.requested_model_id.as_str()),
        ("resolved_model_id", descriptor.resolved_model_id.as_str()),
        ("provider", descriptor.provider.as_str()),
        ("protocol", descriptor.protocol.as_str()),
        (
            "credential_reference_name",
            descriptor.credential_reference_name.as_str(),
        ),
        ("pricing_currency", descriptor.pricing_currency.as_str()),
        ("pricing_unit", descriptor.pricing_unit.as_str()),
        (
            "lifecycle_cost_mapping",
            descriptor.lifecycle_cost_mapping.as_str(),
        ),
        (
            "missing_identity_disposition",
            descriptor.missing_identity_disposition.as_str(),
        ),
        (
            "missing_usage_disposition",
            descriptor.missing_usage_disposition.as_str(),
        ),
    ] {
        mx1_require_id(field, value)?;
    }
    if descriptor.provider != "deepseek"
        || descriptor.protocol != "openai_compatible"
        || !descriptor.endpoint.starts_with("https://")
        || descriptor.endpoint_allowlist != vec![descriptor.endpoint.clone()]
    {
        return Err(mx1_error(
            "mx1_model_endpoint",
            "Model endpoint/protocol is incomplete or not allowlisted",
        ));
    }
    if !descriptor
        .credential_reference_name
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err(mx1_error(
            "mx1_model_credential_reference",
            "Model descriptor may carry only a symbolic credential reference",
        ));
    }
    mx1_require_sha(
        "model admitted_profile_sha256",
        &descriptor.admitted_profile_sha256,
    )?;
    mx1_require_sha(
        "model pricing_source_sha256",
        &descriptor.pricing_source_sha256,
    )?;
    if descriptor.max_provider_requests != 3
        || descriptor.max_input_tokens != 12_000
        || descriptor.max_output_tokens != 8_192
        || descriptor.max_total_tokens != 20_192
        || descriptor.max_retries != 0
        || descriptor.max_wall_time_ms != 900_000
    {
        return Err(mx1_error(
            "mx1_model_budget_drift",
            "Model descriptor must retain the frozen per-cell request/token limits",
        ));
    }
    let expected_role_order = ["planner", "implementer", "reviewer"];
    if descriptor
        .role_order
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != expected_role_order
    {
        return Err(mx1_error(
            "mx1_model_roles",
            "Model role order must retain the frozen plan/implement/review topology",
        ));
    }
    for role in expected_role_order {
        if descriptor.role_assignments.get(role) != Some(&descriptor.resolved_model_id) {
            return Err(mx1_error(
                "mx1_model_roles",
                "Model role assignments must bind every role to the resolved identity",
            ));
        }
    }
    if descriptor.role_assignments.len() != 3 {
        return Err(mx1_error(
            "mx1_model_roles",
            "Model descriptor may not add hidden role routing",
        ));
    }
    for (field, value) in [
        ("tokenizer_identity", descriptor.tokenizer_identity.as_str()),
        ("usage_mapping", descriptor.usage_mapping.as_str()),
        (
            "pricing_effective_date",
            descriptor.pricing_effective_date.as_str(),
        ),
    ] {
        mx1_require_id(field, value)?;
    }
    mx1_validate_sorted_ids(
        "model supported_harness_ids",
        &descriptor.supported_harness_ids,
    )?;
    mx1_validate_sorted_ids(
        "model supported_strategy_ids",
        &descriptor.supported_strategy_ids,
    )?;
    mx1_validate_frozen_model_identity(descriptor)?;
    Ok(())
}

fn mx1_validate_strategy_descriptor(
    descriptor: &Mx1StrategyPlanDescriptor,
) -> Result<(), EvolutionAdmissionError> {
    if descriptor.schema_version != MX1_DESCRIPTOR_SCHEMA_VERSION {
        return Err(mx1_error(
            "mx1_strategy_schema",
            "unsupported Strategy descriptor schema",
        ));
    }
    mx1_require_id("strategy descriptor_id", &descriptor.descriptor_id)?;
    mx1_require_id("strategy_kind", &descriptor.strategy_kind)?;
    if descriptor.composition_order.is_empty()
        || descriptor
            .composition_order
            .iter()
            .any(|value| value.trim().is_empty())
    {
        return Err(mx1_error(
            "mx1_strategy_composition",
            "Strategy composition must be explicit",
        ));
    }
    mx1_require_id("strategy source_identity", &descriptor.source_identity)?;
    mx1_require_sha(
        "strategy source_identity_sha256",
        &descriptor.source_identity_sha256,
    )?;
    if descriptor.source_identity_sha256 != sha256_hex(&descriptor.source_identity) {
        return Err(mx1_error(
            "mx1_strategy_source_drift",
            "Strategy source identity does not match its frozen digest",
        ));
    }
    for (field, value) in [
        (
            "admitted_input_class",
            descriptor.admitted_input_class.as_str(),
        ),
        ("redaction_class", descriptor.redaction_class.as_str()),
    ] {
        mx1_require_id(field, value)?;
    }
    if descriptor.redaction_class != "digest-only"
        || !descriptor.cross_task_isolation
        || !descriptor.cross_arm_isolation
        || !descriptor.no_authority
    {
        return Err(mx1_error(
            "mx1_strategy_isolation",
            "Strategy must be digest-only, isolated, and authority-free",
        ));
    }
    mx1_require_sha(
        "strategy leakage_scan_sha256",
        &descriptor.leakage_scan_sha256,
    )?;
    for (field, value) in [
        (
            "strategy prompt_policy_sha256",
            &descriptor.prompt_policy_sha256,
        ),
        (
            "strategy tool_policy_sha256",
            &descriptor.tool_policy_sha256,
        ),
        (
            "strategy retry_policy_sha256",
            &descriptor.retry_policy_sha256,
        ),
        (
            "strategy compression_policy_sha256",
            &descriptor.compression_policy_sha256,
        ),
    ] {
        mx1_require_sha(field, value)?;
    }
    mx1_validate_sorted_ids(
        "strategy supported_harness_ids",
        &descriptor.supported_harness_ids,
    )?;
    mx1_validate_sorted_ids(
        "strategy supported_model_ids",
        &descriptor.supported_model_ids,
    )?;
    match (&descriptor.projection, descriptor.descriptor_id.as_str()) {
        (None, MX1_NO_PROJECTION_STRATEGY_ID) => {}
        (Some(projection), strategy_id) if strategy_id.contains(":memory-only:") => {
            if projection.kind != Mx1ProjectionKind::Memory {
                return Err(mx1_error(
                    "mx1_strategy_projection",
                    "memory-only Strategy requires a MEMORY projection",
                ));
            }
        }
        (Some(projection), strategy_id) if strategy_id.contains(":skill-only:") => {
            if projection.kind != Mx1ProjectionKind::Skill {
                return Err(mx1_error(
                    "mx1_strategy_projection",
                    "skill-only Strategy requires a SKILL projection",
                ));
            }
        }
        _ => {
            return Err(mx1_error(
                "mx1_strategy_projection",
                "Strategy projection and descriptor identity are inconsistent",
            ));
        }
    }
    if let Some(projection) = &descriptor.projection {
        if projection.expires_at_unix_ms == 0
            || projection.source_handle.len() > 512
            || projection.source_handle.starts_with('/')
            || projection.source_handle.contains("..")
            || projection.source_handle.trim().is_empty()
        {
            return Err(mx1_error(
                "mx1_strategy_projection",
                "Strategy projection source/expiry is malformed",
            ));
        }
        match projection.source_kind {
            Mx1ProjectionSourceKind::GitBlob if !projection.source_handle.starts_with("git:") => {
                return Err(mx1_error(
                    "mx1_strategy_projection",
                    "GIT_BLOB projection source must use a git: handle",
                ));
            }
            Mx1ProjectionSourceKind::ArtifactRef
                if !projection.source_handle.starts_with("artifact:") =>
            {
                return Err(mx1_error(
                    "mx1_strategy_projection",
                    "ARTIFACT_REF projection source must use an artifact: handle",
                ));
            }
            _ => {}
        }
        mx1_require_sha(
            "strategy projection content_sha256",
            &projection.content_sha256,
        )?;
        mx1_require_sha(
            "strategy projection deletion_recipe_sha256",
            &projection.deletion_recipe_sha256,
        )?;
        mx1_require_sha(
            "strategy projection rebuild_recipe_sha256",
            &projection.rebuild_recipe_sha256,
        )?;
    }
    mx1_validate_frozen_strategy_identity(descriptor)?;
    Ok(())
}

fn mx1_manifest_without_digest(
    manifest: &Mx1DescriptorManifest,
) -> Result<Value, EvolutionAdmissionError> {
    let mut value = serde_json::to_value(manifest)
        .map_err(|error| mx1_error("mx1_manifest_encode", error.to_string()))?;
    value["manifest_sha256"] = Value::Null;
    Ok(value)
}

pub fn derive_mx1_manifest_sha256(
    manifest: &Mx1DescriptorManifest,
) -> Result<String, EvolutionAdmissionError> {
    canonical_json_sha256(&mx1_manifest_without_digest(manifest)?)
        .map_err(|error| mx1_error("mx1_manifest_encode", error))
}

fn mx1_validate_manifest_contents(
    manifest: &Mx1DescriptorManifest,
) -> Result<(), EvolutionAdmissionError> {
    if manifest.schema_version != MX1_DESCRIPTOR_MANIFEST_SCHEMA_VERSION
        || manifest.contract_id != MX1_CONTRACT_ID
    {
        return Err(mx1_error(
            "mx1_manifest_schema",
            "manifest must bind the frozen MX1 contract",
        ));
    }
    if manifest.harnesses.len() != 2 || manifest.models.len() != 2 || manifest.strategies.len() != 3
    {
        return Err(mx1_error(
            "mx1_manifest_cardinality",
            "MX1 manifest must contain exactly 2 Harnesses, 2 Models, and 3 Strategies",
        ));
    }
    for (field, ids) in [
        (
            "Harness",
            manifest
                .harnesses
                .iter()
                .map(|item| item.descriptor_id.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "Model",
            manifest
                .models
                .iter()
                .map(|item| item.descriptor_id.as_str())
                .collect::<Vec<_>>(),
        ),
        (
            "Strategy",
            manifest
                .strategies
                .iter()
                .map(|item| item.descriptor_id.as_str())
                .collect::<Vec<_>>(),
        ),
    ] {
        if ids.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(mx1_error(
                "mx1_manifest_order",
                format!("{field} descriptors must have canonical unique ordering"),
            ));
        }
    }
    for descriptor in &manifest.harnesses {
        mx1_validate_harness_descriptor(descriptor)?;
    }
    for descriptor in &manifest.models {
        mx1_validate_model_descriptor(descriptor)?;
    }
    for descriptor in &manifest.strategies {
        mx1_validate_strategy_descriptor(descriptor)?;
    }
    let unique = |values: Vec<&str>, field: &str| -> Result<(), EvolutionAdmissionError> {
        let set = values.iter().copied().collect::<BTreeSet<_>>();
        if set.len() != values.len() {
            return Err(mx1_error(
                "mx1_manifest_identity",
                format!("duplicate {field} descriptor id"),
            ));
        }
        Ok(())
    };
    unique(
        manifest
            .harnesses
            .iter()
            .map(|item| item.descriptor_id.as_str())
            .collect(),
        "Harness",
    )?;
    unique(
        manifest
            .models
            .iter()
            .map(|item| item.descriptor_id.as_str())
            .collect(),
        "Model",
    )?;
    unique(
        manifest
            .strategies
            .iter()
            .map(|item| item.descriptor_id.as_str())
            .collect(),
        "Strategy",
    )?;
    if manifest
        .harnesses
        .iter()
        .filter(|item| item.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
        .count()
        != 1
        || manifest
            .models
            .iter()
            .filter(|item| item.descriptor_id == MX1_ARM_ZERO_MODEL_ID)
            .count()
            != 1
        || manifest
            .strategies
            .iter()
            .filter(|item| item.descriptor_id == MX1_NO_PROJECTION_STRATEGY_ID)
            .count()
            != 1
    {
        return Err(mx1_error(
            "mx1_manifest_arm_zero",
            "manifest is missing a frozen arm-zero descriptor",
        ));
    }
    let arm_zero_harness = manifest
        .harnesses
        .iter()
        .find(|item| item.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
        .expect("arm zero presence was checked above");
    if arm_zero_harness.admission_disposition != Mx1HarnessAdmissionDisposition::EmbedBehindSeam
        || arm_zero_harness.source_identity != "075f995b574fb8a28f08986291751152bf158dd5"
    {
        return Err(mx1_error(
            "mx1_manifest_arm_zero",
            "arm-zero Harness disposition or exact frozen source identity drifted",
        ));
    }
    if manifest
        .harnesses
        .iter()
        .filter(|item| {
            item.admission_disposition == Mx1HarnessAdmissionDisposition::ConfinedSubprocess
        })
        .count()
        != 1
    {
        return Err(mx1_error(
            "mx1_manifest_second_harness",
            "manifest must admit exactly one confined second Harness",
        ));
    }
    if manifest.harnesses.iter().any(|item| {
        item.descriptor_id != MX1_ARM_ZERO_HARNESS_ID
            && item.admission_disposition != Mx1HarnessAdmissionDisposition::ConfinedSubprocess
    }) {
        return Err(mx1_error(
            "mx1_manifest_second_harness",
            "the nonzero Harness must be the admitted confined implementation",
        ));
    }
    Ok(())
}

pub fn seal_mx1_descriptor_manifest(
    mut manifest: Mx1DescriptorManifest,
) -> Result<Mx1DescriptorManifest, EvolutionAdmissionError> {
    manifest
        .harnesses
        .sort_by(|left, right| left.descriptor_id.cmp(&right.descriptor_id));
    manifest
        .models
        .sort_by(|left, right| left.descriptor_id.cmp(&right.descriptor_id));
    manifest
        .strategies
        .sort_by(|left, right| left.descriptor_id.cmp(&right.descriptor_id));
    mx1_validate_manifest_contents(&manifest)?;
    manifest.manifest_sha256 = derive_mx1_manifest_sha256(&manifest)?;
    Ok(manifest)
}

pub fn validate_mx1_descriptor_manifest(
    manifest: &Mx1DescriptorManifest,
) -> Result<(), EvolutionAdmissionError> {
    mx1_validate_manifest_contents(manifest)?;
    let expected = derive_mx1_manifest_sha256(manifest)?;
    if manifest.manifest_sha256 != expected {
        return Err(mx1_error(
            "mx1_manifest_drift",
            "descriptor manifest digest does not match immutable descriptors",
        ));
    }
    Ok(())
}

fn mx1_projection_cell_binding(identity: &Mx1CellIdentity, strategy_id: &str) -> String {
    sha256_hex(&format!(
        "{MX1_CONTRACT_ID}/projection-binding:v1|{}|{}|{}|{}|{strategy_id}",
        identity.harness_id, identity.model_id, identity.strategy_id, identity.task_id
    ))
}

impl Mx1StrategyAdapter {
    pub fn new(descriptor: Mx1StrategyPlanDescriptor) -> Result<Self, EvolutionAdmissionError> {
        mx1_validate_strategy_descriptor(&descriptor)?;
        Ok(Self { descriptor })
    }

    pub fn descriptor(&self) -> &Mx1StrategyPlanDescriptor {
        &self.descriptor
    }

    pub fn prepare_projection(
        &self,
        identity: &Mx1CellIdentity,
        now_unix_ms: u64,
    ) -> Result<Option<Mx1ProjectionLease>, EvolutionAdmissionError> {
        let Some(projection) = &self.descriptor.projection else {
            return Ok(None);
        };
        if now_unix_ms >= projection.expires_at_unix_ms {
            return Err(mx1_error(
                "mx1_projection_expired",
                "projection source expired before cell preparation",
            ));
        }
        if identity.strategy_id != self.descriptor.descriptor_id {
            return Err(mx1_error(
                "mx1_projection_strategy_binding",
                "projection adapter cannot cross a Strategy arm",
            ));
        }
        Ok(Some(Mx1ProjectionLease {
            schema_version: MX1_DESCRIPTOR_SCHEMA_VERSION.to_string(),
            strategy_id: self.descriptor.descriptor_id.clone(),
            cell_binding_sha256: mx1_projection_cell_binding(
                identity,
                &self.descriptor.descriptor_id,
            ),
            source_handle: projection.source_handle.clone(),
            content_sha256: projection.content_sha256.clone(),
            expires_at_unix_ms: projection.expires_at_unix_ms,
            deleted: false,
        }))
    }

    pub fn rebuild_projection(
        &self,
        identity: &Mx1CellIdentity,
        source_content_sha256: &str,
        now_unix_ms: u64,
    ) -> Result<Mx1ProjectionLease, EvolutionAdmissionError> {
        let projection = self
            .prepare_projection(identity, now_unix_ms)?
            .ok_or_else(|| {
                mx1_error(
                    "mx1_projection_absent",
                    "baseline Strategy has no projection",
                )
            })?;
        if projection.content_sha256 != source_content_sha256 {
            return Err(mx1_error(
                "mx1_projection_source_drift",
                "projection rebuild source digest drifted",
            ));
        }
        Ok(projection)
    }

    pub fn validate_projection(
        &self,
        lease: &Mx1ProjectionLease,
        identity: &Mx1CellIdentity,
        now_unix_ms: u64,
    ) -> Result<(), EvolutionAdmissionError> {
        let projection = self.descriptor.projection.as_ref().ok_or_else(|| {
            mx1_error(
                "mx1_projection_absent",
                "baseline Strategy has no projection",
            )
        })?;
        if lease.schema_version != MX1_DESCRIPTOR_SCHEMA_VERSION
            || lease.strategy_id != self.descriptor.descriptor_id
            || lease.cell_binding_sha256
                != mx1_projection_cell_binding(identity, &self.descriptor.descriptor_id)
            || lease.source_handle != projection.source_handle
            || lease.content_sha256 != projection.content_sha256
        {
            return Err(mx1_error(
                "mx1_projection_cross_arm_or_source_drift",
                "projection lease binding does not match this exact cell",
            ));
        }
        if lease.deleted {
            return Err(mx1_error(
                "mx1_projection_deleted",
                "deleted projection cannot be reused",
            ));
        }
        if now_unix_ms >= lease.expires_at_unix_ms {
            return Err(mx1_error(
                "mx1_projection_expired",
                "projection lease expired",
            ));
        }
        Ok(())
    }

    pub fn delete_projection(
        &self,
        lease: &mut Mx1ProjectionLease,
    ) -> Result<(), EvolutionAdmissionError> {
        if lease.strategy_id != self.descriptor.descriptor_id {
            return Err(mx1_error(
                "mx1_projection_strategy_binding",
                "projection deletion cannot cross a Strategy arm",
            ));
        }
        lease.deleted = true;
        Ok(())
    }
}

fn mx1_normalize_run(
    descriptor: &Mx1HarnessImplementationDescriptor,
    plan: &Mx1MatrixPlan,
    cell: &Mx1MatrixCell,
    evidence: &ProductHarnessRunEvidence,
) -> Result<Mx1NormalizedHarnessRun, EvolutionAdmissionError> {
    mx1_validate_harness_descriptor(descriptor)?;
    if plan.schema_version != MX1_MATRIX_PLAN_SCHEMA_VERSION
        || plan.plan_id.len() != 64
        || !plan
            .plan_id
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        || plan.repetition == 0
        || !plan.cells.iter().any(|candidate| candidate == cell)
    {
        return Err(mx1_error(
            "mx1_run_plan_binding",
            "Harness adapter requires an exact, nonempty planned matrix cell",
        ));
    }
    mx1_require_sha("matrix manifest digest", &plan.manifest_sha256)?;
    mx1_require_sha("matrix common basis digest", &plan.common_basis_sha256)?;
    if cell.identity.harness_id != descriptor.descriptor_id {
        return Err(mx1_error(
            "mx1_run_cell_binding",
            "Harness adapter cannot normalize evidence for another Harness cell",
        ));
    }
    mx1_require_sha("matrix cell descriptor digest", &cell.descriptor_digest)?;
    if evidence.schema_version != PRODUCT_HARNESS_RUN_SEAM_SCHEMA_VERSION
        || evidence.product_task_id.trim().is_empty()
        || evidence.workspace_id.trim().is_empty()
    {
        return Err(mx1_error(
            "mx1_run_seam",
            "Harness run evidence is not a complete Product Golden Path projection",
        ));
    }
    mx1_require_sha(
        "product workspace_binding_sha256",
        &evidence.workspace_binding_sha256,
    )?;
    mx1_require_sha(
        "product source_revision_sha256",
        &evidence.source_revision_sha256,
    )?;
    for value in [
        evidence.usage.evidence_sha256.as_deref(),
        evidence.cost.evidence_sha256.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        mx1_require_sha("product usage-or-cost evidence", value)?;
    }
    for value in [
        evidence.failure_detail_sha256.as_deref(),
        evidence.terminal_evidence_sha256.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        mx1_require_sha("product terminal evidence", value)?;
    }
    Ok(Mx1NormalizedHarnessRun {
        schema_version: MX1_NORMALIZED_RUN_SCHEMA_VERSION.to_string(),
        matrix_plan_id: plan.plan_id.clone(),
        matrix_manifest_sha256: plan.manifest_sha256.clone(),
        matrix_rung: plan.rung,
        matrix_repetition: plan.repetition,
        common_basis_sha256: plan.common_basis_sha256.clone(),
        cell_id: cell.cell_id.clone(),
        cell_identity: cell.identity.clone(),
        cell_descriptor_sha256: cell.descriptor_digest.clone(),
        harness_id: descriptor.descriptor_id.clone(),
        harness_descriptor_sha256: mx1_descriptor_digest(descriptor)?,
        product_task_id: evidence.product_task_id.clone(),
        workspace_id: evidence.workspace_id.clone(),
        workspace_binding_sha256: evidence.workspace_binding_sha256.clone(),
        source_revision_sha256: evidence.source_revision_sha256.clone(),
        terminal_outcome: evidence.terminal_outcome,
        verified_deliverable: evidence.verified_deliverable,
        usage: evidence.usage.state,
        usage_evidence_sha256: evidence.usage.evidence_sha256.clone(),
        cost: evidence.cost.state,
        cost_evidence_sha256: evidence.cost.evidence_sha256.clone(),
        cancellation: evidence.cancellation,
        cleanup: evidence.cleanup,
        restart: evidence.restart,
        recovery: evidence.recovery,
        failure: evidence.failure,
        failure_code: evidence.failure_code.clone(),
        failure_detail_sha256: evidence.failure_detail_sha256.clone(),
        terminal_evidence_sha256: evidence.terminal_evidence_sha256.clone(),
    })
}

impl Mx1EngineManagedHarnessAdapter {
    pub fn new(
        descriptor: Mx1HarnessImplementationDescriptor,
    ) -> Result<Self, EvolutionAdmissionError> {
        mx1_validate_harness_descriptor(&descriptor)?;
        if descriptor.descriptor_id != MX1_ARM_ZERO_HARNESS_ID
            || descriptor.admission_disposition != Mx1HarnessAdmissionDisposition::EmbedBehindSeam
        {
            return Err(mx1_error(
                "mx1_arm_zero_harness",
                "engine-managed adapter must bind the frozen arm-zero descriptor",
            ));
        }
        Ok(Self { descriptor })
    }
}

impl Mx1HarnessRunAdapter for Mx1EngineManagedHarnessAdapter {
    fn descriptor(&self) -> &Mx1HarnessImplementationDescriptor {
        &self.descriptor
    }

    fn normalize_run(
        &self,
        plan: &Mx1MatrixPlan,
        cell: &Mx1MatrixCell,
        evidence: &ProductHarnessRunEvidence,
    ) -> Result<Mx1NormalizedHarnessRun, EvolutionAdmissionError> {
        mx1_normalize_run(&self.descriptor, plan, cell, evidence)
    }
}

impl Mx1ConfinedSubprocessHarnessAdapter {
    pub fn new(
        descriptor: Mx1HarnessImplementationDescriptor,
    ) -> Result<Self, EvolutionAdmissionError> {
        mx1_validate_harness_descriptor(&descriptor)?;
        if descriptor.descriptor_id == MX1_ARM_ZERO_HARNESS_ID
            || descriptor.admission_disposition
                != Mx1HarnessAdmissionDisposition::ConfinedSubprocess
        {
            return Err(mx1_error(
                "mx1_second_harness",
                "second adapter must be the one confined Harness behind the seam",
            ));
        }
        mx1_confined_adapter_capability_probe(&descriptor)?;
        Ok(Self { descriptor })
    }
}

impl Mx1HarnessRunAdapter for Mx1ConfinedSubprocessHarnessAdapter {
    fn descriptor(&self) -> &Mx1HarnessImplementationDescriptor {
        &self.descriptor
    }

    fn normalize_run(
        &self,
        plan: &Mx1MatrixPlan,
        cell: &Mx1MatrixCell,
        evidence: &ProductHarnessRunEvidence,
    ) -> Result<Mx1NormalizedHarnessRun, EvolutionAdmissionError> {
        // CORE deliberately performs no subprocess execution. A later effect
        // packet may use this admitted identity only through existing owners.
        mx1_normalize_run(&self.descriptor, plan, cell, evidence)
    }
}

fn mx1_length_prefixed_key(fields: &[String]) -> String {
    let mut encoded = String::new();
    for field in fields {
        encoded.push_str(&format!("{}:{field}", field.len()));
    }
    sha256_hex(&encoded)
}

fn mx1_cell_descriptor_digest(
    harness: &Mx1HarnessImplementationDescriptor,
    model: &Mx1ModelPlanDescriptor,
    strategy: &Mx1StrategyPlanDescriptor,
) -> Result<String, EvolutionAdmissionError> {
    mx1_descriptor_digest(&serde_json::json!({
        "harness_sha256": mx1_descriptor_digest(harness)?,
        "model_sha256": mx1_descriptor_digest(model)?,
        "strategy_sha256": mx1_descriptor_digest(strategy)?,
    }))
}

fn mx1_cell_supported(
    harness: &Mx1HarnessImplementationDescriptor,
    model: &Mx1ModelPlanDescriptor,
    strategy: &Mx1StrategyPlanDescriptor,
) -> bool {
    harness
        .supported_model_ids
        .iter()
        .any(|value| value == &model.descriptor_id)
        && harness
            .supported_strategy_ids
            .iter()
            .any(|value| value == &strategy.descriptor_id)
        && model
            .supported_harness_ids
            .iter()
            .any(|value| value == &harness.descriptor_id)
        && model
            .supported_strategy_ids
            .iter()
            .any(|value| value == &strategy.descriptor_id)
        && strategy
            .supported_harness_ids
            .iter()
            .any(|value| value == &harness.descriptor_id)
        && strategy
            .supported_model_ids
            .iter()
            .any(|value| value == &model.descriptor_id)
}

pub fn build_mx1_matrix_plan(
    manifest: &Mx1DescriptorManifest,
    rung: Mx1MatrixRung,
    task_id: &str,
    repetition: u32,
    common_basis_sha256: &str,
) -> Result<Mx1MatrixPlan, EvolutionAdmissionError> {
    validate_mx1_descriptor_manifest(manifest)?;
    mx1_require_id("matrix task_id", task_id)?;
    mx1_require_sha("matrix common_basis_sha256", common_basis_sha256)?;
    if repetition == 0 {
        return Err(mx1_error(
            "mx1_matrix_repetition",
            "matrix repetition must be one-based",
        ));
    }
    let h0 = manifest
        .harnesses
        .iter()
        .find(|item| item.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
        .expect("manifest validation requires arm zero");
    let h1 = manifest
        .harnesses
        .iter()
        .find(|item| item.descriptor_id != MX1_ARM_ZERO_HARNESS_ID)
        .expect("manifest validation requires exactly two Harnesses");
    let s0 = manifest
        .strategies
        .iter()
        .find(|item| item.descriptor_id == MX1_NO_PROJECTION_STRATEGY_ID)
        .expect("manifest validation requires arm zero");
    let selected_harnesses: Vec<&Mx1HarnessImplementationDescriptor> = match rung {
        Mx1MatrixRung::OneByTwoByOne | Mx1MatrixRung::OneByTwoByThree => vec![h0],
        Mx1MatrixRung::TwoByTwoByThree => vec![h0, h1],
    };
    let selected_strategies: Vec<&Mx1StrategyPlanDescriptor> = match rung {
        Mx1MatrixRung::OneByTwoByOne => vec![s0],
        Mx1MatrixRung::OneByTwoByThree | Mx1MatrixRung::TwoByTwoByThree => {
            manifest.strategies.iter().collect()
        }
    };
    let mut cells = Vec::new();
    for harness in selected_harnesses {
        for model in &manifest.models {
            for strategy in &selected_strategies {
                let identity = Mx1CellIdentity {
                    harness_id: harness.descriptor_id.clone(),
                    model_id: model.descriptor_id.clone(),
                    strategy_id: strategy.descriptor_id.clone(),
                    task_id: task_id.to_string(),
                };
                let descriptor_digest = mx1_cell_descriptor_digest(harness, model, strategy)?;
                let order_key_sha256 = mx1_length_prefixed_key(&[
                    format!("{MX1_CONTRACT_ID}/randomization:v1"),
                    rung.as_str().to_string(),
                    task_id.to_string(),
                    repetition.to_string(),
                    descriptor_digest.clone(),
                ]);
                let cell_id = format!(
                    "{}:{}:{}:{}:r{}",
                    harness.descriptor_id,
                    model.descriptor_id,
                    strategy.descriptor_id,
                    task_id,
                    repetition
                );
                let disposition = if mx1_cell_supported(harness, model, strategy) {
                    Mx1MatrixCellDisposition::Admitted
                } else {
                    Mx1MatrixCellDisposition::Incomparable(
                        "unsupported_cross_product_cell".to_string(),
                    )
                };
                cells.push(Mx1MatrixCell {
                    cell_id,
                    identity,
                    descriptor_digest,
                    disposition,
                    order_key_sha256,
                });
            }
        }
    }
    cells.sort_by(|left, right| {
        left.order_key_sha256
            .cmp(&right.order_key_sha256)
            .then_with(|| left.cell_id.cmp(&right.cell_id))
    });
    let plan_seed = format!(
        "{}|{}|{}|{}|{}",
        manifest.manifest_sha256,
        common_basis_sha256,
        rung.as_str(),
        task_id,
        repetition
    );
    Ok(Mx1MatrixPlan {
        schema_version: MX1_MATRIX_PLAN_SCHEMA_VERSION.to_string(),
        plan_id: sha256_hex(&plan_seed),
        manifest_sha256: manifest.manifest_sha256.clone(),
        common_basis_sha256: common_basis_sha256.to_string(),
        rung,
        task_id: task_id.to_string(),
        repetition,
        cells,
    })
}

/// Refuse a caller-supplied matrix plan unless it is the exact deterministic
/// plan derived from the sealed manifest and fixed experiment inputs. This
/// preserves the contract's randomization/order evidence at the read boundary
/// too: a projection cannot silently accept a reordered, expanded, or
/// cross-manifest cell list.
pub fn validate_mx1_matrix_plan(
    manifest: &Mx1DescriptorManifest,
    plan: &Mx1MatrixPlan,
) -> Result<(), EvolutionAdmissionError> {
    validate_mx1_descriptor_manifest(manifest)?;
    if plan.schema_version != MX1_MATRIX_PLAN_SCHEMA_VERSION {
        return Err(mx1_error(
            "mx1_matrix_schema",
            "unsupported matrix plan schema",
        ));
    }
    if plan.manifest_sha256 != manifest.manifest_sha256 {
        return Err(mx1_error(
            "mx1_matrix_manifest",
            "matrix plan is bound to another descriptor manifest",
        ));
    }
    let expected = build_mx1_matrix_plan(
        manifest,
        plan.rung,
        &plan.task_id,
        plan.repetition,
        &plan.common_basis_sha256,
    )?;
    if &expected != plan {
        return Err(mx1_error(
            "mx1_matrix_plan_drift",
            "matrix plan is not the deterministic sealed plan",
        ));
    }
    Ok(())
}

/// Build the provider-free read model for a planned block. Missing evidence,
/// `OutcomeUnknown`, descriptor mismatch, and unsupported cells stay explicit
/// `INCOMPARABLE`; this function never infers an outcome or retries an effect.
pub fn project_mx1_matrix_read_only(
    manifest: &Mx1DescriptorManifest,
    plan: &Mx1MatrixPlan,
    observations: &[Mx1MatrixObservation],
) -> Result<Mx1MatrixReadOnlyProjection, EvolutionAdmissionError> {
    validate_mx1_matrix_plan(manifest, plan)?;
    let harness_descriptor_digests = manifest
        .harnesses
        .iter()
        .map(|descriptor| {
            Ok((
                descriptor.descriptor_id.as_str(),
                mx1_descriptor_digest(descriptor)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, EvolutionAdmissionError>>()?;
    let mut observations_by_cell = BTreeMap::new();
    for observation in observations {
        if observation.normalized_run.schema_version != MX1_NORMALIZED_RUN_SCHEMA_VERSION {
            return Err(mx1_error(
                "mx1_matrix_observation",
                "normalized run schema is unsupported",
            ));
        }
        if observations_by_cell
            .insert(observation.cell_id.clone(), observation)
            .is_some()
        {
            return Err(mx1_error(
                "mx1_matrix_observation",
                "duplicate normalized evidence for one matrix cell",
            ));
        }
    }
    let mut cells = Vec::with_capacity(plan.cells.len());
    for cell in &plan.cells {
        let (disposition, normalized_run) = match &cell.disposition {
            Mx1MatrixCellDisposition::Incomparable(reason) => (
                Mx1ReadOnlyCellDisposition::Incomparable(reason.clone()),
                None,
            ),
            Mx1MatrixCellDisposition::Admitted => match observations_by_cell.get(&cell.cell_id) {
                None => (
                    Mx1ReadOnlyCellDisposition::Incomparable("evidence_missing".to_string()),
                    None,
                ),
                Some(observation)
                    if observation.normalized_run.matrix_plan_id != plan.plan_id
                        || observation.normalized_run.matrix_manifest_sha256
                            != plan.manifest_sha256
                        || observation.normalized_run.matrix_rung != plan.rung
                        || observation.normalized_run.matrix_repetition != plan.repetition
                        || observation.normalized_run.common_basis_sha256
                            != plan.common_basis_sha256 =>
                {
                    (
                        Mx1ReadOnlyCellDisposition::Incomparable(
                            "matrix_plan_identity_mismatch".to_string(),
                        ),
                        Some(observation.normalized_run.clone()),
                    )
                }
                Some(observation) if observation.normalized_run.cell_id != cell.cell_id => (
                    Mx1ReadOnlyCellDisposition::Incomparable("matrix_cell_id_mismatch".to_string()),
                    Some(observation.normalized_run.clone()),
                ),
                Some(observation) if observation.normalized_run.cell_identity != cell.identity => (
                    Mx1ReadOnlyCellDisposition::Incomparable("cell_identity_mismatch".to_string()),
                    Some(observation.normalized_run.clone()),
                ),
                Some(observation)
                    if observation.normalized_run.cell_descriptor_sha256
                        != cell.descriptor_digest =>
                {
                    (
                        Mx1ReadOnlyCellDisposition::Incomparable(
                            "descriptor_digest_mismatch".to_string(),
                        ),
                        Some(observation.normalized_run.clone()),
                    )
                }
                Some(observation)
                    if harness_descriptor_digests.get(cell.identity.harness_id.as_str())
                        != Some(&observation.normalized_run.harness_descriptor_sha256) =>
                {
                    (
                        Mx1ReadOnlyCellDisposition::Incomparable(
                            "harness_descriptor_digest_mismatch".to_string(),
                        ),
                        Some(observation.normalized_run.clone()),
                    )
                }
                Some(observation)
                    if observation.normalized_run.product_task_id != cell.identity.task_id =>
                {
                    (
                        Mx1ReadOnlyCellDisposition::Incomparable(
                            "product_task_identity_mismatch".to_string(),
                        ),
                        Some(observation.normalized_run.clone()),
                    )
                }
                Some(observation)
                    if observation.normalized_run.harness_id != cell.identity.harness_id =>
                {
                    (
                        Mx1ReadOnlyCellDisposition::Incomparable(
                            "harness_identity_mismatch".to_string(),
                        ),
                        Some(observation.normalized_run.clone()),
                    )
                }
                Some(observation)
                    if observation.normalized_run.terminal_outcome
                        == ProductTaskStatus::OutcomeUnknown =>
                {
                    (
                        Mx1ReadOnlyCellDisposition::Incomparable("outcome_unknown".to_string()),
                        Some(observation.normalized_run.clone()),
                    )
                }
                Some(observation)
                    if observation.normalized_run.terminal_outcome
                        == ProductTaskStatus::Completed
                        && (observation
                            .normalized_run
                            .terminal_evidence_sha256
                            .is_none()
                            || observation.normalized_run.verified_deliverable
                                != ProductHarnessEvidenceState::Observed) =>
                {
                    (
                        Mx1ReadOnlyCellDisposition::Incomparable(
                            "verified_delivery_evidence_missing".to_string(),
                        ),
                        Some(observation.normalized_run.clone()),
                    )
                }
                Some(observation) => (
                    Mx1ReadOnlyCellDisposition::Observed,
                    Some(observation.normalized_run.clone()),
                ),
            },
        };
        cells.push(Mx1ReadOnlyCellProjection {
            cell_id: cell.cell_id.clone(),
            disposition,
            normalized_run,
        });
    }
    Ok(Mx1MatrixReadOnlyProjection {
        schema_version: MX1_MATRIX_PROJECTION_SCHEMA_VERSION.to_string(),
        plan_id: plan.plan_id.clone(),
        manifest_sha256: plan.manifest_sha256.clone(),
        cells,
    })
}

/// The provider-free CORE manifest. All nonzero factors are fully identified
/// here, but the manifest is default-off and contains no authority to execute a
/// Provider call, create a workspace, allocate budget, or promote a Harness.
pub fn sample_mx1_descriptor_manifest() -> Mx1DescriptorManifest {
    let harness_ids = vec![
        MX1_ARM_ZERO_HARNESS_ID.to_string(),
        MX1_SECOND_HARNESS_ID.to_string(),
    ];
    let model_ids = vec![
        MX1_SECOND_MODEL_ID.to_string(),
        MX1_ARM_ZERO_MODEL_ID.to_string(),
    ];
    let strategy_ids = vec![
        MX1_NO_PROJECTION_STRATEGY_ID.to_string(),
        MX1_MEMORY_ONLY_STRATEGY_ID.to_string(),
        MX1_SKILL_ONLY_STRATEGY_ID.to_string(),
    ];
    let mut sorted_harness_ids = harness_ids.clone();
    let mut sorted_model_ids = model_ids.clone();
    let mut sorted_strategy_ids = strategy_ids.clone();
    sorted_harness_ids.sort();
    sorted_model_ids.sort();
    sorted_strategy_ids.sort();
    let roles = |model: &str| {
        BTreeMap::from([
            ("planner".to_string(), model.to_string()),
            ("implementer".to_string(), model.to_string()),
            ("reviewer".to_string(), model.to_string()),
        ])
    };
    let harness = |descriptor_id: &str,
                   source_owner: &str,
                   source_identity: String,
                   admission_disposition: Mx1HarnessAdmissionDisposition| {
        Mx1HarnessImplementationDescriptor {
            schema_version: MX1_DESCRIPTOR_SCHEMA_VERSION.to_string(),
            descriptor_id: descriptor_id.to_string(),
            source_owner: source_owner.to_string(),
            source_identity: source_identity.clone(),
            version: if descriptor_id == MX1_ARM_ZERO_HARNESS_ID {
                "v1".to_string()
            } else {
                "provider-free-adapter-v1".to_string()
            },
            build_identity_sha256: mx1_expected_harness_evidence_digest(
                "build",
                descriptor_id,
                &source_identity,
            ),
            executable_identity_sha256: mx1_expected_harness_evidence_digest(
                "executable",
                descriptor_id,
                &source_identity,
            ),
            capability_probe_sha256: mx1_expected_harness_evidence_digest(
                "capability-probe",
                descriptor_id,
                &source_identity,
            ),
            shared_run_seam_version: PRODUCT_HARNESS_RUN_SEAM_SCHEMA_VERSION.to_string(),
            supported_task_capabilities: vec![
                "failure".to_string(),
                "restart".to_string(),
                "terminal-evidence".to_string(),
                "workspace-confinement".to_string(),
            ],
            supported_tool_capabilities: vec![
                "bounded-tools".to_string(),
                "no-provider-execution-in-core".to_string(),
            ],
            process_confinement: if descriptor_id == MX1_ARM_ZERO_HARNESS_ID {
                "engine-owned-no-exec-in-core".to_string()
            } else {
                "product-owned-confined-subprocess;core-no-spawn".to_string()
            },
            workspace_confinement: "product-workspace-binding-digest".to_string(),
            terminal_outcome_mapping: "product-task-status".to_string(),
            verified_deliverable_mapping: "product-terminal-evidence".to_string(),
            usage_cost_mapping: "existing-product-usage-and-cost-owners".to_string(),
            cancellation_mapping: "product-task-killed-state".to_string(),
            cleanup_mapping: "product-terminal-cleanup-evidence".to_string(),
            restart_mapping: "product-terminal-restart-evidence".to_string(),
            retry_mapping: "model-plan-max-retries".to_string(),
            failure_mapping: "product-task-failure-code-digest".to_string(),
            outcome_unknown_mapping: "product-task-outcome-unknown".to_string(),
            license_id: "Apache-2.0".to_string(),
            sbom_sha256: mx1_expected_harness_evidence_digest(
                "sbom",
                descriptor_id,
                &source_identity,
            ),
            provenance_sha256: mx1_expected_harness_evidence_digest(
                "provenance",
                descriptor_id,
                &source_identity,
            ),
            supported_model_ids: sorted_model_ids.clone(),
            supported_strategy_ids: sorted_strategy_ids.clone(),
            default_off: true,
            rollback_binding_sha256: mx1_expected_harness_evidence_digest(
                "rollback",
                descriptor_id,
                &source_identity,
            ),
            admission_disposition,
        }
    };
    let model = |descriptor_id: &str, resolved_model_id: &str| Mx1ModelPlanDescriptor {
        schema_version: MX1_DESCRIPTOR_SCHEMA_VERSION.to_string(),
        descriptor_id: descriptor_id.to_string(),
        requested_model_id: resolved_model_id.to_string(),
        resolved_model_id: resolved_model_id.to_string(),
        provider: "deepseek".to_string(),
        protocol: "openai_compatible".to_string(),
        endpoint: MX1_DEEPSEEK_CHAT_COMPLETIONS_ENDPOINT.to_string(),
        endpoint_allowlist: vec![MX1_DEEPSEEK_CHAT_COMPLETIONS_ENDPOINT.to_string()],
        admitted_profile_sha256: mx1_expected_model_evidence_digest(
            "admitted-profile",
            descriptor_id,
        ),
        credential_reference_name: MX1_DEEPSEEK_CREDENTIAL_REFERENCE.to_string(),
        role_order: vec![
            "planner".to_string(),
            "implementer".to_string(),
            "reviewer".to_string(),
        ],
        role_assignments: roles(resolved_model_id),
        max_provider_requests: 3,
        max_input_tokens: 12_000,
        max_output_tokens: 8_192,
        max_total_tokens: 20_192,
        max_retries: 0,
        max_wall_time_ms: 900_000,
        tokenizer_identity: "deepseek-tokenizer-current".to_string(),
        usage_mapping: "existing-execution-usage-owner".to_string(),
        pricing_currency: "USD".to_string(),
        pricing_unit: "per-token".to_string(),
        pricing_source_sha256: mx1_expected_model_evidence_digest("pricing-source", descriptor_id),
        pricing_effective_date: "2026-08-24".to_string(),
        lifecycle_cost_mapping: "existing-product-usage-and-cost-owners".to_string(),
        supported_harness_ids: sorted_harness_ids.clone(),
        supported_strategy_ids: sorted_strategy_ids.clone(),
        missing_identity_disposition: "incomparable".to_string(),
        missing_usage_disposition: "incomparable".to_string(),
    };
    let strategy = |descriptor_id: &str, projection: Option<Mx1ProjectionDescriptor>| {
        Mx1StrategyPlanDescriptor {
            schema_version: MX1_DESCRIPTOR_SCHEMA_VERSION.to_string(),
            descriptor_id: descriptor_id.to_string(),
            strategy_kind: "single-pass-plan-implement-review".to_string(),
            composition_order: vec![
                "plan".to_string(),
                "implement".to_string(),
                "review".to_string(),
            ],
            source_identity: mx1_confined_adapter_package_sha256(),
            source_identity_sha256: sha256_hex(&mx1_confined_adapter_package_sha256()),
            projection,
            admitted_input_class: "redacted-digest-reference".to_string(),
            redaction_class: "digest-only".to_string(),
            cross_task_isolation: true,
            cross_arm_isolation: true,
            leakage_scan_sha256: mx1_expected_strategy_evidence_digest("leakage", descriptor_id),
            prompt_policy_sha256: mx1_expected_strategy_evidence_digest(
                "prompt-policy",
                descriptor_id,
            ),
            tool_policy_sha256: mx1_expected_strategy_evidence_digest("tool-policy", descriptor_id),
            retry_policy_sha256: mx1_expected_strategy_evidence_digest(
                "retry-policy",
                descriptor_id,
            ),
            compression_policy_sha256: mx1_expected_strategy_evidence_digest(
                "compression-policy",
                descriptor_id,
            ),
            no_authority: true,
            supported_harness_ids: sorted_harness_ids.clone(),
            supported_model_ids: sorted_model_ids.clone(),
        }
    };
    seal_mx1_descriptor_manifest(Mx1DescriptorManifest {
        schema_version: MX1_DESCRIPTOR_MANIFEST_SCHEMA_VERSION.to_string(),
        contract_id: MX1_CONTRACT_ID.to_string(),
        harnesses: vec![
            harness(
                MX1_ARM_ZERO_HARNESS_ID,
                "rust-engine-product-golden-path",
                "075f995b574fb8a28f08986291751152bf158dd5".to_string(),
                Mx1HarnessAdmissionDisposition::EmbedBehindSeam,
            ),
            harness(
                MX1_SECOND_HARNESS_ID,
                "rust-engine-harness-evolution",
                mx1_confined_adapter_package_sha256(),
                Mx1HarnessAdmissionDisposition::ConfinedSubprocess,
            ),
        ],
        models: vec![
            model(MX1_ARM_ZERO_MODEL_ID, "deepseek-v4-pro"),
            model(MX1_SECOND_MODEL_ID, "deepseek-v4-flash"),
        ],
        strategies: vec![
            strategy(MX1_NO_PROJECTION_STRATEGY_ID, None),
            strategy(
                MX1_MEMORY_ONLY_STRATEGY_ID,
                Some(Mx1ProjectionDescriptor {
                    kind: Mx1ProjectionKind::Memory,
                    source_kind: Mx1ProjectionSourceKind::ArtifactRef,
                    source_handle: "artifact:mx1-memory-projection-v1".to_string(),
                    content_sha256: mx1_expected_strategy_evidence_digest(
                        "projection-content",
                        MX1_MEMORY_ONLY_STRATEGY_ID,
                    ),
                    expires_at_unix_ms: 4_102_444_800_000,
                    deletion_recipe_sha256: mx1_expected_strategy_evidence_digest(
                        "projection-delete",
                        MX1_MEMORY_ONLY_STRATEGY_ID,
                    ),
                    rebuild_recipe_sha256: mx1_expected_strategy_evidence_digest(
                        "projection-rebuild",
                        MX1_MEMORY_ONLY_STRATEGY_ID,
                    ),
                }),
            ),
            strategy(
                MX1_SKILL_ONLY_STRATEGY_ID,
                Some(Mx1ProjectionDescriptor {
                    kind: Mx1ProjectionKind::Skill,
                    source_kind: Mx1ProjectionSourceKind::GitBlob,
                    source_handle: "git:mx1-skill-projection-v1".to_string(),
                    content_sha256: mx1_expected_strategy_evidence_digest(
                        "projection-content",
                        MX1_SKILL_ONLY_STRATEGY_ID,
                    ),
                    expires_at_unix_ms: 4_102_444_800_000,
                    deletion_recipe_sha256: mx1_expected_strategy_evidence_digest(
                        "projection-delete",
                        MX1_SKILL_ONLY_STRATEGY_ID,
                    ),
                    rebuild_recipe_sha256: mx1_expected_strategy_evidence_digest(
                        "projection-rebuild",
                        MX1_SKILL_ONLY_STRATEGY_ID,
                    ),
                }),
            ),
        ],
        manifest_sha256: String::new(),
    })
    .expect("the built-in MX1 provider-free descriptor manifest is valid")
}

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
    RejectedLifecycleBudgetOverrun,
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
            Self::RejectedLifecycleBudgetOverrun => "rejected_lifecycle_budget_overrun",
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

/// Every lifecycle phase represented by a bundle must account for all six
/// dimensions. A phase-level bundle may mark a dimension unavailable, but it
/// may not silently omit the dimension or claim completeness from a partial
/// set.
fn required_ec3_dimensions_for_phase(
    _phase: LifecycleCostPhase,
) -> &'static [LifecycleCostDimension] {
    REQUIRED_LIFECYCLE_COST_DIMENSIONS
}

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

/// One append-only, source-bound EC3 lifecycle-cost input.  This is evidence
/// for later reconciliation only; it cannot reserve or spend an envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleCostObservationV1 {
    pub schema_version: String,
    pub observation_key: String,
    pub record_id: String,
    pub contract_id: String,
    pub candidate_id: String,
    pub evaluation_id: Option<String>,
    pub product_task_id: Option<String>,
    pub run_id: Option<String>,
    pub attempt_id: String,
    pub phase: LifecycleCostPhase,
    pub dimension: LifecycleCostDimension,
    /// Canonical integer unit; absent only for explicit unavailable evidence.
    pub amount: Option<u64>,
    pub trust_source: CostTrustSource,
    pub terminal_class: String,
    pub source_schema_version: String,
    pub source_digest: String,
    /// Redacted structured evidence only; never prompts, outputs, credentials, or paths.
    pub redacted_body: Value,
    pub record_sha256: String,
}

/// Normalized lifecycle accounting row used by the enforcement adapter.
/// Production evidence remains owned by the v38 observation store; this
/// compact shape is only the deterministic reconciliation input.
pub const LIFECYCLE_COST_RECORD_SCHEMA: &str = "harness_evolution_ec3_lifecycle_cost_record.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleCostRecordV1 {
    pub schema_version: String,
    pub record_id: String,
    pub candidate_id: String,
    pub phase: LifecycleCostPhase,
    pub token_cost: u64,
    pub call_count: u64,
    pub provider_cost_microunits: u64,
    pub wall_clock_milliseconds: u64,
    pub compute_milliseconds: u64,
    pub human_effort_milliseconds: u64,
    pub observed_dimensions: Vec<LifecycleCostDimension>,
    pub trust_source: CostTrustSource,
    pub terminal_class: String,
    pub unmeasured: bool,
    pub failure_attempt: bool,
    pub evidence_payload_digest: String,
    pub record_sha256: String,
}

pub fn seal_lifecycle_cost_record(
    mut record: LifecycleCostRecordV1,
) -> Result<LifecycleCostRecordV1, EvolutionAdmissionError> {
    record.schema_version = LIFECYCLE_COST_RECORD_SCHEMA.to_string();
    require_nonempty_id(&record.candidate_id, "ec3_identity_empty")?;
    validate_sha256_hex(&record.evidence_payload_digest)?;
    if record.trust_source == CostTrustSource::CallerEstimate {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_untrusted",
            "caller estimates cannot reconcile lifecycle spend",
        ));
    }
    if record.observed_dimensions.is_empty() {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_dimensions_missing",
            "lifecycle cost record must identify measured dimensions",
        ));
    }
    require_nonempty_id(&record.terminal_class, "ec3_cost_terminal_missing")?;
    let mut dimensions = record.observed_dimensions.clone();
    dimensions.sort_unstable();
    dimensions.dedup();
    if dimensions.len() != record.observed_dimensions.len() {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_dimensions_duplicate",
            "lifecycle cost record dimensions must be unique",
        ));
    }
    record.observed_dimensions = dimensions;
    if record.record_id.trim().is_empty() {
        record.record_id = format!(
            "helcr-{}",
            &sha256_hex(&format!(
                "helcr.v1|{}|{:?}|{}|{}|{}|{}|{}|{}|{}|{:?}|{}",
                record.candidate_id,
                record.phase,
                record.evidence_payload_digest,
                record.token_cost,
                record.call_count,
                record.provider_cost_microunits,
                record.wall_clock_milliseconds,
                record.compute_milliseconds,
                record.human_effort_milliseconds,
                record.observed_dimensions,
                record.terminal_class
            ))[..32]
        );
    }
    let mut value = serde_json::to_value(&record)
        .map_err(|error| EvolutionAdmissionError::new("ec3_record_digest", error.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    record.record_sha256 = record_digest_excluding_sha256(&value)?;
    validate_lifecycle_cost_record(&record)?;
    Ok(record)
}

pub fn validate_lifecycle_cost_record(
    record: &LifecycleCostRecordV1,
) -> Result<(), EvolutionAdmissionError> {
    if record.schema_version != LIFECYCLE_COST_RECORD_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec3_schema_invalid",
            "lifecycle cost record schema mismatch",
        ));
    }
    require_nonempty_id(&record.record_id, "ec3_identity_empty")?;
    require_nonempty_id(&record.candidate_id, "ec3_identity_empty")?;
    validate_sha256_hex(&record.evidence_payload_digest)?;
    let value = serde_json::to_value(record)
        .map_err(|error| EvolutionAdmissionError::new("ec3_record_digest", error.to_string()))?;
    require_record_sha256(&value, &record.record_sha256)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleCostMissingReason {
    Unavailable,
    RequiredMissing,
    JoinAmbiguous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleCostMissingnessV1 {
    pub phase: LifecycleCostPhase,
    pub dimension: LifecycleCostDimension,
    pub reason: LifecycleCostMissingReason,
}

/// A source-bound immutable batch assembled by an existing production owner.
/// Bundles are transport/reconciliation inputs; they do not reserve or spend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleCostObservationBundleV1 {
    pub schema_version: String,
    pub bundle_id: String,
    pub contract_id: String,
    pub candidate_id: String,
    pub evaluation_id: Option<String>,
    pub product_task_id: Option<String>,
    pub run_id: Option<String>,
    pub attempt_id: String,
    pub source_digests: Vec<String>,
    pub observations: Vec<LifecycleCostObservationV1>,
    pub missing: Vec<LifecycleCostMissingnessV1>,
    pub record_sha256: String,
}

/// Redacted read model used by operator evidence and later reconciliation.
/// It intentionally excludes each observation's structured evidence body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleCostReadModelObservationV1 {
    pub record_id: String,
    pub observation_key: String,
    pub contract_id: String,
    pub candidate_id: String,
    pub evaluation_id: Option<String>,
    pub product_task_id: Option<String>,
    pub run_id: Option<String>,
    pub attempt_id: String,
    pub phase: LifecycleCostPhase,
    pub dimension: LifecycleCostDimension,
    pub amount: Option<u64>,
    pub trust_source: CostTrustSource,
    pub terminal_class: String,
    pub source_schema_version: String,
    pub source_digest: String,
    pub record_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleCostReadModelV1 {
    pub schema_version: String,
    pub bundle_id: String,
    pub contract_id: String,
    pub candidate_id: String,
    pub evaluation_id: Option<String>,
    pub product_task_id: Option<String>,
    pub run_id: Option<String>,
    pub attempt_id: String,
    pub observations: Vec<LifecycleCostReadModelObservationV1>,
    pub missing: Vec<LifecycleCostMissingnessV1>,
    pub complete: bool,
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

fn ec3_cost_observation_value(
    observation: &LifecycleCostObservationV1,
) -> Result<Value, EvolutionAdmissionError> {
    let mut value = serde_json::to_value(observation).map_err(|error| {
        EvolutionAdmissionError::new("ec3_cost_record_tamper", error.to_string())
    })?;
    if let Value::Object(map) = &mut value {
        map.insert("record_id".into(), Value::String(String::new()));
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    Ok(value)
}

pub fn derive_ec3_lifecycle_cost_record_id(
    observation: &LifecycleCostObservationV1,
) -> Result<String, EvolutionAdmissionError> {
    let digest = canonical_json_sha256(&ec3_cost_observation_value(observation)?)
        .map_err(|error| EvolutionAdmissionError::new("ec3_cost_record_tamper", error))?;
    Ok(format!("helc-{}", &digest[..32]))
}

pub fn validate_ec3_lifecycle_cost_observation(
    observation: &LifecycleCostObservationV1,
) -> Result<(), EvolutionAdmissionError> {
    if observation.schema_version != EC3_LIFECYCLE_COST_OBSERVATION_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_schema_drift",
            "observation schema mismatch",
        ));
    }
    for value in [
        &observation.observation_key,
        &observation.record_id,
        &observation.contract_id,
        &observation.candidate_id,
        &observation.attempt_id,
        &observation.terminal_class,
        &observation.source_schema_version,
    ] {
        if value.trim().is_empty() {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_required_missing",
                "required observation identity is empty",
            ));
        }
    }
    validate_sha256_hex(&observation.source_digest)?;
    if !observation.redacted_body.is_object() {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_private_evidence",
            "redacted lifecycle-cost evidence must be a structured object",
        ));
    }
    refuse_sensitive_payload_fields(&observation.redacted_body).map_err(|error| {
        EvolutionAdmissionError::new("ec3_cost_private_evidence", error.message)
    })?;
    if observation.trust_source == CostTrustSource::CallerEstimate {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_source_untrusted",
            "caller estimate is not lifecycle-cost evidence",
        ));
    }
    match (observation.trust_source, observation.amount) {
        (CostTrustSource::Unavailable, Some(_)) => {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_unavailable_has_amount",
                "unavailable evidence cannot carry an amount",
            ))
        }
        (CostTrustSource::Unavailable, None) => {}
        (_, None) => {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_amount_invalid",
                "measured or derived evidence requires a canonical integer amount",
            ))
        }
        _ => {}
    }
    if observation.amount == Some(0)
        && observation.trust_source != CostTrustSource::Unavailable
        && observation
            .redacted_body
            .as_object()
            .is_some_and(|body| body.is_empty())
    {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_zero_unproved",
            "explicit zero lifecycle cost requires measured or deterministic evidence",
        ));
    }
    let expected_id = derive_ec3_lifecycle_cost_record_id(observation)?;
    if observation.record_id != expected_id {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_observation_conflict",
            "record_id is not bound to the complete observation",
        ));
    }
    let value = ec3_cost_observation_value(observation)?;
    require_record_sha256(&value, &observation.record_sha256)
        .map_err(|error| EvolutionAdmissionError::new("ec3_cost_record_tamper", error.message))
}

pub fn seal_ec3_lifecycle_cost_observation(
    mut observation: LifecycleCostObservationV1,
) -> Result<LifecycleCostObservationV1, EvolutionAdmissionError> {
    observation.schema_version = EC3_LIFECYCLE_COST_OBSERVATION_SCHEMA.to_string();
    observation.record_id = derive_ec3_lifecycle_cost_record_id(&observation)?;
    let value = ec3_cost_observation_value(&observation)?;
    observation.record_sha256 = record_digest_excluding_sha256(&value).map_err(|error| {
        if error.code == "evolution_sensitive_payload" {
            EvolutionAdmissionError::new("ec3_cost_private_evidence", error.message)
        } else {
            error
        }
    })?;
    validate_ec3_lifecycle_cost_observation(&observation)?;
    Ok(observation)
}

pub fn ec3_lifecycle_cost_unit_for_dimension(dimension: LifecycleCostDimension) -> &'static str {
    match dimension {
        LifecycleCostDimension::ModelTokens | LifecycleCostDimension::ProviderCalls => "count",
        LifecycleCostDimension::ProviderCostMicrounits => "microunits",
        LifecycleCostDimension::WallClockMilliseconds
        | LifecycleCostDimension::ComputeMilliseconds
        | LifecycleCostDimension::HumanEffortMilliseconds => "milliseconds",
    }
}

/// Convert a source measurement into the one canonical integer unit for its
/// EC3 dimension. A missing value is valid only with the explicit unavailable
/// marker; floating point, negative, and mismatched units fail closed.
pub fn normalize_ec3_lifecycle_cost_amount(
    dimension: LifecycleCostDimension,
    amount: Option<i128>,
    unit: &str,
) -> Result<Option<u64>, EvolutionAdmissionError> {
    if amount.is_none() {
        if unit != "unavailable" {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_unit_invalid",
                "missing lifecycle cost must use the unavailable unit marker",
            ));
        }
        return Ok(None);
    }
    if unit != ec3_lifecycle_cost_unit_for_dimension(dimension) {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_unit_invalid",
            "lifecycle cost unit does not match its EC3 dimension",
        ));
    }
    let amount = amount.expect("checked above");
    if amount < 0 {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_amount_invalid",
            "lifecycle cost amount cannot be negative",
        ));
    }
    u64::try_from(amount).map(Some).map_err(|_| {
        EvolutionAdmissionError::new(
            "ec3_cost_amount_invalid",
            "lifecycle cost amount exceeds canonical integer range",
        )
    })
}

/// Build EC3 observations from the canonical execution-usage owner. This is
/// an evidence adapter only: it never reserves, spends, or mutates the usage
/// owner, and ambiguous/partial events become explicit unavailable records.
pub fn ec3_lifecycle_cost_observations_from_usage_event(
    contract_id: &str,
    candidate_id: &str,
    attempt_id: &str,
    phase: LifecycleCostPhase,
    terminal_class: &str,
    event: &crate::execution_usage::ExecutionUsageEventV1,
) -> Result<Vec<LifecycleCostObservationV1>, EvolutionAdmissionError> {
    ec3_lifecycle_cost_observations_from_usage_event_with_identity(
        contract_id,
        candidate_id,
        None,
        event.product_task_id.as_deref(),
        None,
        attempt_id,
        phase,
        terminal_class,
        event,
    )
}

/// Usage adapter variant for the production terminal-evidence join. The
/// canonical usage event remains read-only; callers provide the already-owned
/// evaluation/task/run identities and any additional source digests (for
/// example scorecard/VDE digests) at the bundle boundary.
pub fn ec3_lifecycle_cost_observations_from_usage_event_with_identity(
    contract_id: &str,
    candidate_id: &str,
    evaluation_id: Option<&str>,
    product_task_id: Option<&str>,
    run_id: Option<&str>,
    attempt_id: &str,
    phase: LifecycleCostPhase,
    terminal_class: &str,
    event: &crate::execution_usage::ExecutionUsageEventV1,
) -> Result<Vec<LifecycleCostObservationV1>, EvolutionAdmissionError> {
    require_nonempty_id(contract_id, "ec3_cost_required_missing")?;
    require_nonempty_id(candidate_id, "ec3_cost_required_missing")?;
    require_nonempty_id(attempt_id, "ec3_cost_required_missing")?;
    require_nonempty_id(terminal_class, "ec3_cost_required_missing")?;
    require_nonempty_id(&event.event_id, "ec3_cost_source_missing")?;
    require_nonempty_id(&event.source_schema_version, "ec3_cost_source_missing")?;
    if product_task_id.is_some()
        && event.product_task_id.as_deref().is_some()
        && product_task_id != event.product_task_id.as_deref()
    {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_join_ambiguous",
            "caller task identity disagrees with the canonical usage event",
        ));
    }
    if event.schema_version != crate::execution_usage::EXECUTION_USAGE_EVENT_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_source_schema_drift",
            "execution usage event schema is not the accepted canonical version",
        ));
    }
    let event_value = serde_json::to_value(event).map_err(|error| {
        EvolutionAdmissionError::new("ec3_cost_source_invalid", error.to_string())
    })?;
    let source_digest = canonical_json_sha256(&event_value)
        .map_err(|error| EvolutionAdmissionError::new("ec3_cost_source_invalid", error))?;
    let complete = matches!(
        event.event_completeness,
        crate::execution_usage::EventCompleteness::Complete
    );
    let safe_body = serde_json::json!({
        "event_id": event.event_id,
        "executor_kind": event.executor_kind,
        "evidence_source_kind": event.evidence_source_kind,
        "event_completeness": event.event_completeness,
        "stable_dedupe_identity": event.stable_dedupe_identity,
    });
    let mut observations = Vec::new();
    let mut add = |dimension: LifecycleCostDimension,
                   amount: Option<u64>,
                   trust_source: CostTrustSource|
     -> Result<(), EvolutionAdmissionError> {
        let dimension_name = serde_json::to_value(dimension)
            .map_err(|error| {
                EvolutionAdmissionError::new("ec3_cost_source_invalid", error.to_string())
            })?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        observations.push(seal_ec3_lifecycle_cost_observation(
            LifecycleCostObservationV1 {
                schema_version: String::new(),
                observation_key: format!("usage:{}:{}", event.event_id, dimension_name),
                record_id: String::new(),
                contract_id: contract_id.to_string(),
                candidate_id: candidate_id.to_string(),
                evaluation_id: evaluation_id.map(str::to_string),
                product_task_id: product_task_id
                    .map(str::to_string)
                    .or_else(|| event.product_task_id.clone()),
                run_id: run_id.map(str::to_string),
                attempt_id: attempt_id.to_string(),
                phase,
                dimension,
                amount,
                trust_source,
                terminal_class: terminal_class.to_string(),
                source_schema_version: event.source_schema_version.clone(),
                source_digest: source_digest.clone(),
                redacted_body: safe_body.clone(),
                record_sha256: String::new(),
            },
        )?);
        Ok(())
    };
    let token_source = if complete {
        CostTrustSource::MeasuredDirect
    } else {
        CostTrustSource::Unavailable
    };
    add(
        LifecycleCostDimension::ModelTokens,
        complete.then_some(event.billable_token_total()),
        token_source,
    )?;
    add(
        LifecycleCostDimension::ProviderCalls,
        if complete && event.provider_id.is_some() {
            Some(1)
        } else {
            None
        },
        if complete && event.provider_id.is_some() {
            CostTrustSource::MeasuredDirect
        } else {
            CostTrustSource::Unavailable
        },
    )?;
    let provider_cost = if complete
        && matches!(
            event.cost_source,
            crate::execution_usage::CostSource::ProviderOrExecutorReported
        ) {
        event.provider_reported_cost.and_then(|cost| {
            if cost.is_finite() && cost >= 0.0 {
                let micros = cost * 1_000_000.0;
                if micros <= u64::MAX as f64 {
                    let rounded = micros.round();
                    if (micros - rounded).abs() <= 1e-9 {
                        Some(rounded as u64)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        })
    } else {
        None
    };
    add(
        LifecycleCostDimension::ProviderCostMicrounits,
        provider_cost,
        if provider_cost.is_some() {
            CostTrustSource::MeasuredDirect
        } else {
            CostTrustSource::Unavailable
        },
    )?;
    Ok(observations)
}

fn ec3_cost_bundle_value(
    bundle: &LifecycleCostObservationBundleV1,
) -> Result<Value, EvolutionAdmissionError> {
    let mut value = serde_json::to_value(bundle).map_err(|error| {
        EvolutionAdmissionError::new("ec3_cost_record_tamper", error.to_string())
    })?;
    if let Value::Object(map) = &mut value {
        map.insert("bundle_id".into(), Value::String(String::new()));
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    Ok(value)
}

pub fn derive_ec3_lifecycle_cost_bundle_id(
    bundle: &LifecycleCostObservationBundleV1,
) -> Result<String, EvolutionAdmissionError> {
    let digest = canonical_json_sha256(&ec3_cost_bundle_value(bundle)?)
        .map_err(|error| EvolutionAdmissionError::new("ec3_cost_record_tamper", error))?;
    Ok(format!("helb-{}", &digest[..32]))
}

pub fn validate_ec3_lifecycle_cost_bundle(
    bundle: &LifecycleCostObservationBundleV1,
) -> Result<(), EvolutionAdmissionError> {
    if bundle.schema_version != EC3_LIFECYCLE_COST_BUNDLE_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_schema_drift",
            "lifecycle cost bundle schema mismatch",
        ));
    }
    for value in [
        &bundle.bundle_id,
        &bundle.contract_id,
        &bundle.candidate_id,
        &bundle.attempt_id,
    ] {
        if value.trim().is_empty() {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_required_missing",
                "required lifecycle cost bundle identity is empty",
            ));
        }
    }
    if bundle.observations.is_empty() && bundle.missing.is_empty() {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_required_missing",
            "lifecycle cost bundle must contain observations or explicit missingness",
        ));
    }
    let mut source_digests = BTreeSet::new();
    for digest in &bundle.source_digests {
        validate_sha256_hex(digest)?;
        if !source_digests.insert(digest) {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_join_ambiguous",
                "lifecycle cost source digest is duplicated",
            ));
        }
    }
    let mut dimensions = BTreeSet::new();
    let mut phases = BTreeSet::new();
    for observation in &bundle.observations {
        validate_ec3_lifecycle_cost_observation(observation)?;
        if observation.contract_id != bundle.contract_id
            || observation.candidate_id != bundle.candidate_id
            || observation.evaluation_id != bundle.evaluation_id
            || observation.product_task_id != bundle.product_task_id
            || observation.run_id != bundle.run_id
            || observation.attempt_id != bundle.attempt_id
        {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_join_ambiguous",
                "observation identity does not match its lifecycle-cost bundle",
            ));
        }
        if !source_digests.contains(&observation.source_digest) {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_join_ambiguous",
                "observation source digest is absent from the bundle join set",
            ));
        }
        if !dimensions.insert((observation.phase, observation.dimension)) {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_observation_conflict",
                "lifecycle cost bundle repeats a phase and dimension",
            ));
        }
        phases.insert(observation.phase);
    }
    for missing in &bundle.missing {
        if !dimensions.insert((missing.phase, missing.dimension)) {
            return Err(EvolutionAdmissionError::new(
                "ec3_cost_observation_conflict",
                "lifecycle cost bundle repeats a phase and dimension",
            ));
        }
        phases.insert(missing.phase);
    }
    for phase in phases {
        for dimension in required_ec3_dimensions_for_phase(phase) {
            if !dimensions.contains(&(phase, *dimension)) {
                return Err(EvolutionAdmissionError::new(
                    "ec3_cost_required_missing",
                    "lifecycle cost bundle omits a required phase/dimension; record explicit missingness",
                ));
            }
        }
    }
    let expected_id = derive_ec3_lifecycle_cost_bundle_id(bundle)?;
    if bundle.bundle_id != expected_id {
        return Err(EvolutionAdmissionError::new(
            "ec3_cost_observation_conflict",
            "bundle_id is not bound to the complete lifecycle-cost bundle",
        ));
    }
    let value = ec3_cost_bundle_value(bundle)?;
    require_record_sha256(&value, &bundle.record_sha256)
        .map_err(|error| EvolutionAdmissionError::new("ec3_cost_record_tamper", error.message))
}

pub fn seal_ec3_lifecycle_cost_bundle(
    mut bundle: LifecycleCostObservationBundleV1,
) -> Result<LifecycleCostObservationBundleV1, EvolutionAdmissionError> {
    bundle.schema_version = EC3_LIFECYCLE_COST_BUNDLE_SCHEMA.to_string();
    bundle.observations = bundle
        .observations
        .into_iter()
        .map(seal_ec3_lifecycle_cost_observation)
        .collect::<Result<Vec<_>, _>>()?;
    bundle
        .observations
        .sort_by_key(|observation| (observation.phase, observation.dimension));
    bundle
        .missing
        .sort_by_key(|missing| (missing.phase, missing.dimension));
    bundle.bundle_id = derive_ec3_lifecycle_cost_bundle_id(&bundle)?;
    let value = ec3_cost_bundle_value(&bundle)?;
    bundle.record_sha256 = record_digest_excluding_sha256(&value).map_err(|error| {
        if error.code == "evolution_sensitive_payload" {
            EvolutionAdmissionError::new("ec3_cost_private_evidence", error.message)
        } else {
            error
        }
    })?;
    validate_ec3_lifecycle_cost_bundle(&bundle)?;
    Ok(bundle)
}

pub fn build_ec3_lifecycle_cost_read_model(
    bundle: &LifecycleCostObservationBundleV1,
) -> Result<LifecycleCostReadModelV1, EvolutionAdmissionError> {
    validate_ec3_lifecycle_cost_bundle(bundle)?;
    let observations = bundle
        .observations
        .iter()
        .map(|observation| LifecycleCostReadModelObservationV1 {
            record_id: observation.record_id.clone(),
            observation_key: observation.observation_key.clone(),
            contract_id: observation.contract_id.clone(),
            candidate_id: observation.candidate_id.clone(),
            evaluation_id: observation.evaluation_id.clone(),
            product_task_id: observation.product_task_id.clone(),
            run_id: observation.run_id.clone(),
            attempt_id: observation.attempt_id.clone(),
            phase: observation.phase,
            dimension: observation.dimension,
            amount: observation.amount,
            trust_source: observation.trust_source,
            terminal_class: observation.terminal_class.clone(),
            source_schema_version: observation.source_schema_version.clone(),
            source_digest: observation.source_digest.clone(),
            record_sha256: observation.record_sha256.clone(),
        })
        .collect();
    let mut model = LifecycleCostReadModelV1 {
        schema_version: EC3_LIFECYCLE_COST_READ_MODEL_SCHEMA.to_string(),
        bundle_id: bundle.bundle_id.clone(),
        contract_id: bundle.contract_id.clone(),
        candidate_id: bundle.candidate_id.clone(),
        evaluation_id: bundle.evaluation_id.clone(),
        product_task_id: bundle.product_task_id.clone(),
        run_id: bundle.run_id.clone(),
        attempt_id: bundle.attempt_id.clone(),
        observations,
        missing: bundle.missing.clone(),
        complete: bundle.missing.is_empty(),
        record_sha256: String::new(),
    };
    let value = serde_json::to_value(&model).map_err(|error| {
        EvolutionAdmissionError::new("ec3_cost_record_tamper", error.to_string())
    })?;
    model.record_sha256 = record_digest_excluding_sha256(&value)?;
    Ok(model)
}

pub const LIFECYCLE_BUDGET_RESERVATION_SCHEMA: &str = "harness_evolution_ec3_budget_reservation.v1";
pub const LIFECYCLE_BUDGET_RECONCILIATION_SCHEMA: &str =
    "harness_evolution_ec3_budget_reconciliation.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleBudgetReservationStatus {
    Active,
    Reconciled,
    Cancelled,
    Overrun,
}

impl LifecycleBudgetReservationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Reconciled => "reconciled",
            Self::Cancelled => "cancelled",
            Self::Overrun => "overrun",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleBudgetReservationV1 {
    pub schema_version: String,
    pub reservation_id: String,
    pub candidate_id: String,
    pub contract_id: String,
    pub reserved_token_cost: u64,
    pub reserved_call_count: u64,
    pub reserved_provider_cost_microunits: u64,
    pub reserved_wall_clock_milliseconds: u64,
    pub reserved_compute_milliseconds: u64,
    pub reserved_human_effort_milliseconds: u64,
    pub status: LifecycleBudgetReservationStatus,
    pub record_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleBudgetReconciliationOutcome {
    WithinEnvelope,
    OverrunStopped,
    CancelledReleased,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleBudgetTerminalState {
    Completed,
    Failed,
    Cancelled,
    OutcomeUnknown,
    MissingUsage,
    RecoveryRequired,
}

impl LifecycleBudgetTerminalState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::MissingUsage => "missing_usage",
            Self::RecoveryRequired => "recovery_required",
        }
    }
}

impl LifecycleBudgetReconciliationOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WithinEnvelope => "within_envelope",
            Self::OverrunStopped => "overrun_stopped",
            Self::CancelledReleased => "cancelled_released",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseCostSummary {
    pub phase: LifecycleCostPhase,
    pub token_cost: u64,
    pub call_count: u64,
    pub provider_cost_microunits: u64,
    pub wall_clock_milliseconds: u64,
    pub compute_milliseconds: u64,
    pub human_effort_milliseconds: u64,
    pub observed_dimensions: Vec<LifecycleCostDimension>,
    pub failure_attempts: u64,
    pub unmeasured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleBudgetReconciliationV1 {
    pub schema_version: String,
    pub reconciliation_id: String,
    pub reservation_id: String,
    pub candidate_id: String,
    pub contract_id: String,
    pub total_token_cost: u64,
    pub total_call_count: u64,
    pub total_provider_cost_microunits: u64,
    pub total_wall_clock_milliseconds: u64,
    pub total_compute_milliseconds: u64,
    pub total_human_effort_milliseconds: u64,
    pub total_failure_attempts: u64,
    pub terminal_state: LifecycleBudgetTerminalState,
    pub per_phase_costs: Vec<PhaseCostSummary>,
    pub outcome: LifecycleBudgetReconciliationOutcome,
    pub overrun_phase: Option<LifecycleCostPhase>,
    pub terminal_reason: Option<CandidateTerminalReason>,
    pub record_sha256: String,
}

pub fn derive_lifecycle_budget_reservation_id(candidate_id: &str, contract_id: &str) -> String {
    format!(
        "helbr-{}",
        &sha256_hex(&format!("helbr.v1|{candidate_id}|{contract_id}"))[..32]
    )
}

pub fn derive_lifecycle_budget_reconciliation_id(
    reservation_id: &str,
    candidate_id: &str,
) -> String {
    format!(
        "helbc-{}",
        &sha256_hex(&format!("helbc.v1|{reservation_id}|{candidate_id}"))[..32]
    )
}

pub fn validate_lifecycle_budget_reservation(
    reservation: &LifecycleBudgetReservationV1,
) -> Result<(), EvolutionAdmissionError> {
    if reservation.schema_version != LIFECYCLE_BUDGET_RESERVATION_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec3_schema_invalid",
            "lifecycle budget reservation schema mismatch",
        ));
    }
    require_nonempty_id(&reservation.candidate_id, "ec3_identity_empty")?;
    require_nonempty_id(&reservation.contract_id, "ec3_identity_empty")?;
    require_derived_id(
        &reservation.reservation_id,
        &derive_lifecycle_budget_reservation_id(
            &reservation.candidate_id,
            &reservation.contract_id,
        ),
    )?;
    let value = serde_json::to_value(reservation)
        .map_err(|e| EvolutionAdmissionError::new("ec3_record_digest", e.to_string()))?;
    require_record_sha256(&value, &reservation.record_sha256)?;
    Ok(())
}

pub fn seal_lifecycle_budget_reservation(
    mut reservation: LifecycleBudgetReservationV1,
) -> Result<LifecycleBudgetReservationV1, EvolutionAdmissionError> {
    reservation.schema_version = LIFECYCLE_BUDGET_RESERVATION_SCHEMA.to_string();
    if reservation.reservation_id.trim().is_empty() {
        reservation.reservation_id = derive_lifecycle_budget_reservation_id(
            &reservation.candidate_id,
            &reservation.contract_id,
        );
    }
    let mut value = serde_json::to_value(&reservation)
        .map_err(|e| EvolutionAdmissionError::new("ec3_record_digest", e.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    reservation.record_sha256 = record_digest_excluding_sha256(&value)?;
    validate_lifecycle_budget_reservation(&reservation)?;
    Ok(reservation)
}

pub fn validate_lifecycle_budget_reconciliation(
    reconciliation: &LifecycleBudgetReconciliationV1,
) -> Result<(), EvolutionAdmissionError> {
    if reconciliation.schema_version != LIFECYCLE_BUDGET_RECONCILIATION_SCHEMA {
        return Err(EvolutionAdmissionError::new(
            "ec3_schema_invalid",
            "lifecycle budget reconciliation schema mismatch",
        ));
    }
    require_nonempty_id(&reconciliation.reservation_id, "ec3_identity_empty")?;
    require_nonempty_id(&reconciliation.candidate_id, "ec3_identity_empty")?;
    require_nonempty_id(&reconciliation.contract_id, "ec3_identity_empty")?;
    require_derived_id(
        &reconciliation.reconciliation_id,
        &derive_lifecycle_budget_reconciliation_id(
            &reconciliation.reservation_id,
            &reconciliation.candidate_id,
        ),
    )?;
    let value = serde_json::to_value(reconciliation)
        .map_err(|e| EvolutionAdmissionError::new("ec3_record_digest", e.to_string()))?;
    require_record_sha256(&value, &reconciliation.record_sha256)?;
    Ok(())
}

pub fn seal_lifecycle_budget_reconciliation(
    mut reconciliation: LifecycleBudgetReconciliationV1,
) -> Result<LifecycleBudgetReconciliationV1, EvolutionAdmissionError> {
    reconciliation.schema_version = LIFECYCLE_BUDGET_RECONCILIATION_SCHEMA.to_string();
    if reconciliation.reconciliation_id.trim().is_empty() {
        reconciliation.reconciliation_id = derive_lifecycle_budget_reconciliation_id(
            &reconciliation.reservation_id,
            &reconciliation.candidate_id,
        );
    }
    let mut value = serde_json::to_value(&reconciliation)
        .map_err(|e| EvolutionAdmissionError::new("ec3_record_digest", e.to_string()))?;
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    reconciliation.record_sha256 = record_digest_excluding_sha256(&value)?;
    validate_lifecycle_budget_reconciliation(&reconciliation)?;
    Ok(reconciliation)
}

pub fn reconcile_candidate_lifecycle_costs(
    contract: &Ec3LifecycleBudgetContractV1,
    reservation: &LifecycleBudgetReservationV1,
    records: &[LifecycleCostRecordV1],
) -> Result<LifecycleBudgetReconciliationV1, EvolutionAdmissionError> {
    validate_ec3_lifecycle_budget_contract(contract)?;
    if reservation.contract_id != contract.contract_id {
        return Err(EvolutionAdmissionError::new(
            "ec3_contract_mismatch",
            "reservation contract_id does not match contract",
        ));
    }
    let mut total_tokens = 0_u64;
    let mut total_calls = 0_u64;
    let mut total_provider_cost = 0_u64;
    let mut total_wall_clock = 0_u64;
    let mut total_compute = 0_u64;
    let mut total_human_effort = 0_u64;
    let mut total_failures = 0_u64;
    let mut phase_map: BTreeMap<LifecycleCostPhase, PhaseCostSummary> = BTreeMap::new();

    for record in records {
        if record.candidate_id != reservation.candidate_id {
            return Err(EvolutionAdmissionError::new(
                "ec3_candidate_mismatch",
                "record candidate_id does not match reservation",
            ));
        }
        total_tokens = total_tokens.saturating_add(record.token_cost);
        total_calls = total_calls.saturating_add(record.call_count);
        total_provider_cost = total_provider_cost.saturating_add(record.provider_cost_microunits);
        total_wall_clock = total_wall_clock.saturating_add(record.wall_clock_milliseconds);
        total_compute = total_compute.saturating_add(record.compute_milliseconds);
        total_human_effort = total_human_effort.saturating_add(record.human_effort_milliseconds);
        if record.failure_attempt {
            total_failures = total_failures.saturating_add(1);
        }

        let entry = phase_map
            .entry(record.phase)
            .or_insert_with(|| PhaseCostSummary {
                phase: record.phase,
                token_cost: 0,
                call_count: 0,
                provider_cost_microunits: 0,
                wall_clock_milliseconds: 0,
                compute_milliseconds: 0,
                human_effort_milliseconds: 0,
                observed_dimensions: record.observed_dimensions.clone(),
                failure_attempts: 0,
                unmeasured: record.unmeasured,
            });
        entry.token_cost = entry.token_cost.saturating_add(record.token_cost);
        entry.call_count = entry.call_count.saturating_add(record.call_count);
        entry.provider_cost_microunits = entry
            .provider_cost_microunits
            .saturating_add(record.provider_cost_microunits);
        entry.wall_clock_milliseconds = entry
            .wall_clock_milliseconds
            .saturating_add(record.wall_clock_milliseconds);
        entry.compute_milliseconds = entry
            .compute_milliseconds
            .saturating_add(record.compute_milliseconds);
        entry.human_effort_milliseconds = entry
            .human_effort_milliseconds
            .saturating_add(record.human_effort_milliseconds);
        if record.failure_attempt {
            entry.failure_attempts = entry.failure_attempts.saturating_add(1);
        }
        if record.unmeasured {
            entry.unmeasured = true;
        }
        entry
            .observed_dimensions
            .extend(record.observed_dimensions.iter().copied());
        entry.observed_dimensions.sort_unstable();
        entry.observed_dimensions.dedup();
    }

    let per_phase_costs: Vec<PhaseCostSummary> = phase_map.into_values().collect();
    let terminal_state = if records.is_empty() {
        LifecycleBudgetTerminalState::MissingUsage
    } else if records.iter().any(|record| {
        let class = record.terminal_class.to_ascii_lowercase();
        class.contains("outcome_unknown") || class.contains("unknown")
    }) {
        LifecycleBudgetTerminalState::OutcomeUnknown
    } else if records.iter().any(|record| {
        let class = record.terminal_class.to_ascii_lowercase();
        class.contains("recovery") || class.contains("cleanup")
    }) {
        LifecycleBudgetTerminalState::RecoveryRequired
    } else if records.iter().any(|record| {
        let class = record.terminal_class.to_ascii_lowercase();
        class == "cancelled" || class == "canceled" || class == "killed"
    }) {
        LifecycleBudgetTerminalState::Cancelled
    } else if records.iter().any(|record| record.failure_attempt) {
        LifecycleBudgetTerminalState::Failed
    } else {
        LifecycleBudgetTerminalState::Completed
    };
    let limits = |resources: &[LifecycleResourceLimit]| -> BTreeMap<LifecycleCostDimension, u64> {
        resources
            .iter()
            .map(|item| (item.dimension, item.limit))
            .collect()
    };
    let candidate_limits = limits(&contract.candidate_envelope.resource_limits);
    let token_limit = candidate_limits
        .get(&LifecycleCostDimension::ModelTokens)
        .copied()
        .unwrap_or(0);
    let call_limit = candidate_limits
        .get(&LifecycleCostDimension::ProviderCalls)
        .copied()
        .unwrap_or(0);
    let provider_cost_limit = candidate_limits
        .get(&LifecycleCostDimension::ProviderCostMicrounits)
        .copied()
        .unwrap_or(0);
    let wall_limit = candidate_limits
        .get(&LifecycleCostDimension::WallClockMilliseconds)
        .copied()
        .unwrap_or(0);
    let compute_limit = candidate_limits
        .get(&LifecycleCostDimension::ComputeMilliseconds)
        .copied()
        .unwrap_or(0);
    let human_effort_limit = candidate_limits
        .get(&LifecycleCostDimension::HumanEffortMilliseconds)
        .copied()
        .unwrap_or(0);
    // An empty record set or an explicitly unmeasured record is not evidence
    // of zero spend.  Keep the terminal result conservative so a missing
    // usage/late-write/outcome-unknown path cannot be accepted as within the
    // envelope.
    let incomplete = records.is_empty()
        || records.iter().any(|record| record.unmeasured)
        || per_phase_costs.iter().any(|summary| {
            REQUIRED_LIFECYCLE_COST_DIMENSIONS
                .iter()
                .any(|dimension| !summary.observed_dimensions.contains(dimension))
        });
    let overrun = incomplete
        || total_tokens > token_limit
        || total_calls > call_limit
        || total_provider_cost > provider_cost_limit
        || total_wall_clock > wall_limit
        || total_compute > compute_limit
        || total_human_effort > human_effort_limit;
    let overrun_phase = if overrun {
        per_phase_costs.iter().find_map(|summary| {
            let phase_over = summary.unmeasured
                || summary.token_cost > token_limit
                || summary.call_count > call_limit
                || summary.provider_cost_microunits > provider_cost_limit
                || summary.wall_clock_milliseconds > wall_limit
                || summary.compute_milliseconds > compute_limit
                || summary.human_effort_milliseconds > human_effort_limit;
            phase_over.then_some(summary.phase)
        })
    } else {
        None
    };

    let (outcome, terminal_reason) = if overrun {
        (
            LifecycleBudgetReconciliationOutcome::OverrunStopped,
            Some(CandidateTerminalReason::RejectedLifecycleBudgetOverrun),
        )
    } else if terminal_state == LifecycleBudgetTerminalState::Cancelled {
        (
            LifecycleBudgetReconciliationOutcome::CancelledReleased,
            None,
        )
    } else {
        (LifecycleBudgetReconciliationOutcome::WithinEnvelope, None)
    };

    let rec = LifecycleBudgetReconciliationV1 {
        schema_version: LIFECYCLE_BUDGET_RECONCILIATION_SCHEMA.to_string(),
        reconciliation_id: String::new(),
        reservation_id: reservation.reservation_id.clone(),
        candidate_id: reservation.candidate_id.clone(),
        contract_id: contract.contract_id.clone(),
        total_token_cost: total_tokens,
        total_call_count: total_calls,
        total_provider_cost_microunits: total_provider_cost,
        total_wall_clock_milliseconds: total_wall_clock,
        total_compute_milliseconds: total_compute,
        total_human_effort_milliseconds: total_human_effort,
        total_failure_attempts: total_failures,
        terminal_state,
        per_phase_costs,
        outcome,
        overrun_phase,
        terminal_reason,
        record_sha256: String::new(),
    };

    seal_lifecycle_budget_reconciliation(rec)
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

pub fn sample_ec3_budget_contract() -> Ec3LifecycleBudgetContractV1 {
    let source_semantics = vec![
        CostTrustSource::MeasuredDirect,
        CostTrustSource::DerivedDeterministic,
        CostTrustSource::Unavailable,
    ];
    let phase_policies = REQUIRED_LIFECYCLE_COST_PHASES
        .iter()
        .map(|phase| LifecyclePhasePolicy {
            phase: *phase,
            required_dimensions: REQUIRED_LIFECYCLE_COST_DIMENSIONS.to_vec(),
            source_semantics: source_semantics.clone(),
            incomplete_cost_disposition: IncompleteCostDisposition::CandidateIneligible,
        })
        .collect();
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
            resource_limits: limits(100_000),
            max_repair_attempts: 2,
            max_ci_runs: 2,
            max_recovery_attempts: 1,
        },
        global_envelope: GlobalLifecycleEnvelope {
            scope: EnvelopeScope::AggregateAcrossCandidates,
            resource_limits: limits(1_000_000),
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
                resource_limits: limits(100_000),
                max_repair_attempts: 2,
                max_ci_runs: 2,
                max_recovery_attempts: 1,
            },
            global_envelope: GlobalLifecycleEnvelope {
                scope: EnvelopeScope::AggregateAcrossCandidates,
                resource_limits: limits(1_000_000),
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

        let mut unavailable_omitted = sample_ec3_lifecycle_budget_contract();
        unavailable_omitted.phase_policies[0]
            .source_semantics
            .retain(|source| *source != CostTrustSource::Unavailable);
        assert_eq!(
            seal_ec3_lifecycle_budget_contract(unavailable_omitted)
                .unwrap_err()
                .code,
            "ec3_source_semantics_incomplete"
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

    fn sample_ec3_lifecycle_cost_observation() -> LifecycleCostObservationV1 {
        seal_ec3_lifecycle_cost_observation(LifecycleCostObservationV1 {
            schema_version: String::new(),
            observation_key: "ec3-observation-1".to_string(),
            record_id: String::new(),
            contract_id: "hebc-example".to_string(),
            candidate_id: "hec-example".to_string(),
            evaluation_id: Some("hee-example".to_string()),
            product_task_id: None,
            run_id: Some("run-example".to_string()),
            attempt_id: "attempt-1".to_string(),
            phase: LifecycleCostPhase::Evaluation,
            dimension: LifecycleCostDimension::ModelTokens,
            amount: Some(42),
            trust_source: CostTrustSource::MeasuredDirect,
            terminal_class: "completed".to_string(),
            source_schema_version: "execution_usage.v1".to_string(),
            source_digest: sha256_hex("source"),
            redacted_body: json!({"source":"execution_usage","redacted":true}),
            record_sha256: String::new(),
        })
        .unwrap()
    }

    fn missing_for_phase_except(
        phase: LifecycleCostPhase,
        present: &[LifecycleCostDimension],
    ) -> Vec<LifecycleCostMissingnessV1> {
        REQUIRED_LIFECYCLE_COST_DIMENSIONS
            .iter()
            .copied()
            .filter(|dimension| !present.contains(dimension))
            .map(|dimension| LifecycleCostMissingnessV1 {
                phase,
                dimension,
                reason: LifecycleCostMissingReason::Unavailable,
            })
            .collect()
    }

    #[test]
    fn ec3_lifecycle_budget_contract_seals_and_validates() {
        let contract = sample_ec3_budget_contract();
        assert!(validate_ec3_lifecycle_budget_contract(&contract).is_ok());
        assert_eq!(contract.schema_version, EC3_LIFECYCLE_BUDGET_SCHEMA);
        assert!(!contract.contract_id.is_empty());
        assert!(!contract.record_sha256.is_empty());
    }

    #[test]
    fn ec3_lifecycle_cost_observation_seals_and_validates() {
        let observation = sample_ec3_lifecycle_cost_observation();
        assert!(validate_ec3_lifecycle_cost_observation(&observation).is_ok());
        assert!(observation.record_id.starts_with("helc-"));
    }

    #[test]
    fn ec3_lifecycle_cost_observation_rejects_unavailable_amount() {
        let mut observation = sample_ec3_lifecycle_cost_observation();
        observation.trust_source = CostTrustSource::Unavailable;
        observation.amount = Some(0);
        let error = seal_ec3_lifecycle_cost_observation(observation).unwrap_err();
        assert_eq!(error.code, "ec3_cost_unavailable_has_amount");
    }

    #[test]
    fn ec3_lifecycle_cost_observation_rejects_untrusted_or_tampered_evidence() {
        let mut observation = sample_ec3_lifecycle_cost_observation();
        observation.trust_source = CostTrustSource::CallerEstimate;
        let error = seal_ec3_lifecycle_cost_observation(observation).unwrap_err();
        assert_eq!(error.code, "ec3_cost_source_untrusted");

        let mut tampered = sample_ec3_lifecycle_cost_observation();
        tampered.amount = Some(43);
        let error = validate_ec3_lifecycle_cost_observation(&tampered).unwrap_err();
        assert_eq!(error.code, "ec3_cost_observation_conflict");
    }

    #[test]
    fn ec3_lifecycle_cost_normalization_enforces_canonical_units_and_missingness() {
        assert_eq!(
            normalize_ec3_lifecycle_cost_amount(
                LifecycleCostDimension::ProviderCostMicrounits,
                Some(12),
                "microunits",
            )
            .unwrap(),
            Some(12)
        );
        assert_eq!(
            normalize_ec3_lifecycle_cost_amount(
                LifecycleCostDimension::WallClockMilliseconds,
                None,
                "unavailable",
            )
            .unwrap(),
            None
        );
        assert_eq!(
            normalize_ec3_lifecycle_cost_amount(
                LifecycleCostDimension::ModelTokens,
                Some(1),
                "milliseconds",
            )
            .unwrap_err()
            .code,
            "ec3_cost_unit_invalid"
        );
        assert_eq!(
            normalize_ec3_lifecycle_cost_amount(
                LifecycleCostDimension::ModelTokens,
                Some(-1),
                "count",
            )
            .unwrap_err()
            .code,
            "ec3_cost_amount_invalid"
        );
    }

    #[test]
    fn ec3_usage_adapter_binds_canonical_source_identity_and_usage() {
        let event = crate::execution_usage::ExecutionUsageEventV1 {
            schema_version: crate::execution_usage::EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
            event_id: "usage-event-1".into(),
            product_task_id: Some("task-1".into()),
            workflow_node_id: None,
            managed_execution_id: Some("execution-1".into()),
            executor_kind: crate::execution_usage::ExecutorKind::ProviderProxy,
            evidence_source_kind: crate::execution_usage::EvidenceSourceKind::ProviderResponse,
            provider_id: Some("deepseek".into()),
            requested_model: Some("deepseek-chat".into()),
            resolved_model: Some("deepseek-chat".into()),
            executable_path_fingerprint: None,
            executable_version: None,
            executable_sha256: None,
            root_session_id: Some("session-1".into()),
            parent_session_id: None,
            request_or_message_id: Some("request-1".into()),
            input_tokens: 10,
            cached_input_tokens: 2,
            cache_creation_tokens: 0,
            output_tokens: 5,
            reasoning_output_tokens: 1,
            cumulative_task_tokens: Some(18),
            provider_reported_cost: Some(0.000123),
            locally_estimated_cost: None,
            cost_source: crate::execution_usage::CostSource::ProviderOrExecutorReported,
            pricing_table_version: None,
            timestamp: "2026-08-24T00:00:00Z".into(),
            event_completeness: crate::execution_usage::EventCompleteness::Complete,
            source_schema_version: "provider_response.v1".into(),
            stable_dedupe_identity: "dedupe-1".into(),
            provenance_refs: vec!["provider:deepseek".into()],
        };
        let observations = ec3_lifecycle_cost_observations_from_usage_event(
            "contract-1",
            "candidate-1",
            "attempt-1",
            LifecycleCostPhase::Evaluation,
            "completed",
            &event,
        )
        .unwrap();
        assert_eq!(observations.len(), 3);
        assert_eq!(observations[0].amount, Some(18));
        assert_eq!(observations[1].amount, Some(1));
        assert_eq!(observations[2].amount, Some(123));
        assert!(observations
            .iter()
            .all(|observation| observation.product_task_id.as_deref() == Some("task-1")));
        let ambiguous = ec3_lifecycle_cost_observations_from_usage_event_with_identity(
            "contract-1",
            "candidate-1",
            Some("evaluation-1"),
            Some("task-joined"),
            Some("run-joined"),
            "attempt-1",
            LifecycleCostPhase::Evaluation,
            "completed",
            &event,
        )
        .unwrap_err();
        assert_eq!(ambiguous.code, "ec3_cost_join_ambiguous");
        let joined = ec3_lifecycle_cost_observations_from_usage_event_with_identity(
            "contract-1",
            "candidate-1",
            Some("evaluation-1"),
            Some("task-1"),
            Some("run-joined"),
            "attempt-1",
            LifecycleCostPhase::Evaluation,
            "completed",
            &event,
        )
        .unwrap();
        assert!(joined.iter().all(|observation| {
            observation.evaluation_id.as_deref() == Some("evaluation-1")
                && observation.product_task_id.as_deref() == Some("task-1")
                && observation.run_id.as_deref() == Some("run-joined")
        }));
        let mut invalid = event;
        invalid.schema_version = "execution_usage_event.v0".into();
        assert_eq!(
            ec3_lifecycle_cost_observations_from_usage_event(
                "contract-1",
                "candidate-1",
                "attempt-1",
                LifecycleCostPhase::Evaluation,
                "completed",
                &invalid,
            )
            .unwrap_err()
            .code,
            "ec3_cost_source_schema_drift"
        );
    }

    #[test]
    fn ec3_lifecycle_cost_bundle_and_read_model_are_source_bound_and_redacted() {
        let observation = sample_ec3_lifecycle_cost_observation();
        let source_digest = observation.source_digest.clone();
        let bundle = seal_ec3_lifecycle_cost_bundle(LifecycleCostObservationBundleV1 {
            schema_version: String::new(),
            bundle_id: String::new(),
            contract_id: observation.contract_id.clone(),
            candidate_id: observation.candidate_id.clone(),
            evaluation_id: observation.evaluation_id.clone(),
            product_task_id: observation.product_task_id.clone(),
            run_id: observation.run_id.clone(),
            attempt_id: observation.attempt_id.clone(),
            source_digests: vec![source_digest],
            observations: vec![observation],
            missing: [
                missing_for_phase_except(
                    LifecycleCostPhase::Evaluation,
                    &[LifecycleCostDimension::ModelTokens],
                ),
                missing_for_phase_except(LifecycleCostPhase::Review, &[]),
            ]
            .into_iter()
            .flatten()
            .collect(),
            record_sha256: String::new(),
        })
        .unwrap();
        let model = build_ec3_lifecycle_cost_read_model(&bundle).unwrap();
        assert!(!model.complete);
        assert_eq!(model.observations.len(), 1);
        let encoded = serde_json::to_string(&model).unwrap();
        assert!(!encoded.contains("redacted"));
        assert!(!encoded.contains("\"source\":\"execution_usage\""));
    }

    #[test]
    fn ec3_lifecycle_cost_bundle_seals_unsealed_child_observations() {
        let sealed = sample_ec3_lifecycle_cost_observation();
        let mut child = sealed.clone();
        child.schema_version.clear();
        child.record_id.clear();
        child.record_sha256.clear();
        let bundle = seal_ec3_lifecycle_cost_bundle(LifecycleCostObservationBundleV1 {
            schema_version: String::new(),
            bundle_id: String::new(),
            contract_id: sealed.contract_id.clone(),
            candidate_id: sealed.candidate_id.clone(),
            evaluation_id: sealed.evaluation_id.clone(),
            product_task_id: sealed.product_task_id.clone(),
            run_id: sealed.run_id.clone(),
            attempt_id: sealed.attempt_id.clone(),
            source_digests: vec![sealed.source_digest.clone()],
            observations: vec![child],
            missing: missing_for_phase_except(
                LifecycleCostPhase::Evaluation,
                &[LifecycleCostDimension::ModelTokens],
            ),
            record_sha256: String::new(),
        })
        .unwrap();
        assert_eq!(bundle.observations[0].record_id, sealed.record_id);
        assert_eq!(bundle.observations[0].record_sha256, sealed.record_sha256);
    }

    #[test]
    fn ec3_lifecycle_cost_rejects_private_evidence_and_unproved_zero() {
        let mut private = sample_ec3_lifecycle_cost_observation();
        private.redacted_body = json!({"raw_prompt":"do not store"});
        let error = seal_ec3_lifecycle_cost_observation(private).unwrap_err();
        assert_eq!(error.code, "ec3_cost_private_evidence");

        let mut zero = sample_ec3_lifecycle_cost_observation();
        zero.amount = Some(0);
        zero.redacted_body = json!({});
        let error = seal_ec3_lifecycle_cost_observation(zero).unwrap_err();
        assert_eq!(error.code, "ec3_cost_zero_unproved");
    }

    #[test]
    fn lifecycle_budget_reservation_and_reconciliation_within_envelope() {
        let contract = sample_ec3_budget_contract();
        let reservation = seal_lifecycle_budget_reservation(LifecycleBudgetReservationV1 {
            schema_version: LIFECYCLE_BUDGET_RESERVATION_SCHEMA.to_string(),
            reservation_id: String::new(),
            candidate_id: "cand-ec3-100".to_string(),
            contract_id: contract.contract_id.clone(),
            reserved_token_cost: 100_000,
            reserved_call_count: 50,
            reserved_provider_cost_microunits: 300_000,
            reserved_wall_clock_milliseconds: 400_000,
            reserved_compute_milliseconds: 500_000,
            reserved_human_effort_milliseconds: 600_000,
            status: LifecycleBudgetReservationStatus::Active,
            record_sha256: String::new(),
        })
        .unwrap();

        assert!(validate_lifecycle_budget_reservation(&reservation).is_ok());

        let rec1 = seal_lifecycle_cost_record(LifecycleCostRecordV1 {
            schema_version: LIFECYCLE_COST_RECORD_SCHEMA.to_string(),
            record_id: String::new(),
            candidate_id: "cand-ec3-100".to_string(),
            phase: LifecycleCostPhase::Evaluation,
            token_cost: 5_000,
            call_count: 2,
            provider_cost_microunits: 3_000,
            wall_clock_milliseconds: 30_000,
            compute_milliseconds: 4_000,
            human_effort_milliseconds: 5_000,
            observed_dimensions: REQUIRED_LIFECYCLE_COST_DIMENSIONS.to_vec(),
            trust_source: CostTrustSource::MeasuredDirect,
            terminal_class: "completed".to_string(),
            unmeasured: false,
            failure_attempt: false,
            evidence_payload_digest: sha256_hex("eval_cost"),
            record_sha256: String::new(),
        })
        .unwrap();

        let rec2_failure = seal_lifecycle_cost_record(LifecycleCostRecordV1 {
            schema_version: LIFECYCLE_COST_RECORD_SCHEMA.to_string(),
            record_id: String::new(),
            candidate_id: "cand-ec3-100".to_string(),
            phase: LifecycleCostPhase::Repair,
            token_cost: 4_000,
            call_count: 2,
            provider_cost_microunits: 2_000,
            wall_clock_milliseconds: 40_000,
            compute_milliseconds: 3_000,
            human_effort_milliseconds: 4_000,
            observed_dimensions: REQUIRED_LIFECYCLE_COST_DIMENSIONS.to_vec(),
            trust_source: CostTrustSource::MeasuredDirect,
            terminal_class: "failed".to_string(),
            unmeasured: false,
            failure_attempt: true,
            evidence_payload_digest: sha256_hex("repair_failed"),
            record_sha256: String::new(),
        })
        .unwrap();

        let reconciliation =
            reconcile_candidate_lifecycle_costs(&contract, &reservation, &[rec1, rec2_failure])
                .unwrap();

        assert_eq!(
            reconciliation.outcome,
            LifecycleBudgetReconciliationOutcome::WithinEnvelope
        );
        assert_eq!(reconciliation.terminal_reason, None);
        assert_eq!(reconciliation.total_token_cost, 9_000);
        assert_eq!(reconciliation.total_call_count, 4);
        assert_eq!(reconciliation.total_provider_cost_microunits, 5_000);
        assert_eq!(reconciliation.total_wall_clock_milliseconds, 70_000);
        assert_eq!(reconciliation.total_compute_milliseconds, 7_000);
        assert_eq!(reconciliation.total_human_effort_milliseconds, 9_000);
        assert_eq!(reconciliation.total_failure_attempts, 1);
        assert!(validate_lifecycle_budget_reconciliation(&reconciliation).is_ok());
    }

    #[test]
    fn lifecycle_budget_reconciliation_stops_on_overrun() {
        let contract = sample_ec3_budget_contract();
        let reservation = seal_lifecycle_budget_reservation(LifecycleBudgetReservationV1 {
            schema_version: LIFECYCLE_BUDGET_RESERVATION_SCHEMA.to_string(),
            reservation_id: String::new(),
            candidate_id: "cand-ec3-overrun".to_string(),
            contract_id: contract.contract_id.clone(),
            reserved_token_cost: 100_000,
            reserved_call_count: 50,
            reserved_provider_cost_microunits: 300_000,
            reserved_wall_clock_milliseconds: 400_000,
            reserved_compute_milliseconds: 500_000,
            reserved_human_effort_milliseconds: 600_000,
            status: LifecycleBudgetReservationStatus::Active,
            record_sha256: String::new(),
        })
        .unwrap();

        // 120_000 tokens exceeds 100_000 total token limit
        let rec = seal_lifecycle_cost_record(LifecycleCostRecordV1 {
            schema_version: LIFECYCLE_COST_RECORD_SCHEMA.to_string(),
            record_id: String::new(),
            candidate_id: "cand-ec3-overrun".to_string(),
            phase: LifecycleCostPhase::CandidateMaterialization,
            token_cost: 120_000,
            call_count: 10,
            provider_cost_microunits: 10_000,
            wall_clock_milliseconds: 50_000,
            compute_milliseconds: 5_000,
            human_effort_milliseconds: 5_000,
            observed_dimensions: REQUIRED_LIFECYCLE_COST_DIMENSIONS.to_vec(),
            trust_source: CostTrustSource::MeasuredDirect,
            terminal_class: "completed".to_string(),
            unmeasured: false,
            failure_attempt: false,
            evidence_payload_digest: sha256_hex("huge_mat"),
            record_sha256: String::new(),
        })
        .unwrap();

        let reconciliation =
            reconcile_candidate_lifecycle_costs(&contract, &reservation, &[rec]).unwrap();

        assert_eq!(
            reconciliation.outcome,
            LifecycleBudgetReconciliationOutcome::OverrunStopped
        );
        assert_eq!(
            reconciliation.terminal_reason,
            Some(CandidateTerminalReason::RejectedLifecycleBudgetOverrun)
        );
    }

    fn mx1_product_run(status: ProductTaskStatus) -> ProductHarnessRunEvidence {
        let terminal = if status == ProductTaskStatus::Completed {
            let mut evidence = json!({
                "schema_version": "product_task_terminal_evidence.v2",
                "product_task_id": "mx1-task",
                "task_status": "completed",
                "workspace_scope_id": "mx1-workspace-scope",
                "source_revision": "0123456789abcdef0123456789abcdef01234567",
                "verification": {"trustworthy": true, "status": "evidence_recorded"},
                "usage": {"tokens": 17},
                "cost": {"microunits": 19},
                "cleanup": {"status": "complete"},
                "restart": {"status": "not_observed"},
                "recovery": {"status": "reconciled"},
                "raw_output": "never-projected",
                "content_sha256": null,
            });
            let digest = hex::encode(Sha256::digest(serde_json::to_vec(&evidence).unwrap()));
            evidence["content_sha256"] = Value::String(digest);
            Some(evidence)
        } else {
            None
        };
        crate::product_golden_path::project_product_harness_run(
            &json!({
                "task_id": "mx1-task",
                "status": status.as_str(),
                "workspace_id": "mx1-workspace-scope",
                "workspace_binding": {
                    "workspace_id": "mx1-workspace",
                    "workspace_path": "/private/mx1-workspace",
                    "source_revision": "0123456789abcdef0123456789abcdef01234567",
                    "allowed_paths": ["engine/src/harness_evolution.rs"]
                },
                "failure_code": if status == ProductTaskStatus::Failed {
                    Value::String("execution_failed".to_string())
                } else {
                    Value::Null
                },
                "failure_detail": if status == ProductTaskStatus::Failed {
                    Value::String("private process detail".to_string())
                } else {
                    Value::Null
                }
            }),
            terminal.as_ref(),
        )
        .unwrap()
    }

    #[test]
    fn mx1_manifest_is_exact_and_rejects_descriptor_drift() {
        let manifest = sample_mx1_descriptor_manifest();
        validate_mx1_descriptor_manifest(&manifest).unwrap();
        let duplicate = sample_mx1_descriptor_manifest();
        assert_eq!(manifest.manifest_sha256, duplicate.manifest_sha256);

        let mut drifted = manifest.clone();
        drifted.models[0].max_input_tokens = 1;
        assert_eq!(
            validate_mx1_descriptor_manifest(&drifted).unwrap_err().code,
            "mx1_model_budget_drift"
        );

        let mut arm_zero_drifted = manifest.clone();
        arm_zero_drifted
            .harnesses
            .iter_mut()
            .find(|descriptor| descriptor.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
            .unwrap()
            .source_identity = "f".repeat(40);
        assert_eq!(
            validate_mx1_descriptor_manifest(&arm_zero_drifted)
                .unwrap_err()
                .code,
            "mx1_harness_identity"
        );

        let mut endpoint_drifted = manifest.clone();
        endpoint_drifted.models[0].endpoint = "https://other.example/v1".to_string();
        endpoint_drifted.models[0].endpoint_allowlist =
            vec!["https://other.example/v1".to_string()];
        assert_eq!(
            seal_mx1_descriptor_manifest(endpoint_drifted)
                .unwrap_err()
                .code,
            "mx1_model_identity"
        );

        let mut confined_harness_drifted = manifest.clone();
        confined_harness_drifted
            .harnesses
            .iter_mut()
            .find(|descriptor| descriptor.descriptor_id == MX1_SECOND_HARNESS_ID)
            .unwrap()
            .process_confinement = "none".to_string();
        assert_eq!(
            seal_mx1_descriptor_manifest(confined_harness_drifted)
                .unwrap_err()
                .code,
            "mx1_harness_identity"
        );

        let mut strategy_source_drifted = manifest.clone();
        strategy_source_drifted.strategies[0].source_identity = "0".repeat(64);
        strategy_source_drifted.strategies[0].source_identity_sha256 =
            sha256_hex(&strategy_source_drifted.strategies[0].source_identity);
        assert_eq!(
            validate_mx1_descriptor_manifest(&strategy_source_drifted)
                .unwrap_err()
                .code,
            "mx1_strategy_identity"
        );
    }

    #[test]
    fn mx1_harness_adapters_share_the_product_golden_path_run_contract() {
        let manifest = sample_mx1_descriptor_manifest();
        let basis = sha256_hex("mx1-common-basis");
        let plan = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "mx1-task",
            1,
            &basis,
        )
        .unwrap();
        let arm_zero = manifest
            .harnesses
            .iter()
            .find(|descriptor| descriptor.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
            .unwrap()
            .clone();
        let second = manifest
            .harnesses
            .iter()
            .find(|descriptor| descriptor.descriptor_id != MX1_ARM_ZERO_HARNESS_ID)
            .unwrap()
            .clone();
        let evidence = mx1_product_run(ProductTaskStatus::Completed);
        let left_cell = plan
            .cells
            .iter()
            .find(|cell| cell.identity.harness_id == MX1_ARM_ZERO_HARNESS_ID)
            .unwrap();
        let right_cell = plan
            .cells
            .iter()
            .find(|cell| cell.identity.harness_id == MX1_SECOND_HARNESS_ID)
            .unwrap();
        let left = Mx1EngineManagedHarnessAdapter::new(arm_zero)
            .unwrap()
            .normalize_run(&plan, left_cell, &evidence)
            .unwrap();
        let right = Mx1ConfinedSubprocessHarnessAdapter::new(second)
            .unwrap()
            .normalize_run(&plan, right_cell, &evidence)
            .unwrap();
        assert_ne!(left.harness_id, right.harness_id);
        assert_eq!(left.product_task_id, right.product_task_id);
        assert_eq!(
            left.workspace_binding_sha256,
            right.workspace_binding_sha256
        );
        assert_eq!(left.terminal_outcome, right.terminal_outcome);
        assert_eq!(left.verified_deliverable, right.verified_deliverable);
        assert_eq!(left.usage, right.usage);
        assert_eq!(left.cost, right.cost);
        assert_eq!(left.cancellation, right.cancellation);
        assert_eq!(left.cleanup, right.cleanup);
        assert_eq!(left.restart, right.restart);
        assert_eq!(left.recovery, right.recovery);
        assert_eq!(left.failure, right.failure);
        assert!(!serde_json::to_string(&left).unwrap().contains("/private/"));
    }

    #[test]
    fn mx1_projection_leases_enforce_expiry_rebuild_deletion_and_arm_isolation() {
        let manifest = sample_mx1_descriptor_manifest();
        let strategy = manifest
            .strategies
            .iter()
            .find(|descriptor| descriptor.descriptor_id == MX1_MEMORY_ONLY_STRATEGY_ID)
            .unwrap()
            .clone();
        let adapter = Mx1StrategyAdapter::new(strategy.clone()).unwrap();
        let identity = Mx1CellIdentity {
            harness_id: MX1_ARM_ZERO_HARNESS_ID.to_string(),
            model_id: MX1_ARM_ZERO_MODEL_ID.to_string(),
            strategy_id: strategy.descriptor_id.clone(),
            task_id: "mx1-task-a".to_string(),
        };
        let mut lease = adapter.prepare_projection(&identity, 1).unwrap().unwrap();
        adapter.validate_projection(&lease, &identity, 1).unwrap();
        let rebuilt = adapter
            .rebuild_projection(
                &identity,
                &strategy.projection.as_ref().unwrap().content_sha256,
                1,
            )
            .unwrap();
        assert_eq!(rebuilt, lease);
        let cross_task = Mx1CellIdentity {
            task_id: "mx1-task-b".to_string(),
            ..identity.clone()
        };
        assert_eq!(
            adapter
                .validate_projection(&lease, &cross_task, 1)
                .unwrap_err()
                .code,
            "mx1_projection_cross_arm_or_source_drift"
        );
        adapter.delete_projection(&mut lease).unwrap();
        assert_eq!(
            adapter
                .validate_projection(&lease, &identity, 1)
                .unwrap_err()
                .code,
            "mx1_projection_deleted"
        );
        assert_eq!(
            adapter
                .prepare_projection(&identity, strategy.projection.unwrap().expires_at_unix_ms)
                .unwrap_err()
                .code,
            "mx1_projection_expired"
        );
    }

    #[test]
    fn mx1_matrix_is_deterministic_and_never_coerces_unsupported_or_unknown_cells() {
        let manifest = sample_mx1_descriptor_manifest();
        let basis = sha256_hex("mx1-common-basis");
        let plan = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "mx1-task",
            1,
            &basis,
        )
        .unwrap();
        let same_plan = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "mx1-task",
            1,
            &basis,
        )
        .unwrap();
        assert_eq!(plan, same_plan);
        assert_eq!(plan.cells.len(), 12);
        assert_eq!(
            build_mx1_matrix_plan(
                &manifest,
                Mx1MatrixRung::OneByTwoByOne,
                "mx1-task",
                1,
                &basis,
            )
            .unwrap()
            .cells
            .len(),
            2
        );
        assert_eq!(
            build_mx1_matrix_plan(
                &manifest,
                Mx1MatrixRung::OneByTwoByThree,
                "mx1-task",
                1,
                &basis,
            )
            .unwrap()
            .cells
            .len(),
            6
        );
        let mut reordered = plan.clone();
        reordered.cells.reverse();
        assert_eq!(
            validate_mx1_matrix_plan(&manifest, &reordered)
                .unwrap_err()
                .code,
            "mx1_matrix_plan_drift"
        );

        let arm_zero = manifest
            .harnesses
            .iter()
            .find(|descriptor| descriptor.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
            .unwrap()
            .clone();
        let target = plan
            .cells
            .iter()
            .find(|cell| {
                cell.identity.harness_id == MX1_ARM_ZERO_HARNESS_ID
                    && cell.disposition == Mx1MatrixCellDisposition::Admitted
            })
            .unwrap();
        let unknown = Mx1EngineManagedHarnessAdapter::new(arm_zero)
            .unwrap()
            .normalize_run(
                &plan,
                target,
                &mx1_product_run(ProductTaskStatus::OutcomeUnknown),
            )
            .unwrap();
        let projection = project_mx1_matrix_read_only(
            &manifest,
            &plan,
            &[Mx1MatrixObservation {
                cell_id: target.cell_id.clone(),
                normalized_run: unknown,
            }],
        )
        .unwrap();
        assert_eq!(
            projection
                .cells
                .iter()
                .find(|cell| cell.cell_id == target.cell_id)
                .unwrap()
                .disposition,
            Mx1ReadOnlyCellDisposition::Incomparable("outcome_unknown".to_string())
        );

        let mut descriptor_drift = Mx1EngineManagedHarnessAdapter::new(
            manifest
                .harnesses
                .iter()
                .find(|descriptor| descriptor.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
                .unwrap()
                .clone(),
        )
        .unwrap()
        .normalize_run(
            &plan,
            target,
            &mx1_product_run(ProductTaskStatus::Completed),
        )
        .unwrap();
        descriptor_drift.cell_descriptor_sha256 = sha256_hex("wrong-mx1-cell-descriptor");
        let projection = project_mx1_matrix_read_only(
            &manifest,
            &plan,
            &[Mx1MatrixObservation {
                cell_id: target.cell_id.clone(),
                normalized_run: descriptor_drift,
            }],
        )
        .unwrap();
        assert_eq!(
            projection
                .cells
                .iter()
                .find(|cell| cell.cell_id == target.cell_id)
                .unwrap()
                .disposition,
            Mx1ReadOnlyCellDisposition::Incomparable("descriptor_digest_mismatch".to_string())
        );

        let repetition_two = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "mx1-task",
            2,
            &basis,
        )
        .unwrap();
        let repetition_two_target = repetition_two
            .cells
            .iter()
            .find(|cell| cell.identity == target.identity)
            .unwrap();
        let replayed_repetition = Mx1EngineManagedHarnessAdapter::new(
            manifest
                .harnesses
                .iter()
                .find(|descriptor| descriptor.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
                .unwrap()
                .clone(),
        )
        .unwrap()
        .normalize_run(
            &plan,
            target,
            &mx1_product_run(ProductTaskStatus::Completed),
        )
        .unwrap();
        let projection = project_mx1_matrix_read_only(
            &manifest,
            &repetition_two,
            &[Mx1MatrixObservation {
                cell_id: repetition_two_target.cell_id.clone(),
                normalized_run: replayed_repetition,
            }],
        )
        .unwrap();
        assert_eq!(
            projection
                .cells
                .iter()
                .find(|cell| cell.cell_id == repetition_two_target.cell_id)
                .unwrap()
                .disposition,
            Mx1ReadOnlyCellDisposition::Incomparable("matrix_plan_identity_mismatch".to_string())
        );

        let different_basis = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "mx1-task",
            1,
            &sha256_hex("other-mx1-common-basis"),
        )
        .unwrap();
        let different_basis_target = different_basis
            .cells
            .iter()
            .find(|cell| cell.identity == target.identity)
            .unwrap();
        let replayed_basis = Mx1EngineManagedHarnessAdapter::new(
            manifest
                .harnesses
                .iter()
                .find(|descriptor| descriptor.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
                .unwrap()
                .clone(),
        )
        .unwrap()
        .normalize_run(
            &plan,
            target,
            &mx1_product_run(ProductTaskStatus::Completed),
        )
        .unwrap();
        let projection = project_mx1_matrix_read_only(
            &manifest,
            &different_basis,
            &[Mx1MatrixObservation {
                cell_id: different_basis_target.cell_id.clone(),
                normalized_run: replayed_basis,
            }],
        )
        .unwrap();
        assert_eq!(
            projection
                .cells
                .iter()
                .find(|cell| cell.cell_id == different_basis_target.cell_id)
                .unwrap()
                .disposition,
            Mx1ReadOnlyCellDisposition::Incomparable("matrix_plan_identity_mismatch".to_string())
        );

        let mut missing_terminal = mx1_product_run(ProductTaskStatus::Completed);
        missing_terminal.terminal_evidence_sha256 = None;
        missing_terminal.verified_deliverable = ProductHarnessEvidenceState::Unavailable;
        let missing_delivery = Mx1EngineManagedHarnessAdapter::new(
            manifest
                .harnesses
                .iter()
                .find(|descriptor| descriptor.descriptor_id == MX1_ARM_ZERO_HARNESS_ID)
                .unwrap()
                .clone(),
        )
        .unwrap()
        .normalize_run(&plan, target, &missing_terminal)
        .unwrap();
        let projection = project_mx1_matrix_read_only(
            &manifest,
            &plan,
            &[Mx1MatrixObservation {
                cell_id: target.cell_id.clone(),
                normalized_run: missing_delivery,
            }],
        )
        .unwrap();
        assert_eq!(
            projection
                .cells
                .iter()
                .find(|cell| cell.cell_id == target.cell_id)
                .unwrap()
                .disposition,
            Mx1ReadOnlyCellDisposition::Incomparable(
                "verified_delivery_evidence_missing".to_string()
            )
        );

        let mut unsupported = manifest.clone();
        unsupported
            .models
            .iter_mut()
            .find(|descriptor| descriptor.descriptor_id.contains("flash"))
            .unwrap()
            .supported_strategy_ids
            .retain(|strategy_id| !strategy_id.contains("skill-only"));
        let unsupported = seal_mx1_descriptor_manifest(unsupported).unwrap();
        let unsupported_plan = build_mx1_matrix_plan(
            &unsupported,
            Mx1MatrixRung::TwoByTwoByThree,
            "mx1-task",
            1,
            &basis,
        )
        .unwrap();
        assert!(unsupported_plan.cells.iter().any(|cell| {
            matches!(
                cell.disposition,
                Mx1MatrixCellDisposition::Incomparable(ref reason)
                    if reason == "unsupported_cross_product_cell"
            )
        }));
    }
}
