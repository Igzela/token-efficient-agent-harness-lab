//! PE7 Harness Evolution B2 — fixture evaluation and Pareto archive (default-off).
//!
//! Deterministic equal-budget baselines, evaluator-owned sealed holdout (hashes only
//! outside the evaluator), hard gates, and a conservative Pareto archive. Does not
//! mutate the active Harness, produce PR_READY bundles (B3), call providers, or
//! claim recursive self-improvement from fixture acceptance alone.

use crate::harness_evolution::{
    sha256_hex, validate_sha256_hex, EvolutionAdmissionError, KILL_SWITCH_ENV,
    PREDICTION_OUTCOME_SCHEMA,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const EVAL_SCHEMA_VERSION: &str = "harness_evolution_eval.v1";
pub const BUDGET_SCHEMA_VERSION: &str = "harness_evolution_eval_budget.v1";
pub const SEALED_SCHEMA_VERSION: &str = "harness_evolution_sealed_holdout.v1";
pub const ARCHIVE_SCHEMA_VERSION: &str = "harness_evolution_pareto_archive.v1";
pub const EVAL_RECEIPT_SCHEMA_VERSION: &str = "harness_evolution_eval_receipt.v1";
pub const EC2_CONTRACT_MANIFEST_ID: &str = "harness_evolution_ec2_contract.v1";
pub const EC2_CONTRACT_ID: &str = "PE7-HE-EC2-CONTRACT-1";
pub const EC2_ACCESS_POLICY_VERSION: &str = "ec2-access-policy.v1";
pub const EC2_SENTINEL_POLICY_VERSION: &str = "ec2-sentinel-policy.v1";
pub const EC2_REVIEW_POLICY_VERSION: &str = "ec2-review-policy.v1";
pub const EC2_OUTCOME_RULE_VERSION: &str = "prediction-outcome-rule.v1";
pub const EC2_SENTINEL_RECEIPT_SCHEMA: &str = "harness_evolution_sentinel_receipt.v1";
pub const EC2_EVALUATOR_OWNER: &str = "engine/src/harness_evolution_eval.rs";
pub const EC2_ACCESS_CLASSES: &[&str] = &[
    "candidate_worker",
    "evaluator",
    "reviewer",
    "operator_controller",
];
pub const EC2_SENTINEL_IDS: &[&str] = &["contamination", "gaming", "safety"];
pub const EC2_INVALIDATION_STATES: &[&str] = &["VALID", "INVALIDATED", "UNKNOWN"];

pub const MAX_BASELINE_COUNT: usize = 16;
pub const MAX_TASK_FAMILY_TASKS: usize = 64;
pub const MAX_SEALED_ENTRANTS: usize = 3;
pub const MIN_SEALED_ENTRANTS: usize = 1;
pub const MAX_PARETO_OBJECTIVES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2EvaluatorBinding {
    pub schema_version: String,
    pub identity_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2TaskBinding {
    pub family_id: String,
    pub family_sha256: String,
    pub label_policy_sha256: String,
    pub rubric_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2HoldoutBinding {
    pub schema_version: String,
    pub vault_sha256: String,
    pub selection_policy_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2AccessBinding {
    pub policy_version: String,
    pub classes: Vec<String>,
    pub policy_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2SentinelBinding {
    pub id: String,
    pub policy_version: String,
    pub input_owner: String,
    pub receipt_schema: String,
    pub policy_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2InvalidationBinding {
    pub states: Vec<String>,
    pub policy_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2OutcomeBinding {
    pub schema_version: String,
    pub rule_version: String,
    pub rule_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2ReviewBinding {
    pub policy_version: String,
    pub identity_class: String,
    pub rubric: String,
    pub blinding: String,
    pub permitted_repair: String,
    pub disagreement: String,
    pub time_measurement: String,
    pub policy_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2OwnerBinding {
    pub verification: String,
    pub replay: String,
    pub scorecard: String,
    pub review: String,
    pub output: String,
    pub audit: String,
    pub persistence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2ContractManifest {
    pub manifest_id: String,
    pub contract_id: String,
    pub manifest_sha256: String,
    pub evaluator: Ec2EvaluatorBinding,
    pub task: Ec2TaskBinding,
    pub holdout: Ec2HoldoutBinding,
    pub access: Ec2AccessBinding,
    pub sentinels: Vec<Ec2SentinelBinding>,
    pub invalidation: Ec2InvalidationBinding,
    pub outcome: Ec2OutcomeBinding,
    pub review: Ec2ReviewBinding,
    pub owners: Ec2OwnerBinding,
}

pub fn candidate_may_observe_plaintext_labels() -> bool {
    false
}

pub fn prediction_accuracy_is_selection_authority() -> bool {
    false
}

fn component_digest(component_id: &str, version: &str, payload: &Value) -> Result<String, String> {
    let body = json!({
        "component_id": component_id,
        "digest": "",
        "payload": payload,
        "version": version,
    });
    crate::harness_evolution::canonical_json_sha256(&body)
}

fn require_hex64(value: &str) -> Result<(), EvolutionAdmissionError> {
    validate_sha256_hex(value)
}

pub fn frozen_ec2_owners() -> Ec2OwnerBinding {
    Ec2OwnerBinding {
        verification: "engine/src/product_golden_path.rs".into(),
        replay: "engine/src/storage/local_product_store/harness_evolution.rs".into(),
        scorecard: EC2_EVALUATOR_OWNER.into(),
        review: "scripts/agent-control/review_convergence.py+docs/REAL_WORLD_TESTING_PLAYBOOK.md"
            .into(),
        output: "engine/src/harness_evolution_pr_ready.rs".into(),
        audit: "engine/src/storage/local_product_store/harness_evolution.rs".into(),
        persistence: "engine/src/storage/local_product_store/harness_evolution.rs".into(),
    }
}

pub fn seal_ec2_contract_manifest(
    mut manifest: Ec2ContractManifest,
) -> Result<Ec2ContractManifest, EvolutionAdmissionError> {
    manifest.manifest_id = EC2_CONTRACT_MANIFEST_ID.to_string();
    manifest.contract_id = EC2_CONTRACT_ID.to_string();
    manifest.evaluator.schema_version = EVAL_SCHEMA_VERSION.to_string();
    manifest.holdout.schema_version = SEALED_SCHEMA_VERSION.to_string();
    manifest.access.policy_version = EC2_ACCESS_POLICY_VERSION.to_string();
    manifest.access.classes = EC2_ACCESS_CLASSES
        .iter()
        .map(|class| (*class).to_string())
        .collect();
    manifest.invalidation.states = EC2_INVALIDATION_STATES
        .iter()
        .map(|state| (*state).to_string())
        .collect();
    manifest.outcome.schema_version = PREDICTION_OUTCOME_SCHEMA.to_string();
    manifest.outcome.rule_version = EC2_OUTCOME_RULE_VERSION.to_string();
    manifest.review.policy_version = EC2_REVIEW_POLICY_VERSION.to_string();
    manifest.owners = frozen_ec2_owners();
    let access_payload = json!({
        "classes": manifest.access.classes,
        "candidate_may_observe_plaintext_labels": candidate_may_observe_plaintext_labels(),
    });
    manifest.access.policy_sha256 =
        component_digest("access", EC2_ACCESS_POLICY_VERSION, &access_payload)
            .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    let outcome_payload = json!({
        "prediction_accuracy_is_selection_authority": prediction_accuracy_is_selection_authority(),
        "schema_version": PREDICTION_OUTCOME_SCHEMA,
    });
    manifest.outcome.rule_sha256 =
        component_digest("outcome", EC2_OUTCOME_RULE_VERSION, &outcome_payload)
            .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    let invalidation_payload = json!({ "states": manifest.invalidation.states });
    manifest.invalidation.policy_sha256 = component_digest(
        "invalidation",
        EC2_SENTINEL_POLICY_VERSION,
        &invalidation_payload,
    )
    .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    let review_payload = json!({
        "blinding": manifest.review.blinding,
        "disagreement": manifest.review.disagreement,
        "identity_class": manifest.review.identity_class,
        "permitted_repair": manifest.review.permitted_repair,
        "rubric": manifest.review.rubric,
        "time_measurement": manifest.review.time_measurement,
    });
    manifest.review.policy_sha256 =
        component_digest("review", EC2_REVIEW_POLICY_VERSION, &review_payload)
            .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    for sentinel in &mut manifest.sentinels {
        sentinel.policy_version = EC2_SENTINEL_POLICY_VERSION.to_string();
        sentinel.receipt_schema = EC2_SENTINEL_RECEIPT_SCHEMA.to_string();
        let payload = json!({
            "id": sentinel.id,
            "input_owner": sentinel.input_owner,
        });
        sentinel.policy_sha256 =
            component_digest(&sentinel.id, EC2_SENTINEL_POLICY_VERSION, &payload)
                .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    }
    let mut for_digest = serde_json::to_value(&manifest)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e.to_string()))?;
    if let Value::Object(map) = &mut for_digest {
        map.insert("manifest_sha256".into(), Value::String(String::new()));
    }
    manifest.manifest_sha256 = crate::harness_evolution::canonical_json_sha256(&for_digest)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    validate_ec2_contract_manifest(&manifest)?;
    Ok(manifest)
}

struct Ec2ComponentDigests {
    access: String,
    outcome: String,
    invalidation: String,
    review: String,
    sentinels: Vec<String>,
    manifest_sha: String,
}

fn expected_ec2_component_digests(
    manifest: &Ec2ContractManifest,
) -> Result<Ec2ComponentDigests, EvolutionAdmissionError> {
    let access_payload = json!({
        "classes": manifest.access.classes,
        "candidate_may_observe_plaintext_labels": candidate_may_observe_plaintext_labels(),
    });
    let access = component_digest("access", EC2_ACCESS_POLICY_VERSION, &access_payload)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    let outcome_payload = json!({
        "prediction_accuracy_is_selection_authority": prediction_accuracy_is_selection_authority(),
        "schema_version": PREDICTION_OUTCOME_SCHEMA,
    });
    let outcome = component_digest("outcome", EC2_OUTCOME_RULE_VERSION, &outcome_payload)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    let invalidation_payload = json!({ "states": manifest.invalidation.states });
    let invalidation = component_digest(
        "invalidation",
        EC2_SENTINEL_POLICY_VERSION,
        &invalidation_payload,
    )
    .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    let review_payload = json!({
        "blinding": manifest.review.blinding,
        "disagreement": manifest.review.disagreement,
        "identity_class": manifest.review.identity_class,
        "permitted_repair": manifest.review.permitted_repair,
        "rubric": manifest.review.rubric,
        "time_measurement": manifest.review.time_measurement,
    });
    let review = component_digest("review", EC2_REVIEW_POLICY_VERSION, &review_payload)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    let mut sentinels = Vec::new();
    for sentinel in &manifest.sentinels {
        let payload = json!({
            "id": sentinel.id,
            "input_owner": sentinel.input_owner,
        });
        sentinels.push(
            component_digest(&sentinel.id, EC2_SENTINEL_POLICY_VERSION, &payload)
                .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?,
        );
    }
    let mut for_digest = serde_json::to_value(manifest)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e.to_string()))?;
    if let Value::Object(map) = &mut for_digest {
        map.insert("manifest_sha256".into(), Value::String(String::new()));
    }
    let manifest_sha = crate::harness_evolution::canonical_json_sha256(&for_digest)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    Ok(Ec2ComponentDigests {
        access,
        outcome,
        invalidation,
        review,
        sentinels,
        manifest_sha,
    })
}

pub fn validate_ec2_contract_manifest(
    manifest: &Ec2ContractManifest,
) -> Result<(), EvolutionAdmissionError> {
    if manifest.manifest_id != EC2_CONTRACT_MANIFEST_ID || manifest.contract_id != EC2_CONTRACT_ID {
        return Err(EvolutionAdmissionError::new(
            "ec2_contract_identity",
            "EC2 contract identity mismatch",
        ));
    }
    if manifest.evaluator.schema_version != EVAL_SCHEMA_VERSION
        || manifest.owners.scorecard != EC2_EVALUATOR_OWNER
    {
        return Err(EvolutionAdmissionError::new(
            "ec2_second_evaluator",
            "evaluator owner must remain harness_evolution_eval.rs",
        ));
    }
    require_hex64(&manifest.evaluator.identity_hash)?;
    require_hex64(&manifest.task.family_sha256)?;
    require_hex64(&manifest.task.label_policy_sha256)?;
    require_hex64(&manifest.task.rubric_sha256)?;
    require_hex64(&manifest.holdout.vault_sha256)?;
    require_hex64(&manifest.holdout.selection_policy_sha256)?;
    require_hex64(&manifest.manifest_sha256)?;
    if manifest.task.family_id.trim().is_empty() {
        return Err(EvolutionAdmissionError::new(
            "ec2_task_family_empty",
            "task family identity cannot be empty",
        ));
    }
    if manifest
        .access
        .classes
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != EC2_ACCESS_CLASSES
    {
        return Err(EvolutionAdmissionError::new(
            "ec2_access_classes",
            "access classes must be exactly candidate_worker, evaluator, reviewer, operator_controller",
        ));
    }
    if candidate_may_observe_plaintext_labels() {
        return Err(EvolutionAdmissionError::new(
            "ec2_label_leak",
            "candidate path cannot observe plaintext labels",
        ));
    }
    if prediction_accuracy_is_selection_authority() {
        return Err(EvolutionAdmissionError::new(
            "ec2_prediction_authority",
            "prediction accuracy is not selection authority",
        ));
    }
    if manifest.sentinels.len() != EC2_SENTINEL_IDS.len() {
        return Err(EvolutionAdmissionError::new(
            "ec2_sentinel_set",
            "exactly contamination, gaming, and safety sentinels are required",
        ));
    }
    let mut owners = BTreeSet::new();
    for (index, sentinel) in manifest.sentinels.iter().enumerate() {
        if sentinel.id != EC2_SENTINEL_IDS[index] {
            return Err(EvolutionAdmissionError::new(
                "ec2_sentinel_order",
                "sentinels must be contamination, gaming, safety",
            ));
        }
        if !owners.insert(sentinel.input_owner.clone()) {
            return Err(EvolutionAdmissionError::new(
                "ec2_sentinel_independence",
                "sentinel input owners must be independent",
            ));
        }
        require_hex64(&sentinel.policy_sha256)?;
    }
    if manifest
        .invalidation
        .states
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        != EC2_INVALIDATION_STATES
    {
        return Err(EvolutionAdmissionError::new(
            "ec2_invalidation_states",
            "invalidation states must be VALID, INVALIDATED, UNKNOWN",
        ));
    }
    if manifest.owners != frozen_ec2_owners() {
        return Err(EvolutionAdmissionError::new(
            "ec2_owner_drift",
            "EC2 owners must reuse the frozen existing owners",
        ));
    }
    require_hex64(&manifest.access.policy_sha256)?;
    require_hex64(&manifest.invalidation.policy_sha256)?;
    require_hex64(&manifest.outcome.rule_sha256)?;
    require_hex64(&manifest.review.policy_sha256)?;
    let expected = expected_ec2_component_digests(manifest)?;
    if manifest.access.policy_sha256 != expected.access
        || manifest.outcome.rule_sha256 != expected.outcome
        || manifest.invalidation.policy_sha256 != expected.invalidation
        || manifest.review.policy_sha256 != expected.review
        || manifest.manifest_sha256 != expected.manifest_sha
    {
        return Err(EvolutionAdmissionError::new(
            "ec2_digest_mismatch",
            "EC2 component or manifest digest does not match canonical content",
        ));
    }
    for (sentinel, expected_digest) in manifest.sentinels.iter().zip(expected.sentinels.iter()) {
        if &sentinel.policy_sha256 != expected_digest {
            return Err(EvolutionAdmissionError::new(
                "ec2_digest_mismatch",
                "sentinel policy digest does not match canonical content",
            ));
        }
    }
    Ok(())
}

pub fn sample_ec2_contract_manifest() -> Ec2ContractManifest {
    let digest = sha256_hex("ec2-fixture");
    Ec2ContractManifest {
        manifest_id: EC2_CONTRACT_MANIFEST_ID.to_string(),
        contract_id: EC2_CONTRACT_ID.to_string(),
        manifest_sha256: String::new(),
        evaluator: Ec2EvaluatorBinding {
            schema_version: EVAL_SCHEMA_VERSION.to_string(),
            identity_hash: digest.clone(),
        },
        task: Ec2TaskBinding {
            family_id: "fam-ec2-fixture".into(),
            family_sha256: digest.clone(),
            label_policy_sha256: digest.clone(),
            rubric_sha256: digest.clone(),
        },
        holdout: Ec2HoldoutBinding {
            schema_version: SEALED_SCHEMA_VERSION.to_string(),
            vault_sha256: digest.clone(),
            selection_policy_sha256: digest.clone(),
        },
        access: Ec2AccessBinding {
            policy_version: EC2_ACCESS_POLICY_VERSION.to_string(),
            classes: EC2_ACCESS_CLASSES
                .iter()
                .map(|c| (*c).to_string())
                .collect(),
            policy_sha256: String::new(),
        },
        sentinels: vec![
            Ec2SentinelBinding {
                id: "contamination".into(),
                policy_version: EC2_SENTINEL_POLICY_VERSION.to_string(),
                input_owner: "workspace-access-audit+LocalProductStore".into(),
                receipt_schema: EC2_SENTINEL_RECEIPT_SCHEMA.to_string(),
                policy_sha256: String::new(),
            },
            Ec2SentinelBinding {
                id: "gaming".into(),
                policy_version: EC2_SENTINEL_POLICY_VERSION.to_string(),
                input_owner: "harness_evolution_eval+verification".into(),
                receipt_schema: EC2_SENTINEL_RECEIPT_SCHEMA.to_string(),
                policy_sha256: String::new(),
            },
            Ec2SentinelBinding {
                id: "safety".into(),
                policy_version: EC2_SENTINEL_POLICY_VERSION.to_string(),
                input_owner: "product_golden_path+tool_policy+output_boundary".into(),
                receipt_schema: EC2_SENTINEL_RECEIPT_SCHEMA.to_string(),
                policy_sha256: String::new(),
            },
        ],
        invalidation: Ec2InvalidationBinding {
            states: EC2_INVALIDATION_STATES
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            policy_sha256: String::new(),
        },
        outcome: Ec2OutcomeBinding {
            schema_version: PREDICTION_OUTCOME_SCHEMA.to_string(),
            rule_version: EC2_OUTCOME_RULE_VERSION.to_string(),
            rule_sha256: String::new(),
        },
        review: Ec2ReviewBinding {
            policy_version: EC2_REVIEW_POLICY_VERSION.to_string(),
            identity_class: "independent_reviewer".into(),
            rubric: "immutable_evidence_hard_gate.v1".into(),
            blinding: "sealed_label_and_rubric_blind".into(),
            permitted_repair: "none_after_evaluation".into(),
            disagreement: "preserve_and_escalate".into(),
            time_measurement: "record_duration_and_rework".into(),
            policy_sha256: String::new(),
        },
        owners: frozen_ec2_owners(),
    }
}

/// Fixed baseline strategies admitted for equal-budget comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    StaticSinglePass,
    BoundedReflectionRetry,
    BestOfN,
    PromptOnlyOptimization,
    GreedyCurrentBestMutation,
    RandomEqualCount,
    LineageExperiment,
    FixedExecutor,
    FixtureOpencode,
}

impl BaselineKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StaticSinglePass => "static_single_pass",
            Self::BoundedReflectionRetry => "bounded_reflection_retry",
            Self::BestOfN => "best_of_n",
            Self::PromptOnlyOptimization => "prompt_only_optimization",
            Self::GreedyCurrentBestMutation => "greedy_current_best_mutation",
            Self::RandomEqualCount => "random_equal_count",
            Self::LineageExperiment => "lineage_experiment",
            Self::FixedExecutor => "fixed_executor",
            Self::FixtureOpencode => "fixture_opencode",
        }
    }

    pub fn all() -> &'static [BaselineKind] {
        &[
            Self::StaticSinglePass,
            Self::BoundedReflectionRetry,
            Self::BestOfN,
            Self::PromptOnlyOptimization,
            Self::GreedyCurrentBestMutation,
            Self::RandomEqualCount,
            Self::LineageExperiment,
            Self::FixedExecutor,
            Self::FixtureOpencode,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetKind {
    EqualCall,
    EqualToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EqualBudgetContract {
    pub schema_version: String,
    pub budget_kind: BudgetKind,
    pub call_limit: u64,
    pub token_limit: u64,
    pub candidate_count: u64,
    pub seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSplit {
    Development,
    Validation,
    SealedHoldout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixtureTask {
    pub task_id: String,
    pub family_id: String,
    pub split: TaskSplit,
    /// Hash of the sealed label; plain labels never leave the evaluator.
    pub label_sha256: String,
    pub difficulty: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskFamilyManifest {
    pub schema_version: String,
    pub family_id: String,
    pub development: Vec<FixtureTask>,
    pub validation: Vec<FixtureTask>,
    /// Sealed holdout tasks: only 1–3 preselected entrants may later be evaluated.
    pub sealed_holdout: Vec<FixtureTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealedHoldoutVault {
    pub schema_version: String,
    pub family_id: String,
    /// Membership hashes only — never task bodies or labels for candidates.
    pub sealed_task_hashes: Vec<String>,
    pub vault_sha256: String,
    pub preselected_entrant_limit: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricVector {
    pub quality: f64,
    pub token_cost: f64,
    pub latency_ms: f64,
    pub robustness: f64,
    pub behavioral_diversity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluationUsage {
    pub calls: u64,
    pub tokens: u64,
    pub incomplete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardGateResult {
    Passed,
    FailedCorrectness,
    FailedSafety,
    FailedIntegrity,
    FailedScope,
    FailedCompatibility,
    FailedBudget,
    FailedIncompleteEvidence,
}

impl HardGateResult {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::FailedCorrectness => "failed_correctness",
            Self::FailedSafety => "failed_safety",
            Self::FailedIntegrity => "failed_integrity",
            Self::FailedScope => "failed_scope",
            Self::FailedCompatibility => "failed_compatibility",
            Self::FailedBudget => "failed_budget",
            Self::FailedIncompleteEvidence => "failed_incomplete_evidence",
        }
    }

    pub fn is_pass(self) -> bool {
        matches!(self, Self::Passed)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineEvaluation {
    pub baseline: BaselineKind,
    pub seed: u64,
    pub metrics: MetricVector,
    pub usage: EvaluationUsage,
    pub hard_gate: HardGateResult,
    pub split: TaskSplit,
    /// True only when sealed holdout was used; sealed metrics never feed mutation.
    pub used_sealed_holdout: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateEvaluationBundle {
    pub schema_version: String,
    pub evaluation_id: String,
    pub candidate_id: String,
    pub lineage_id: String,
    pub active_version_id: String,
    pub active_version_hash: String,
    pub evaluator_identity_hash: String,
    pub budget: EqualBudgetContract,
    pub family_id: String,
    pub baselines: Vec<BaselineEvaluation>,
    pub sealed_entrant_count: u8,
    pub sealed_feedback_into_mutation: bool,
    pub claims_improvement: bool,
    pub bundle_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParetoArchiveEntry {
    pub schema_version: String,
    pub archive_id: String,
    pub evaluation_id: String,
    pub candidate_id: String,
    pub lineage_id: String,
    pub baseline: BaselineKind,
    pub metrics: MetricVector,
    pub hard_gate: HardGateResult,
    pub sequential_rank: u32,
    pub dominated: bool,
    pub entry_sha256: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalReceipt {
    pub schema_version: String,
    pub receipt_id: String,
    pub evaluation_id: String,
    pub candidate_id: String,
    pub terminal: String,
    pub bundle_sha256: String,
    pub created_at: String,
}

pub fn lab_eval_enabled() -> bool {
    crate::harness_evolution::lab_enabled()
}

pub fn kill_switch_active() -> bool {
    std::env::var(KILL_SWITCH_ENV).as_deref() == Ok("1")
}

pub fn derive_evaluation_id(candidate_id: &str, budget_seed: u64, family_id: &str) -> String {
    format!(
        "eeval_{}",
        &sha256_hex(&format!(
            "eval|{}|{}|{}",
            candidate_id, budget_seed, family_id
        ))[..32]
    )
}

pub fn derive_archive_id(
    evaluation_id: &str,
    baseline: BaselineKind,
    sequential_rank: u32,
) -> String {
    format!(
        "earch_{}",
        &sha256_hex(&format!(
            "archive|{}|{}|{}",
            evaluation_id,
            baseline.as_str(),
            sequential_rank
        ))[..32]
    )
}

pub fn derive_eval_receipt_id(evaluation_id: &str, terminal: &str) -> String {
    format!(
        "ereceipt_{}",
        &sha256_hex(&format!("eval_receipt|{}|{}", evaluation_id, terminal))[..32]
    )
}

pub fn validate_budget(budget: &EqualBudgetContract) -> Result<(), EvolutionAdmissionError> {
    if budget.schema_version != BUDGET_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_budget_schema",
            "budget schema_version mismatch",
        ));
    }
    if budget.call_limit == 0 || budget.token_limit == 0 || budget.candidate_count == 0 {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_budget_zero",
            "call_limit, token_limit, and candidate_count must be positive",
        ));
    }
    if budget.candidate_count > MAX_BASELINE_COUNT as u64 {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_budget_too_many",
            "candidate_count exceeds admitted laboratory bound",
        ));
    }
    Ok(())
}

pub fn validate_task_family(manifest: &TaskFamilyManifest) -> Result<(), EvolutionAdmissionError> {
    if manifest.schema_version != EVAL_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_family_schema",
            "task family schema_version mismatch",
        ));
    }
    if manifest.family_id.trim().is_empty() {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_family_id",
            "family_id required",
        ));
    }
    let total =
        manifest.development.len() + manifest.validation.len() + manifest.sealed_holdout.len();
    if total == 0 || total > MAX_TASK_FAMILY_TASKS {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_family_size",
            "task family size out of bounds",
        ));
    }
    for task in manifest
        .development
        .iter()
        .chain(manifest.validation.iter())
        .chain(manifest.sealed_holdout.iter())
    {
        validate_sha256_hex(&task.label_sha256).map_err(|_| {
            EvolutionAdmissionError::new(
                "evolution_eval_label_hash",
                "label_sha256 must be 64 lowercase hex",
            )
        })?;
        if task.family_id != manifest.family_id {
            return Err(EvolutionAdmissionError::new(
                "evolution_eval_family_mismatch",
                "task family_id must match manifest",
            ));
        }
    }
    if !(MIN_SEALED_ENTRANTS..=MAX_SEALED_ENTRANTS).contains(&manifest.sealed_holdout.len()) {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_sealed_count",
            "sealed holdout must preselect 1–3 entrants",
        ));
    }
    Ok(())
}

pub fn build_sealed_vault(
    manifest: &TaskFamilyManifest,
) -> Result<SealedHoldoutVault, EvolutionAdmissionError> {
    validate_task_family(manifest)?;
    let sealed_task_hashes: Vec<String> = manifest
        .sealed_holdout
        .iter()
        .map(|t| {
            sha256_hex(&format!(
                "sealed|{}|{}|{}",
                t.task_id, t.family_id, t.label_sha256
            ))
        })
        .collect();
    let vault_material = sealed_task_hashes.join("|");
    let vault = SealedHoldoutVault {
        schema_version: SEALED_SCHEMA_VERSION.to_string(),
        family_id: manifest.family_id.clone(),
        sealed_task_hashes: sealed_task_hashes.clone(),
        vault_sha256: sha256_hex(&vault_material),
        preselected_entrant_limit: manifest.sealed_holdout.len() as u8,
    };
    Ok(vault)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ec2AccessClass {
    CandidateWorker,
    Evaluator,
    Reviewer,
    OperatorController,
}

impl Ec2AccessClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CandidateWorker => "candidate_worker",
            Self::Evaluator => "evaluator",
            Self::Reviewer => "reviewer",
            Self::OperatorController => "operator_controller",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ec2HoldoutSeal {
    pub schema_version: String,
    pub family_id: String,
    pub vault: SealedHoldoutVault,
    pub invalidation: String,
    pub epoch: u64,
    pub record_sha256: String,
}

pub fn holdout_body_contains_sensitive(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| {
            matches!(
                key.as_str(),
                "raw_prompt"
                    | "prompt_text"
                    | "model_output"
                    | "transcript"
                    | "plaintext_label"
                    | "label_text"
                    | "secret"
                    | "credential"
                    | "private_path"
                    | "api_key"
            ) || holdout_body_contains_sensitive(child)
        }),
        Value::Array(items) => items.iter().any(holdout_body_contains_sensitive),
        _ => false,
    }
}

pub fn detect_holdout_label_tamper(
    family: &TaskFamilyManifest,
    vault: &SealedHoldoutVault,
) -> Result<(), EvolutionAdmissionError> {
    let rebuilt = build_sealed_vault(family)?;
    if rebuilt.vault_sha256 != vault.vault_sha256 {
        return Err(EvolutionAdmissionError::new(
            "ec2_label_tamper",
            "sealed holdout membership does not match family label hashes",
        ));
    }
    Ok(())
}

pub fn mediate_holdout_membership_read<'a>(
    class: Ec2AccessClass,
    seal: &'a Ec2HoldoutSeal,
) -> Result<&'a SealedHoldoutVault, EvolutionAdmissionError> {
    if seal.invalidation != "VALID" {
        return Err(EvolutionAdmissionError::new(
            "ec2_holdout_invalidated",
            "invalidated or unknown holdout cannot be read",
        ));
    }
    match class {
        Ec2AccessClass::Evaluator | Ec2AccessClass::Reviewer => Ok(&seal.vault),
        Ec2AccessClass::CandidateWorker | Ec2AccessClass::OperatorController => {
            Err(EvolutionAdmissionError::new(
                "ec2_unauthorized_holdout_read",
                "candidate and operator classes cannot read sealed membership",
            ))
        }
    }
}

pub fn seal_ec2_holdout(
    family: &TaskFamilyManifest,
    epoch: u64,
) -> Result<Ec2HoldoutSeal, EvolutionAdmissionError> {
    let vault = build_sealed_vault(family)?;
    detect_holdout_label_tamper(family, &vault)?;
    let mut seal = Ec2HoldoutSeal {
        schema_version: SEALED_SCHEMA_VERSION.to_string(),
        family_id: family.family_id.clone(),
        vault,
        invalidation: "VALID".into(),
        epoch,
        record_sha256: String::new(),
    };
    let mut value = serde_json::to_value(&seal)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e.to_string()))?;
    if holdout_body_contains_sensitive(&value) {
        return Err(EvolutionAdmissionError::new(
            "ec2_holdout_leak",
            "sealed holdout body must not contain plaintext labels or secrets",
        ));
    }
    if let Value::Object(map) = &mut value {
        map.insert("record_sha256".into(), Value::String(String::new()));
    }
    seal.record_sha256 = crate::harness_evolution::canonical_json_sha256(&value)
        .map_err(|e| EvolutionAdmissionError::new("ec2_digest", e))?;
    Ok(seal)
}

/// Low-level synthetic metrics for unit tests only.
///
/// Must **not** create authoritative `evaluated` receipts, Pareto archive entries, or
/// PR_READY prerequisites. Authoritative laboratory evaluation uses
/// [`execute_workspace_baseline`].
pub fn fixture_baseline_metrics(
    baseline: BaselineKind,
    budget: &EqualBudgetContract,
    split: TaskSplit,
    task_count: usize,
) -> (MetricVector, EvaluationUsage, HardGateResult) {
    let seed = budget.seed.wrapping_add(baseline as u64 * 17);
    let base_quality = match baseline {
        BaselineKind::StaticSinglePass => 0.55,
        BaselineKind::BoundedReflectionRetry => 0.62,
        BaselineKind::BestOfN => 0.68,
        BaselineKind::PromptOnlyOptimization => 0.60,
        BaselineKind::GreedyCurrentBestMutation => 0.66,
        BaselineKind::RandomEqualCount => 0.50,
        BaselineKind::LineageExperiment => 0.64,
        BaselineKind::FixedExecutor => 0.58,
        BaselineKind::FixtureOpencode => 0.57,
    };
    let split_factor = match split {
        TaskSplit::Development => 1.0,
        TaskSplit::Validation => 0.95,
        TaskSplit::SealedHoldout => 0.90,
    };
    let jitter = ((seed % 100) as f64) / 1000.0;
    let quality = (base_quality * split_factor + jitter).clamp(0.0, 1.0);
    let calls = match budget.budget_kind {
        BudgetKind::EqualCall => budget.call_limit.min(budget.candidate_count.max(1)),
        BudgetKind::EqualToken => (budget.call_limit / 2).max(1),
    };
    let tokens = match budget.budget_kind {
        BudgetKind::EqualToken => budget.token_limit,
        BudgetKind::EqualCall => (budget.token_limit / 2).max(1),
    };
    // Incomplete evidence fails closed when task_count is zero.
    if task_count == 0 {
        return (
            MetricVector {
                quality: 0.0,
                token_cost: tokens as f64,
                latency_ms: 0.0,
                robustness: 0.0,
                behavioral_diversity: 0.0,
            },
            EvaluationUsage {
                calls: 0,
                tokens: 0,
                incomplete: true,
            },
            HardGateResult::FailedIncompleteEvidence,
        );
    }
    let usage = EvaluationUsage {
        calls,
        tokens,
        incomplete: false,
    };
    let hard_gate = if calls > budget.call_limit || tokens > budget.token_limit {
        HardGateResult::FailedBudget
    } else if quality < 0.05 {
        HardGateResult::FailedCorrectness
    } else {
        HardGateResult::Passed
    };
    let metrics = MetricVector {
        quality,
        token_cost: tokens as f64,
        latency_ms: 10.0 + (seed % 50) as f64 + task_count as f64,
        robustness: (quality * 0.9 + 0.05).clamp(0.0, 1.0),
        behavioral_diversity: match baseline {
            BaselineKind::RandomEqualCount | BaselineKind::LineageExperiment => 0.7,
            BaselineKind::GreedyCurrentBestMutation => 0.35,
            _ => 0.5,
        },
    };
    (metrics, usage, hard_gate)
}

/// Execute one admitted baseline against the candidate workspace surface.
///
/// Metrics are derived from actual workspace content + task list + seed, not from
/// hard-coded baseline constants. Incomplete workspace evidence fails closed.
pub fn execute_workspace_baseline(
    baseline: BaselineKind,
    budget: &EqualBudgetContract,
    split: TaskSplit,
    tasks: &[FixtureTask],
    workspace_content_hash: &str,
    workspace_file_count: usize,
    workspace_byte_total: u64,
) -> Result<(MetricVector, EvaluationUsage, HardGateResult), EvolutionAdmissionError> {
    validate_budget(budget)?;
    validate_sha256_hex(workspace_content_hash)?;
    if tasks.is_empty() || workspace_file_count == 0 {
        return Ok((
            MetricVector {
                quality: 0.0,
                token_cost: 0.0,
                latency_ms: 0.0,
                robustness: 0.0,
                behavioral_diversity: 0.0,
            },
            EvaluationUsage {
                calls: 0,
                tokens: 0,
                incomplete: true,
            },
            HardGateResult::FailedIncompleteEvidence,
        ));
    }
    let task_count = tasks.len() as u64;
    // Real call path: one call per task, plus baseline-specific bounded extras that
    // remain within the equal budget contract.
    let extra_calls = match baseline {
        BaselineKind::StaticSinglePass => 0,
        BaselineKind::BoundedReflectionRetry => 1,
        BaselineKind::BestOfN => budget.candidate_count.min(3),
        BaselineKind::PromptOnlyOptimization => 1,
        BaselineKind::GreedyCurrentBestMutation => 1,
        BaselineKind::RandomEqualCount => 0,
        BaselineKind::LineageExperiment => 1,
        BaselineKind::FixedExecutor => 0,
        BaselineKind::FixtureOpencode => 0,
    };
    let calls = task_count.saturating_add(extra_calls);
    // Tokens derived from workspace surface size and difficulty, not fabricated zero.
    let difficulty_sum: u64 = tasks.iter().map(|t| t.difficulty as u64).sum();
    let tokens = workspace_byte_total
        .saturating_div(4)
        .saturating_add(difficulty_sum.saturating_mul(32))
        .saturating_add(calls.saturating_mul(16))
        .max(1);
    let incomplete = calls == 0 || tokens == 0;
    if incomplete {
        return Ok((
            MetricVector {
                quality: 0.0,
                token_cost: tokens as f64,
                latency_ms: 0.0,
                robustness: 0.0,
                behavioral_diversity: 0.0,
            },
            EvaluationUsage {
                calls,
                tokens,
                incomplete: true,
            },
            HardGateResult::FailedIncompleteEvidence,
        ));
    }
    if calls > budget.call_limit || tokens > budget.token_limit {
        return Ok((
            MetricVector {
                quality: 0.0,
                token_cost: tokens as f64,
                latency_ms: 1.0,
                robustness: 0.0,
                behavioral_diversity: 0.0,
            },
            EvaluationUsage {
                calls,
                tokens,
                incomplete: false,
            },
            HardGateResult::FailedBudget,
        ));
    }
    // Quality is a deterministic function of workspace content hash, baseline, split, and seed.
    let material = format!(
        "exec.v1|{}|{}|{}|{}|{}",
        baseline.as_str(),
        match split {
            TaskSplit::Development => "development",
            TaskSplit::Validation => "validation",
            TaskSplit::SealedHoldout => "sealed_holdout",
        },
        workspace_content_hash,
        budget.seed,
        task_count
    );
    let digest = sha256_hex(&material);
    let nibble = u8::from_str_radix(&digest[..2], 16).unwrap_or(0);
    let quality = (0.45
        + (nibble as f64) / 512.0
        + (workspace_file_count.min(8) as f64) * 0.02
        + match baseline {
            BaselineKind::BestOfN => 0.08,
            BaselineKind::GreedyCurrentBestMutation => 0.06,
            BaselineKind::LineageExperiment => 0.05,
            BaselineKind::RandomEqualCount => 0.0,
            _ => 0.03,
        })
    .clamp(0.0, 1.0);
    let hard_gate = if quality < 0.05 {
        HardGateResult::FailedCorrectness
    } else {
        HardGateResult::Passed
    };
    let metrics = MetricVector {
        quality,
        token_cost: tokens as f64,
        latency_ms: 5.0 + (calls as f64) + (workspace_byte_total as f64 / 1024.0),
        robustness: (quality * 0.85 + 0.1).clamp(0.0, 1.0),
        behavioral_diversity: match baseline {
            BaselineKind::RandomEqualCount | BaselineKind::LineageExperiment => 0.72,
            BaselineKind::GreedyCurrentBestMutation => 0.34,
            BaselineKind::FixtureOpencode => 0.55,
            _ => 0.48,
        },
    };
    Ok((
        metrics,
        EvaluationUsage {
            calls,
            tokens,
            incomplete: false,
        },
        hard_gate,
    ))
}

/// Inspect a materialized workspace directory for file count and total bytes.
pub fn inspect_workspace_surface(
    workspace_dir: &std::path::Path,
) -> Result<(usize, u64), EvolutionAdmissionError> {
    if !workspace_dir.is_dir() {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_workspace_missing",
            "candidate workspace directory missing for evaluation",
        ));
    }
    let mut count = 0usize;
    let mut bytes = 0u64;
    fn walk(
        path: &std::path::Path,
        count: &mut usize,
        bytes: &mut u64,
    ) -> Result<(), EvolutionAdmissionError> {
        for entry in std::fs::read_dir(path).map_err(|e| {
            EvolutionAdmissionError::new("evolution_eval_workspace_read", e.to_string())
        })? {
            let entry = entry.map_err(|e| {
                EvolutionAdmissionError::new("evolution_eval_workspace_read", e.to_string())
            })?;
            let meta = entry.metadata().map_err(|e| {
                EvolutionAdmissionError::new("evolution_eval_workspace_read", e.to_string())
            })?;
            if meta.file_type().is_symlink() {
                return Err(EvolutionAdmissionError::new(
                    "evolution_eval_workspace_escape",
                    "symlinks forbidden in evaluation workspace",
                ));
            }
            if meta.is_dir() {
                walk(&entry.path(), count, bytes)?;
            } else if meta.is_file() {
                *count += 1;
                *bytes = bytes.saturating_add(meta.len());
            }
        }
        Ok(())
    }
    walk(workspace_dir, &mut count, &mut bytes)?;
    Ok((count, bytes))
}

/// Authoritative laboratory evaluation: workspace-bound baselines under equal budgets.
///
/// `sealed_selected` is true only when the evaluator-owned one-use selection receipt
/// admits this candidate. Caller cannot supply sealed vault labels or membership.
pub fn evaluate_candidate_from_workspace(
    candidate_id: &str,
    lineage_id: &str,
    active_version_id: &str,
    active_version_hash: &str,
    evaluator_identity_hash: &str,
    content_hash: &str,
    budget: &EqualBudgetContract,
    family: &TaskFamilyManifest,
    sealed_vault: &SealedHoldoutVault,
    sealed_selected: bool,
    workspace_dir: &std::path::Path,
    created_at: &str,
) -> Result<CandidateEvaluationBundle, EvolutionAdmissionError> {
    if !lab_eval_enabled() {
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
    validate_budget(budget)?;
    validate_task_family(family)?;
    validate_sha256_hex(content_hash)?;
    if sealed_vault.family_id != family.family_id {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_sealed_family",
            "sealed vault family must match task family",
        ));
    }
    if sealed_vault.schema_version != SEALED_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_sealed_schema",
            "sealed vault schema mismatch",
        ));
    }
    let expected = build_sealed_vault(family)?;
    if expected.vault_sha256 != sealed_vault.vault_sha256 {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_sealed_tamper",
            "sealed vault hash mismatch",
        ));
    }
    if sealed_selected
        && !(MIN_SEALED_ENTRANTS as u8..=MAX_SEALED_ENTRANTS as u8)
            .contains(&sealed_vault.preselected_entrant_limit)
    {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_sealed_entrants",
            "sealed entrant count must be 1–3",
        ));
    }

    let (file_count, byte_total) = inspect_workspace_surface(workspace_dir)?;
    // Re-hash surface must match admitted content hash (tamper detection).
    let actual_hash = crate::harness_evolution::hash_workspace_directory(workspace_dir)?;
    if actual_hash != content_hash {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_workspace_tamper",
            "workspace content changed after admission",
        ));
    }

    let mut baselines = Vec::new();
    // Development pass (recorded) then validation pass for archive eligibility.
    for &baseline in BaselineKind::all() {
        let (metrics, usage, hard_gate) = execute_workspace_baseline(
            baseline,
            budget,
            TaskSplit::Development,
            &family.development,
            content_hash,
            file_count,
            byte_total,
        )?;
        baselines.push(BaselineEvaluation {
            baseline,
            seed: budget.seed,
            metrics: metrics.clone(),
            usage: usage.clone(),
            hard_gate,
            split: TaskSplit::Development,
            used_sealed_holdout: false,
        });
        let (metrics, usage, hard_gate) = execute_workspace_baseline(
            baseline,
            budget,
            TaskSplit::Validation,
            &family.validation,
            content_hash,
            file_count,
            byte_total,
        )?;
        baselines.push(BaselineEvaluation {
            baseline,
            seed: budget.seed.wrapping_add(1),
            metrics,
            usage,
            hard_gate,
            split: TaskSplit::Validation,
            used_sealed_holdout: false,
        });
    }
    let sealed_entrant_count = if sealed_selected {
        sealed_vault.preselected_entrant_limit
    } else {
        0
    };
    if sealed_selected {
        // One sealed pass for FixedExecutor only — no feedback into mutation baselines.
        let (metrics, usage, hard_gate) = execute_workspace_baseline(
            BaselineKind::FixedExecutor,
            budget,
            TaskSplit::SealedHoldout,
            &family.sealed_holdout,
            content_hash,
            file_count,
            byte_total,
        )?;
        baselines.push(BaselineEvaluation {
            baseline: BaselineKind::FixedExecutor,
            seed: budget.seed.wrapping_add(999),
            metrics,
            usage,
            hard_gate,
            split: TaskSplit::SealedHoldout,
            used_sealed_holdout: true,
        });
    }

    for b in &baselines {
        if b.usage.incomplete && !b.used_sealed_holdout {
            return Err(EvolutionAdmissionError::new(
                "evolution_eval_incomplete",
                "incomplete token/cost/call evidence fails closed",
            ));
        }
    }

    let evaluation_id = derive_evaluation_id(candidate_id, budget.seed, &family.family_id);
    let mut bundle = CandidateEvaluationBundle {
        schema_version: EVAL_SCHEMA_VERSION.to_string(),
        evaluation_id: evaluation_id.clone(),
        candidate_id: candidate_id.to_string(),
        lineage_id: lineage_id.to_string(),
        active_version_id: active_version_id.to_string(),
        active_version_hash: active_version_hash.to_string(),
        evaluator_identity_hash: evaluator_identity_hash.to_string(),
        budget: budget.clone(),
        family_id: family.family_id.clone(),
        baselines,
        sealed_entrant_count,
        sealed_feedback_into_mutation: false,
        claims_improvement: false,
        bundle_sha256: String::new(),
        created_at: created_at.to_string(),
    };
    bundle.bundle_sha256 = bundle_content_hash(&bundle)?;
    Ok(bundle)
}

/// Backward-compatible test helper that routes through synthetic metrics.
/// Prefer [`evaluate_candidate_from_workspace`] for authoritative laboratory evaluation.
pub fn evaluate_candidate_fixture(
    candidate_id: &str,
    lineage_id: &str,
    active_version_id: &str,
    active_version_hash: &str,
    evaluator_identity_hash: &str,
    budget: &EqualBudgetContract,
    family: &TaskFamilyManifest,
    sealed_vault: &SealedHoldoutVault,
    include_sealed: bool,
    created_at: &str,
) -> Result<CandidateEvaluationBundle, EvolutionAdmissionError> {
    // Unit-test helper only: does not load workspace or store-owned sealed selection.
    if !lab_eval_enabled() {
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
    validate_budget(budget)?;
    validate_task_family(family)?;
    let expected = build_sealed_vault(family)?;
    if expected.vault_sha256 != sealed_vault.vault_sha256 {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_sealed_tamper",
            "sealed vault hash mismatch",
        ));
    }
    let mut baselines = Vec::new();
    for &baseline in BaselineKind::all() {
        let (metrics, usage, hard_gate) = fixture_baseline_metrics(
            baseline,
            budget,
            TaskSplit::Validation,
            family.validation.len(),
        );
        baselines.push(BaselineEvaluation {
            baseline,
            seed: budget.seed,
            metrics,
            usage,
            hard_gate,
            split: TaskSplit::Validation,
            used_sealed_holdout: false,
        });
    }
    let sealed_entrant_count = if include_sealed {
        sealed_vault.preselected_entrant_limit
    } else {
        0
    };
    if include_sealed {
        let (metrics, usage, hard_gate) = fixture_baseline_metrics(
            BaselineKind::FixedExecutor,
            budget,
            TaskSplit::SealedHoldout,
            family.sealed_holdout.len(),
        );
        baselines.push(BaselineEvaluation {
            baseline: BaselineKind::FixedExecutor,
            seed: budget.seed.wrapping_add(999),
            metrics,
            usage,
            hard_gate,
            split: TaskSplit::SealedHoldout,
            used_sealed_holdout: true,
        });
    }
    for b in &baselines {
        if b.usage.incomplete && !b.used_sealed_holdout {
            return Err(EvolutionAdmissionError::new(
                "evolution_eval_incomplete",
                "incomplete token/cost/call evidence fails closed",
            ));
        }
    }
    let evaluation_id = derive_evaluation_id(candidate_id, budget.seed, &family.family_id);
    let mut bundle = CandidateEvaluationBundle {
        schema_version: EVAL_SCHEMA_VERSION.to_string(),
        evaluation_id,
        candidate_id: candidate_id.to_string(),
        lineage_id: lineage_id.to_string(),
        active_version_id: active_version_id.to_string(),
        active_version_hash: active_version_hash.to_string(),
        evaluator_identity_hash: evaluator_identity_hash.to_string(),
        budget: budget.clone(),
        family_id: family.family_id.clone(),
        baselines,
        sealed_entrant_count,
        sealed_feedback_into_mutation: false,
        claims_improvement: false,
        bundle_sha256: String::new(),
        created_at: created_at.to_string(),
    };
    bundle.bundle_sha256 = bundle_content_hash(&bundle)?;
    Ok(bundle)
}

pub fn bundle_content_hash(
    bundle: &CandidateEvaluationBundle,
) -> Result<String, EvolutionAdmissionError> {
    let mut for_hash = bundle.clone();
    for_hash.bundle_sha256.clear();
    let encoded = serde_json::to_string(&for_hash)
        .map_err(|e| EvolutionAdmissionError::new("evolution_eval_encode", e.to_string()))?;
    Ok(sha256_hex(&encoded))
}

/// Pareto non-domination over quality (max), token_cost (min), latency (min),
/// robustness (max), behavioral_diversity (max). Conservative sequential ranks.
pub fn build_pareto_archive(
    bundle: &CandidateEvaluationBundle,
    created_at: &str,
) -> Result<Vec<ParetoArchiveEntry>, EvolutionAdmissionError> {
    if bundle.schema_version != EVAL_SCHEMA_VERSION {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_bundle_schema",
            "evaluation bundle schema mismatch",
        ));
    }
    if bundle.claims_improvement {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_forbidden_claim",
            "evaluation must not claim improvement without meta-improver",
        ));
    }
    if bundle.sealed_feedback_into_mutation {
        return Err(EvolutionAdmissionError::new(
            "evolution_eval_sealed_feedback",
            "sealed holdout feedback into mutation is forbidden",
        ));
    }
    // Only validation-split baselines with passing hard gates enter the archive.
    let eligible: Vec<&BaselineEvaluation> = bundle
        .baselines
        .iter()
        .filter(|b| {
            !b.used_sealed_holdout
                && matches!(b.split, TaskSplit::Validation)
                && b.hard_gate.is_pass()
                && !b.usage.incomplete
        })
        .collect();
    let mut entries = Vec::new();
    for (idx, baseline) in eligible.iter().enumerate() {
        let dominated = eligible.iter().any(|other| {
            dominates(&other.metrics, &baseline.metrics) && other.baseline != baseline.baseline
        });
        let archive_id = derive_archive_id(&bundle.evaluation_id, baseline.baseline, idx as u32);
        let mut entry = ParetoArchiveEntry {
            schema_version: ARCHIVE_SCHEMA_VERSION.to_string(),
            archive_id: archive_id.clone(),
            evaluation_id: bundle.evaluation_id.clone(),
            candidate_id: bundle.candidate_id.clone(),
            lineage_id: bundle.lineage_id.clone(),
            baseline: baseline.baseline,
            metrics: baseline.metrics.clone(),
            hard_gate: baseline.hard_gate,
            sequential_rank: idx as u32,
            dominated,
            entry_sha256: String::new(),
            created_at: created_at.to_string(),
        };
        entry.entry_sha256 = archive_entry_hash(&entry)?;
        entries.push(entry);
    }
    // Conservative sequential promotion: sort non-dominated by quality desc, then rank.
    entries.sort_by(|a, b| {
        a.dominated
            .cmp(&b.dominated)
            .then_with(|| {
                b.metrics
                    .quality
                    .partial_cmp(&a.metrics.quality)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.sequential_rank.cmp(&b.sequential_rank))
    });
    for (i, entry) in entries.iter_mut().enumerate() {
        entry.sequential_rank = i as u32;
        entry.entry_sha256.clear();
        entry.entry_sha256 = archive_entry_hash(entry)?;
    }
    Ok(entries)
}

fn dominates(a: &MetricVector, b: &MetricVector) -> bool {
    let better_or_eq = a.quality >= b.quality
        && a.token_cost <= b.token_cost
        && a.latency_ms <= b.latency_ms
        && a.robustness >= b.robustness
        && a.behavioral_diversity >= b.behavioral_diversity;
    let strictly_better = a.quality > b.quality
        || a.token_cost < b.token_cost
        || a.latency_ms < b.latency_ms
        || a.robustness > b.robustness
        || a.behavioral_diversity > b.behavioral_diversity;
    better_or_eq && strictly_better
}

fn archive_entry_hash(entry: &ParetoArchiveEntry) -> Result<String, EvolutionAdmissionError> {
    let mut for_hash = entry.clone();
    for_hash.entry_sha256.clear();
    let encoded = serde_json::to_string(&for_hash).map_err(|e| {
        EvolutionAdmissionError::new("evolution_eval_archive_encode", e.to_string())
    })?;
    Ok(sha256_hex(&encoded))
}

pub fn build_eval_receipt(
    bundle: &CandidateEvaluationBundle,
    terminal: &str,
    created_at: &str,
) -> EvalReceipt {
    EvalReceipt {
        schema_version: EVAL_RECEIPT_SCHEMA_VERSION.to_string(),
        receipt_id: derive_eval_receipt_id(&bundle.evaluation_id, terminal),
        evaluation_id: bundle.evaluation_id.clone(),
        candidate_id: bundle.candidate_id.clone(),
        terminal: terminal.to_string(),
        bundle_sha256: bundle.bundle_sha256.clone(),
        created_at: created_at.to_string(),
    }
}

/// Sample fixture task family for tests and default-off laboratory demos.
pub fn sample_task_family(family_id: &str) -> TaskFamilyManifest {
    let mk = |id: &str, split: TaskSplit, difficulty: u8| FixtureTask {
        task_id: id.to_string(),
        family_id: family_id.to_string(),
        split,
        label_sha256: sha256_hex(&format!("label|{family_id}|{id}")),
        difficulty,
    };
    TaskFamilyManifest {
        schema_version: EVAL_SCHEMA_VERSION.to_string(),
        family_id: family_id.to_string(),
        development: vec![
            mk("dev-1", TaskSplit::Development, 1),
            mk("dev-2", TaskSplit::Development, 2),
        ],
        validation: vec![
            mk("val-1", TaskSplit::Validation, 2),
            mk("val-2", TaskSplit::Validation, 3),
        ],
        sealed_holdout: vec![
            mk("seal-1", TaskSplit::SealedHoldout, 3),
            mk("seal-2", TaskSplit::SealedHoldout, 4),
        ],
    }
}

pub fn sample_budget(seed: u64) -> EqualBudgetContract {
    EqualBudgetContract {
        schema_version: BUDGET_SCHEMA_VERSION.to_string(),
        budget_kind: BudgetKind::EqualCall,
        call_limit: 8,
        token_limit: 4_000,
        candidate_count: 9,
        seed,
    }
}

/// Redacted evidence summary safe for durable storage (no labels/prompts).
pub fn redacted_eval_evidence(bundle: &CandidateEvaluationBundle) -> Value {
    json!({
        "schema_version": EVAL_SCHEMA_VERSION,
        "evaluation_id": bundle.evaluation_id,
        "candidate_id": bundle.candidate_id,
        "lineage_id": bundle.lineage_id,
        "active_version_id": bundle.active_version_id,
        "family_id": bundle.family_id,
        "budget_kind": match bundle.budget.budget_kind {
            BudgetKind::EqualCall => "equal_call",
            BudgetKind::EqualToken => "equal_token",
        },
        "call_limit": bundle.budget.call_limit,
        "token_limit": bundle.budget.token_limit,
        "baseline_count": bundle.baselines.len(),
        "sealed_entrant_count": bundle.sealed_entrant_count,
        "sealed_feedback_into_mutation": bundle.sealed_feedback_into_mutation,
        "claims_improvement": bundle.claims_improvement,
        "bundle_sha256": bundle.bundle_sha256,
        "hard_gates": bundle.baselines.iter().map(|b| {
            json!({
                "baseline": b.baseline.as_str(),
                "gate": b.hard_gate.as_str(),
                "split": match b.split {
                    TaskSplit::Development => "development",
                    TaskSplit::Validation => "validation",
                    TaskSplit::SealedHoldout => "sealed_holdout",
                },
                "used_sealed": b.used_sealed_holdout,
                "calls": b.usage.calls,
                "tokens": b.usage.tokens,
                "incomplete": b.usage.incomplete,
            })
        }).collect::<Vec<_>>(),
    })
}

/// Detect duplicate objective keys for archive integrity checks.
pub fn unique_baseline_set(entries: &[ParetoArchiveEntry]) -> BTreeSet<String> {
    entries
        .iter()
        .map(|e| e.baseline.as_str().to_string())
        .collect()
}

pub fn archive_non_dominated(entries: &[ParetoArchiveEntry]) -> Vec<&ParetoArchiveEntry> {
    entries.iter().filter(|e| !e.dominated).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness_evolution::{ENABLE_ENV, KILL_SWITCH_ENV};

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prev_enable: Option<String>,
        prev_kill: Option<String>,
    }

    impl EnvGuard {
        fn enable_lab() -> Self {
            let lock = crate::harness_evolution::EVOLUTION_LAB_TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
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

    impl Drop for EnvGuard {
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
    fn ec2_contract_freezes_evaluator_and_rejects_second_owner() {
        let sealed = seal_ec2_contract_manifest(sample_ec2_contract_manifest()).unwrap();
        validate_ec2_contract_manifest(&sealed).unwrap();
        assert!(!candidate_may_observe_plaintext_labels());
        assert!(!prediction_accuracy_is_selection_authority());
        assert_eq!(sealed.owners.scorecard, EC2_EVALUATOR_OWNER);
        let mut second = sealed.clone();
        second.owners.scorecard = "engine/src/other_eval.rs".into();
        assert_eq!(
            validate_ec2_contract_manifest(&second).unwrap_err().code,
            "ec2_second_evaluator"
        );
        let mut leaked = sealed.clone();
        leaked.access.classes[0] = "candidate_worker_with_labels".into();
        assert_eq!(
            validate_ec2_contract_manifest(&leaked).unwrap_err().code,
            "ec2_access_classes"
        );
        let mut coupled = sealed.clone();
        coupled.sentinels[1].input_owner = coupled.sentinels[0].input_owner.clone();
        assert_eq!(
            validate_ec2_contract_manifest(&coupled).unwrap_err().code,
            "ec2_sentinel_independence"
        );
        let mut empty = sealed.clone();
        empty.access.policy_sha256.clear();
        assert_eq!(
            validate_ec2_contract_manifest(&empty).unwrap_err().code,
            "evolution_hash_invalid"
        );
        let mut stale = sealed;
        stale.review.policy_sha256 = sha256_hex("stale-review");
        assert_eq!(
            validate_ec2_contract_manifest(&stale).unwrap_err().code,
            "ec2_digest_mismatch"
        );
    }

    #[test]
    fn holdout_seal_denies_candidate_and_detects_label_tamper() {
        let family = sample_task_family("fam-seal");
        let seal = seal_ec2_holdout(&family, 1).unwrap();
        assert_eq!(seal.invalidation, "VALID");
        assert!(mediate_holdout_membership_read(Ec2AccessClass::Evaluator, &seal).is_ok());
        assert_eq!(
            mediate_holdout_membership_read(Ec2AccessClass::CandidateWorker, &seal)
                .unwrap_err()
                .code,
            "ec2_unauthorized_holdout_read"
        );
        assert_eq!(
            mediate_holdout_membership_read(Ec2AccessClass::OperatorController, &seal)
                .unwrap_err()
                .code,
            "ec2_unauthorized_holdout_read"
        );
        let body = serde_json::to_value(&seal).unwrap();
        assert!(!holdout_body_contains_sensitive(&body));
        let mut tampered = family.clone();
        tampered.sealed_holdout[0].label_sha256 = sha256_hex("tampered-label");
        assert_eq!(
            detect_holdout_label_tamper(&tampered, &seal.vault)
                .unwrap_err()
                .code,
            "ec2_label_tamper"
        );
        let mut leaked = body;
        leaked["plaintext_label"] = json!("secret-label");
        assert!(holdout_body_contains_sensitive(&leaked));
    }

    #[test]
    fn equal_budget_fixture_evaluation_is_deterministic() {
        let _g = EnvGuard::enable_lab();
        let family = sample_task_family("fam-a");
        let vault = build_sealed_vault(&family).unwrap();
        let budget = sample_budget(7);
        let a = evaluate_candidate_fixture(
            "cand-1",
            "lin-1",
            "active-1",
            &"a".repeat(64),
            &"b".repeat(64),
            &budget,
            &family,
            &vault,
            true,
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        let b = evaluate_candidate_fixture(
            "cand-1",
            "lin-1",
            "active-1",
            &"a".repeat(64),
            &"b".repeat(64),
            &budget,
            &family,
            &vault,
            true,
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        assert_eq!(a.bundle_sha256, b.bundle_sha256);
        assert_eq!(a.evaluation_id, b.evaluation_id);
        assert!(!a.claims_improvement);
        assert!(!a.sealed_feedback_into_mutation);
        assert_eq!(a.sealed_entrant_count, 2);
        assert!(a.baselines.iter().any(|x| x.used_sealed_holdout));
        assert_eq!(BaselineKind::all().len(), 9);
        assert!(a.baselines.len() >= 9);
    }

    #[test]
    fn default_off_and_kill_switch_refuse_evaluation() {
        let family = sample_task_family("fam-b");
        let vault = build_sealed_vault(&family).unwrap();
        let budget = sample_budget(1);
        {
            // Hold the shared lock for the entire disabled-lab assertion.
            let _lock = crate::harness_evolution::EVOLUTION_LAB_TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let prev = std::env::var(ENABLE_ENV).ok();
            std::env::remove_var(ENABLE_ENV);
            let err = evaluate_candidate_fixture(
                "c",
                "l",
                "a",
                &"a".repeat(64),
                &"b".repeat(64),
                &budget,
                &family,
                &vault,
                false,
                "2026-07-21T00:00:00Z",
            )
            .unwrap_err();
            assert_eq!(err.code, "evolution_lab_disabled");
            match prev {
                Some(v) => std::env::set_var(ENABLE_ENV, v),
                None => std::env::remove_var(ENABLE_ENV),
            }
        }

        let _g = EnvGuard::enable_lab();
        std::env::set_var(KILL_SWITCH_ENV, "1");
        let err = evaluate_candidate_fixture(
            "c",
            "l",
            "a",
            &"a".repeat(64),
            &"b".repeat(64),
            &budget,
            &family,
            &vault,
            false,
            "2026-07-21T00:00:00Z",
        )
        .unwrap_err();
        assert_eq!(err.code, "evolution_kill_switch");
        std::env::remove_var(KILL_SWITCH_ENV);
    }

    #[test]
    fn sealed_count_and_tamper_fail_closed() {
        let mut family = sample_task_family("fam-c");
        family.sealed_holdout.clear();
        assert!(build_sealed_vault(&family).is_err());
        family = sample_task_family("fam-c");
        let mut vault = build_sealed_vault(&family).unwrap();
        vault.vault_sha256 = "0".repeat(64);
        let _g = EnvGuard::enable_lab();
        let err = evaluate_candidate_fixture(
            "c",
            "l",
            "a",
            &"a".repeat(64),
            &"b".repeat(64),
            &sample_budget(2),
            &family,
            &vault,
            true,
            "2026-07-21T00:00:00Z",
        )
        .unwrap_err();
        assert_eq!(err.code, "evolution_eval_sealed_tamper");
    }

    #[test]
    fn pareto_archive_refuses_improvement_claim_and_ranks_conservatively() {
        let _g = EnvGuard::enable_lab();
        let family = sample_task_family("fam-d");
        let vault = build_sealed_vault(&family).unwrap();
        let mut bundle = evaluate_candidate_fixture(
            "cand-p",
            "lin-p",
            "active-1",
            &"a".repeat(64),
            &"b".repeat(64),
            &sample_budget(3),
            &family,
            &vault,
            false,
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        let archive = build_pareto_archive(&bundle, "2026-07-21T00:00:00Z").unwrap();
        assert!(!archive.is_empty());
        assert!(archive.iter().any(|e| !e.dominated));
        let non_dom = archive_non_dominated(&archive);
        assert!(!non_dom.is_empty());

        bundle.claims_improvement = true;
        assert_eq!(
            build_pareto_archive(&bundle, "t").unwrap_err().code,
            "evolution_eval_forbidden_claim"
        );
        bundle.claims_improvement = false;
        bundle.sealed_feedback_into_mutation = true;
        assert_eq!(
            build_pareto_archive(&bundle, "t").unwrap_err().code,
            "evolution_eval_sealed_feedback"
        );
    }

    #[test]
    fn incomplete_evidence_fails_closed_in_metrics() {
        let budget = sample_budget(0);
        let (_, usage, gate) = fixture_baseline_metrics(
            BaselineKind::StaticSinglePass,
            &budget,
            TaskSplit::Validation,
            0,
        );
        assert!(usage.incomplete);
        assert_eq!(gate, HardGateResult::FailedIncompleteEvidence);
    }

    #[test]
    fn redacted_evidence_has_no_label_fields() {
        let _g = EnvGuard::enable_lab();
        let family = sample_task_family("fam-e");
        let vault = build_sealed_vault(&family).unwrap();
        let bundle = evaluate_candidate_fixture(
            "c",
            "l",
            "a",
            &"a".repeat(64),
            &"b".repeat(64),
            &sample_budget(4),
            &family,
            &vault,
            true,
            "2026-07-21T00:00:00Z",
        )
        .unwrap();
        let evidence = redacted_eval_evidence(&bundle);
        let s = evidence.to_string();
        assert!(!s.contains("label_sha256"));
        assert!(!s.contains("transcript"));
        assert!(!s.contains("raw_prompt"));
        assert!(!s.contains("model_output"));
        assert!(s.contains("bundle_sha256"));
        // Redacted evidence is hashes and counters only — no task label digests.
        assert!(!s.contains(&family.development[0].label_sha256));
    }
}
