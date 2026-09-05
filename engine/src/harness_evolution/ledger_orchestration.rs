//! Crate-local, provider-free candidate loop for deterministic harness research.
//!
//! This module is intentionally not a public runtime API. Its injected
//! controller, worker, and verifier traits are test/evaluation seams only; the
//! loop has no ProductStore, scheduler, provider, workspace, budget, effect,
//! or external recovery authority. `rollback_to_checkpoint` restores only an
//! in-memory candidate ledger after the owning effect layer has confirmed that
//! no unknown external outcome remains.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn sanitize_text(value: &str, field: &str, max_bytes: usize) -> Result<String, OrchestrationError> {
    let sanitized = crate::provider::redaction::redact_sensitive_patterns(value);
    if sanitized.len() > max_bytes {
        return Err(OrchestrationError::OversizedField {
            field: field.to_string(),
            current: sanitized.len(),
            max: max_bytes,
        });
    }
    Ok(sanitized)
}

/// Keep ledger prose as a bounded semantic summary rather than a transcript,
/// command result, or path-bearing payload. Digests and artifact references
/// carry the durable identity; this field is only for short human-readable
/// routing context.
fn sanitize_summary_text(
    value: &str,
    field: &str,
    max_bytes: usize,
) -> Result<String, OrchestrationError> {
    let sanitized = sanitize_text(value, field, max_bytes)?;
    let lower = sanitized.to_ascii_lowercase();
    let has_path_like_token = sanitized.split_whitespace().any(|token| {
        token.starts_with('/')
            || token.starts_with('\\')
            || token.starts_with("~/")
            || token.starts_with("file://")
            || token.contains('/')
            || token.contains('\\')
    });
    let has_transcript_marker = [
        "system:",
        "user:",
        "assistant:",
        "tool:",
        "stdout:",
        "stderr:",
        "traceback",
        "```",
    ]
    .iter()
    .any(|marker| lower.contains(marker));
    if sanitized.chars().any(char::is_control) || has_path_like_token || has_transcript_marker {
        return Err(OrchestrationError::InvalidControllerDecision(format!(
            "{field} must be a single-line semantic summary"
        )));
    }
    Ok(sanitized)
}

fn sanitize_optional_summary_text(
    value: Option<&str>,
    field: &str,
    max_bytes: usize,
) -> Result<Option<String>, OrchestrationError> {
    value
        .map(|item| sanitize_summary_text(item, field, max_bytes))
        .transpose()
}

fn validate_summary_text(
    value: &str,
    field: &str,
    max_bytes: usize,
) -> Result<(), OrchestrationError> {
    let sanitized = sanitize_summary_text(value, field, max_bytes)?;
    if sanitized != value {
        return Err(OrchestrationError::SecretDetected(field.to_string()));
    }
    Ok(())
}

fn validate_sanitized_text(
    value: &str,
    field: &str,
    max_bytes: usize,
) -> Result<(), OrchestrationError> {
    let sanitized = sanitize_text(value, field, max_bytes)?;
    if sanitized != value {
        return Err(OrchestrationError::SecretDetected(field.to_string()));
    }
    Ok(())
}

fn sanitize_optional_text(
    value: Option<&str>,
    field: &str,
    max_bytes: usize,
) -> Result<Option<String>, OrchestrationError> {
    value
        .map(|item| sanitize_text(item, field, max_bytes))
        .transpose()
}

fn is_canonical_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sanitize_digest(
    value: &str,
    field: &str,
    max_bytes: usize,
) -> Result<String, OrchestrationError> {
    let sanitized = sanitize_text(value, field, max_bytes.max(64))?;
    if sanitized.len() != 64 || !sanitized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(OrchestrationError::InvalidControllerDecision(format!(
            "{field} must be a canonical SHA-256 digest"
        )));
    }
    Ok(sanitized)
}

fn sanitize_optional_digest(
    value: Option<&str>,
    field: &str,
    max_bytes: usize,
) -> Result<Option<String>, OrchestrationError> {
    value
        .map(|item| sanitize_digest(item, field, max_bytes))
        .transpose()
}

fn validate_digest(value: &str, field: &str, max_bytes: usize) -> Result<(), OrchestrationError> {
    let sanitized = sanitize_text(value, field, max_bytes.max(64))?;
    if sanitized != value {
        return Err(OrchestrationError::SecretDetected(field.to_string()));
    }
    if sanitized.len() != 64 || !sanitized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(OrchestrationError::InvalidControllerDecision(format!(
            "{field} must be a canonical SHA-256 digest"
        )));
    }
    Ok(())
}

fn validate_artifact_reference(
    value: &str,
    max_bytes: usize,
) -> Result<String, OrchestrationError> {
    let sanitized = sanitize_text(value, "artifact_ref", max_bytes)?;
    if sanitized.is_empty()
        || sanitized.starts_with('/')
        || sanitized.starts_with('\\')
        || sanitized.contains('/')
        || sanitized.contains('\\')
        || sanitized.contains('\0')
        || sanitized.split(':').any(|part| part == "..")
    {
        return Err(OrchestrationError::InvalidArtifactReference(sanitized));
    }
    Ok(sanitized)
}

pub const LEDGER_ORCHESTRATED_SCHEMA_VERSION: &str = "harness_evolution_ledger_orchestration.v2";
/// Frozen identity of the last pre-contract-digest ledger schema. Records
/// carrying this version are only readable through the explicit
/// [`WorkingLedger::migrate_v1_record`] path; they are never silently
/// accepted as v2 records.
pub const LEDGER_ORCHESTRATED_SCHEMA_VERSION_V1: &str = "harness_evolution_ledger_orchestration.v1";
pub const DEFAULT_MAX_LEDGER_TASKS: usize = 32;
pub const DEFAULT_MAX_LEDGER_FINDINGS: usize = 64;
pub const DEFAULT_MAX_LEDGER_OBSERVATIONS: usize = 64;
pub const DEFAULT_MAX_LEDGER_ARTIFACT_REFS: usize = 256;
pub const DEFAULT_MAX_SUMMARY_BYTES: usize = 4096;
pub const DEFAULT_MAX_LEDGER_BYTES: usize = 128 * 1024;
pub const DEFAULT_MAX_ORCHESTRATION_ROUNDS: u32 = 30;
pub const DEFAULT_MAX_TASK_ATTEMPTS: u32 = 3;
pub const DEFAULT_MAX_NO_PROGRESS_ROUNDS: u32 = 2;
pub const DEFAULT_MAX_TRUNCATION_RECOVERIES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerOrchestratorConfig {
    pub max_tasks: usize,
    pub max_findings: usize,
    pub max_observations: usize,
    pub max_artifact_refs: usize,
    pub max_summary_bytes: usize,
    pub max_ledger_bytes: usize,
    pub max_orchestration_rounds: u32,
    pub max_task_attempts: u32,
    pub max_no_progress_rounds: u32,
    pub max_truncations: u32,
}

impl Default for LedgerOrchestratorConfig {
    fn default() -> Self {
        Self {
            max_tasks: DEFAULT_MAX_LEDGER_TASKS,
            max_findings: DEFAULT_MAX_LEDGER_FINDINGS,
            max_observations: DEFAULT_MAX_LEDGER_OBSERVATIONS,
            max_artifact_refs: DEFAULT_MAX_LEDGER_ARTIFACT_REFS,
            max_summary_bytes: DEFAULT_MAX_SUMMARY_BYTES,
            max_ledger_bytes: DEFAULT_MAX_LEDGER_BYTES,
            max_orchestration_rounds: DEFAULT_MAX_ORCHESTRATION_ROUNDS,
            max_task_attempts: DEFAULT_MAX_TASK_ATTEMPTS,
            max_no_progress_rounds: DEFAULT_MAX_NO_PROGRESS_ROUNDS,
            max_truncations: DEFAULT_MAX_TRUNCATION_RECOVERIES,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerTaskStatus {
    Pending,
    Selected,
    Running,
    Verified,
    Failed,
    Blocked,
    Superseded,
    OutcomeUnknown,
}

impl LedgerTaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Verified | Self::Failed | Self::Blocked | Self::Superseded | Self::OutcomeUnknown
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerTaskRecord {
    pub id: String,
    pub description: String,
    pub status: LedgerTaskStatus,
    pub result_digest: Option<String>,
    pub evidence_refs: Vec<String>,
    pub attempt_count: u32,
    #[serde(default)]
    pub attempt_id: Option<String>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerFinding {
    pub id: String,
    pub summary: String,
    pub source: String,
    pub related_task_id: Option<String>,
    pub evidence_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationOutcome {
    Pass,
    Fail,
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationObservation {
    pub round: u32,
    pub task_id: String,
    #[serde(default)]
    pub attempt_id: Option<String>,
    #[serde(default)]
    pub result_digest: Option<String>,
    pub outcome: VerificationOutcome,
    pub observation_summary: String,
    pub evidence_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkingLedger {
    pub schema_version: String,
    /// Canonical 64-hex SHA-256 identity of the orchestrated contract. This
    /// field is the identity only; it is never worker-readable prose. See
    /// [`WorkingLedger::contract_summary`] for optional semantic context.
    pub contract_digest: String,
    /// Bounded optional worker-readable semantic context for the contract.
    /// Separate from [`WorkingLedger::contract_digest`] by construction so one
    /// field is never overloaded with both jobs.
    #[serde(default)]
    pub contract_summary: Option<String>,
    pub current_plan: String,
    pub tasks: Vec<LedgerTaskRecord>,
    pub findings: Vec<LedgerFinding>,
    pub artifact_refs: Vec<String>,
    pub observations: Vec<VerificationObservation>,
    pub current_selected_task_id: Option<String>,
    pub progress_fingerprint: String,
    pub round_count: u32,
    pub replan_count: u32,
    pub truncation_count: u32,
    pub no_progress_count: u32,
    /// Persistent fail-closed fences survive ledger serialization and
    /// rehydration; an unknown worker outcome cannot be superseded.
    #[serde(default)]
    pub outcome_unknown_task_ids: Vec<String>,
    /// Digest of the last pre-attempt checkpoint and an explicit owner-facing
    /// rollback target. These are evidence only; ProductStore remains the
    /// owner of any external effect reconciliation.
    #[serde(default)]
    pub checkpoint_digest: Option<String>,
    #[serde(default)]
    pub rollback_target: Option<String>,
    /// Monotonic local generation used to prevent a rollback from recreating
    /// an attempt identity that may still arrive late from the old attempt.
    #[serde(default)]
    pub attempt_generation: u64,
    /// Restart-safe no-progress state. These fields are deliberately separate
    /// from the cumulative observation counter.
    #[serde(default)]
    pub no_progress_streak: u32,
    #[serde(default)]
    pub last_progress_fingerprint: Option<String>,
}

impl WorkingLedger {
    pub fn new(contract_digest: impl Into<String>, initial_plan: impl Into<String>) -> Self {
        let raw_contract =
            crate::provider::redaction::redact_sensitive_patterns(&contract_digest.into());
        let plan = crate::provider::redaction::redact_sensitive_patterns(&initial_plan.into());
        // Canonical contract identity is always a 64-hex SHA-256 digest. When
        // the caller supplies an already-canonical digest, it is adopted as
        // the identity with no separate summary; otherwise the digest is
        // derived from the supplied text and the text is preserved as bounded
        // worker-readable context.
        let (contract, summary) = if is_canonical_sha256(&raw_contract) {
            (raw_contract.clone(), None)
        } else {
            let digest = sha256_hex(&raw_contract);
            let summary = sanitize_summary_text(&raw_contract, "contract_summary", 512).ok();
            (digest, summary)
        };
        let fingerprint = compute_progress_fingerprint(None, None, None, &contract);
        Self {
            schema_version: LEDGER_ORCHESTRATED_SCHEMA_VERSION.to_string(),
            contract_digest: contract,
            contract_summary: summary,
            current_plan: plan,
            tasks: Vec::new(),
            findings: Vec::new(),
            artifact_refs: Vec::new(),
            observations: Vec::new(),
            current_selected_task_id: None,
            progress_fingerprint: fingerprint,
            round_count: 0,
            replan_count: 0,
            truncation_count: 0,
            no_progress_count: 0,
            outcome_unknown_task_ids: Vec::new(),
            checkpoint_digest: None,
            rollback_target: None,
            attempt_generation: 0,
            no_progress_streak: 0,
            last_progress_fingerprint: None,
        }
    }

    /// Explicit migration for serialized v1 records whose `contract_digest`
    /// carried arbitrary text. The legacy text is hashed into the canonical
    /// digest identity and moved into `contract_summary`; nothing is silently
    /// dual-purposed. v2 records deserialize directly and must not pass
    /// through this path.
    pub fn migrate_v1_record(raw: &str) -> Result<Self, OrchestrationError> {
        let mut value: serde_json::Value = serde_json::from_str(raw).map_err(|_| {
            OrchestrationError::InvalidControllerDecision(
                "ledger v1 migration requires valid JSON".to_string(),
            )
        })?;
        let object = value.as_object_mut().ok_or_else(|| {
            OrchestrationError::InvalidControllerDecision(
                "ledger v1 migration requires a JSON object".to_string(),
            )
        })?;
        match object.get("schema_version").and_then(|v| v.as_str()) {
            Some(version) if version == LEDGER_ORCHESTRATED_SCHEMA_VERSION_V1 => {}
            Some(LEDGER_ORCHESTRATED_SCHEMA_VERSION) | Some(_) | None => {
                return Err(OrchestrationError::InvalidControllerDecision(
                    "ledger v1 migration applies only to v1 schema records".to_string(),
                ))
            }
        }
        let legacy_contract = object
            .get("contract_digest")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let (contract, summary) = if is_canonical_sha256(&legacy_contract) {
            (legacy_contract, None)
        } else {
            let digest = sha256_hex(&legacy_contract);
            let summary = sanitize_summary_text(&legacy_contract, "contract_summary", 512).ok();
            (digest, summary)
        };
        object.insert(
            "schema_version".to_string(),
            serde_json::Value::String(LEDGER_ORCHESTRATED_SCHEMA_VERSION.to_string()),
        );
        object.insert(
            "contract_digest".to_string(),
            serde_json::Value::String(contract),
        );
        object.insert(
            "contract_summary".to_string(),
            summary
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        );
        let migrated: WorkingLedger = serde_json::from_value(value).map_err(|_| {
            OrchestrationError::InvalidControllerDecision(
                "ledger v1 record does not migrate to a valid v2 ledger".to_string(),
            )
        })?;
        migrated.validate_bounds(&LedgerOrchestratorConfig::default())?;
        Ok(migrated)
    }

    pub fn add_task(
        &mut self,
        id: &str,
        description: &str,
        config: &LedgerOrchestratorConfig,
    ) -> Result<(), OrchestrationError> {
        let task_id = sanitize_text(id, "task_id", config.max_summary_bytes)?;
        validate_task_id(&task_id)?;
        if self.tasks.iter().any(|t| t.id == task_id) {
            return Err(OrchestrationError::DuplicateTaskId(task_id));
        }
        if self.tasks.len() >= config.max_tasks {
            return Err(OrchestrationError::OversizedField {
                field: "tasks".to_string(),
                current: self.tasks.len() + 1,
                max: config.max_tasks,
            });
        }
        if description.len() > config.max_summary_bytes {
            return Err(OrchestrationError::OversizedField {
                field: "task_description".to_string(),
                current: description.len(),
                max: config.max_summary_bytes,
            });
        }
        let sanitized =
            sanitize_summary_text(description, "task_description", config.max_summary_bytes)?;
        self.tasks.push(LedgerTaskRecord {
            id: task_id,
            description: sanitized,
            status: LedgerTaskStatus::Pending,
            result_digest: None,
            evidence_refs: Vec::new(),
            attempt_count: 0,
            attempt_id: None,
            failure_reason: None,
        });
        if let Err(error) = self.validate_bounds(config) {
            self.tasks.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn add_finding(
        &mut self,
        mut finding: LedgerFinding,
        config: &LedgerOrchestratorConfig,
    ) -> Result<(), OrchestrationError> {
        if self.findings.len() >= config.max_findings {
            return Err(OrchestrationError::OversizedField {
                field: "findings".to_string(),
                current: self.findings.len() + 1,
                max: config.max_findings,
            });
        }
        if finding.summary.len() > config.max_summary_bytes {
            return Err(OrchestrationError::OversizedField {
                field: "finding_summary".to_string(),
                current: finding.summary.len(),
                max: config.max_summary_bytes,
            });
        }
        finding.id = sanitize_text(&finding.id, "finding_id", config.max_summary_bytes)?;
        if finding.id.is_empty() {
            return Err(OrchestrationError::InvalidControllerDecision(
                "finding id must not be empty".to_string(),
            ));
        }
        finding.source =
            sanitize_summary_text(&finding.source, "finding_source", config.max_summary_bytes)?;
        if finding.source.is_empty() {
            return Err(OrchestrationError::InvalidControllerDecision(
                "finding source must not be empty".to_string(),
            ));
        }
        finding.related_task_id = finding
            .related_task_id
            .as_deref()
            .map(|id| sanitize_text(id, "finding_related_task_id", config.max_summary_bytes))
            .transpose()?;
        if let Some(related_task_id) = &finding.related_task_id {
            validate_task_id(related_task_id)?;
        }
        finding.evidence_digest = sanitize_optional_digest(
            finding.evidence_digest.as_deref(),
            "finding_evidence_digest",
            config.max_summary_bytes,
        )?;
        finding.summary = sanitize_summary_text(
            &finding.summary,
            "finding_summary",
            config.max_summary_bytes,
        )?;
        // Stable novelty semantics: an identical finding (same identity,
        // content, task binding, and evidence) is already recorded, so
        // appending it again is not progress. Skip the duplicate without
        // consuming capacity; a genuinely new finding still appends.
        if self.findings.iter().any(|existing| {
            existing.id == finding.id
                && existing.summary == finding.summary
                && existing.source == finding.source
                && existing.related_task_id == finding.related_task_id
                && existing.evidence_digest == finding.evidence_digest
        }) {
            return Ok(());
        }
        self.findings.push(finding);
        if let Err(error) = self.validate_bounds(config) {
            self.findings.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn add_observation(
        &mut self,
        mut observation: VerificationObservation,
        config: &LedgerOrchestratorConfig,
    ) -> Result<(), OrchestrationError> {
        if self.observations.len() >= config.max_observations {
            return Err(OrchestrationError::OversizedField {
                field: "observations".to_string(),
                current: self.observations.len() + 1,
                max: config.max_observations,
            });
        }
        if observation.observation_summary.len() > config.max_summary_bytes {
            return Err(OrchestrationError::OversizedField {
                field: "observation_summary".to_string(),
                current: observation.observation_summary.len(),
                max: config.max_summary_bytes,
            });
        }
        observation.task_id = sanitize_text(
            &observation.task_id,
            "observation_task_id",
            config.max_summary_bytes,
        )?;
        if observation.task_id.is_empty() {
            return Err(OrchestrationError::InvalidControllerDecision(
                "observation task id must not be empty".to_string(),
            ));
        }
        validate_task_id(&observation.task_id)?;
        observation.observation_summary = sanitize_summary_text(
            &observation.observation_summary,
            "observation_summary",
            config.max_summary_bytes,
        )?;
        observation.evidence_digest = sanitize_optional_digest(
            observation.evidence_digest.as_deref(),
            "observation_evidence_digest",
            config.max_summary_bytes,
        )?;
        self.observations.push(observation);
        if let Err(error) = self.validate_bounds(config) {
            self.observations.pop();
            return Err(error);
        }
        Ok(())
    }

    pub fn get_task(&self, id: &str) -> Option<&LedgerTaskRecord> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn get_task_mut(&mut self, id: &str) -> Option<&mut LedgerTaskRecord> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn pending_task_ids(&self) -> Vec<String> {
        self.tasks
            .iter()
            .filter(|t| t.status == LedgerTaskStatus::Pending)
            .map(|t| t.id.clone())
            .collect()
    }

    pub fn all_verified(&self) -> bool {
        self.outcome_unknown_task_ids.is_empty()
            && !self.tasks.is_empty()
            && self.tasks.iter().all(|t| {
                t.status == LedgerTaskStatus::Superseded
                    || (t.status == LedgerTaskStatus::Verified
                        && self.verified_task_has_pass_evidence(t))
            })
            && self
                .tasks
                .iter()
                .any(|t| t.status == LedgerTaskStatus::Verified)
    }

    fn verified_task_has_pass_evidence(&self, task: &LedgerTaskRecord) -> bool {
        task.result_digest
            .as_deref()
            .is_some_and(|digest| !digest.is_empty())
            && self.observations.iter().any(|observation| {
                observation.task_id == task.id
                    && task.attempt_id.is_some()
                    && observation.attempt_id.as_deref() == task.attempt_id.as_deref()
                    && observation.result_digest.as_deref() == task.result_digest.as_deref()
                    && observation.outcome == VerificationOutcome::Pass
                    && observation
                        .evidence_digest
                        .as_deref()
                        .is_some_and(|digest| !digest.is_empty())
            })
    }

    pub fn validate_bounds(
        &self,
        config: &LedgerOrchestratorConfig,
    ) -> Result<(), OrchestrationError> {
        if self.schema_version != LEDGER_ORCHESTRATED_SCHEMA_VERSION {
            return Err(OrchestrationError::InvalidControllerDecision(
                "working ledger schema version mismatch".to_string(),
            ));
        }
        // Contract identity is a canonical 64-hex SHA-256 digest, never
        // arbitrary text. Legacy v1 text records must pass through the
        // explicit migration path; forged or malformed digests fail closed.
        if !is_canonical_sha256(&self.contract_digest) {
            return Err(OrchestrationError::InvalidControllerDecision(
                "contract_digest must be a canonical SHA-256 digest".to_string(),
            ));
        }
        // The summary is created under a fixed 512-byte cap, independent of
        // the prose bound used for task content.
        if let Some(summary) = &self.contract_summary {
            validate_summary_text(
                summary,
                "contract_summary",
                config.max_summary_bytes.max(512),
            )?;
        }
        if self.tasks.len() > config.max_tasks {
            return Err(OrchestrationError::OversizedField {
                field: "tasks".to_string(),
                current: self.tasks.len(),
                max: config.max_tasks,
            });
        }
        if self.findings.len() > config.max_findings {
            return Err(OrchestrationError::OversizedField {
                field: "findings".to_string(),
                current: self.findings.len(),
                max: config.max_findings,
            });
        }
        if self.observations.len() > config.max_observations {
            return Err(OrchestrationError::OversizedField {
                field: "observations".to_string(),
                current: self.observations.len(),
                max: config.max_observations,
            });
        }
        if self.artifact_refs.len() > config.max_artifact_refs {
            return Err(OrchestrationError::OversizedField {
                field: "artifact_refs".to_string(),
                current: self.artifact_refs.len(),
                max: config.max_artifact_refs,
            });
        }
        if self.outcome_unknown_task_ids.len() > config.max_tasks {
            return Err(OrchestrationError::OversizedField {
                field: "outcome_unknown_task_ids".to_string(),
                current: self.outcome_unknown_task_ids.len(),
                max: config.max_tasks,
            });
        }
        let mut fenced_ids = std::collections::BTreeSet::new();
        for task_id in &self.outcome_unknown_task_ids {
            validate_task_id(task_id)?;
            validate_sanitized_text(task_id, "outcome_unknown_task_id", config.max_summary_bytes)?;
            if !fenced_ids.insert(task_id) {
                return Err(OrchestrationError::InvalidControllerDecision(
                    "duplicate outcome-unknown task fence".to_string(),
                ));
            }
            let task = self
                .get_task(task_id)
                .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.clone()))?;
            if task.status != LedgerTaskStatus::OutcomeUnknown {
                return Err(OrchestrationError::InvalidControllerDecision(
                    "outcome-unknown fence must retain the task's outcome-unknown status"
                        .to_string(),
                ));
            }
        }
        if self.truncation_count > config.max_truncations {
            return Err(OrchestrationError::TruncationLimitExceeded(
                config.max_truncations,
            ));
        }
        if self.round_count > config.max_orchestration_rounds {
            return Err(OrchestrationError::MaxRoundsExceeded(
                config.max_orchestration_rounds,
            ));
        }
        if self.round_count.saturating_add(self.replan_count) > config.max_orchestration_rounds {
            return Err(OrchestrationError::MaxRoundsExceeded(
                config.max_orchestration_rounds,
            ));
        }
        if let Some(selected_task_id) = &self.current_selected_task_id {
            validate_task_id(selected_task_id)?;
            if self.get_task(selected_task_id).is_none() {
                return Err(OrchestrationError::TaskNotFound(selected_task_id.clone()));
            }
        }
        validate_digest(
            &self.progress_fingerprint,
            "progress_fingerprint",
            config.max_summary_bytes,
        )?;
        for task in &self.tasks {
            validate_task_id(&task.id)?;
            validate_sanitized_text(&task.id, "task_id", config.max_summary_bytes)?;
            if task.evidence_refs.len() > config.max_artifact_refs {
                return Err(OrchestrationError::OversizedField {
                    field: "task_evidence_refs".to_string(),
                    current: task.evidence_refs.len(),
                    max: config.max_artifact_refs,
                });
            }
            if task.attempt_count > config.max_task_attempts {
                return Err(OrchestrationError::InvalidControllerDecision(
                    "task attempt count exceeds configured maximum".to_string(),
                ));
            }
            if let Some(attempt_id) = &task.attempt_id {
                validate_digest(attempt_id, "task_attempt_id", config.max_summary_bytes)?;
            }
            validate_summary_text(
                &task.description,
                "task_description",
                config.max_summary_bytes,
            )?;
            if let Some(result_digest) = &task.result_digest {
                validate_digest(
                    result_digest,
                    "task_result_digest",
                    config.max_summary_bytes,
                )?;
            }
            if let Some(failure_reason) = &task.failure_reason {
                validate_summary_text(
                    failure_reason,
                    "task_failure_reason",
                    config.max_summary_bytes,
                )?;
            }
            if task.status == LedgerTaskStatus::Verified
                && !self.verified_task_has_pass_evidence(task)
            {
                return Err(OrchestrationError::InvalidControllerDecision(
                    "verified task lacks a result digest and passing verification evidence"
                        .to_string(),
                ));
            }
            for reference in &task.evidence_refs {
                let sanitized = validate_artifact_reference(reference, config.max_summary_bytes)?;
                if sanitized != *reference {
                    return Err(OrchestrationError::SecretDetected(
                        "task_evidence_ref".to_string(),
                    ));
                }
            }
        }
        for finding in &self.findings {
            validate_sanitized_text(&finding.id, "finding_id", config.max_summary_bytes)?;
            validate_summary_text(&finding.source, "finding_source", config.max_summary_bytes)?;
            validate_summary_text(
                &finding.summary,
                "finding_summary",
                config.max_summary_bytes,
            )?;
            if let Some(related_task_id) = &finding.related_task_id {
                validate_task_id(related_task_id)?;
                validate_sanitized_text(
                    related_task_id,
                    "finding_related_task_id",
                    config.max_summary_bytes,
                )?;
            }
            if let Some(evidence_digest) = &finding.evidence_digest {
                validate_digest(
                    evidence_digest,
                    "finding_evidence_digest",
                    config.max_summary_bytes,
                )?;
            }
        }
        for observation in &self.observations {
            validate_sanitized_text(
                &observation.task_id,
                "observation_task_id",
                config.max_summary_bytes,
            )?;
            validate_task_id(&observation.task_id)?;
            if self.get_task(&observation.task_id).is_none() {
                return Err(OrchestrationError::TaskNotFound(
                    observation.task_id.clone(),
                ));
            }
            if let Some(attempt_id) = &observation.attempt_id {
                validate_digest(
                    attempt_id,
                    "observation_attempt_id",
                    config.max_summary_bytes,
                )?;
            }
            if let Some(result_digest) = &observation.result_digest {
                validate_digest(
                    result_digest,
                    "observation_result_digest",
                    config.max_summary_bytes,
                )?;
            }
            validate_summary_text(
                &observation.observation_summary,
                "observation_summary",
                config.max_summary_bytes,
            )?;
            if let Some(evidence_digest) = &observation.evidence_digest {
                validate_digest(
                    evidence_digest,
                    "observation_evidence_digest",
                    config.max_summary_bytes,
                )?;
            }
        }
        // The contract identity digest is a fixed 64 bytes; prose length
        // bounds never apply to it (canonical form already enforced above).
        validate_digest(
            &self.contract_digest,
            "contract_digest",
            config.max_summary_bytes,
        )?;
        if let Some(checkpoint_digest) = &self.checkpoint_digest {
            validate_digest(
                checkpoint_digest,
                "checkpoint_digest",
                config.max_summary_bytes,
            )?;
        }
        if let Some(rollback_target) = &self.rollback_target {
            validate_sanitized_text(rollback_target, "rollback_target", config.max_summary_bytes)?;
        }
        if self.no_progress_streak > config.max_no_progress_rounds {
            return Err(OrchestrationError::NoProgressLimitExceeded(
                self.no_progress_streak,
            ));
        }
        if let Some(last_progress_fingerprint) = &self.last_progress_fingerprint {
            validate_digest(
                last_progress_fingerprint,
                "last_progress_fingerprint",
                config.max_summary_bytes,
            )?;
        }
        validate_summary_text(&self.current_plan, "current_plan", config.max_summary_bytes)?;
        for reference in &self.artifact_refs {
            let sanitized = validate_artifact_reference(reference, config.max_summary_bytes)?;
            if sanitized != *reference {
                return Err(OrchestrationError::SecretDetected(
                    "artifact_ref".to_string(),
                ));
            }
        }
        let bytes = self.estimate_bytes();
        if bytes > config.max_ledger_bytes {
            return Err(OrchestrationError::OversizedLedger {
                current: bytes,
                max: config.max_ledger_bytes,
            });
        }
        Ok(())
    }

    pub fn estimate_bytes(&self) -> usize {
        serde_json::to_vec(self)
            .expect("WorkingLedger serialization must remain infallible")
            .len()
    }

    pub fn state_digest(&self) -> String {
        let bytes =
            serde_json::to_vec(self).expect("WorkingLedger serialization must remain infallible");
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    pub fn progress_state_digest(&self) -> String {
        let progress_state = serde_json::json!({
            "contract_digest": self.contract_digest,
            "current_plan": self.current_plan,
            "tasks": self.tasks.iter().map(|task| serde_json::json!({
                "id": task.id,
                "description": task.description,
                "status": task.status,
                "result_digest": task.result_digest,
                "evidence_refs": task.evidence_refs,
            })).collect::<Vec<_>>(),
            "findings": self.findings,
            "artifact_refs": self.artifact_refs,
        });
        let bytes = serde_json::to_vec(&progress_state)
            .expect("WorkingLedger progress serialization must remain infallible");
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    /// Digest only the evidence-bearing completion state. Runtime counters,
    /// selection state, and controller prose cannot change the deliverable
    /// identity after the last verified task is recorded.
    pub fn deliverable_digest(&self) -> String {
        let deliverable_state = serde_json::json!({
            "contract_digest": self.contract_digest,
            "current_plan": self.current_plan,
            "tasks": self.tasks,
            "findings": self.findings,
            "artifact_refs": self.artifact_refs,
            "observations": self.observations,
        });
        let bytes = serde_json::to_vec(&deliverable_state)
            .expect("WorkingLedger deliverable serialization must remain infallible");
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }
}

pub fn validate_task_id(id: &str) -> Result<(), OrchestrationError> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(OrchestrationError::InvalidTaskId(
            crate::provider::redaction::redact_sensitive_patterns(id),
        ));
    }
    Ok(())
}

pub fn compute_progress_fingerprint(
    task_id: Option<&str>,
    result_digest: Option<&str>,
    verif_digest: Option<&str>,
    ledger_state_digest: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(task_id.unwrap_or("none").as_bytes());
    hasher.update(b"|");
    hasher.update(result_digest.unwrap_or("none").as_bytes());
    hasher.update(b"|");
    hasher.update(verif_digest.unwrap_or("none").as_bytes());
    hasher.update(b"|");
    hasher.update(ledger_state_digest.as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerContext {
    pub contract_digest: String,
    /// Optional worker-readable contract context; the authoritative identity
    /// remains [`WorkerContext::contract_digest`].
    #[serde(default)]
    pub contract_summary: Option<String>,
    pub plan_summary: String,
    pub selected_task: LedgerTaskRecord,
    pub relevant_findings: Vec<LedgerFinding>,
    pub relevant_observations: Vec<VerificationObservation>,
    pub relevant_artifact_refs: Vec<String>,
    pub expected_evidence: String,
    pub execution_metadata: BTreeMap<String, String>,
}

pub fn project_worker_context(
    ledger: &WorkingLedger,
    task_id: &str,
    expected_evidence: &str,
) -> Result<WorkerContext, OrchestrationError> {
    let config = LedgerOrchestratorConfig::default();
    ledger.validate_bounds(&config)?;
    project_worker_context_with_limit(
        ledger,
        task_id,
        expected_evidence,
        DEFAULT_MAX_SUMMARY_BYTES,
        DEFAULT_MAX_LEDGER_BYTES,
    )
}

fn project_worker_context_with_limit(
    ledger: &WorkingLedger,
    task_id: &str,
    expected_evidence: &str,
    max_bytes: usize,
    max_context_bytes: usize,
) -> Result<WorkerContext, OrchestrationError> {
    let task_id = sanitize_text(task_id, "task_id", max_bytes)?;
    validate_task_id(&task_id)?;
    let selected_task = ledger
        .get_task(&task_id)
        .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.to_string()))?
        .clone();
    let selected_task = sanitize_task_record(&selected_task, max_bytes)?;

    // Deterministic fresh-context projection: worker sees only findings that are
    // either unbound (global discovered facts) or explicitly bound to this task.
    // Unrelated tasks and their private findings are strictly omitted.
    let relevant_findings: Vec<LedgerFinding> = ledger
        .findings
        .iter()
        .filter(|f| match &f.related_task_id {
            None => true,
            Some(rel) => rel == &task_id,
        })
        .map(|finding| sanitize_finding(finding, max_bytes))
        .collect::<Result<Vec<_>, _>>()?;

    let relevant_observations: Vec<VerificationObservation> = ledger
        .observations
        .iter()
        .filter(|observation| observation.task_id == task_id)
        .map(|observation| sanitize_observation(observation, max_bytes))
        .collect::<Result<Vec<_>, _>>()?;

    // Collect only artifacts explicitly attached to the selected task. The
    // ledger-level artifact list is audit state, not worker-visible context;
    // projecting it here would leak another task's artifacts across tasks.
    let relevant_artifact_refs: Vec<String> = selected_task
        .evidence_refs
        .iter()
        .map(|reference| validate_artifact_reference(reference, max_bytes))
        .collect::<Result<Vec<_>, _>>()?;

    let mut execution_metadata = BTreeMap::new();
    execution_metadata.insert("round".to_string(), ledger.round_count.to_string());
    execution_metadata.insert(
        "attempt".to_string(),
        selected_task.attempt_count.to_string(),
    );
    execution_metadata.insert("task_id".to_string(), task_id.clone());
    if let Some(attempt_id) = &selected_task.attempt_id {
        execution_metadata.insert("attempt_id".to_string(), attempt_id.clone());
    }
    if let Some(checkpoint_digest) = &ledger.checkpoint_digest {
        execution_metadata.insert("checkpoint_digest".to_string(), checkpoint_digest.clone());
    }

    let context = WorkerContext {
        contract_digest: {
            // The identity digest is a fixed 64 bytes; prose bounds do not
            // apply to it.
            let digest = sanitize_text(
                &ledger.contract_digest,
                "contract_digest",
                max_bytes.max(64),
            )?;
            // Worker context is bound to the real contract identity.
            validate_digest(&digest, "contract_digest", max_bytes)?;
            digest
        },
        contract_summary: sanitize_optional_summary_text(
            ledger.contract_summary.as_deref(),
            "contract_summary",
            max_bytes,
        )?,
        plan_summary: sanitize_summary_text(&ledger.current_plan, "plan_summary", max_bytes)?,
        selected_task,
        relevant_findings,
        relevant_observations,
        relevant_artifact_refs,
        expected_evidence: sanitize_summary_text(
            expected_evidence,
            "expected_evidence",
            max_bytes,
        )?,
        execution_metadata,
    };
    let context_bytes = serde_json::to_vec(&context)
        .expect("WorkerContext serialization must remain infallible")
        .len();
    if context_bytes > max_context_bytes {
        return Err(OrchestrationError::OversizedLedger {
            current: context_bytes,
            max: max_context_bytes,
        });
    }
    Ok(context)
}

fn sanitize_observation(
    observation: &VerificationObservation,
    max_bytes: usize,
) -> Result<VerificationObservation, OrchestrationError> {
    let task_id = sanitize_text(&observation.task_id, "observation_task_id", max_bytes)?;
    validate_task_id(&task_id)?;
    Ok(VerificationObservation {
        round: observation.round,
        task_id,
        attempt_id: sanitize_optional_digest(
            observation.attempt_id.as_deref(),
            "observation_attempt_id",
            max_bytes,
        )?,
        result_digest: sanitize_optional_digest(
            observation.result_digest.as_deref(),
            "observation_result_digest",
            max_bytes,
        )?,
        outcome: observation.outcome,
        observation_summary: sanitize_summary_text(
            &observation.observation_summary,
            "observation_summary",
            max_bytes,
        )?,
        evidence_digest: sanitize_optional_digest(
            observation.evidence_digest.as_deref(),
            "observation_evidence_digest",
            max_bytes,
        )?,
    })
}

fn sanitize_task_record(
    task: &LedgerTaskRecord,
    max_bytes: usize,
) -> Result<LedgerTaskRecord, OrchestrationError> {
    let id = sanitize_text(&task.id, "task_id", max_bytes)?;
    validate_task_id(&id)?;
    Ok(LedgerTaskRecord {
        id,
        description: sanitize_summary_text(&task.description, "task_description", max_bytes)?,
        status: task.status,
        result_digest: sanitize_optional_digest(
            task.result_digest.as_deref(),
            "task_result_digest",
            max_bytes,
        )?,
        evidence_refs: task
            .evidence_refs
            .iter()
            .map(|reference| validate_artifact_reference(reference, max_bytes))
            .collect::<Result<Vec<_>, _>>()?,
        attempt_count: task.attempt_count,
        attempt_id: sanitize_optional_digest(
            task.attempt_id.as_deref(),
            "task_attempt_id",
            max_bytes,
        )?,
        failure_reason: sanitize_optional_summary_text(
            task.failure_reason.as_deref(),
            "task_failure_reason",
            max_bytes,
        )?,
    })
}

fn sanitize_finding(
    finding: &LedgerFinding,
    max_bytes: usize,
) -> Result<LedgerFinding, OrchestrationError> {
    let id = sanitize_text(&finding.id, "finding_id", max_bytes)?;
    validate_task_id(&id)?;
    let related_task_id = sanitize_optional_text(
        finding.related_task_id.as_deref(),
        "finding_related_task_id",
        max_bytes,
    )?;
    if let Some(related_task_id) = &related_task_id {
        validate_task_id(related_task_id)?;
    }
    Ok(LedgerFinding {
        id,
        summary: sanitize_summary_text(&finding.summary, "finding_summary", max_bytes)?,
        source: sanitize_summary_text(&finding.source, "finding_source", max_bytes)?,
        related_task_id,
        evidence_digest: sanitize_optional_digest(
            finding.evidence_digest.as_deref(),
            "finding_evidence_digest",
            max_bytes,
        )?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerOutcomeStatus {
    Completed,
    Failed,
    Truncated,
    Blocked,
}

/// Store/provider-owned effect receipt disposition for one worker attempt.
/// The orchestrator core has no effect authority: this disposition must be
/// derived from an existing store/provider-owned receipt, never from generic
/// worker status claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectReceiptDisposition {
    /// Proved no effect was issued before the failure (safe to retry).
    FailedBeforeSendNoEffect,
    /// A known effect failed (retryable under the attempt budget).
    KnownFailedEffect,
    /// A known effect succeeded.
    Success,
    /// The effect status cannot be proven; retry is fenced as unknown.
    OutcomeUnknown,
}

/// Store-owned receipt binding for one worker attempt. Attempt identity is
/// bound so a receipt can never authorize a different attempt's retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreEffectReceipt {
    pub attempt_id: String,
    pub disposition: EffectReceiptDisposition,
    /// Digest of the store/provider-owned receipt evidence this disposition
    /// was derived from. Missingness is explicit: the core never invents it.
    #[serde(default)]
    pub receipt_evidence_digest: Option<String>,
    #[serde(default)]
    pub store_evidence_ref: Option<String>,
}

/// Typed per-round provider usage envelope. Values originate from existing
/// provider/store-owned evidence; a worker never self-reports authoritative
/// cost. Unavailable values stay `None` (explicit missingness), never zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationUsageEnvelope {
    #[serde(default)]
    pub prompt_tokens: Option<u64>,
    #[serde(default)]
    pub completion_tokens: Option<u64>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub provider_calls: Option<u64>,
    #[serde(default)]
    pub cost_usd_micros: Option<u64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

impl OrchestrationUsageEnvelope {
    /// Combine two reported envelopes field-wise. A dimension merges only
    /// when both sides report it; otherwise the combined value is explicit
    /// missingness. Arithmetic overflow is a fail-closed error, never a wrap
    /// or a clamp. This is the two-envelope combine operator; multi-round
    /// accumulation additionally tracks sticky omission (see the
    /// orchestrator's usage journal), so an omit-then-report sequence can
    /// never resurrect a dropped total.
    pub fn checked_merge(&self, other: &Self) -> Result<Self, OrchestrationError> {
        let merge = |a: Option<u64>,
                     b: Option<u64>,
                     field: &str|
         -> Result<Option<u64>, OrchestrationError> {
            match (a, b) {
                (Some(x), Some(y)) => x.checked_add(y).map(Some).ok_or_else(|| {
                    OrchestrationError::WorkerExecutionError(format!(
                        "orchestration usage overflow in {field}"
                    ))
                }),
                _ => Ok(None),
            }
        };
        Ok(Self {
            prompt_tokens: merge(self.prompt_tokens, other.prompt_tokens, "prompt_tokens")?,
            completion_tokens: merge(
                self.completion_tokens,
                other.completion_tokens,
                "completion_tokens",
            )?,
            total_tokens: merge(self.total_tokens, other.total_tokens, "total_tokens")?,
            provider_calls: merge(self.provider_calls, other.provider_calls, "provider_calls")?,
            cost_usd_micros: merge(
                self.cost_usd_micros,
                other.cost_usd_micros,
                "cost_usd_micros",
            )?,
            duration_ms: merge(self.duration_ms, other.duration_ms, "duration_ms")?,
        })
    }

    pub fn missing_dimensions(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.prompt_tokens.is_none() {
            missing.push("prompt_tokens");
        }
        if self.completion_tokens.is_none() {
            missing.push("completion_tokens");
        }
        if self.total_tokens.is_none() {
            missing.push("total_tokens");
        }
        if self.provider_calls.is_none() {
            missing.push("provider_calls");
        }
        if self.cost_usd_micros.is_none() {
            missing.push("cost_usd_micros");
        }
        if self.duration_ms.is_none() {
            missing.push("duration_ms");
        }
        missing
    }
}

/// Envelope field order shared by the sticky omission flags: prompt,
/// completion, total, calls, cost, duration.
fn accumulate_usage_dimension(
    total: &mut Option<u64>,
    omitted: &mut bool,
    reported: Option<u64>,
    field: &'static str,
) -> Result<(), OrchestrationError> {
    let Some(value) = reported else {
        *omitted = true;
        *total = None;
        return Ok(());
    };
    if *omitted {
        // A dimension omitted by any earlier round stays explicit
        // missingness; a later report can never resurrect the dropped total.
        *total = None;
        return Ok(());
    }
    *total = Some(match *total {
        Some(base) => base.checked_add(value).ok_or_else(|| {
            OrchestrationError::WorkerExecutionError(format!(
                "orchestration usage overflow in {field}"
            ))
        })?,
        None => value,
    });
    Ok(())
}

fn fold_usage_envelope(
    totals: &mut OrchestrationUsageEnvelope,
    omitted: &mut [bool; 6],
    reported: &OrchestrationUsageEnvelope,
) -> Result<(), OrchestrationError> {
    accumulate_usage_dimension(
        &mut totals.prompt_tokens,
        &mut omitted[0],
        reported.prompt_tokens,
        "prompt_tokens",
    )?;
    accumulate_usage_dimension(
        &mut totals.completion_tokens,
        &mut omitted[1],
        reported.completion_tokens,
        "completion_tokens",
    )?;
    accumulate_usage_dimension(
        &mut totals.total_tokens,
        &mut omitted[2],
        reported.total_tokens,
        "total_tokens",
    )?;
    accumulate_usage_dimension(
        &mut totals.provider_calls,
        &mut omitted[3],
        reported.provider_calls,
        "provider_calls",
    )?;
    accumulate_usage_dimension(
        &mut totals.cost_usd_micros,
        &mut omitted[4],
        reported.cost_usd_micros,
        "cost_usd_micros",
    )?;
    accumulate_usage_dimension(
        &mut totals.duration_ms,
        &mut omitted[5],
        reported.duration_ms,
        "duration_ms",
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerResult {
    pub task_id: String,
    pub attempt_id: String,
    pub status: WorkerOutcomeStatus,
    /// Truncation may be retried only when the worker attests that this
    /// attempt performed no ProductStore-owned external effect. The core has
    /// no effect authority; a missing/false attestation is fenced unknown.
    #[serde(default)]
    pub effect_free: bool,
    pub output_digest: Option<String>,
    pub partial_summary: Option<String>,
    pub artifact_refs: Vec<String>,
    pub findings: Vec<LedgerFinding>,
    pub failure_reason: Option<String>,
    /// Per-round provider usage carried from the existing store-owned
    /// evidence path. The orchestrator accumulates it; it never authorizes
    /// cost from a worker claim alone.
    #[serde(default)]
    pub usage: Option<OrchestrationUsageEnvelope>,
    /// Store-owned effect receipt for this attempt. Retry safety reads the
    /// receipt disposition, never the generic worker status.
    #[serde(default)]
    pub effect_receipt: Option<StoreEffectReceipt>,
}

pub trait LedgerWorker {
    fn execute_task(&mut self, context: &WorkerContext)
        -> Result<WorkerResult, OrchestrationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationReport {
    pub outcome: VerificationOutcome,
    pub observation_summary: String,
    pub evidence_digest: Option<String>,
}

pub trait LedgerVerifier {
    fn verify_task(
        &mut self,
        ledger: &WorkingLedger,
        task: &LedgerTaskRecord,
        result: &WorkerResult,
    ) -> Result<VerificationReport, OrchestrationError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTaskSpec {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ControllerAction {
    ExecuteTask {
        task_id: String,
    },
    Replan {
        new_plan_summary: Option<String>,
        new_tasks: Vec<NewTaskSpec>,
        supersede_task_ids: Vec<String>,
    },
    DeclareComplete {
        deliverable_digest: String,
    },
    DeclareFailed {
        reason: String,
    },
    DeclareNoProgress {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextTaskDecision {
    pub action: ControllerAction,
    pub reason_summary: String,
    pub expected_evidence: String,
}

pub trait LedgerController {
    fn decide_next_action(
        &mut self,
        ledger: &WorkingLedger,
    ) -> Result<NextTaskDecision, OrchestrationError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationLifecycleState {
    Initialized,
    SelectingAction,
    ExecutingWorker,
    Verifying,
    Replanning,
    Completed,
    Failed,
    NoProgress,
    Cancelled,
}

impl OrchestrationLifecycleState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::NoProgress | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationMetrics {
    pub round_count: u32,
    pub manager_call_count: u32,
    pub worker_call_count: u32,
    pub verification_count: u32,
    pub replan_count: u32,
    pub failed_worker_attempts: u32,
    pub truncation_count: u32,
    pub no_progress_count: u32,
    pub verification_recovery_count: u32,
    pub context_input_bytes: u64,
    pub ledger_peak_bytes: usize,
    pub task_count: usize,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    /// Accumulated total tokens across rounds, when reported. Missingness is
    /// explicit: providers that omit totals stay `None`, never zero.
    #[serde(default)]
    pub total_tokens: Option<u64>,
    /// Accumulated provider calls across rounds, when reported.
    #[serde(default)]
    pub provider_calls: Option<u64>,
    pub cost_usd_micros: Option<u64>,
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrchestrationSummary {
    pub terminal_state: OrchestrationLifecycleState,
    pub deliverable_digest: Option<String>,
    pub terminal_reason: String,
    pub rounds_executed: u32,
    pub metrics: OrchestrationMetrics,
    pub final_ledger_hash: String,
    pub recovery_checkpoint_digest: Option<String>,
    pub rollback_target: Option<String>,
}

/// Typed terminal disposition for every bounded run. Fail-closed error
/// semantics are preserved: an error path yields a `Failed`-family
/// disposition with its failure code, never a converted success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerTerminalDisposition {
    Completed,
    Failed,
    NoProgress,
    Cancelled,
    /// A worker attempt or transport outcome cannot be proven against a
    /// store-owned receipt. Owner reconciliation is required; the fenced
    /// attempt must not be replayed.
    OutcomeUnknown,
    MaxRoundsExhausted,
    TruncationExhausted,
    VerifierFailure,
    MalformedControllerResponse,
}

impl LedgerTerminalDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::NoProgress => "no_progress",
            Self::Cancelled => "cancelled",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::MaxRoundsExhausted => "max_rounds_exhausted",
            Self::TruncationExhausted => "truncation_exhausted",
            Self::VerifierFailure => "verifier_failure",
            Self::MalformedControllerResponse => "malformed_controller_response",
        }
    }

    fn for_error(error: &OrchestrationError) -> Self {
        match error {
            OrchestrationError::MaxRoundsExceeded(_) => Self::MaxRoundsExhausted,
            OrchestrationError::TruncationLimitExceeded(_) => Self::TruncationExhausted,
            OrchestrationError::NoProgressLimitExceeded(_) => Self::NoProgress,
            OrchestrationError::VerificationError(_) => Self::VerifierFailure,
            OrchestrationError::InvalidControllerDecision(_)
            | OrchestrationError::InvalidTaskId(_)
            | OrchestrationError::DuplicateTaskId(_)
            | OrchestrationError::TaskNotFound(_)
            | OrchestrationError::InvalidArtifactReference(_) => Self::MalformedControllerResponse,
            OrchestrationError::Cancelled => Self::Cancelled,
            OrchestrationError::OversizedField { .. }
            | OrchestrationError::OversizedLedger { .. }
            | OrchestrationError::WorkerExecutionError(_)
            | OrchestrationError::SecretDetected(_) => Self::Failed,
        }
    }
}

/// Structured terminal evidence for one bounded run. This is what the RWE
/// bridge consumes: it retains the disposition, failure identity, metrics
/// collected before the failure, the ledger/evidence digest, the
/// outcome-unknown fence, explicit missingness, and recovery references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerTerminalRecord {
    pub disposition: LedgerTerminalDisposition,
    pub terminal_state: OrchestrationLifecycleState,
    /// Stable failure code for this terminal outcome (`completed` when there
    /// is no failure).
    pub failure_code: String,
    /// Digest of the failure reason text, never the raw reason.
    pub failure_reason_digest: Option<String>,
    pub rounds_executed: u32,
    pub metrics: OrchestrationMetrics,
    /// Conservation verdict for `sum(round usage) = cell lifecycle usage`.
    /// False means the totals cannot be trusted and must be treated as
    /// missing by downstream accounting.
    pub usage_conservation_verified: bool,
    /// Evidence dimensions that are unavailable for this run (explicit
    /// missingness, never silent zeros).
    pub missing_evidence: Vec<String>,
    pub final_ledger_hash: String,
    pub recovery_checkpoint_digest: Option<String>,
    pub rollback_target: Option<String>,
    pub outcome_unknown_task_ids: Vec<String>,
    /// The successful completion summary when the run completed; `None` on
    /// every failed-family disposition.
    pub summary: Option<OrchestrationSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrchestrationError {
    InvalidTaskId(String),
    DuplicateTaskId(String),
    TaskNotFound(String),
    InvalidArtifactReference(String),
    OversizedLedger {
        current: usize,
        max: usize,
    },
    OversizedField {
        field: String,
        current: usize,
        max: usize,
    },
    MaxRoundsExceeded(u32),
    TruncationLimitExceeded(u32),
    NoProgressLimitExceeded(u32),
    InvalidControllerDecision(String),
    WorkerExecutionError(String),
    VerificationError(String),
    SecretDetected(String),
    Cancelled,
}

impl std::fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTaskId(id) => write!(
                f,
                "invalid task id: {}",
                crate::provider::redaction::redact_sensitive_patterns(id)
            ),
            Self::DuplicateTaskId(id) => write!(
                f,
                "duplicate task id: {}",
                crate::provider::redaction::redact_sensitive_patterns(id)
            ),
            Self::TaskNotFound(id) => write!(
                f,
                "task not found: {}",
                crate::provider::redaction::redact_sensitive_patterns(id)
            ),
            Self::InvalidArtifactReference(reference) => write!(
                f,
                "invalid artifact reference: {}",
                crate::provider::redaction::redact_sensitive_patterns(reference)
            ),
            Self::OversizedLedger { current, max } => {
                write!(f, "oversized ledger: {current} bytes exceeds max {max}")
            }
            Self::OversizedField {
                field,
                current,
                max,
            } => {
                write!(f, "field {field} size {current} exceeds max {max}")
            }
            Self::MaxRoundsExceeded(r) => write!(f, "max orchestration rounds {r} exceeded"),
            Self::TruncationLimitExceeded(r) => write!(f, "max truncations {r} exceeded"),
            Self::NoProgressLimitExceeded(r) => {
                write!(f, "no progress limit {r} consecutive rounds exceeded")
            }
            Self::InvalidControllerDecision(s) => write!(
                f,
                "invalid controller decision: {}",
                crate::provider::redaction::redact_sensitive_patterns(s)
            ),
            Self::WorkerExecutionError(s) => write!(
                f,
                "worker execution error: {}",
                crate::provider::redaction::redact_sensitive_patterns(s)
            ),
            Self::VerificationError(s) => write!(
                f,
                "verification error: {}",
                crate::provider::redaction::redact_sensitive_patterns(s)
            ),
            Self::SecretDetected(s) => write!(
                f,
                "credential or secret shape detected: {}",
                crate::provider::redaction::redact_sensitive_patterns(s)
            ),
            Self::Cancelled => write!(f, "orchestration cancelled"),
        }
    }
}

impl std::error::Error for OrchestrationError {}

fn sanitize_controller_action(
    action: ControllerAction,
    max_bytes: usize,
    max_tasks: usize,
) -> Result<ControllerAction, OrchestrationError> {
    match action {
        ControllerAction::ExecuteTask { task_id } => {
            let task_id = sanitize_text(&task_id, "controller_task_id", max_bytes)?;
            validate_task_id(&task_id)?;
            Ok(ControllerAction::ExecuteTask { task_id })
        }
        ControllerAction::Replan {
            new_plan_summary,
            new_tasks,
            supersede_task_ids,
        } => {
            if new_tasks.len() > max_tasks {
                return Err(OrchestrationError::OversizedField {
                    field: "new_tasks".to_string(),
                    current: new_tasks.len(),
                    max: max_tasks,
                });
            }
            if supersede_task_ids.len() > max_tasks {
                return Err(OrchestrationError::OversizedField {
                    field: "supersede_task_ids".to_string(),
                    current: supersede_task_ids.len(),
                    max: max_tasks,
                });
            }
            Ok(ControllerAction::Replan {
                new_plan_summary: sanitize_optional_summary_text(
                    new_plan_summary.as_deref(),
                    "new_plan_summary",
                    max_bytes,
                )?,
                new_tasks: new_tasks
                    .into_iter()
                    .map(|task| {
                        let id = sanitize_text(&task.id, "new_task_id", max_bytes)?;
                        validate_task_id(&id)?;
                        Ok(NewTaskSpec {
                            id,
                            description: sanitize_summary_text(
                                &task.description,
                                "new_task_description",
                                max_bytes,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, OrchestrationError>>()?,
                supersede_task_ids: supersede_task_ids
                    .into_iter()
                    .map(|id| {
                        let id = sanitize_text(&id, "supersede_task_id", max_bytes)?;
                        validate_task_id(&id)?;
                        Ok(id)
                    })
                    .collect::<Result<Vec<_>, OrchestrationError>>()?,
            })
        }
        ControllerAction::DeclareComplete { deliverable_digest } => {
            let digest = sanitize_digest(&deliverable_digest, "deliverable_digest", max_bytes)?;
            if digest.is_empty() {
                return Err(OrchestrationError::InvalidControllerDecision(
                    "completion digest must not be empty".to_string(),
                ));
            }
            Ok(ControllerAction::DeclareComplete {
                deliverable_digest: digest,
            })
        }
        ControllerAction::DeclareFailed { reason } => Ok(ControllerAction::DeclareFailed {
            reason: sanitize_summary_text(&reason, "failure_reason", max_bytes)?,
        }),
        ControllerAction::DeclareNoProgress { reason } => Ok(ControllerAction::DeclareNoProgress {
            reason: sanitize_summary_text(&reason, "no_progress_reason", max_bytes)?,
        }),
    }
}

fn sanitize_next_task_decision(
    decision: NextTaskDecision,
    max_bytes: usize,
    max_tasks: usize,
) -> Result<NextTaskDecision, OrchestrationError> {
    Ok(NextTaskDecision {
        action: sanitize_controller_action(decision.action, max_bytes, max_tasks)?,
        reason_summary: sanitize_summary_text(
            &decision.reason_summary,
            "reason_summary",
            max_bytes,
        )?,
        expected_evidence: sanitize_summary_text(
            &decision.expected_evidence,
            "expected_evidence",
            max_bytes,
        )?,
    })
}

fn sanitize_worker_result(
    result: WorkerResult,
    max_bytes: usize,
    max_findings: usize,
    max_artifact_refs: usize,
) -> Result<WorkerResult, OrchestrationError> {
    if result.findings.len() > max_findings {
        return Err(OrchestrationError::OversizedField {
            field: "worker_findings".to_string(),
            current: result.findings.len(),
            max: max_findings,
        });
    }
    if result.artifact_refs.len() > max_artifact_refs {
        return Err(OrchestrationError::OversizedField {
            field: "worker_artifact_refs".to_string(),
            current: result.artifact_refs.len(),
            max: max_artifact_refs,
        });
    }
    Ok(WorkerResult {
        task_id: {
            let task_id = sanitize_text(&result.task_id, "worker_task_id", max_bytes)?;
            validate_task_id(&task_id)?;
            task_id
        },
        attempt_id: {
            let attempt_id = sanitize_digest(&result.attempt_id, "worker_attempt_id", max_bytes)?;
            if attempt_id.is_empty() {
                return Err(OrchestrationError::InvalidControllerDecision(
                    "worker attempt identity must not be empty".to_string(),
                ));
            }
            attempt_id
        },
        status: result.status,
        effect_free: result.effect_free,
        output_digest: sanitize_optional_digest(
            result.output_digest.as_deref(),
            "output_digest",
            max_bytes,
        )?,
        partial_summary: sanitize_optional_summary_text(
            result.partial_summary.as_deref(),
            "partial_summary",
            max_bytes,
        )?,
        artifact_refs: result
            .artifact_refs
            .iter()
            .map(|reference| validate_artifact_reference(reference, max_bytes))
            .collect::<Result<Vec<_>, _>>()?,
        findings: result
            .findings
            .iter()
            .map(|finding| sanitize_finding(finding, max_bytes))
            .collect::<Result<Vec<_>, _>>()?,
        failure_reason: sanitize_optional_summary_text(
            result.failure_reason.as_deref(),
            "failure_reason",
            max_bytes,
        )?,
        usage: result.usage,
        effect_receipt: result
            .effect_receipt
            .map(
                |receipt| -> Result<StoreEffectReceipt, OrchestrationError> {
                    let attempt_id =
                        sanitize_digest(&receipt.attempt_id, "receipt_attempt_id", max_bytes)?;
                    let receipt_evidence_digest = receipt
                        .receipt_evidence_digest
                        .as_deref()
                        .map(|digest| sanitize_digest(digest, "receipt_evidence_digest", max_bytes))
                        .transpose()?;
                    let store_evidence_ref = receipt
                        .store_evidence_ref
                        .as_deref()
                        .map(|reference| {
                            sanitize_text(reference, "receipt_store_evidence_ref", max_bytes)
                        })
                        .transpose()?;
                    Ok(StoreEffectReceipt {
                        attempt_id,
                        disposition: receipt.disposition,
                        receipt_evidence_digest,
                        store_evidence_ref,
                    })
                },
            )
            .transpose()?,
    })
}

fn sanitize_verification_report(
    report: VerificationReport,
    max_bytes: usize,
) -> Result<VerificationReport, OrchestrationError> {
    Ok(VerificationReport {
        outcome: report.outcome,
        observation_summary: sanitize_summary_text(
            &report.observation_summary,
            "observation_summary",
            max_bytes,
        )?,
        evidence_digest: sanitize_optional_digest(
            report.evidence_digest.as_deref(),
            "verification_evidence_digest",
            max_bytes,
        )?,
    })
}

pub struct LedgerOrchestrator<C: LedgerController, W: LedgerWorker, V: LedgerVerifier> {
    pub config: LedgerOrchestratorConfig,
    pub state: OrchestrationLifecycleState,
    pub ledger: WorkingLedger,
    pub controller: C,
    pub worker: W,
    pub verifier: V,
    pub metrics: OrchestrationMetrics,
    consecutive_no_progress: u32,
    last_fingerprint: Option<String>,
    pending_decision: Option<NextTaskDecision>,
    recovery_checkpoint: Option<WorkingLedger>,
    pub terminal_summary: Option<OrchestrationSummary>,
    /// Latest structured terminal record produced by [`LedgerOrchestrator::run_bounded`].
    pub terminal_record: Option<LedgerTerminalRecord>,
    /// Tasks that recorded a verification failure and have not yet recovered.
    /// A later verified pass removes the entry and counts exactly one
    /// verification recovery; repeated passes never double-count.
    verification_failed_task_ids: std::collections::BTreeSet<String>,
    /// Per-round usage envelopes in execution order; the conservation check
    /// requires their sticky-merge to equal [`LedgerOrchestrator::usage_totals`].
    round_usage: Vec<OrchestrationUsageEnvelope>,
    /// Accumulated per-cell provider usage across executed rounds.
    usage_totals: OrchestrationUsageEnvelope,
    /// Sticky per-dimension omission flags in envelope field order
    /// (prompt, completion, total, calls, cost, duration). Once any round
    /// omits a dimension, the cell total for that dimension stays explicit
    /// missingness; a later report can never resurrect the dropped total.
    usage_omitted: [bool; 6],
    cancelled: bool,
}

impl<C: LedgerController, W: LedgerWorker, V: LedgerVerifier> LedgerOrchestrator<C, W, V> {
    pub fn new(
        config: LedgerOrchestratorConfig,
        ledger: WorkingLedger,
        controller: C,
        worker: W,
        verifier: V,
    ) -> Result<Self, OrchestrationError> {
        ledger.validate_bounds(&config)?;
        if ledger.tasks.iter().any(|task| {
            matches!(
                task.status,
                LedgerTaskStatus::Selected | LedgerTaskStatus::Running
            )
        }) {
            return Err(OrchestrationError::InvalidControllerDecision(
                "in-flight task requires owner recovery before rehydration".to_string(),
            ));
        }
        let metrics = OrchestrationMetrics {
            task_count: ledger.tasks.len(),
            ledger_peak_bytes: ledger.estimate_bytes(),
            ..Default::default()
        };
        let initial_no_progress_streak = ledger.no_progress_streak;
        let initial_last_progress_fingerprint = ledger.last_progress_fingerprint.clone();
        Ok(Self {
            config,
            state: OrchestrationLifecycleState::Initialized,
            ledger,
            controller,
            worker,
            verifier,
            metrics,
            consecutive_no_progress: initial_no_progress_streak,
            last_fingerprint: initial_last_progress_fingerprint,
            pending_decision: None,
            recovery_checkpoint: None,
            terminal_summary: None,
            terminal_record: None,
            verification_failed_task_ids: std::collections::BTreeSet::new(),
            round_usage: Vec::new(),
            usage_totals: OrchestrationUsageEnvelope::default(),
            usage_omitted: [false; 6],
            cancelled: false,
        })
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.state = OrchestrationLifecycleState::Cancelled;
    }

    pub fn step(&mut self) -> Result<OrchestrationLifecycleState, OrchestrationError> {
        if self.cancelled {
            self.state = OrchestrationLifecycleState::Cancelled;
            return Ok(self.state);
        }
        if self.state.is_terminal() {
            return Ok(self.state);
        }

        if self
            .ledger
            .round_count
            .saturating_add(self.ledger.replan_count)
            >= self.config.max_orchestration_rounds
        {
            self.state = OrchestrationLifecycleState::Failed;
            return Err(OrchestrationError::MaxRoundsExceeded(
                self.config.max_orchestration_rounds,
            ));
        }

        match self.state {
            OrchestrationLifecycleState::Initialized => {
                self.state = OrchestrationLifecycleState::SelectingAction;
                Ok(self.state)
            }
            OrchestrationLifecycleState::SelectingAction => {
                self.metrics.manager_call_count += 1;
                let decision = sanitize_next_task_decision(
                    self.controller.decide_next_action(&self.ledger)?,
                    self.config.max_summary_bytes,
                    self.config.max_tasks,
                )?;
                match &decision.action {
                    ControllerAction::DeclareComplete { deliverable_digest } => {
                        if !self.ledger.all_verified() {
                            return Err(OrchestrationError::InvalidControllerDecision(
                                "cannot declare completion before all tasks are verified"
                                    .to_string(),
                            ));
                        }
                        if deliverable_digest != &self.ledger.deliverable_digest() {
                            return Err(OrchestrationError::InvalidControllerDecision(
                                "completion digest is not bound to the verified ledger evidence"
                                    .to_string(),
                            ));
                        }
                        self.state = OrchestrationLifecycleState::Completed;
                        self.pending_decision = Some(decision);
                        Ok(self.state)
                    }
                    ControllerAction::DeclareFailed { reason: _ } => {
                        self.state = OrchestrationLifecycleState::Failed;
                        self.pending_decision = Some(decision);
                        Ok(self.state)
                    }
                    ControllerAction::DeclareNoProgress { reason } => {
                        self.metrics.no_progress_count += 1;
                        self.ledger.no_progress_count += 1;
                        self.block_active_tasks(reason);
                        self.state = OrchestrationLifecycleState::NoProgress;
                        self.pending_decision = Some(decision);
                        Ok(self.state)
                    }
                    ControllerAction::Replan {
                        new_plan_summary,
                        new_tasks,
                        supersede_task_ids,
                    } => {
                        let ledger_snapshot = self.ledger.clone();
                        let metrics_snapshot = self.metrics.clone();
                        self.metrics.replan_count += 1;
                        self.ledger.replan_count += 1;
                        if let Some(plan) = new_plan_summary {
                            self.ledger.current_plan = plan.clone();
                        }
                        for id in supersede_task_ids {
                            if self.ledger.outcome_unknown_task_ids.contains(id) {
                                self.ledger = ledger_snapshot;
                                self.metrics = metrics_snapshot;
                                return Err(OrchestrationError::InvalidControllerDecision(
                                    "cannot supersede a task with an unknown outcome".to_string(),
                                ));
                            }
                            let Some(task) = self.ledger.get_task_mut(id) else {
                                self.ledger = ledger_snapshot;
                                self.metrics = metrics_snapshot;
                                return Err(OrchestrationError::InvalidControllerDecision(
                                    format!("cannot supersede unknown task {id}"),
                                ));
                            };
                            if task.status == LedgerTaskStatus::OutcomeUnknown {
                                self.ledger = ledger_snapshot;
                                self.metrics = metrics_snapshot;
                                return Err(OrchestrationError::InvalidControllerDecision(
                                    "cannot supersede a task with an unknown outcome".to_string(),
                                ));
                            }
                            task.status = LedgerTaskStatus::Superseded;
                        }
                        for new_t in new_tasks {
                            if let Err(error) =
                                self.ledger
                                    .add_task(&new_t.id, &new_t.description, &self.config)
                            {
                                self.ledger = ledger_snapshot;
                                self.metrics = metrics_snapshot;
                                return Err(error);
                            }
                        }
                        if let Err(error) = self.ledger.validate_bounds(&self.config) {
                            self.ledger = ledger_snapshot;
                            self.metrics = metrics_snapshot;
                            return Err(error);
                        }
                        self.metrics.task_count = self.ledger.tasks.len();
                        self.state = OrchestrationLifecycleState::SelectingAction;
                        Ok(self.state)
                    }
                    ControllerAction::ExecuteTask { task_id } => {
                        let ledger_snapshot = self.ledger.clone();
                        let task = self
                            .ledger
                            .get_task_mut(task_id)
                            .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.clone()))?;
                        if task.status.is_terminal() {
                            return Err(OrchestrationError::InvalidControllerDecision(format!(
                                "cannot execute task {task_id} with terminal status {:?}",
                                task.status
                            )));
                        }
                        task.status = LedgerTaskStatus::Selected;
                        self.ledger.current_selected_task_id = Some(task_id.clone());
                        if let Err(error) = self.ledger.validate_bounds(&self.config) {
                            self.ledger = ledger_snapshot;
                            return Err(error);
                        }
                        self.pending_decision = Some(decision);
                        self.state = OrchestrationLifecycleState::ExecutingWorker;
                        Ok(self.state)
                    }
                }
            }
            OrchestrationLifecycleState::ExecutingWorker => {
                let ledger_snapshot = self.ledger.clone();
                self.recovery_checkpoint = Some(ledger_snapshot.clone());
                let metrics_snapshot = self.metrics.clone();
                // Usage accumulation participates in the same atomic
                // pre-attempt snapshot: a restored round contributes neither
                // totals nor round envelopes, keeping the conservation check
                // meaningful after recovery.
                let usage_snapshot = self.usage_totals;
                let round_usage_snapshot = self.round_usage.clone();
                let usage_omitted_snapshot = self.usage_omitted;
                let verification_failed_snapshot = self.verification_failed_task_ids.clone();
                let state_snapshot = self.state;
                let pending_decision_snapshot = self.pending_decision.clone();
                let checkpoint_digest = ledger_snapshot.state_digest();
                let restore_execution_snapshot = |orchestrator: &mut Self| {
                    orchestrator.ledger = ledger_snapshot.clone();
                    orchestrator.metrics = metrics_snapshot.clone();
                    orchestrator.usage_totals = usage_snapshot;
                    orchestrator.round_usage = round_usage_snapshot.clone();
                    orchestrator.usage_omitted = usage_omitted_snapshot;
                    orchestrator.verification_failed_task_ids =
                        verification_failed_snapshot.clone();
                    orchestrator.ledger.checkpoint_digest = Some(checkpoint_digest.clone());
                    orchestrator.ledger.rollback_target =
                        Some(format!("ledger-checkpoint:{checkpoint_digest}"));
                    orchestrator.state = state_snapshot;
                    orchestrator.pending_decision = pending_decision_snapshot.clone();
                };
                let terminate_unknown_worker_attempt = |orchestrator: &mut Self, task_id: &str| {
                    // A worker error does not prove that no external work was
                    // performed. Restore only the pre-attempt ledger contents,
                    // then leave an explicit terminal fence so callers cannot
                    // replay an outcome whose effect status is unknown.
                    orchestrator.ledger = ledger_snapshot.clone();
                    orchestrator.metrics = metrics_snapshot.clone();
                    orchestrator.usage_totals = usage_snapshot;
                    orchestrator.round_usage = round_usage_snapshot.clone();
                    orchestrator.usage_omitted = usage_omitted_snapshot;
                    orchestrator.verification_failed_task_ids =
                        verification_failed_snapshot.clone();
                    orchestrator.ledger.checkpoint_digest = Some(checkpoint_digest.clone());
                    orchestrator.ledger.rollback_target =
                        Some(format!("ledger-checkpoint:{checkpoint_digest}"));
                    orchestrator.ledger.round_count =
                        orchestrator.ledger.round_count.saturating_add(1);
                    orchestrator.metrics.round_count =
                        orchestrator.metrics.round_count.saturating_add(1);
                    orchestrator.metrics.worker_call_count =
                        orchestrator.metrics.worker_call_count.saturating_add(1);
                    let attempt_generation = orchestrator.ledger.attempt_generation;
                    if let Some(task) = orchestrator.ledger.get_task_mut(task_id) {
                        let next_attempt = task.attempt_count.saturating_add(1);
                        task.status = LedgerTaskStatus::OutcomeUnknown;
                        task.attempt_count =
                            next_attempt.min(orchestrator.config.max_task_attempts);
                        task.attempt_id = Some(sha256_hex(&format!(
                            "ledger-attempt|{}|{task_id}|{next_attempt}|{checkpoint_digest}",
                            attempt_generation
                        )));
                        task.failure_reason = Some(
                            "worker outcome unknown; external effect status cannot be proven"
                                .to_string(),
                        );
                    }
                    if !orchestrator
                        .ledger
                        .outcome_unknown_task_ids
                        .iter()
                        .any(|id| id == task_id)
                    {
                        orchestrator
                            .ledger
                            .outcome_unknown_task_ids
                            .push(task_id.to_string());
                    }
                    orchestrator.state = OrchestrationLifecycleState::Failed;
                    orchestrator.pending_decision = None;
                };
                let task_id = self
                    .ledger
                    .current_selected_task_id
                    .clone()
                    .ok_or_else(|| {
                        OrchestrationError::InvalidControllerDecision(
                            "missing selected task in ExecutingWorker state".into(),
                        )
                    })?;
                let decision = match self.pending_decision.take() {
                    Some(decision) => decision,
                    None => {
                        restore_execution_snapshot(self);
                        return Err(OrchestrationError::InvalidControllerDecision(
                            "missing controller decision in ExecutingWorker state".into(),
                        ));
                    }
                };

                self.ledger.round_count += 1;
                self.metrics.round_count += 1;

                let next_attempt = self
                    .ledger
                    .get_task(&task_id)
                    .map(|task| task.attempt_count.saturating_add(1))
                    .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.clone()))?;
                let attempt_id = sha256_hex(&format!(
                    "ledger-attempt|{}|{task_id}|{next_attempt}|{checkpoint_digest}",
                    self.ledger.attempt_generation
                ));
                self.ledger.checkpoint_digest = Some(checkpoint_digest.clone());
                self.ledger.rollback_target = None;
                if let Some(task) = self.ledger.get_task_mut(&task_id) {
                    task.status = LedgerTaskStatus::Running;
                    task.attempt_count = next_attempt;
                    task.attempt_id = Some(attempt_id);
                }
                if let Err(error) = self.ledger.validate_bounds(&self.config) {
                    restore_execution_snapshot(self);
                    return Err(error);
                }

                let context = match project_worker_context_with_limit(
                    &self.ledger,
                    &task_id,
                    &decision.expected_evidence,
                    self.config.max_summary_bytes,
                    self.config.max_ledger_bytes,
                ) {
                    Ok(context) => context,
                    Err(error) => {
                        restore_execution_snapshot(self);
                        return Err(error);
                    }
                };
                let context_bytes = serde_json::to_vec(&context).map(|v| v.len()).unwrap_or(0);
                self.metrics.context_input_bytes += context_bytes as u64;

                self.metrics.worker_call_count += 1;
                let raw_worker_result = match self.worker.execute_task(&context) {
                    Ok(result) => result,
                    Err(error) => {
                        terminate_unknown_worker_attempt(self, &task_id);
                        return Err(error);
                    }
                };
                let worker_result = match sanitize_worker_result(
                    raw_worker_result,
                    self.config.max_summary_bytes,
                    self.config.max_findings,
                    self.config.max_artifact_refs,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        terminate_unknown_worker_attempt(self, &task_id);
                        return Err(error);
                    }
                };

                if worker_result.task_id != task_id {
                    terminate_unknown_worker_attempt(self, &task_id);
                    return Err(OrchestrationError::InvalidControllerDecision(
                        "worker result is bound to a different task".to_string(),
                    ));
                }
                if worker_result.attempt_id
                    != self
                        .ledger
                        .get_task(&task_id)
                        .and_then(|task| task.attempt_id.as_deref())
                        .unwrap_or_default()
                {
                    terminate_unknown_worker_attempt(self, &task_id);
                    return Err(OrchestrationError::InvalidControllerDecision(
                        "worker result is bound to a different attempt".to_string(),
                    ));
                }
                // An effect receipt can only authorize its own attempt. A
                // receipt minted for another attempt proves nothing about this
                // attempt's effect status, so the attempt is fenced unknown.
                if let Some(receipt) = &worker_result.effect_receipt {
                    if receipt.attempt_id != worker_result.attempt_id {
                        terminate_unknown_worker_attempt(self, &task_id);
                        return Err(OrchestrationError::InvalidControllerDecision(
                            "effect receipt is bound to a different attempt".to_string(),
                        ));
                    }
                }
                let receipt_disposition = worker_result
                    .effect_receipt
                    .as_ref()
                    .map(|receipt| receipt.disposition);
                if worker_result.findings.iter().any(|finding| {
                    finding
                        .related_task_id
                        .as_deref()
                        .is_some_and(|id| id != task_id)
                }) {
                    terminate_unknown_worker_attempt(self, &task_id);
                    return Err(OrchestrationError::InvalidControllerDecision(
                        "worker finding is bound to a different task".to_string(),
                    ));
                }
                if worker_result.status == WorkerOutcomeStatus::Completed
                    && worker_result.output_digest.is_none()
                {
                    terminate_unknown_worker_attempt(self, &task_id);
                    return Err(OrchestrationError::InvalidControllerDecision(
                        "completed worker result must include an output digest".to_string(),
                    ));
                }

                // Handle worker outcomes
                match worker_result.status {
                    WorkerOutcomeStatus::Completed => {
                        // A completed claim with an outcome-unknown receipt
                        // proves nothing: the external effect status is
                        // unknown, so the attempt is fenced without replay. A
                        // completed claim over a known-failed effect is
                        // contradictory and fenced for the same reason.
                        if matches!(
                            receipt_disposition,
                            Some(
                                EffectReceiptDisposition::OutcomeUnknown
                                    | EffectReceiptDisposition::KnownFailedEffect
                            )
                        ) {
                            terminate_unknown_worker_attempt(self, &task_id);
                            return Err(OrchestrationError::InvalidControllerDecision(
                                "completed worker result carries an effect receipt that contradicts completion"
                                    .to_string(),
                            ));
                        }
                        self.accumulate_round_usage(worker_result.usage.as_ref())?;
                        // Update task with output
                        if let Some(task) = self.ledger.get_task_mut(&task_id) {
                            task.result_digest = worker_result.output_digest.clone();
                            for art in &worker_result.artifact_refs {
                                if !task.evidence_refs.contains(art) {
                                    task.evidence_refs.push(art.clone());
                                }
                            }
                        }
                        for art in &worker_result.artifact_refs {
                            if !self.ledger.artifact_refs.contains(art) {
                                self.ledger.artifact_refs.push(art.clone());
                            }
                        }
                        for f in &worker_result.findings {
                            if let Err(error) = self.ledger.add_finding(f.clone(), &self.config) {
                                terminate_unknown_worker_attempt(self, &task_id);
                                return Err(error);
                            }
                        }

                        self.state = OrchestrationLifecycleState::Verifying;
                        self.pending_decision = Some(NextTaskDecision {
                            action: ControllerAction::ExecuteTask {
                                task_id: task_id.clone(),
                            },
                            reason_summary: decision.reason_summary,
                            expected_evidence: decision.expected_evidence,
                        });
                        // Pass worker result along via state
                        if let Err(error) = self.verify_worker_result(&task_id, &worker_result) {
                            terminate_unknown_worker_attempt(self, &task_id);
                            return Err(error);
                        }
                        if let Err(error) = self.ledger.validate_bounds(&self.config) {
                            terminate_unknown_worker_attempt(self, &task_id);
                            return Err(error);
                        }
                        Ok(self.state)
                    }
                    WorkerOutcomeStatus::Truncated => {
                        if !worker_result.effect_free {
                            terminate_unknown_worker_attempt(self, &task_id);
                            return Err(OrchestrationError::InvalidControllerDecision(
                                "truncated attempt lacks an effect-free attestation".to_string(),
                            ));
                        }
                        // A truncation may be recovered only when the attempt
                        // provably issued no real effect. Any receipt that
                        // records a possibly-issued effect (unknown, known
                        // failed, or claimed success on a truncated attempt)
                        // fences the attempt as unknown with zero replay.
                        if matches!(
                            receipt_disposition,
                            Some(
                                EffectReceiptDisposition::OutcomeUnknown
                                    | EffectReceiptDisposition::KnownFailedEffect
                                    | EffectReceiptDisposition::Success
                            )
                        ) {
                            terminate_unknown_worker_attempt(self, &task_id);
                            return Err(OrchestrationError::InvalidControllerDecision(
                                "truncated attempt carries an effect receipt that does not prove no effect"
                                    .to_string(),
                            ));
                        }
                        if self.ledger.truncation_count >= self.config.max_truncations {
                            restore_execution_snapshot(self);
                            self.state = OrchestrationLifecycleState::Failed;
                            return Err(OrchestrationError::TruncationLimitExceeded(
                                self.config.max_truncations,
                            ));
                        }
                        self.metrics.truncation_count += 1;
                        self.ledger.truncation_count += 1;
                        if let Err(error) =
                            self.accumulate_round_usage(worker_result.usage.as_ref())
                        {
                            restore_execution_snapshot(self);
                            return Err(error);
                        }
                        // Recover bounded partial information
                        if let Some(output_digest) = &worker_result.output_digest {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.result_digest = Some(output_digest.clone());
                            }
                        }
                        for art in &worker_result.artifact_refs {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                if !task.evidence_refs.contains(art) {
                                    task.evidence_refs.push(art.clone());
                                }
                            }
                            if !self.ledger.artifact_refs.contains(art) {
                                self.ledger.artifact_refs.push(art.clone());
                            }
                        }
                        if let Some(summary) = &worker_result.partial_summary {
                            if let Err(error) = self.ledger.add_finding(
                                LedgerFinding {
                                    id: sha256_hex(&format!(
                                        "truncation-recovery|{task_id}|{}",
                                        self.ledger.round_count
                                    )),
                                    summary: summary.clone(),
                                    source: "worker_truncation_recovery".to_string(),
                                    related_task_id: Some(task_id.clone()),
                                    evidence_digest: worker_result.output_digest.clone(),
                                },
                                &self.config,
                            ) {
                                restore_execution_snapshot(self);
                                return Err(error);
                            }
                        }
                        for f in &worker_result.findings {
                            if let Err(error) = self.ledger.add_finding(f.clone(), &self.config) {
                                restore_execution_snapshot(self);
                                return Err(error);
                            }
                        }

                        let attempt = self
                            .ledger
                            .get_task(&task_id)
                            .map(|t| t.attempt_count)
                            .unwrap_or(0);
                        if attempt >= self.config.max_task_attempts {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.status = LedgerTaskStatus::Failed;
                                task.failure_reason =
                                    Some("worker truncation attempt limit reached".into());
                            }
                        } else {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.status = LedgerTaskStatus::Pending;
                            }
                        }

                        if let Err(error) = self.ledger.validate_bounds(&self.config) {
                            restore_execution_snapshot(self);
                            return Err(error);
                        }
                        self.state = OrchestrationLifecycleState::SelectingAction;
                        Ok(self.state)
                    }
                    WorkerOutcomeStatus::Failed => {
                        // Retry safety reads the store-owned receipt, never
                        // the generic worker status. An outcome-unknown
                        // receipt, a success receipt on a failed result
                        // (contradictory), or a failure with no receipt at
                        // all — which proves nothing about the effect status
                        // — fences the attempt as unknown instead of
                        // replaying it. Only a receipt that proves no effect
                        // was sent, or a known failed effect, is retryable
                        // under the attempt budget.
                        if !matches!(
                            receipt_disposition,
                            Some(
                                EffectReceiptDisposition::FailedBeforeSendNoEffect
                                    | EffectReceiptDisposition::KnownFailedEffect
                            )
                        ) {
                            terminate_unknown_worker_attempt(self, &task_id);
                            return Err(OrchestrationError::InvalidControllerDecision(
                                "failed worker attempt lacks a retry-safe effect receipt"
                                    .to_string(),
                            ));
                        }
                        if let Err(error) =
                            self.accumulate_round_usage(worker_result.usage.as_ref())
                        {
                            restore_execution_snapshot(self);
                            return Err(error);
                        }
                        self.metrics.failed_worker_attempts += 1;
                        let attempt = self
                            .ledger
                            .get_task(&task_id)
                            .map(|t| t.attempt_count)
                            .unwrap_or(0);
                        if attempt >= self.config.max_task_attempts {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.status = LedgerTaskStatus::Failed;
                                task.failure_reason = worker_result.failure_reason.clone();
                            }
                        } else {
                            if let Some(task) = self.ledger.get_task_mut(&task_id) {
                                task.status = LedgerTaskStatus::Pending;
                                task.failure_reason = worker_result.failure_reason.clone();
                            }
                        }
                        if let Err(error) = self.ledger.validate_bounds(&self.config) {
                            restore_execution_snapshot(self);
                            return Err(error);
                        }
                        self.state = OrchestrationLifecycleState::SelectingAction;
                        Ok(self.state)
                    }
                    WorkerOutcomeStatus::Blocked => {
                        // A blocked round still contributes its reported
                        // usage to the lifecycle totals; only error paths
                        // without a worker result skip accumulation.
                        if let Err(error) =
                            self.accumulate_round_usage(worker_result.usage.as_ref())
                        {
                            restore_execution_snapshot(self);
                            return Err(error);
                        }
                        if let Some(task) = self.ledger.get_task_mut(&task_id) {
                            task.status = LedgerTaskStatus::Blocked;
                            task.failure_reason = worker_result.failure_reason.clone();
                        }
                        if let Err(error) = self.ledger.validate_bounds(&self.config) {
                            restore_execution_snapshot(self);
                            return Err(error);
                        }
                        self.state = OrchestrationLifecycleState::SelectingAction;
                        Ok(self.state)
                    }
                }
            }
            OrchestrationLifecycleState::Verifying => {
                // Handled in verify_worker_result
                self.state = OrchestrationLifecycleState::SelectingAction;
                Ok(self.state)
            }
            OrchestrationLifecycleState::Replanning => {
                self.state = OrchestrationLifecycleState::SelectingAction;
                Ok(self.state)
            }
            OrchestrationLifecycleState::Completed
            | OrchestrationLifecycleState::Failed
            | OrchestrationLifecycleState::NoProgress
            | OrchestrationLifecycleState::Cancelled => Ok(self.state),
        }
    }

    fn verify_worker_result(
        &mut self,
        task_id: &str,
        worker_result: &WorkerResult,
    ) -> Result<(), OrchestrationError> {
        self.metrics.verification_count += 1;
        let task_record = self
            .ledger
            .get_task(task_id)
            .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.to_string()))?
            .clone();

        let report = sanitize_verification_report(
            self.verifier
                .verify_task(&self.ledger, &task_record, worker_result)?,
            self.config.max_summary_bytes,
        )?;

        let observation = VerificationObservation {
            round: self.ledger.round_count,
            task_id: task_id.to_string(),
            attempt_id: Some(worker_result.attempt_id.clone()),
            result_digest: worker_result.output_digest.clone(),
            outcome: report.outcome,
            observation_summary: report.observation_summary,
            evidence_digest: report.evidence_digest.clone(),
        };
        self.ledger.add_observation(observation, &self.config)?;

        // Progress fingerprint and no-progress loop detection
        let new_fp = compute_progress_fingerprint(
            Some(task_id),
            worker_result.output_digest.as_deref(),
            report.evidence_digest.as_deref(),
            &self.ledger.progress_state_digest(),
        );
        if self.last_fingerprint.as_deref() == Some(&new_fp) {
            self.consecutive_no_progress += 1;
            self.metrics.no_progress_count += 1;
            self.ledger.no_progress_count += 1;
        } else {
            self.consecutive_no_progress = 0;
            self.last_fingerprint = Some(new_fp.clone());
        }
        self.ledger.no_progress_streak = self.consecutive_no_progress;
        self.ledger.last_progress_fingerprint = Some(new_fp.clone());
        self.ledger.progress_fingerprint = new_fp;

        if self.consecutive_no_progress >= self.config.max_no_progress_rounds {
            self.block_active_task(task_id, "no progress detected across verification rounds");
            self.state = OrchestrationLifecycleState::NoProgress;
            return Ok(());
        }

        match report.outcome {
            VerificationOutcome::Pass => {
                // A verified pass after a recorded verification failure for
                // the same task counts exactly one recovery. Removing the
                // entry on the first recovery means repeated passes cannot
                // double-count.
                if self.verification_failed_task_ids.remove(task_id) {
                    self.metrics.verification_recovery_count =
                        self.metrics.verification_recovery_count.saturating_add(1);
                }
                if let Some(t) = self.ledger.get_task_mut(task_id) {
                    t.status = LedgerTaskStatus::Verified;
                }
                self.state = OrchestrationLifecycleState::SelectingAction;
            }
            VerificationOutcome::Fail => {
                self.verification_failed_task_ids
                    .insert(task_id.to_string());
                self.metrics.failed_worker_attempts += 1;
                let attempt = self
                    .ledger
                    .get_task(task_id)
                    .map(|t| t.attempt_count)
                    .unwrap_or(0);
                if attempt >= self.config.max_task_attempts {
                    if let Some(t) = self.ledger.get_task_mut(task_id) {
                        t.status = LedgerTaskStatus::Failed;
                        t.failure_reason = Some("verification failed repeatedly".to_string());
                    }
                } else {
                    if let Some(t) = self.ledger.get_task_mut(task_id) {
                        t.status = LedgerTaskStatus::Pending;
                    }
                }
                self.state = OrchestrationLifecycleState::SelectingAction;
            }
            VerificationOutcome::Inconclusive => {
                if let Some(t) = self.ledger.get_task_mut(task_id) {
                    if t.attempt_count >= self.config.max_task_attempts {
                        t.status = LedgerTaskStatus::Blocked;
                        t.failure_reason = Some("verification inconclusive".to_string());
                    } else {
                        t.status = LedgerTaskStatus::Pending;
                    }
                }
                self.state = OrchestrationLifecycleState::SelectingAction;
            }
        }

        // Update ledger size peak metric
        let peak = self.ledger.estimate_bytes();
        if peak > self.metrics.ledger_peak_bytes {
            self.metrics.ledger_peak_bytes = peak;
        }

        Ok(())
    }

    /// Accumulate one round's store-evidence usage envelope into the
    /// per-cell totals, the per-round journal, and the lifecycle metrics.
    /// Omission is sticky per dimension: once any round omits a dimension,
    /// the cell total stays explicit missingness even if a later round
    /// reports it. The metrics projections track the totals exactly, so the
    /// conservation check stays meaningful.
    fn accumulate_round_usage(
        &mut self,
        usage: Option<&OrchestrationUsageEnvelope>,
    ) -> Result<(), OrchestrationError> {
        let Some(envelope) = usage.copied() else {
            return Ok(());
        };
        self.round_usage.push(envelope);
        fold_usage_envelope(&mut self.usage_totals, &mut self.usage_omitted, &envelope)?;
        self.metrics.prompt_tokens = self.usage_totals.prompt_tokens;
        self.metrics.completion_tokens = self.usage_totals.completion_tokens;
        self.metrics.total_tokens = self.usage_totals.total_tokens;
        self.metrics.provider_calls = self.usage_totals.provider_calls;
        self.metrics.cost_usd_micros = self.usage_totals.cost_usd_micros;
        self.metrics.duration_ms = self.usage_totals.duration_ms;
        Ok(())
    }

    /// Verify the conservation invariant: the sticky-merge of every recorded
    /// round envelope must equal the accumulated per-cell totals, the
    /// omission flags must match, and the metrics projections must agree
    /// with those totals. A tampered, dropped, or resurrected round envelope
    /// fails this check instead of silently changing the totals.
    pub fn usage_conservation_verified(&self) -> bool {
        let mut totals = OrchestrationUsageEnvelope::default();
        let mut omitted = [false; 6];
        for envelope in &self.round_usage {
            if fold_usage_envelope(&mut totals, &mut omitted, envelope).is_err() {
                return false;
            }
        }
        totals == self.usage_totals
            && omitted == self.usage_omitted
            && self.metrics.prompt_tokens == self.usage_totals.prompt_tokens
            && self.metrics.completion_tokens == self.usage_totals.completion_tokens
            && self.metrics.total_tokens == self.usage_totals.total_tokens
            && self.metrics.provider_calls == self.usage_totals.provider_calls
            && self.metrics.cost_usd_micros == self.usage_totals.cost_usd_micros
            && self.metrics.duration_ms == self.usage_totals.duration_ms
    }

    fn block_active_task(&mut self, task_id: &str, reason: &str) {
        if let Some(task) = self.ledger.get_task_mut(task_id) {
            if matches!(
                task.status,
                LedgerTaskStatus::Selected | LedgerTaskStatus::Running
            ) {
                task.status = LedgerTaskStatus::Blocked;
                task.failure_reason = Some(reason.to_string());
            }
        }
    }

    fn block_active_tasks(&mut self, reason: &str) {
        for task in &mut self.ledger.tasks {
            if matches!(
                task.status,
                LedgerTaskStatus::Selected | LedgerTaskStatus::Running
            ) {
                task.status = LedgerTaskStatus::Blocked;
                task.failure_reason = Some(reason.to_string());
            }
        }
    }

    pub fn run_to_completion(&mut self) -> Result<OrchestrationSummary, OrchestrationError> {
        while !self.state.is_terminal() {
            self.step()?;
        }

        let deliverable_digest = match &self.pending_decision {
            Some(NextTaskDecision {
                action: ControllerAction::DeclareComplete { deliverable_digest },
                ..
            }) => Some(deliverable_digest.clone()),
            _ => {
                // If all tasks verified, compute digest from all verified deliverables
                if self.ledger.all_verified() {
                    Some(self.ledger.deliverable_digest())
                } else {
                    None
                }
            }
        };

        let terminal_reason = match self.state {
            OrchestrationLifecycleState::Completed => {
                "orchestration completed successfully".to_string()
            }
            OrchestrationLifecycleState::Failed => "orchestration failed".to_string(),
            OrchestrationLifecycleState::NoProgress => {
                "no progress detected across rounds".to_string()
            }
            OrchestrationLifecycleState::Cancelled => "orchestration cancelled".to_string(),
            _ => "unknown terminal state".to_string(),
        };

        let final_ledger_hash = self.ledger.state_digest();
        let summary = OrchestrationSummary {
            terminal_state: self.state,
            deliverable_digest,
            terminal_reason,
            rounds_executed: self.ledger.round_count,
            metrics: self.metrics.clone(),
            final_ledger_hash,
            recovery_checkpoint_digest: self.ledger.checkpoint_digest.clone(),
            rollback_target: self.ledger.rollback_target.clone(),
        };
        self.terminal_summary = Some(summary.clone());
        Ok(summary)
    }

    fn terminal_failure_identity(
        &self,
        failure: Option<&OrchestrationError>,
    ) -> (LedgerTerminalDisposition, String, Option<String>) {
        fn error_code(error: &OrchestrationError) -> &'static str {
            match error {
                OrchestrationError::MaxRoundsExceeded(_) => "max_rounds_exceeded",
                OrchestrationError::TruncationLimitExceeded(_) => "truncation_limit_exceeded",
                OrchestrationError::NoProgressLimitExceeded(_) => "no_progress_limit_exceeded",
                OrchestrationError::InvalidControllerDecision(_) => "invalid_controller_decision",
                OrchestrationError::InvalidTaskId(_) => "invalid_task_id",
                OrchestrationError::DuplicateTaskId(_) => "duplicate_task_id",
                OrchestrationError::TaskNotFound(_) => "task_not_found",
                OrchestrationError::InvalidArtifactReference(_) => "invalid_artifact_reference",
                OrchestrationError::OversizedField { .. } => "oversized_field",
                OrchestrationError::OversizedLedger { .. } => "oversized_ledger",
                OrchestrationError::WorkerExecutionError(_) => "worker_execution_error",
                OrchestrationError::VerificationError(_) => "verification_error",
                OrchestrationError::SecretDetected(_) => "secret_detected",
                OrchestrationError::Cancelled => "cancelled",
            }
        }
        match failure {
            None => {
                let disposition = match self.state {
                    OrchestrationLifecycleState::Completed => LedgerTerminalDisposition::Completed,
                    OrchestrationLifecycleState::Failed => {
                        if self.ledger.outcome_unknown_task_ids.is_empty() {
                            LedgerTerminalDisposition::Failed
                        } else {
                            LedgerTerminalDisposition::OutcomeUnknown
                        }
                    }
                    OrchestrationLifecycleState::NoProgress => {
                        LedgerTerminalDisposition::NoProgress
                    }
                    OrchestrationLifecycleState::Cancelled => LedgerTerminalDisposition::Cancelled,
                    _ => LedgerTerminalDisposition::Failed,
                };
                let code = disposition.as_str().to_string();
                let reason_digest =
                    (disposition != LedgerTerminalDisposition::Completed).then(|| {
                        sha256_hex(&format!("terminal|{}|{}", code, self.ledger.state_digest()))
                    });
                (disposition, code, reason_digest)
            }
            Some(error) => {
                let disposition = if !self.ledger.outcome_unknown_task_ids.is_empty() {
                    // An error on a run with an unresolved unknown-outcome
                    // fence is owner-reconciliation-required by construction.
                    LedgerTerminalDisposition::OutcomeUnknown
                } else {
                    LedgerTerminalDisposition::for_error(error)
                };
                // The failure code names the underlying error kind even when
                // the fence dominates the disposition, so the record never
                // loses why the run stopped.
                let code = error_code(error).to_string();
                let reason_digest = Some(sha256_hex(&format!("terminal-error|{code}|{error}")));
                (disposition, code, reason_digest)
            }
        }
    }

    /// Build the structured terminal record for the current run, with or
    /// without a terminal error. Error semantics stay fail-closed: the record
    /// retains the disposition, failure identity, pre-failure metrics, the
    /// ledger/evidence digest, the unknown-outcome fence, explicit
    /// missingness, and recovery references. It never converts an error into
    /// success.
    pub fn build_terminal_record(
        &self,
        failure: Option<&OrchestrationError>,
    ) -> LedgerTerminalRecord {
        let (disposition, failure_code, failure_reason_digest) =
            self.terminal_failure_identity(failure);
        let summary = match failure {
            None if self.state == OrchestrationLifecycleState::Completed => {
                self.terminal_summary.clone()
            }
            _ => None,
        };
        let mut missing_evidence = Vec::new();
        if summary
            .as_ref()
            .and_then(|summary| summary.deliverable_digest.as_ref())
            .is_none()
            && disposition == LedgerTerminalDisposition::Completed
        {
            missing_evidence.push("deliverable_digest".to_string());
        }
        for dimension in self.usage_totals.missing_dimensions() {
            missing_evidence.push(format!("usage:{dimension}"));
        }
        if !self.ledger.outcome_unknown_task_ids.is_empty()
            && disposition != LedgerTerminalDisposition::OutcomeUnknown
        {
            missing_evidence.push("outcome_unknown_reconciliation".to_string());
        }
        LedgerTerminalRecord {
            disposition,
            terminal_state: self.state,
            failure_code,
            failure_reason_digest,
            rounds_executed: self.ledger.round_count,
            metrics: self.metrics.clone(),
            usage_conservation_verified: self.usage_conservation_verified(),
            missing_evidence,
            final_ledger_hash: self.ledger.state_digest(),
            recovery_checkpoint_digest: self.ledger.checkpoint_digest.clone(),
            rollback_target: self.ledger.rollback_target.clone(),
            outcome_unknown_task_ids: self.ledger.outcome_unknown_task_ids.clone(),
            summary,
        }
    }

    /// Execute a bounded run to a terminal state and always return a typed
    /// terminal record, including for error paths. The record preserves the
    /// fail-closed disposition and failure identity; the caller decides how
    /// to surface it. This is the entry point the RWE bridge must use.
    pub fn run_bounded(&mut self) -> LedgerTerminalRecord {
        let mut failure: Option<OrchestrationError> = None;
        while !self.state.is_terminal() {
            if let Err(error) = self.step() {
                failure = Some(error);
                break;
            }
        }
        if failure.is_some() && !self.state.is_terminal() {
            // Defensive: every error path must already have set a terminal
            // state; if a future path forgets, fail closed here.
            self.state = OrchestrationLifecycleState::Failed;
        }
        if failure.is_none() && self.state == OrchestrationLifecycleState::Completed {
            let _ = self.run_to_completion();
        }
        let record = self.build_terminal_record(failure.as_ref());
        self.terminal_record = Some(record.clone());
        record
    }

    /// Restore only the provider-independent in-memory ledger checkpoint.
    /// Outcome-unknown attempts are deliberately not rollbackable here: the
    /// external effect owner must reconcile them before any continuation.
    pub fn rollback_to_checkpoint(&mut self) -> Result<(), OrchestrationError> {
        if !self.ledger.outcome_unknown_task_ids.is_empty() {
            return Err(OrchestrationError::InvalidControllerDecision(
                "outcome-unknown effects require owner reconciliation before rollback".to_string(),
            ));
        }
        let Some(checkpoint) = self.recovery_checkpoint.clone() else {
            return Err(OrchestrationError::InvalidControllerDecision(
                "no in-memory orchestration checkpoint is available".to_string(),
            ));
        };
        let next_attempt_generation =
            checkpoint
                .attempt_generation
                .checked_add(1)
                .ok_or_else(|| {
                    OrchestrationError::InvalidControllerDecision(
                        "attempt generation exhausted; recovery is required".to_string(),
                    )
                })?;
        self.ledger = checkpoint;
        for task in &mut self.ledger.tasks {
            if matches!(
                task.status,
                LedgerTaskStatus::Selected | LedgerTaskStatus::Running
            ) {
                task.status = LedgerTaskStatus::Pending;
            }
        }
        self.ledger.current_selected_task_id = None;
        self.ledger.rollback_target = None;
        self.ledger.attempt_generation = next_attempt_generation;
        self.state = OrchestrationLifecycleState::SelectingAction;
        self.pending_decision = None;
        self.consecutive_no_progress = 0;
        self.last_fingerprint = None;
        self.ledger.no_progress_streak = 0;
        self.ledger.last_progress_fingerprint = None;
        self.terminal_summary = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    struct ScriptedController {
        decisions: Vec<NextTaskDecision>,
        index: usize,
    }

    impl ScriptedController {
        fn new(decisions: Vec<NextTaskDecision>) -> Self {
            Self {
                decisions,
                index: 0,
            }
        }
    }

    impl LedgerController for ScriptedController {
        fn decide_next_action(
            &mut self,
            _ledger: &WorkingLedger,
        ) -> Result<NextTaskDecision, OrchestrationError> {
            if self.index < self.decisions.len() {
                let d = self.decisions[self.index].clone();
                self.index += 1;
                Ok(d)
            } else {
                Ok(NextTaskDecision {
                    action: ControllerAction::DeclareNoProgress {
                        reason: "no more scripted decisions".to_string(),
                    },
                    reason_summary: "exhausted script".to_string(),
                    expected_evidence: "".to_string(),
                })
            }
        }
    }

    struct AdaptiveController;

    impl LedgerController for AdaptiveController {
        fn decide_next_action(
            &mut self,
            ledger: &WorkingLedger,
        ) -> Result<NextTaskDecision, OrchestrationError> {
            if ledger.all_verified() {
                return Ok(NextTaskDecision {
                    action: ControllerAction::DeclareComplete {
                        deliverable_digest: ledger.deliverable_digest(),
                    },
                    reason_summary: "all tasks verified".to_string(),
                    expected_evidence: "final_deliverable".to_string(),
                });
            }

            if let Some(pending_id) = ledger.pending_task_ids().into_iter().next() {
                return Ok(NextTaskDecision {
                    action: ControllerAction::ExecuteTask {
                        task_id: pending_id.clone(),
                    },
                    reason_summary: format!("executing pending task {pending_id}"),
                    expected_evidence: format!("evidence_for_{pending_id}"),
                });
            }

            Ok(NextTaskDecision {
                action: ControllerAction::DeclareFailed {
                    reason: "no pending or verifiable tasks remain".to_string(),
                },
                reason_summary: "terminal failure".to_string(),
                expected_evidence: "".to_string(),
            })
        }
    }

    type WorkerHandler =
        Box<dyn Fn(&WorkerContext) -> Result<WorkerResult, OrchestrationError> + Send + Sync>;

    struct MockWorker {
        handler: WorkerHandler,
    }

    impl MockWorker {
        fn new(
            f: impl Fn(&WorkerContext) -> Result<WorkerResult, OrchestrationError>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            Self {
                handler: Box::new(f),
            }
        }
    }

    impl LedgerWorker for MockWorker {
        fn execute_task(
            &mut self,
            context: &WorkerContext,
        ) -> Result<WorkerResult, OrchestrationError> {
            (self.handler)(context)
        }
    }

    type VerifierHandler = Box<
        dyn Fn(
                &WorkingLedger,
                &LedgerTaskRecord,
                &WorkerResult,
            ) -> Result<VerificationReport, OrchestrationError>
            + Send
            + Sync,
    >;

    struct MockVerifier {
        handler: VerifierHandler,
    }

    impl MockVerifier {
        fn new(
            f: impl Fn(
                    &WorkingLedger,
                    &LedgerTaskRecord,
                    &WorkerResult,
                ) -> Result<VerificationReport, OrchestrationError>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            Self {
                handler: Box::new(f),
            }
        }

        fn pass_all() -> Self {
            Self::new(|_, _, _| {
                Ok(VerificationReport {
                    outcome: VerificationOutcome::Pass,
                    observation_summary: "checks passed".to_string(),
                    evidence_digest: Some(sha256_hex("pass_digest")),
                })
            })
        }
    }

    impl LedgerVerifier for MockVerifier {
        fn verify_task(
            &mut self,
            ledger: &WorkingLedger,
            task: &LedgerTaskRecord,
            result: &WorkerResult,
        ) -> Result<VerificationReport, OrchestrationError> {
            (self.handler)(ledger, task, result)
        }
    }

    #[test]
    fn scenario_a_successful_decomposition() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:decomp-v1", "decompose and implement");
        ledger
            .add_task("task-a", "implement component a", &config)
            .unwrap();
        ledger
            .add_task("task-b", "implement component b", &config)
            .unwrap();

        let controller = AdaptiveController;
        let worker = MockWorker::new(|ctx| {
            let task_id = &ctx.selected_task.id;
            Ok(WorkerResult {
                task_id: task_id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                output_digest: Some(sha256_hex(&format!("output_digest_{task_id}"))),
                partial_summary: None,
                artifact_refs: vec![format!("artifact:{task_id}")],
                findings: vec![LedgerFinding {
                    id: format!("finding-{task_id}"),
                    summary: format!("successfully built {task_id}"),
                    source: "worker".to_string(),
                    related_task_id: Some(task_id.clone()),
                    evidence_digest: Some(sha256_hex(&format!("ev-{task_id}"))),
                }],
                failure_reason: None,
                usage: None,
                effect_receipt: None,
            })
        });
        let verifier = MockVerifier::pass_all();

        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier).unwrap();
        let summary = orchestrator.run_to_completion().unwrap();

        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        assert_eq!(
            summary.deliverable_digest,
            Some(orchestrator.ledger.deliverable_digest())
        );
        assert_eq!(summary.metrics.round_count, 2);
        assert_eq!(summary.metrics.worker_call_count, 2);
        assert_eq!(summary.metrics.verification_count, 2);
        assert_eq!(summary.metrics.replan_count, 0);
        assert!(orchestrator.ledger.all_verified());
        assert_eq!(
            orchestrator.ledger.get_task("task-a").unwrap().status,
            LedgerTaskStatus::Verified
        );
        assert_eq!(
            orchestrator.ledger.get_task("task-b").unwrap().status,
            LedgerTaskStatus::Verified
        );
    }

    #[test]
    fn scenario_b_verification_repair() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:repair-v1", "implement with self-repair");
        ledger
            .add_task("task-bugfix", "fix memory leak", &config)
            .unwrap();

        let controller = AdaptiveController;
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let worker = MockWorker::new(move |ctx| {
            let cur = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if cur == 1 {
                assert_eq!(ctx.relevant_observations.len(), 1);
                assert_eq!(
                    ctx.relevant_observations[0].outcome,
                    VerificationOutcome::Fail
                );
            }
            if cur == 0 {
                Ok(WorkerResult {
                    task_id: ctx.selected_task.id.clone(),
                    attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                    status: WorkerOutcomeStatus::Completed,
                    effect_free: true,
                    output_digest: Some(sha256_hex("buggy_output_attempt_1")),
                    partial_summary: None,
                    artifact_refs: vec!["patch:leak_incomplete".to_string()],
                    findings: vec![],
                    failure_reason: None,
                    usage: None,
                    effect_receipt: None,
                })
            } else {
                Ok(WorkerResult {
                    task_id: ctx.selected_task.id.clone(),
                    attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                    status: WorkerOutcomeStatus::Completed,
                    effect_free: true,
                    output_digest: Some(sha256_hex("fixed_output_attempt_2")),
                    partial_summary: None,
                    artifact_refs: vec!["patch:leak_repaired".to_string()],
                    findings: vec![LedgerFinding {
                        id: "finding-repaired".to_string(),
                        summary: "fixed memory free in cleanup".to_string(),
                        source: "worker_repair".to_string(),
                        related_task_id: Some("task-bugfix".to_string()),
                        evidence_digest: Some(sha256_hex("ev:repaired")),
                    }],
                    failure_reason: None,
                    usage: None,
                    effect_receipt: None,
                })
            }
        });

        let verifier = MockVerifier::new(|_ledger, _task, result| {
            if result.output_digest.as_deref()
                == Some(sha256_hex("buggy_output_attempt_1").as_str())
            {
                Ok(VerificationReport {
                    outcome: VerificationOutcome::Fail,
                    observation_summary: "leak still detected on stress test".to_string(),
                    evidence_digest: Some(sha256_hex("test_fail_leak_persists")),
                })
            } else {
                Ok(VerificationReport {
                    outcome: VerificationOutcome::Pass,
                    observation_summary: "leak resolved; valgrind clean".to_string(),
                    evidence_digest: Some(sha256_hex("test_pass_leak_clean")),
                })
            }
        });

        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier).unwrap();
        let summary = orchestrator.run_to_completion().unwrap();

        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        assert_eq!(summary.metrics.round_count, 2);
        assert_eq!(summary.metrics.failed_worker_attempts, 1);
        assert_eq!(summary.metrics.verification_count, 2);
        assert_eq!(orchestrator.ledger.observations.len(), 2);
        assert_eq!(
            orchestrator.ledger.observations[0].outcome,
            VerificationOutcome::Fail
        );
        assert_eq!(
            orchestrator.ledger.observations[1].outcome,
            VerificationOutcome::Pass
        );
        assert_eq!(
            orchestrator.ledger.get_task("task-bugfix").unwrap().status,
            LedgerTaskStatus::Verified
        );
    }

    #[test]
    fn scenario_c_no_progress_guard() {
        let config = LedgerOrchestratorConfig {
            max_no_progress_rounds: 2,
            ..Default::default()
        };
        let mut ledger = WorkingLedger::new("contract:loop-v1", "test no progress");
        ledger
            .add_task("task-stuck", "cannot make progress", &config)
            .unwrap();

        let controller = ScriptedController::new(vec![
            NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: "task-stuck".to_string(),
                },
                reason_summary: "attempt 1".to_string(),
                expected_evidence: "ev".to_string(),
            },
            NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: "task-stuck".to_string(),
                },
                reason_summary: "attempt 2 same".to_string(),
                expected_evidence: "ev".to_string(),
            },
            NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: "task-stuck".to_string(),
                },
                reason_summary: "attempt 3 same".to_string(),
                expected_evidence: "ev".to_string(),
            },
        ]);

        let worker = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                output_digest: Some(sha256_hex("identical_stuck_output")),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: None,
                effect_receipt: None,
            })
        });

        let verifier = MockVerifier::new(|_, _, _| {
            Ok(VerificationReport {
                outcome: VerificationOutcome::Fail,
                observation_summary: "stuck output fails verification identically".to_string(),
                evidence_digest: Some(sha256_hex("fail_digest_identical")),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier).unwrap();
        let summary = orchestrator.run_to_completion().unwrap();

        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::NoProgress
        );
        assert!(summary.metrics.no_progress_count >= 2);
        assert_eq!(
            orchestrator.ledger.get_task("task-stuck").unwrap().status,
            LedgerTaskStatus::Blocked
        );
    }

    #[test]
    fn scenario_d_worker_truncation_recovery() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:trunc-v1", "test truncation recovery");
        ledger
            .add_task("task-large", "produce large code change", &config)
            .unwrap();

        let controller = AdaptiveController;
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let worker = MockWorker::new(move |ctx| {
            let cur = attempts_clone.fetch_add(1, Ordering::SeqCst);
            if cur == 0 {
                Ok(WorkerResult {
                    task_id: "task-large".to_string(),
                    attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                    status: WorkerOutcomeStatus::Truncated,
                    effect_free: true,
                    output_digest: Some(sha256_hex("partial_out_sha")),
                    partial_summary: Some("Discovered root cause in token buffer".to_string()),
                    artifact_refs: vec!["partial_patch.diff".to_string()],
                    findings: vec![LedgerFinding {
                        id: "f-partial".to_string(),
                        summary: "token buffer boundary is 4096 bytes".to_string(),
                        source: "worker_truncation".to_string(),
                        related_task_id: Some("task-large".to_string()),
                        evidence_digest: Some(sha256_hex("ev:buffer")),
                    }],
                    failure_reason: None,
                    usage: None,
                    effect_receipt: None,
                })
            } else {
                Ok(WorkerResult {
                    task_id: "task-large".to_string(),
                    attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                    status: WorkerOutcomeStatus::Completed,
                    effect_free: true,
                    output_digest: Some(sha256_hex("full_completed_patch")),
                    partial_summary: None,
                    artifact_refs: vec!["complete_patch.diff".to_string()],
                    findings: vec![],
                    failure_reason: None,
                    usage: None,
                    effect_receipt: None,
                })
            }
        });

        let verifier = MockVerifier::pass_all();
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier).unwrap();
        let summary = orchestrator.run_to_completion().unwrap();

        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        assert_eq!(summary.metrics.truncation_count, 1);
        assert_eq!(orchestrator.ledger.findings.len(), 2); // 1 partial summary finding + 1 finding from result
        assert!(orchestrator
            .ledger
            .artifact_refs
            .contains(&"partial_patch.diff".to_string()));
        assert!(orchestrator
            .ledger
            .get_task("task-large")
            .unwrap()
            .evidence_refs
            .contains(&"partial_patch.diff".to_string()));
        assert_eq!(
            orchestrator
                .ledger
                .get_task("task-large")
                .unwrap()
                .result_digest
                .as_deref(),
            Some(sha256_hex("full_completed_patch").as_str())
        );
        assert_eq!(orchestrator.ledger.findings[0].id.len(), 64);
        assert_eq!(
            orchestrator.ledger.get_task("task-large").unwrap().status,
            LedgerTaskStatus::Verified
        );
    }

    #[test]
    fn scenario_e_state_bounds_enforced() {
        let config = LedgerOrchestratorConfig {
            max_tasks: 2,
            max_findings: 2,
            max_observations: 2,
            max_artifact_refs: 2,
            max_summary_bytes: 50,
            max_ledger_bytes: 4096,
            max_orchestration_rounds: 5,
            max_task_attempts: 2,
            max_no_progress_rounds: 2,
            max_truncations: 2,
        };
        let mut ledger = WorkingLedger::new("contract:bounds", "bounds testing");

        // 1. Task bounds
        ledger.add_task("task-1", "description 1", &config).unwrap();
        ledger.add_task("task-2", "description 2", &config).unwrap();
        let err = ledger
            .add_task("task-3", "description 3", &config)
            .unwrap_err();
        assert!(
            matches!(err, OrchestrationError::OversizedField { ref field, .. } if field == "tasks")
        );

        // 2. Summary bytes bound
        let mut ledger2 = WorkingLedger::new("contract:bounds2", "bounds testing 2");
        let long_desc = "a".repeat(100);
        let err2 = ledger2
            .add_task("task-long", &long_desc, &config)
            .unwrap_err();
        assert!(
            matches!(err2, OrchestrationError::OversizedField { ref field, .. } if field == "task_description")
        );

        // 3. Finding bounds
        ledger2
            .add_finding(
                LedgerFinding {
                    id: "f1".into(),
                    summary: "short 1".into(),
                    source: "s".into(),
                    related_task_id: None,
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();
        ledger2
            .add_finding(
                LedgerFinding {
                    id: "f2".into(),
                    summary: "short 2".into(),
                    source: "s".into(),
                    related_task_id: None,
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();
        let err3 = ledger2
            .add_finding(
                LedgerFinding {
                    id: "f3".into(),
                    summary: "short 3".into(),
                    source: "s".into(),
                    related_task_id: None,
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap_err();
        assert!(
            matches!(err3, OrchestrationError::OversizedField { ref field, .. } if field == "findings")
        );
    }

    #[test]
    fn scenario_f_strict_isolation_between_cells() {
        let config = LedgerOrchestratorConfig::default();

        // Cell 1
        let mut ledger_cell_1 = WorkingLedger::new("contract:cell_1", "plan 1");
        ledger_cell_1
            .add_task("task-cell-1", "task 1", &config)
            .unwrap();
        ledger_cell_1
            .add_finding(
                LedgerFinding {
                    id: "cell-1-secret-finding".to_string(),
                    summary: "private finding in cell 1".to_string(),
                    source: "cell_1".to_string(),
                    related_task_id: Some("task-cell-1".to_string()),
                    evidence_digest: Some(sha256_hex("ev:cell1")),
                },
                &config,
            )
            .unwrap();

        // Cell 2
        let mut ledger_cell_2 = WorkingLedger::new("contract:cell_2", "plan 2");
        ledger_cell_2
            .add_task("task-cell-2", "task 2", &config)
            .unwrap();

        // Check Cell 2 cannot see Cell 1's findings, tasks, or digests
        assert!(ledger_cell_2.get_task("task-cell-1").is_none());
        assert!(ledger_cell_2.findings.is_empty());
        assert_ne!(ledger_cell_1.state_digest(), ledger_cell_2.state_digest());
        assert_ne!(
            ledger_cell_1.progress_fingerprint,
            ledger_cell_2.progress_fingerprint
        );
    }

    #[test]
    fn scenario_g_fresh_context_filtering() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:fresh-context", "multi-task plan");
        ledger.add_task("task-1", "task one", &config).unwrap();
        ledger.add_task("task-2", "task two", &config).unwrap();
        ledger.add_task("task-3", "task three", &config).unwrap();

        // Finding 1 bound to task-1
        ledger
            .add_finding(
                LedgerFinding {
                    id: "f-task-1".to_string(),
                    summary: "finding for task 1".to_string(),
                    source: "worker".to_string(),
                    related_task_id: Some("task-1".to_string()),
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();

        // Finding 2 bound to task-2 (unrelated to task-1)
        ledger
            .add_finding(
                LedgerFinding {
                    id: "f-task-2".to_string(),
                    summary: "private finding for task 2".to_string(),
                    source: "worker".to_string(),
                    related_task_id: Some("task-2".to_string()),
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();

        // Finding 3 is global (related_task_id is None)
        ledger
            .add_finding(
                LedgerFinding {
                    id: "f-global".to_string(),
                    summary: "global project invariant".to_string(),
                    source: "system".to_string(),
                    related_task_id: None,
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();

        let ctx = project_worker_context(&ledger, "task-1", "expected").unwrap();

        assert_eq!(ctx.selected_task.id, "task-1");
        // Must contain task-1 finding and global finding
        assert!(ctx.relevant_findings.iter().any(|f| f.id == "f-task-1"));
        assert!(ctx.relevant_findings.iter().any(|f| f.id == "f-global"));
        // Must NOT contain task-2 finding
        assert!(!ctx.relevant_findings.iter().any(|f| f.id == "f-task-2"));
        assert_eq!(ctx.relevant_findings.len(), 2);
    }

    #[test]
    fn worker_context_isolates_task_artifacts() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:artifact-isolation", "plan");
        ledger.add_task("task-1", "first task", &config).unwrap();
        ledger.add_task("task-2", "second task", &config).unwrap();
        ledger.tasks[0].evidence_refs = vec!["artifact:task-1".to_string()];
        ledger.tasks[1].evidence_refs = vec!["artifact:task-2".to_string()];
        ledger.artifact_refs = vec!["artifact:task-1".to_string()];

        let context = project_worker_context(&ledger, "task-2", "expected").unwrap();

        assert_eq!(context.relevant_artifact_refs, vec!["artifact:task-2"]);
    }

    #[test]
    fn security_and_authority_boundaries_isolated() {
        let config = LedgerOrchestratorConfig::default();
        let test_key = format!("{}_{}", "sk-test", "1234567890abcdef");
        let mut ledger = WorkingLedger::new(
            "contract:security",
            format!("testing boundaries with DEEPSEEK_API_KEY={test_key}"),
        );
        assert!(!ledger.current_plan.contains(&test_key));

        // Secrets in task description are sanitized/redacted
        let task_desc = format!("API key: DEEPSEEK_API_KEY={test_key}");
        ledger.add_task("task-sec", &task_desc, &config).unwrap();
        let task = ledger.get_task("task-sec").unwrap();
        assert!(!task.description.contains(&test_key));

        // Secrets in findings are sanitized/redacted
        let sensitive_token = format!("{}_{}", "ghp", "1234567890abcdef");
        let finding_summary = format!("Found Bearer {sensitive_token} in header");
        ledger
            .add_finding(
                LedgerFinding {
                    id: "f-sec".to_string(),
                    summary: finding_summary,
                    source: "scanner".to_string(),
                    related_task_id: Some("task-sec".to_string()),
                    evidence_digest: None,
                },
                &config,
            )
            .unwrap();
        assert!(!ledger.findings[0].summary.contains(&sensitive_token));

        // H_ledger is purely an in-memory/ephemeral state machine without authority
        // to invoke ProductStore transactions or schedule external processes.
    }

    #[test]
    fn bounded_inputs_and_artifacts_are_rejected_at_orchestrator_boundary() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:unsafe", "bounded plan");
        ledger.add_task("task-safe", "safe task", &config).unwrap();
        ledger.artifact_refs.push("../escape".to_string());

        let result = LedgerOrchestrator::new(
            config,
            ledger,
            AdaptiveController,
            MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
            MockVerifier::pass_all(),
        );
        assert!(matches!(
            result,
            Err(OrchestrationError::InvalidArtifactReference(_))
        ));

        let tight_config = LedgerOrchestratorConfig {
            max_summary_bytes: 8,
            ..Default::default()
        };
        let mut tight_ledger = WorkingLedger::new("c", "p");
        tight_ledger
            .add_task("task", "safe", &tight_config)
            .unwrap();
        assert!(matches!(
            project_worker_context_with_limit(&tight_ledger, "task", "123456789", 8, 4096),
            Err(OrchestrationError::OversizedField { ref field, .. }) if field == "expected_evidence"
        ));
    }

    #[test]
    fn invalid_replan_is_atomic() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:replan", "plan");
        ledger.add_task("task", "task", &config).unwrap();
        let before = ledger.clone();
        let controller = ScriptedController::new(vec![NextTaskDecision {
            action: ControllerAction::Replan {
                new_plan_summary: Some("new plan".to_string()),
                new_tasks: vec![NewTaskSpec {
                    id: "new-task".to_string(),
                    description: "new task".to_string(),
                }],
                supersede_task_ids: vec!["missing".to_string()],
            },
            reason_summary: "replan".to_string(),
            expected_evidence: "evidence".to_string(),
        }]);
        let mut orchestrator = LedgerOrchestrator::new(
            config,
            ledger,
            controller,
            MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
            MockVerifier::pass_all(),
        )
        .unwrap();
        orchestrator.step().unwrap();
        let result = orchestrator.step();
        assert!(matches!(
            result,
            Err(OrchestrationError::InvalidControllerDecision(_))
        ));
        assert_eq!(orchestrator.ledger, before);
        assert_eq!(orchestrator.ledger.replan_count, 0);
    }

    #[test]
    fn worker_error_is_fenced_as_unknown_without_replay() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:worker-error", "plan");
        ledger.add_task("task", "task", &config).unwrap();
        let controller = ScriptedController::new(vec![NextTaskDecision {
            action: ControllerAction::ExecuteTask {
                task_id: "task".to_string(),
            },
            reason_summary: "execute".to_string(),
            expected_evidence: "evidence".to_string(),
        }]);
        let mut orchestrator = LedgerOrchestrator::new(
            config,
            ledger,
            controller,
            MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
            MockVerifier::pass_all(),
        )
        .unwrap();
        orchestrator.step().unwrap();
        orchestrator.step().unwrap();
        let ledger_before_execution = orchestrator.ledger.clone();
        let metrics_before_execution = orchestrator.metrics.clone();

        assert!(matches!(
            orchestrator.step(),
            Err(OrchestrationError::Cancelled)
        ));
        assert_ne!(orchestrator.ledger, ledger_before_execution);
        assert_ne!(orchestrator.metrics, metrics_before_execution);
        assert_eq!(orchestrator.state, OrchestrationLifecycleState::Failed);
        assert!(orchestrator.pending_decision.is_none());
        assert_eq!(
            orchestrator.ledger.get_task("task").unwrap().status,
            LedgerTaskStatus::OutcomeUnknown
        );
        assert_eq!(
            orchestrator.ledger.get_task("task").unwrap().attempt_count,
            1
        );
        assert!(orchestrator.ledger.checkpoint_digest.is_some());
        assert!(orchestrator.ledger.rollback_target.is_some());
        assert!(orchestrator
            .ledger
            .get_task("task")
            .unwrap()
            .attempt_id
            .is_some());
        assert!(orchestrator.rollback_to_checkpoint().is_err());
        assert_eq!(
            orchestrator.step().unwrap(),
            OrchestrationLifecycleState::Failed
        );
    }

    #[test]
    fn rehydration_rejects_in_flight_tasks_and_restores_no_progress_state() {
        let config = LedgerOrchestratorConfig::default();
        for status in [LedgerTaskStatus::Selected, LedgerTaskStatus::Running] {
            let mut ledger = WorkingLedger::new("contract:rehydration", "plan");
            ledger.add_task("task", "task", &config).unwrap();
            ledger.tasks[0].status = status;
            assert!(matches!(
                LedgerOrchestrator::new(
                    config.clone(),
                    ledger,
                    AdaptiveController,
                    MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
                    MockVerifier::pass_all(),
                ),
                Err(OrchestrationError::InvalidControllerDecision(_))
            ));
        }

        let mut ledger = WorkingLedger::new("contract:rehydration", "plan");
        ledger.add_task("task", "task", &config).unwrap();
        ledger.no_progress_streak = 1;
        ledger.last_progress_fingerprint = Some(sha256_hex("last-progress"));
        let serialized = serde_json::to_string(&ledger).unwrap();
        let rehydrated: WorkingLedger = serde_json::from_str(&serialized).unwrap();
        let orchestrator = LedgerOrchestrator::new(
            config,
            rehydrated,
            AdaptiveController,
            MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
            MockVerifier::pass_all(),
        )
        .unwrap();
        assert_eq!(orchestrator.consecutive_no_progress, 1);
        assert_eq!(
            orchestrator.last_fingerprint,
            Some(sha256_hex("last-progress"))
        );
    }

    #[test]
    fn truncation_without_effect_free_attestation_is_fenced_unknown() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:trunc-unknown", "plan");
        ledger.add_task("task", "task", &config).unwrap();
        let worker = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Truncated,
                effect_free: false,
                output_digest: Some(sha256_hex("partial")),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: None,
                effect_receipt: None,
            })
        });
        let mut orchestrator = LedgerOrchestrator::new(
            config,
            ledger,
            AdaptiveController,
            worker,
            MockVerifier::pass_all(),
        )
        .unwrap();
        assert!(matches!(
            orchestrator.run_to_completion(),
            Err(OrchestrationError::InvalidControllerDecision(_))
        ));
        assert_eq!(orchestrator.state, OrchestrationLifecycleState::Failed);
        assert_eq!(
            orchestrator.ledger.get_task("task").unwrap().status,
            LedgerTaskStatus::OutcomeUnknown
        );
    }

    #[test]
    fn rollback_advances_attempt_generation_before_retry() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:rollback-generation", "plan");
        ledger.add_task("task", "task", &config).unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let calls_clone = calls.clone();
        let worker = MockWorker::new(move |ctx| {
            if calls_clone.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(WorkerResult {
                    task_id: ctx.selected_task.id.clone(),
                    attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                    status: WorkerOutcomeStatus::Truncated,
                    effect_free: true,
                    output_digest: Some(sha256_hex("partial")),
                    partial_summary: None,
                    artifact_refs: vec![],
                    findings: vec![],
                    failure_reason: None,
                    usage: None,
                    effect_receipt: None,
                })
            } else {
                Ok(WorkerResult {
                    task_id: ctx.selected_task.id.clone(),
                    attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                    status: WorkerOutcomeStatus::Completed,
                    effect_free: true,
                    output_digest: Some(sha256_hex("complete")),
                    partial_summary: None,
                    artifact_refs: vec![],
                    findings: vec![],
                    failure_reason: None,
                    usage: None,
                    effect_receipt: None,
                })
            }
        });
        let mut orchestrator = LedgerOrchestrator::new(
            config,
            ledger,
            AdaptiveController,
            worker,
            MockVerifier::pass_all(),
        )
        .unwrap();
        orchestrator.step().unwrap();
        orchestrator.step().unwrap();
        orchestrator.step().unwrap();
        let first_attempt = orchestrator
            .ledger
            .get_task("task")
            .unwrap()
            .attempt_id
            .clone()
            .unwrap();
        assert_eq!(orchestrator.ledger.attempt_generation, 0);

        orchestrator.rollback_to_checkpoint().unwrap();
        assert_eq!(orchestrator.ledger.attempt_generation, 1);
        assert_eq!(
            orchestrator.ledger.get_task("task").unwrap().status,
            LedgerTaskStatus::Pending
        );
        orchestrator.step().unwrap();
        orchestrator.step().unwrap();
        let second_attempt = orchestrator
            .ledger
            .get_task("task")
            .unwrap()
            .attempt_id
            .clone()
            .unwrap();
        assert_ne!(first_attempt, second_attempt);
    }

    #[test]
    fn ledger_summaries_reject_transcripts_and_paths() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:summaries", "plan");
        assert!(ledger
            .add_task("task", "assistant: raw output", &config)
            .is_err());
        assert!(ledger
            .add_task("task", "private /home/user/file", &config)
            .is_err());
        assert!(ledger
            .add_task("task", "safe semantic summary", &config)
            .is_ok());
        assert!(ledger
            .add_finding(
                LedgerFinding {
                    id: "finding".to_string(),
                    summary: "```raw output```".to_string(),
                    source: "worker".to_string(),
                    related_task_id: Some("task".to_string()),
                    evidence_digest: None,
                },
                &config,
            )
            .is_err());
    }

    #[test]
    fn truncation_budget_is_enforced() {
        let config = LedgerOrchestratorConfig {
            max_truncations: 1,
            max_task_attempts: 10,
            max_orchestration_rounds: 10,
            ..Default::default()
        };
        let mut ledger = WorkingLedger::new("contract:trunc-budget", "bounded truncation");
        ledger
            .add_task("task-trunc", "always truncated", &config)
            .unwrap();
        let worker = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Truncated,
                effect_free: true,
                output_digest: Some(sha256_hex("partial")),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: None,
                effect_receipt: None,
            })
        });
        let mut orchestrator = LedgerOrchestrator::new(
            config,
            ledger,
            AdaptiveController,
            worker,
            MockVerifier::pass_all(),
        )
        .unwrap();

        let result = orchestrator.run_to_completion();
        assert!(matches!(
            result,
            Err(OrchestrationError::TruncationLimitExceeded(1))
        ));
    }

    #[test]
    fn outcome_unknown_fence_survives_rehydration_and_replan() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:unknown-fence", "reconcile safely");
        ledger
            .add_task("task-unknown", "external attempt", &config)
            .unwrap();
        ledger.tasks[0].status = LedgerTaskStatus::OutcomeUnknown;
        ledger
            .outcome_unknown_task_ids
            .push("task-unknown".to_string());

        let rehydrated: WorkingLedger =
            serde_json::from_str(&serde_json::to_string(&ledger).unwrap()).unwrap();
        assert!(!rehydrated.all_verified());
        let mut forged = rehydrated.clone();
        forged.tasks[0].status = LedgerTaskStatus::Superseded;
        assert!(matches!(
            LedgerOrchestrator::new(
                config.clone(),
                forged,
                AdaptiveController,
                MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
                MockVerifier::pass_all(),
            ),
            Err(OrchestrationError::InvalidControllerDecision(_))
        ));

        let controller = ScriptedController::new(vec![NextTaskDecision {
            action: ControllerAction::Replan {
                new_plan_summary: None,
                new_tasks: vec![],
                supersede_task_ids: vec!["task-unknown".to_string()],
            },
            reason_summary: "attempt to erase unknown outcome".to_string(),
            expected_evidence: "reconciliation".to_string(),
        }]);
        let mut orchestrator = LedgerOrchestrator::new(
            config,
            rehydrated,
            controller,
            MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
            MockVerifier::pass_all(),
        )
        .unwrap();
        orchestrator.step().unwrap();
        assert!(matches!(
            orchestrator.step(),
            Err(OrchestrationError::InvalidControllerDecision(_))
        ));
    }

    #[test]
    fn completion_requires_verified_tasks_and_digest_covers_state() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:digest", "original plan");
        ledger
            .add_task("task-digest", "digest task", &config)
            .unwrap();
        let mut forged = ledger.clone();
        forged.tasks[0].status = LedgerTaskStatus::Verified;
        assert!(!forged.all_verified());
        assert!(matches!(
            LedgerOrchestrator::new(
                config.clone(),
                forged,
                AdaptiveController,
                MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
                MockVerifier::pass_all(),
            ),
            Err(OrchestrationError::InvalidControllerDecision(_))
        ));
        let mut changed = ledger.clone();
        changed.current_plan = "changed plan".to_string();
        changed.tasks[0].attempt_count = 1;
        changed.tasks[0].failure_reason = Some("failed once".to_string());
        assert_ne!(ledger.state_digest(), changed.state_digest());

        let controller = ScriptedController::new(vec![NextTaskDecision {
            action: ControllerAction::DeclareComplete {
                deliverable_digest: "premature".to_string(),
            },
            reason_summary: "premature completion".to_string(),
            expected_evidence: "missing".to_string(),
        }]);
        let mut orchestrator = LedgerOrchestrator::new(
            config,
            ledger,
            controller,
            MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
            MockVerifier::pass_all(),
        )
        .unwrap();
        assert!(matches!(
            orchestrator.run_to_completion(),
            Err(OrchestrationError::InvalidControllerDecision(_))
        ));
    }

    #[test]
    fn failure_handling_cases() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:fail", "failure testing");

        // Invalid task id
        let err1 = ledger
            .add_task("bad task id with spaces!", "desc", &config)
            .unwrap_err();
        assert!(matches!(err1, OrchestrationError::InvalidTaskId(_)));

        // Duplicate task id
        ledger.add_task("task-dup", "desc 1", &config).unwrap();
        let err2 = ledger.add_task("task-dup", "desc 2", &config).unwrap_err();
        assert!(matches!(err2, OrchestrationError::DuplicateTaskId(_)));

        // Task not found in context projection
        let err3 = project_worker_context(&ledger, "non-existent-task", "ev").unwrap_err();
        assert!(matches!(err3, OrchestrationError::TaskNotFound(_)));

        // Cancellation handling
        let controller = AdaptiveController;
        let worker = MockWorker::new(|_| {
            Ok(WorkerResult {
                task_id: "task".to_string(),
                attempt_id: "unused-attempt".to_string(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                output_digest: Some(sha256_hex("out")),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: None,
                effect_receipt: None,
            })
        });
        let verifier = MockVerifier::pass_all();
        let mut orchestrator =
            LedgerOrchestrator::new(config.clone(), ledger, controller, worker, verifier).unwrap();
        orchestrator.cancel();
        let summary = orchestrator.run_to_completion().unwrap();
        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Cancelled
        );

        // Max rounds exhaustion
        let small_rounds_config = LedgerOrchestratorConfig {
            max_orchestration_rounds: 1,
            ..Default::default()
        };
        let mut ledger_rounds = WorkingLedger::new("contract:rounds", "plan");
        ledger_rounds
            .add_task("task-r", "desc", &small_rounds_config)
            .unwrap();
        // A bare worker failure without a store-owned receipt proves nothing
        // about the effect status, so it fences unknown instead of replaying.
        let controller_unknown = AdaptiveController;
        let worker_unknown = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Failed,
                effect_free: true,
                output_digest: None,
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: Some("retry needed".to_string()),
                usage: None,
                effect_receipt: None,
            })
        });
        let mut orchestrator_unknown = LedgerOrchestrator::new(
            small_rounds_config.clone(),
            ledger_rounds.clone(),
            controller_unknown,
            worker_unknown,
            MockVerifier::pass_all(),
        )
        .unwrap();
        let unknown = orchestrator_unknown.run_to_completion();
        assert!(unknown.is_err());
        assert!(matches!(
            unknown.unwrap_err(),
            OrchestrationError::InvalidControllerDecision(_)
        ));
        assert_eq!(
            orchestrator_unknown
                .ledger
                .get_task("task-r")
                .unwrap()
                .status,
            LedgerTaskStatus::OutcomeUnknown
        );

        let controller2 = AdaptiveController;
        let worker2 = MockWorker::new(|ctx| {
            let attempt_id = ctx.execution_metadata["attempt_id"].clone();
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: attempt_id.clone(),
                status: WorkerOutcomeStatus::Failed,
                effect_free: true,
                output_digest: None,
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: Some("retry needed".to_string()),
                usage: None,
                effect_receipt: Some(StoreEffectReceipt {
                    attempt_id,
                    disposition: EffectReceiptDisposition::FailedBeforeSendNoEffect,
                    receipt_evidence_digest: None,
                    store_evidence_ref: None,
                }),
            })
        });
        let verifier2 = MockVerifier::pass_all();
        let mut orchestrator2 = LedgerOrchestrator::new(
            small_rounds_config,
            ledger_rounds,
            controller2,
            worker2,
            verifier2,
        )
        .unwrap();
        let res = orchestrator2.run_to_completion();
        assert!(res.is_err());
        assert!(matches!(
            res.unwrap_err(),
            OrchestrationError::MaxRoundsExceeded(1)
        ));
    }

    #[test]
    fn matrix_planning_with_ledger_orchestrated_harness() {
        use crate::harness_evolution::{
            build_mx1_matrix_plan, sample_mx1_descriptor_manifest_with_ledger_harness,
            validate_mx1_descriptor_manifest, Mx1MatrixRung, LEDGER_ORCHESTRATED_HARNESS_ID,
            MX1_ARM_ZERO_HARNESS_ID,
        };

        let manifest = sample_mx1_descriptor_manifest_with_ledger_harness();
        validate_mx1_descriptor_manifest(&manifest).unwrap();

        // Check the ledger-orchestrated harness descriptor
        let ledger_harness = manifest
            .harnesses
            .iter()
            .find(|h| h.descriptor_id == LEDGER_ORCHESTRATED_HARNESS_ID)
            .expect("ledger-orchestrated harness must be present in manifest");
        assert!(ledger_harness.default_off);
        assert_eq!(ledger_harness.source_owner, "rust-engine-harness-evolution");

        // Build 2x2x3 matrix plan: 2 Harnesses * 2 Models * 3 Strategies = 12 cells
        let plan = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "task-pe7-test-matrix",
            1,
            &"a".repeat(64),
        )
        .unwrap();

        assert_eq!(plan.cells.len(), 12);
        let arm_zero_cells = plan
            .cells
            .iter()
            .filter(|c| c.identity.harness_id == MX1_ARM_ZERO_HARNESS_ID)
            .count();
        let ledger_cells = plan
            .cells
            .iter()
            .filter(|c| c.identity.harness_id == LEDGER_ORCHESTRATED_HARNESS_ID)
            .count();

        assert_eq!(arm_zero_cells, 6);
        assert_eq!(ledger_cells, 6);

        for cell in &plan.cells {
            assert_eq!(
                cell.disposition,
                crate::harness_evolution::Mx1MatrixCellDisposition::Admitted
            );
        }
    }

    #[test]
    fn adapter_normalization_ledger_orchestrated() {
        use crate::harness_evolution::{
            build_mx1_matrix_plan, sample_mx1_descriptor_manifest_with_ledger_harness,
            Mx1HarnessRunAdapter, Mx1LedgerOrchestratedHarnessAdapter, Mx1MatrixRung,
            LEDGER_ORCHESTRATED_HARNESS_ID,
        };
        use crate::product_golden_path::ProductTaskStatus;

        let manifest = sample_mx1_descriptor_manifest_with_ledger_harness();
        let descriptor = manifest
            .harnesses
            .iter()
            .find(|h| h.descriptor_id == LEDGER_ORCHESTRATED_HARNESS_ID)
            .unwrap()
            .clone();

        let adapter = Mx1LedgerOrchestratedHarnessAdapter::new(descriptor.clone()).unwrap();
        assert_eq!(
            adapter.descriptor().descriptor_id,
            LEDGER_ORCHESTRATED_HARNESS_ID
        );

        let plan = build_mx1_matrix_plan(
            &manifest,
            Mx1MatrixRung::TwoByTwoByThree,
            "task-pe7-test-norm",
            1,
            &"b".repeat(64),
        )
        .unwrap();

        let cell = plan
            .cells
            .iter()
            .find(|c| c.identity.harness_id == LEDGER_ORCHESTRATED_HARNESS_ID)
            .unwrap();

        let terminal = serde_json::json!({
            "schema_version": "product_task_terminal_evidence.v2",
            "product_task_id": "task-pe7-test-norm",
            "task_status": "completed",
            "workspace_scope_id": "norm-workspace-scope",
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
        let mut terminal_val = terminal.clone();
        let digest = hex::encode(sha2::Sha256::digest(
            serde_json::to_vec(&terminal_val).unwrap(),
        ));
        terminal_val["content_sha256"] = serde_json::Value::String(digest);

        let mut evidence = crate::product_golden_path::project_product_harness_run(
            &serde_json::json!({
                "task_id": "task-pe7-test-norm",
                "status": "completed",
                "workspace_id": "norm-workspace-scope",
                "workspace_binding": {
                    "workspace_id": "norm-workspace",
                    "workspace_path": "/private/norm-workspace",
                    "source_revision": "0123456789abcdef0123456789abcdef01234567",
                    "allowed_paths": ["engine/src/harness_evolution.rs"]
                },
                "failure_code": serde_json::Value::Null,
                "failure_detail": serde_json::Value::Null,
            }),
            Some(&terminal_val),
        )
        .unwrap();
        evidence.matrix_binding = Some(crate::product_golden_path::ProductHarnessMatrixBinding {
            plan_id: plan.plan_id.clone(),
            manifest_sha256: plan.manifest_sha256.clone(),
            rung: plan.rung.as_str().to_string(),
            repetition: plan.repetition,
            cell_id: cell.cell_id.clone(),
            cell_descriptor_sha256: cell.descriptor_digest.clone(),
            harness_id: cell.identity.harness_id.clone(),
            model_id: cell.identity.model_id.clone(),
            strategy_id: cell.identity.strategy_id.clone(),
            task_id: cell.identity.task_id.clone(),
        });

        let mut forged_plan = plan.clone();
        forged_plan.plan_id = "0".repeat(64);
        assert_eq!(
            adapter
                .normalize_run(&forged_plan, cell, &evidence)
                .unwrap_err()
                .code,
            "mx1_matrix_plan_drift"
        );

        let mut foreign_evidence = evidence.clone();
        foreign_evidence.product_task_id = "different-product-task".to_string();
        assert_eq!(
            adapter
                .normalize_run(&plan, cell, &foreign_evidence)
                .unwrap_err()
                .code,
            "mx1_run_task_binding"
        );

        let normalized = adapter.normalize_run(&plan, cell, &evidence).unwrap();
        assert_eq!(normalized.cell_id, cell.cell_id);
        assert_eq!(normalized.harness_id, LEDGER_ORCHESTRATED_HARNESS_ID);
        assert_eq!(normalized.terminal_outcome, ProductTaskStatus::Completed);
    }

    fn scripted_execute(task_id: &str, count: usize) -> Vec<NextTaskDecision> {
        (0..count)
            .map(|round| NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: task_id.to_string(),
                },
                reason_summary: format!("attempt {round}"),
                expected_evidence: "ev".to_string(),
            })
            .collect()
    }

    fn effect_free_receipt(
        ctx: &WorkerContext,
        disposition: EffectReceiptDisposition,
    ) -> StoreEffectReceipt {
        StoreEffectReceipt {
            attempt_id: ctx.execution_metadata["attempt_id"].clone(),
            disposition,
            receipt_evidence_digest: None,
            store_evidence_ref: None,
        }
    }

    #[test]
    fn verification_recovery_count_tracks_exactly_one_recovery() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:recovery", "repair loop");
        ledger
            .add_task("task-fix", "fix the defect", &config)
            .unwrap();
        let controller = AdaptiveController;
        let worker_rounds = Arc::new(AtomicU32::new(0));
        let worker_rounds_in_handler = Arc::clone(&worker_rounds);
        let worker = MockWorker::new(move |ctx| {
            let round = worker_rounds_in_handler.fetch_add(1, Ordering::SeqCst);
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                // Each attempt produces a distinct output so the
                // verification failures below are novel progress, not a
                // no-progress loop.
                output_digest: Some(sha256_hex(&format!("repaired-output-{round}"))),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: None,
                effect_receipt: None,
            })
        });
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_in_verifier = Arc::clone(&attempts);
        let verifier = MockVerifier::new(move |_, _, _| {
            let attempt = attempts_in_verifier.fetch_add(1, Ordering::SeqCst);
            // Fail twice, then pass; the terminal completion pass and the
            // extra selection pass must not double-count the recovery.
            let outcome = if attempt < 2 {
                VerificationOutcome::Fail
            } else {
                VerificationOutcome::Pass
            };
            Ok(VerificationReport {
                outcome,
                observation_summary: "repair verification".to_string(),
                evidence_digest: Some(sha256_hex("repair-evidence")),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier).unwrap();
        let summary = orchestrator.run_to_completion().unwrap();
        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        assert_eq!(summary.metrics.verification_recovery_count, 1);
        assert_eq!(orchestrator.metrics.verification_recovery_count, 1);
    }

    #[test]
    fn contract_digest_is_canonical_and_migrated_explicitly() {
        // Arbitrary text becomes a canonical digest with bounded context.
        let ledger = WorkingLedger::new("contract:human-readable-v1", "plan");
        assert!(is_canonical_sha256(&ledger.contract_digest));
        assert_eq!(
            ledger.contract_digest,
            sha256_hex("contract:human-readable-v1")
        );
        assert_eq!(
            ledger.contract_summary.as_deref(),
            Some("contract:human-readable-v1")
        );
        // An already-canonical digest is adopted with no separate summary.
        let canonical = sha256_hex("exact-contract");
        let ledger2 = WorkingLedger::new(canonical.clone(), "plan");
        assert_eq!(ledger2.contract_digest, canonical);
        assert_eq!(ledger2.contract_summary, None);
        // Worker context binds the real identity and carries the summary.
        // Forged or non-canonical digests fail closed at the bounds check.
        let mut forged_value =
            serde_json::to_value(WorkingLedger::new("contract:forged", "plan")).unwrap();
        forged_value["contract_digest"] = serde_json::Value::String("not-a-digest".to_string());
        let forged: WorkingLedger = serde_json::from_value(forged_value).unwrap();
        assert!(forged
            .validate_bounds(&LedgerOrchestratorConfig::default())
            .is_err());
        // v1 records migrate explicitly and only from the v1 schema.
        let mut v1_ledger = WorkingLedger::new("contract:legacy-v1", "legacy plan");
        v1_ledger.schema_version = LEDGER_ORCHESTRATED_SCHEMA_VERSION_V1.to_string();
        v1_ledger.contract_digest = "legacy contract text".to_string();
        v1_ledger.contract_summary = None;
        let serialized = serde_json::to_string(&v1_ledger).unwrap();
        let migrated = WorkingLedger::migrate_v1_record(&serialized).unwrap();
        assert_eq!(migrated.schema_version, LEDGER_ORCHESTRATED_SCHEMA_VERSION);
        assert_eq!(migrated.contract_digest, sha256_hex("legacy contract text"));
        assert_eq!(
            migrated.contract_summary.as_deref(),
            Some("legacy contract text")
        );
        migrated
            .validate_bounds(&LedgerOrchestratorConfig::default())
            .unwrap();
        // A v2 record must not pass through the v1 migration path.
        let v2_serialized = serde_json::to_string(&migrated).unwrap();
        assert!(WorkingLedger::migrate_v1_record(&v2_serialized).is_err());
    }

    #[test]
    fn duplicate_findings_do_not_reset_novelty() {
        let config = LedgerOrchestratorConfig {
            max_no_progress_rounds: 2,
            max_task_attempts: 10,
            max_orchestration_rounds: 20,
            ..Default::default()
        };
        let mut ledger = WorkingLedger::new("contract:novelty", "test novelty");
        ledger
            .add_task("task-stuck", "repeat identical evidence", &config)
            .unwrap();
        // The worker appends the identical finding every round. Without
        // deduplication each append would perturb the progress fingerprint
        // and no-progress would never trigger.
        let controller = ScriptedController::new(scripted_execute("task-stuck", 12));
        let worker = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                output_digest: Some(sha256_hex("identical_output")),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![LedgerFinding {
                    id: "same-finding".to_string(),
                    summary: "identical observation".to_string(),
                    source: "worker".to_string(),
                    related_task_id: Some(ctx.selected_task.id.clone()),
                    evidence_digest: Some(sha256_hex("identical_evidence")),
                }],
                failure_reason: None,
                usage: None,
                effect_receipt: None,
            })
        });
        let verifier = MockVerifier::new(|_, _, _| {
            Ok(VerificationReport {
                outcome: VerificationOutcome::Fail,
                observation_summary: "identical failure".to_string(),
                evidence_digest: Some(sha256_hex("identical_evidence")),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier).unwrap();
        let summary = orchestrator.run_to_completion().unwrap();
        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::NoProgress
        );
        assert_eq!(orchestrator.ledger.findings.len(), 1);
    }

    #[test]
    fn genuinely_new_evidence_counts_as_progress() {
        let config = LedgerOrchestratorConfig {
            max_no_progress_rounds: 3,
            max_task_attempts: 10,
            max_orchestration_rounds: 20,
            ..Default::default()
        };
        let mut ledger = WorkingLedger::new("contract:progress", "test progress");
        ledger
            .add_task("task-moving", "produce new evidence", &config)
            .unwrap();
        let controller = ScriptedController::new(scripted_execute("task-moving", 6));
        let rounds = Arc::new(AtomicU32::new(0));
        let rounds_in_worker = Arc::clone(&rounds);
        let worker = MockWorker::new(move |ctx| {
            let round = rounds_in_worker.fetch_add(1, Ordering::SeqCst);
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                output_digest: Some(sha256_hex(&format!("output-round-{round}"))),
                partial_summary: None,
                artifact_refs: vec![format!("artifact:round-{round}")],
                findings: vec![LedgerFinding {
                    id: format!("finding-{round}"),
                    summary: format!("new observation {round}"),
                    source: "worker".to_string(),
                    related_task_id: Some(ctx.selected_task.id.clone()),
                    evidence_digest: Some(sha256_hex(&format!("evidence-{round}"))),
                }],
                failure_reason: None,
                usage: None,
                effect_receipt: None,
            })
        });
        let verifier = MockVerifier::new(|_, _, _| {
            Ok(VerificationReport {
                outcome: VerificationOutcome::Fail,
                observation_summary: "still failing".to_string(),
                evidence_digest: Some(sha256_hex("failing")),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, verifier).unwrap();
        // Six rounds of strictly novel evidence must not trip the
        // no-progress bound of three.
        for _ in 0..12 {
            let state = orchestrator.step().unwrap();
            assert_ne!(state, OrchestrationLifecycleState::NoProgress);
            if state == OrchestrationLifecycleState::Failed {
                break;
            }
        }
        assert_ne!(orchestrator.state, OrchestrationLifecycleState::NoProgress);
    }

    #[test]
    fn round_usage_conserves_into_cell_totals_with_explicit_missingness() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:usage", "measure cost");
        ledger
            .add_task("task-a", "first measured task", &config)
            .unwrap();
        ledger
            .add_task("task-b", "second partially measured task", &config)
            .unwrap();
        let controller = AdaptiveController;
        let worker = MockWorker::new(|ctx| {
            let envelope = if ctx.selected_task.id == "task-a" {
                OrchestrationUsageEnvelope {
                    prompt_tokens: Some(100),
                    completion_tokens: Some(50),
                    total_tokens: Some(150),
                    provider_calls: Some(1),
                    cost_usd_micros: Some(7),
                    duration_ms: Some(120),
                }
            } else {
                // The second round omits totals and cost: the cell totals
                // must become explicit missingness for those dimensions.
                OrchestrationUsageEnvelope {
                    prompt_tokens: Some(200),
                    completion_tokens: Some(60),
                    total_tokens: None,
                    provider_calls: Some(2),
                    cost_usd_micros: None,
                    duration_ms: Some(240),
                }
            };
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                output_digest: Some(sha256_hex(&format!("out-{}", ctx.selected_task.id))),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: Some(envelope),
                effect_receipt: None,
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, MockVerifier::pass_all())
                .unwrap();
        let summary = orchestrator.run_to_completion().unwrap();
        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        assert_eq!(summary.metrics.prompt_tokens, Some(300));
        assert_eq!(summary.metrics.completion_tokens, Some(110));
        assert_eq!(summary.metrics.total_tokens, None);
        assert_eq!(summary.metrics.provider_calls, Some(3));
        assert_eq!(summary.metrics.cost_usd_micros, None);
        assert_eq!(summary.metrics.duration_ms, Some(360));
        assert!(orchestrator.usage_conservation_verified());
        // A tampered or dropped round envelope must fail conservation.
        orchestrator.round_usage.pop();
        assert!(!orchestrator.usage_conservation_verified());
    }

    #[test]
    fn usage_omission_is_sticky_across_later_reports() {
        // report(100) -> omit -> report(50) must leave the dimension
        // explicitly missing, never resurrect Some(50) from the later
        // report and never silently keep Some(100).
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:sticky-usage", "sticky omission");
        for task in ["task-1", "task-2", "task-3"] {
            ledger.add_task(task, "measured task", &config).unwrap();
        }
        let controller = AdaptiveController;
        let worker = MockWorker::new(|ctx| {
            let prompt = match ctx.selected_task.id.as_str() {
                "task-1" => Some(100),
                "task-2" => None,
                _ => Some(50),
            };
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                output_digest: Some(sha256_hex(&format!("out-{}", ctx.selected_task.id))),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: Some(OrchestrationUsageEnvelope {
                    prompt_tokens: prompt,
                    completion_tokens: Some(10),
                    total_tokens: Some(110),
                    provider_calls: Some(1),
                    cost_usd_micros: Some(3),
                    duration_ms: Some(20),
                }),
                effect_receipt: None,
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, MockVerifier::pass_all())
                .unwrap();
        let summary = orchestrator.run_to_completion().unwrap();
        assert_eq!(
            summary.terminal_state,
            OrchestrationLifecycleState::Completed
        );
        // The omitted-then-reported prompt dimension stays missing.
        assert_eq!(summary.metrics.prompt_tokens, None);
        // Dimensions reported by every round still conserve exactly.
        assert_eq!(summary.metrics.completion_tokens, Some(30));
        assert_eq!(summary.metrics.provider_calls, Some(3));
        assert!(orchestrator.usage_conservation_verified());
        assert!(orchestrator.usage_omitted[0]);
        assert!(!orchestrator.usage_omitted[1]);
    }

    #[test]
    fn usage_overflow_is_fail_closed() {
        let merged = OrchestrationUsageEnvelope {
            prompt_tokens: Some(u64::MAX),
            ..Default::default()
        }
        .checked_merge(&OrchestrationUsageEnvelope {
            prompt_tokens: Some(1),
            ..Default::default()
        });
        assert!(matches!(
            merged.unwrap_err(),
            OrchestrationError::WorkerExecutionError(_)
        ));
    }

    #[test]
    fn effect_receipt_gates_retry_safety() {
        // A failed attempt with an outcome-unknown receipt fences unknown and
        // is never replayed: the worker runs exactly once.
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:receipt", "retry safety");
        ledger
            .add_task("task-fx", "effectful task", &config)
            .unwrap();
        let calls = Arc::new(AtomicU32::new(0));
        let calls_in_worker = Arc::clone(&calls);
        let controller = ScriptedController::new(scripted_execute("task-fx", 4));
        let worker = MockWorker::new(move |ctx| {
            calls_in_worker.fetch_add(1, Ordering::SeqCst);
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Failed,
                effect_free: false,
                output_digest: None,
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: Some("transport broke".to_string()),
                usage: None,
                effect_receipt: Some(effect_free_receipt(
                    ctx,
                    EffectReceiptDisposition::OutcomeUnknown,
                )),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, MockVerifier::pass_all())
                .unwrap();
        let record = orchestrator.run_bounded();
        assert_eq!(
            record.disposition,
            LedgerTerminalDisposition::OutcomeUnknown
        );
        assert_eq!(record.terminal_state, OrchestrationLifecycleState::Failed);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(record.outcome_unknown_task_ids, vec!["task-fx".to_string()]);
        assert_eq!(
            orchestrator.ledger.get_task("task-fx").unwrap().status,
            LedgerTaskStatus::OutcomeUnknown
        );

        // A failure receipt that proves no effect was sent is retryable under
        // the attempt budget.
        let config2 = LedgerOrchestratorConfig {
            max_task_attempts: 2,
            max_orchestration_rounds: 10,
            ..Default::default()
        };
        let mut ledger2 = WorkingLedger::new("contract:receipt-retry", "retry safety");
        ledger2
            .add_task("task-rx", "safe retry task", &config2)
            .unwrap();
        let controller2 = AdaptiveController;
        let worker2 = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Failed,
                effect_free: true,
                output_digest: None,
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: Some("retry needed".to_string()),
                usage: None,
                effect_receipt: Some(effect_free_receipt(
                    ctx,
                    EffectReceiptDisposition::FailedBeforeSendNoEffect,
                )),
            })
        });
        let mut orchestrator2 = LedgerOrchestrator::new(
            config2,
            ledger2,
            controller2,
            worker2,
            MockVerifier::pass_all(),
        )
        .unwrap();
        let record2 = orchestrator2.run_bounded();
        assert_eq!(
            orchestrator2
                .ledger
                .get_task("task-rx")
                .unwrap()
                .attempt_count,
            2
        );
        assert!(record2.outcome_unknown_task_ids.is_empty());
    }

    #[test]
    fn truncation_with_issued_effect_receipt_is_fenced_unknown() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:trunc-receipt", "truncation safety");
        ledger
            .add_task("task-t", "truncated task", &config)
            .unwrap();
        let controller = ScriptedController::new(scripted_execute("task-t", 4));
        let worker = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Truncated,
                // Attests effect-free, but the receipt records a known
                // failed effect: the receipt dominates and fences unknown.
                effect_free: true,
                output_digest: Some(sha256_hex("partial")),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: None,
                effect_receipt: Some(effect_free_receipt(
                    ctx,
                    EffectReceiptDisposition::KnownFailedEffect,
                )),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, MockVerifier::pass_all())
                .unwrap();
        let record = orchestrator.run_bounded();
        assert_eq!(
            record.disposition,
            LedgerTerminalDisposition::OutcomeUnknown
        );
        assert_eq!(
            orchestrator.ledger.get_task("task-t").unwrap().status,
            LedgerTaskStatus::OutcomeUnknown
        );
    }

    #[test]
    fn completed_claim_with_unknown_receipt_is_fenced_unknown() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:completed-receipt", "completion safety");
        ledger
            .add_task("task-c", "claimed completion", &config)
            .unwrap();
        let controller = ScriptedController::new(scripted_execute("task-c", 4));
        let worker = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: false,
                output_digest: Some(sha256_hex("claimed")),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: None,
                effect_receipt: Some(effect_free_receipt(
                    ctx,
                    EffectReceiptDisposition::OutcomeUnknown,
                )),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, MockVerifier::pass_all())
                .unwrap();
        let record = orchestrator.run_bounded();
        assert_eq!(
            record.disposition,
            LedgerTerminalDisposition::OutcomeUnknown
        );
        assert_eq!(record.failure_code, "invalid_controller_decision");
        assert!(record.summary.is_none());
    }

    #[test]
    fn receipt_bound_to_another_attempt_is_rejected() {
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:receipt-binding", "receipt binding");
        ledger.add_task("task-b", "bound task", &config).unwrap();
        let controller = ScriptedController::new(scripted_execute("task-b", 2));
        let worker = MockWorker::new(|ctx| {
            Ok(WorkerResult {
                task_id: ctx.selected_task.id.clone(),
                attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                status: WorkerOutcomeStatus::Completed,
                effect_free: true,
                output_digest: Some(sha256_hex("out")),
                partial_summary: None,
                artifact_refs: vec![],
                findings: vec![],
                failure_reason: None,
                usage: None,
                effect_receipt: Some(StoreEffectReceipt {
                    attempt_id: sha256_hex("some-other-attempt"),
                    disposition: EffectReceiptDisposition::FailedBeforeSendNoEffect,
                    receipt_evidence_digest: None,
                    store_evidence_ref: None,
                }),
            })
        });
        let mut orchestrator =
            LedgerOrchestrator::new(config, ledger, controller, worker, MockVerifier::pass_all())
                .unwrap();
        let record = orchestrator.run_bounded();
        assert_eq!(
            record.disposition,
            LedgerTerminalDisposition::OutcomeUnknown
        );
        assert_eq!(record.failure_code, "invalid_controller_decision");
    }

    #[test]
    fn run_bounded_returns_typed_terminal_records() {
        // Successful completion carries a summary and records no failure.
        let config = LedgerOrchestratorConfig::default();
        let mut ledger = WorkingLedger::new("contract:terminal-ok", "terminal success");
        ledger.add_task("task-ok", "simple task", &config).unwrap();
        let mut ok = LedgerOrchestrator::new(
            config.clone(),
            ledger,
            AdaptiveController,
            MockWorker::new(|ctx| {
                Ok(WorkerResult {
                    task_id: ctx.selected_task.id.clone(),
                    attempt_id: ctx.execution_metadata["attempt_id"].clone(),
                    status: WorkerOutcomeStatus::Completed,
                    effect_free: true,
                    output_digest: Some(sha256_hex("done")),
                    partial_summary: None,
                    artifact_refs: vec![],
                    findings: vec![],
                    failure_reason: None,
                    usage: Some(OrchestrationUsageEnvelope {
                        prompt_tokens: Some(10),
                        completion_tokens: Some(5),
                        total_tokens: Some(15),
                        provider_calls: Some(1),
                        cost_usd_micros: Some(2),
                        duration_ms: Some(30),
                    }),
                    effect_receipt: None,
                })
            }),
            MockVerifier::pass_all(),
        )
        .unwrap();
        let ok_record = ok.run_bounded();
        assert_eq!(ok_record.disposition, LedgerTerminalDisposition::Completed);
        assert_eq!(ok_record.failure_code, "completed");
        assert_eq!(ok_record.failure_reason_digest, None);
        assert!(ok_record.summary.is_some());
        assert!(ok_record.usage_conservation_verified);
        assert!(ok.terminal_record.is_some());

        // Max-round exhaustion yields a typed record with pre-failure
        // metrics and no converted success.
        let small = LedgerOrchestratorConfig {
            max_orchestration_rounds: 1,
            ..Default::default()
        };
        let mut ledger_rounds = WorkingLedger::new("contract:terminal-rounds", "terminal rounds");
        ledger_rounds
            .add_task("task-r", "round task", &small)
            .unwrap();
        let mut rounds = LedgerOrchestrator::new(
            small,
            ledger_rounds,
            ScriptedController::new(scripted_execute("task-r", 4)),
            MockWorker::new(|ctx| {
                let attempt_id = ctx.execution_metadata["attempt_id"].clone();
                Ok(WorkerResult {
                    task_id: ctx.selected_task.id.clone(),
                    attempt_id: attempt_id.clone(),
                    status: WorkerOutcomeStatus::Failed,
                    effect_free: true,
                    output_digest: None,
                    partial_summary: None,
                    artifact_refs: vec![],
                    findings: vec![],
                    failure_reason: Some("retry".to_string()),
                    usage: None,
                    effect_receipt: Some(StoreEffectReceipt {
                        attempt_id,
                        disposition: EffectReceiptDisposition::FailedBeforeSendNoEffect,
                        receipt_evidence_digest: None,
                        store_evidence_ref: None,
                    }),
                })
            }),
            MockVerifier::pass_all(),
        )
        .unwrap();
        let rounds_record = rounds.run_bounded();
        assert_eq!(
            rounds_record.disposition,
            LedgerTerminalDisposition::MaxRoundsExhausted
        );
        assert_eq!(rounds_record.failure_code, "max_rounds_exceeded");
        assert!(rounds_record.failure_reason_digest.is_some());
        assert!(rounds_record.summary.is_none());

        // A malformed controller response is typed, not coerced.
        let mut ledger_bad = WorkingLedger::new("contract:terminal-bad", "terminal malformed");
        ledger_bad
            .add_task("task-m", "malformed task", &config)
            .unwrap();
        let mut bad = LedgerOrchestrator::new(
            LedgerOrchestratorConfig::default(),
            ledger_bad,
            ScriptedController::new(vec![NextTaskDecision {
                action: ControllerAction::DeclareComplete {
                    deliverable_digest: "wrong-digest".to_string(),
                },
                reason_summary: "forged completion".to_string(),
                expected_evidence: "ev".to_string(),
            }]),
            MockWorker::new(|_| Err(OrchestrationError::Cancelled)),
            MockVerifier::pass_all(),
        )
        .unwrap();
        let bad_record = bad.run_bounded();
        assert_eq!(
            bad_record.disposition,
            LedgerTerminalDisposition::MalformedControllerResponse
        );
        assert_eq!(bad_record.failure_code, "invalid_controller_decision");
        assert!(bad_record.summary.is_none());
    }
}
