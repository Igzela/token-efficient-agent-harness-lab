#![allow(unused_imports)]

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::cli::codex_budget_authority::{
    CodexBudgetAuthority, CodexBudgetGateway, CodexExecutableIdentity, CodexProviderIdentity,
    CodexUpstreamAuth, CODEX_BUDGET_AUTHORITY_SCHEMA,
};
use crate::harness_evolution::{
    ControllerAction, EffectReceiptDisposition, LedgerController, LedgerOrchestrator,
    LedgerOrchestratorConfig, LedgerTaskRecord, LedgerTaskStatus, LedgerTerminalDisposition,
    LedgerTerminalRecord, LedgerVerifier, LedgerWorker, NewTaskSpec, NextTaskDecision,
    OrchestrationError, OrchestrationUsageEnvelope, StoreEffectReceipt, VerificationOutcome,
    VerificationReport, WorkerContext, WorkerOutcomeStatus, WorkerResult, WorkingLedger,
    LEDGER_ORCHESTRATED_HARNESS_ID,
};
use crate::product_golden_path::{validate_matrix_binding, ProductHarnessMatrixBinding};
use crate::provider::config::{CredentialRef, ProviderConfig};
use crate::provider::credential::CredentialBoundary;
use crate::provider::managed_deepseek::{
    DeepSeekProtocol, ManagedAuthoritySource, ManagedCallBinding, ManagedCallLimits,
    ManagedDeepSeekProvider, ManagedFailureEffect, ManagedFunctionCall, ManagedMessage,
    ManagedModelRole, ManagedProviderCallAuthority, ManagedProviderCallError,
    ManagedProviderCallRequest, ManagedProviderResponse, ManagedToolCall, ManagedUsage,
    DEEPSEEK_CREDENTIAL_REFERENCE, DEEPSEEK_OPENAI_BASE_URL, DEEPSEEK_OPENAI_PATH,
    DEEPSEEK_PROVIDER_KIND,
};
use crate::provider::managed_deepseek_executor::{
    ManagedDeepSeekExecutorConfig, ManagedDeepSeekNodeExecutor, ManagedNodeProvider,
};
use crate::provider::transport::{HttpError, HttpRequest, HttpTransport};
use crate::rwe::corpus::RweTaskDefinition;
use crate::rwe::frozen_rwe_bindings::{
    current_comparison_manifest, rwe_composition_seam_ready, RweCellBudgetEnvelope,
    FROZEN_RWE_RISK_CLASS, FROZEN_RWE_TARGET_MAIN_SHA,
};
use crate::rwe::operator_corpus::{
    freeze_current_operator_contract_set, OperatorFrozenContractSet, OPERATOR_ADMITTED_BINARY_PATH,
    OPERATOR_ADMITTED_BINARY_VERSION, OPERATOR_ADMITTED_MODEL,
    OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL, OPERATOR_TARGET_REPO,
};
use crate::rwe::runner::{
    persist_rwe_run_authorization_v2, RWE_RUN_AUTH_V2_SCHEMA, RWE_RUN_EVIDENCE_SCHEMA,
};
use crate::storage::local_product_store::{
    compute_attempt_manifest_sha256, AuthenticatedPrincipal, DelegationContract, LocalProductStore,
    RweAuthorizationV2IssueRequest, DELEGATION_SCHEMA_VERSION,
};

use super::cell::*;
use super::provider::*;
use super::schedule::*;
use super::{
    CODEX_PREREQUISITE_DEFAULT_MAX_RECOVERY_GENERATIONS, CODEX_PREREQUISITE_FALLBACK_TASK_ID,
    CODEX_PREREQUISITE_PRIMARY_TASK_ID, LIVE_LEDGER_MAX_PROMPT_BYTES,
    LIVE_LEDGER_MAX_VERIFIER_OUTPUT_BYTES, RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
    RWE_CODEX_PREREQUISITE_MAX_RECOVERY_GENERATIONS_TOKEN, RWE_LEDGER_CELL_COMPOSITION_SEAM,
    RWE_LEDGER_MANAGER_SEMANTICS, RWE_LIVE_BASELINE_COORDINATOR_SCHEMA,
    RWE_LIVE_CELL_COMPOSITION_SEAM, RWE_LIVE_LEDGER_CELL_COMPOSITION, RWE_OPERATOR_LIVE_RUN_TOKEN,
};

/// Provider-free deterministic manager for ledger-orchestrated cells.
/// Composition only: drives the single cell-bound work task to a verified
/// terminal state through the admitted orchestration policy. Cannot seal
/// live evidence.
pub struct ProviderFreeLedgerCellController;

impl LedgerController for ProviderFreeLedgerCellController {
    fn decide_next_action(
        &mut self,
        ledger: &WorkingLedger,
    ) -> Result<NextTaskDecision, OrchestrationError> {
        if ledger.all_verified() {
            return Ok(NextTaskDecision {
                action: ControllerAction::DeclareComplete {
                    deliverable_digest: ledger.deliverable_digest(),
                },
                reason_summary: "provider-free fixture cell verified".to_string(),
                expected_evidence: "fixture_cell_evidence".to_string(),
            });
        }
        if let Some(pending_id) = ledger.pending_task_ids().into_iter().next() {
            return Ok(NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: pending_id.clone(),
                },
                reason_summary: format!("provider-free fixture execution of {pending_id}"),
                expected_evidence: format!("fixture_evidence_for_{pending_id}"),
            });
        }
        Ok(NextTaskDecision {
            action: ControllerAction::DeclareFailed {
                reason: "provider-free fixture cell has no executable task".to_string(),
            },
            reason_summary: "fixture terminal failure".to_string(),
            expected_evidence: String::new(),
        })
    }
}

/// Provider-free effect-free worker for ledger-orchestrated cells. Attests
/// that the attempt performed no ProductStore-owned external effect and
/// emits a deterministic output digest bound to the attempt identity.
/// Provider usage is explicit missingness: a fixture invents no cost.
pub struct EffectFreeLedgerCellWorker;

impl LedgerWorker for EffectFreeLedgerCellWorker {
    fn execute_task(
        &mut self,
        context: &WorkerContext,
    ) -> Result<WorkerResult, OrchestrationError> {
        let attempt_id = context
            .execution_metadata
            .get("attempt_id")
            .cloned()
            .unwrap_or_default();
        Ok(WorkerResult {
            task_id: context.selected_task.id.clone(),
            attempt_id: attempt_id.clone(),
            status: WorkerOutcomeStatus::Completed,
            effect_free: true,
            output_digest: Some(sha256_hex(
                format!("ledger-cell-fixture|{attempt_id}").as_bytes(),
            )),
            partial_summary: None,
            artifact_refs: vec![],
            findings: vec![],
            failure_reason: None,
            usage: None,
            effect_receipt: None,
        })
    }
}

/// Provider-free verifier for ledger-orchestrated cells. Records an
/// explicit fixture observation: provider-free composition never executes
/// the frozen verification command against the target, so the CellOutcome
/// seam reports verification as not-run and untrustworthy.
pub struct ProviderFreeLedgerCellVerifier;

impl LedgerVerifier for ProviderFreeLedgerCellVerifier {
    fn verify_task(
        &mut self,
        _ledger: &WorkingLedger,
        task: &LedgerTaskRecord,
        result: &WorkerResult,
    ) -> Result<VerificationReport, OrchestrationError> {
        if result.output_digest.is_none() {
            return Ok(VerificationReport {
                outcome: VerificationOutcome::Fail,
                observation_summary: "provider-free fixture: worker produced no output digest"
                    .to_string(),
                evidence_digest: None,
            });
        }
        Ok(VerificationReport {
            outcome: VerificationOutcome::Pass,
            observation_summary: format!(
                "provider-free fixture observation for {} (frozen verifier not executed; not live evidence)",
                task.id
            ),
            // Fixture observation digest bound to the verified attempt. It
            // records that the fixture verifier ran; it is not live
            // verification evidence (the CellOutcome seam reports not_run).
            evidence_digest: Some(sha256_hex(
                format!(
                    "ledger-cell-fixture-verification|{}|{}",
                    result.attempt_id,
                    result.output_digest.as_deref().unwrap_or("none")
                )
                .as_bytes(),
            )),
        })
    }
}

/// Live manager for ledger-orchestrated cells: deterministic bounded
/// next-action selection over live worker evidence under the admitted
/// campaign package. The manager is policy-backed, not model-backed: the
/// frozen contract defines no manager model role for ledger cells, and the
/// admitted planner/implementer/reviewer sharing belongs to the H0 3-node
/// route. Manager calls are lifecycle-accounted through the orchestration
/// metrics (`manager_call_count`, round counts); manager rounds issue no
/// provider call, so their usage contribution is explicit missingness.
pub struct LiveLedgerCellController;

impl LedgerController for LiveLedgerCellController {
    fn decide_next_action(
        &mut self,
        ledger: &WorkingLedger,
    ) -> Result<NextTaskDecision, OrchestrationError> {
        if ledger.all_verified() {
            return Ok(NextTaskDecision {
                action: ControllerAction::DeclareComplete {
                    deliverable_digest: ledger.deliverable_digest(),
                },
                reason_summary: "live ledger cell verified".to_string(),
                expected_evidence: "final_live_deliverable".to_string(),
            });
        }
        // If a task suffered a workspace action failure after provider execution,
        // do not replay: preserve evidence and terminate failed immediately (B5).
        for task in &ledger.tasks {
            if let Some(ref reason) = task.failure_reason {
                if reason.contains("managed workspace action failed") {
                    return Ok(NextTaskDecision {
                        action: ControllerAction::DeclareFailed {
                            reason: reason.clone(),
                        },
                        reason_summary: "live terminal workspace action failure; zero replay"
                            .to_string(),
                        expected_evidence: String::new(),
                    });
                }
            }
        }
        // Inspect for verification failures on active tasks that require repair replanning.
        for task in &ledger.tasks {
            if task.status == LedgerTaskStatus::Pending || task.status == LedgerTaskStatus::Failed {
                let latest_fail_obs =
                    ledger.observations.iter().rev().find(|obs| {
                        obs.task_id == task.id && obs.outcome == VerificationOutcome::Fail
                    });
                if let Some(fail_obs) = latest_fail_obs {
                    if ledger.replan_count >= 2 || task.attempt_count >= 3 {
                        return Ok(NextTaskDecision {
                            action: ControllerAction::DeclareNoProgress {
                                reason: format!(
                                    "task {} failed verification repeatedly; replan budget exhausted",
                                    task.id
                                ),
                            },
                            reason_summary: "live terminal no-progress".to_string(),
                            expected_evidence: String::new(),
                        });
                    }
                    let base_id = task.id.split("-rep-").next().unwrap_or(&task.id);
                    let base_id = base_id.split("-repair-").next().unwrap_or(base_id);
                    let repair_task_id = format!("{base_id}-rep-{}", ledger.replan_count + 1);
                    if !ledger.tasks.iter().any(|t| t.id == repair_task_id) {
                        return Ok(NextTaskDecision {
                            action: ControllerAction::Replan {
                                new_plan_summary: Some(format!(
                                    "repair task {} after verification failure: {}",
                                    task.id, fail_obs.observation_summary
                                )),
                                new_tasks: vec![NewTaskSpec {
                                    id: repair_task_id.clone(),
                                    description: format!(
                                        "repair {} based on verifier feedback: {}",
                                        task.id, fail_obs.observation_summary
                                    ),
                                }],
                                supersede_task_ids: vec![task.id.clone()],
                            },
                            reason_summary: format!(
                                "verifier failed for {}; replanning with repair task {}",
                                task.id, repair_task_id
                            ),
                            expected_evidence: format!("replan_for_{}", task.id),
                        });
                    }
                }
            }
        }
        if let Some(pending_id) = ledger.pending_task_ids().into_iter().next() {
            return Ok(NextTaskDecision {
                action: ControllerAction::ExecuteTask {
                    task_id: pending_id.clone(),
                },
                reason_summary: format!("live ledger execution of {pending_id}"),
                expected_evidence: format!("live_evidence_for_{pending_id}"),
            });
        }
        Ok(NextTaskDecision {
            action: ControllerAction::DeclareFailed {
                reason: "live ledger cell has no executable task".to_string(),
            },
            reason_summary: "live terminal failure".to_string(),
            expected_evidence: String::new(),
        })
    }
}

/// One live ledger worker round, bound to store-owned evidence.
#[derive(Debug, Clone)]
pub(crate) struct LiveLedgerRoundRef {
    pub delegated_attempt_id: String,
    pub node_id: String,
    #[allow(dead_code)]
    pub action_receipt: Option<Value>,
}

/// Folded store-owned sums over one live cell's round journals.
pub(super) struct LiveLedgerJournalSums {
    entries: Vec<Value>,
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    realized_cost_usd: f64,
    cost_missing: bool,
    duration_ms: u64,
    duration_missing: bool,
    prompt_complete: bool,
}

/// Store-derived per-round evidence for one reconciled journal entry.
pub(super) struct LiveLedgerRoundEvidence {
    pub(super) envelope: OrchestrationUsageEnvelope,
    pub(super) disposition: EffectReceiptDisposition,
    pub(super) entry_digest: String,
    pub(super) realized_cost_usd: f64,
    pub(super) provider_calls: u64,
}

/// Map one store-owned provider journal entry into the ledger usage envelope
/// and receipt disposition. Strictness mirrors
/// [`provider_execution_from_journal`]: unknown statuses, missing usage on
/// succeeded entries, invalid costs, and non-admitted provenance fail closed
/// instead of projecting false zeroes. Only `succeeded` and
/// `outcome_unknown` entries count as provider requests, exactly like the
/// accepted RWE journal projection.
pub(super) fn map_live_ledger_journal_entry(
    entry: &Value,
) -> Result<LiveLedgerRoundEvidence, String> {
    let status = entry
        .get("status")
        .and_then(Value::as_str)
        .ok_or("ledger round journal entry lacks status")?;
    let disposition = match status {
        "succeeded" => EffectReceiptDisposition::Success,
        "failed_before_send" => EffectReceiptDisposition::FailedBeforeSendNoEffect,
        "failed_known_outcome" => EffectReceiptDisposition::KnownFailedEffect,
        "outcome_unknown" => EffectReceiptDisposition::OutcomeUnknown,
        _ => {
            return Err(format!(
                "ledger round journal entry has an unmappable status: {status}"
            ));
        }
    };
    let provenance = entry
        .get("transport_provenance")
        .and_then(Value::as_str)
        .ok_or("ledger round journal entry lacks transport provenance")?;
    if !matches!(provenance, "external" | "injected") {
        return Err("ledger round journal entry transport provenance is invalid".into());
    }
    let (realized_cost_usd, cost_usd_micros) =
        match entry.get("effective_cost_usd").and_then(Value::as_f64) {
            Some(cost) => {
                if !cost.is_finite() || cost < 0.0 {
                    return Err("ledger round journal entry effective cost is invalid".into());
                }
                let micros = (cost * 1_000_000.0).round();
                if !micros.is_finite() || micros < 0.0 {
                    return Err("ledger round journal entry cost does not fit microunits".into());
                }
                (cost, Some(micros as u64))
            }
            None => (0.0, None),
        };
    let effective_tokens = entry
        .get("effective_tokens")
        .and_then(Value::as_u64)
        .ok_or("ledger round journal entry lacks effective tokens")?;
    let duration_ms = match (
        entry.get("claimed_at").and_then(Value::as_str),
        entry.get("reconciled_at").and_then(Value::as_str),
    ) {
        (Some(claimed), Some(reconciled)) => {
            parse_ledger_round_timestamp_duration(claimed, reconciled)
        }
        _ => None,
    };
    let envelope = match disposition {
        EffectReceiptDisposition::Success => {
            let input_tokens = entry
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64)
                .ok_or("ledger round journal succeeded entry lacks input tokens")?;
            let output_tokens = entry
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)
                .ok_or("ledger round journal succeeded entry lacks output tokens")?;
            OrchestrationUsageEnvelope {
                prompt_tokens: Some(input_tokens),
                completion_tokens: Some(output_tokens),
                total_tokens: Some(effective_tokens),
                provider_calls: Some(1),
                cost_usd_micros,
                duration_ms,
            }
        }
        EffectReceiptDisposition::FailedBeforeSendNoEffect
        | EffectReceiptDisposition::KnownFailedEffect => {
            // Proved zero effect: observed zeroes, never missingness.
            OrchestrationUsageEnvelope {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: Some(effective_tokens),
                provider_calls: Some(0),
                cost_usd_micros,
                duration_ms,
            }
        }
        EffectReceiptDisposition::OutcomeUnknown => {
            // Reservations are not observations: tokens stay proved-zero
            // accounting only through effective tokens, cost stays missing.
            OrchestrationUsageEnvelope {
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: Some(effective_tokens),
                provider_calls: Some(1),
                cost_usd_micros: None,
                duration_ms,
            }
        }
    };
    let entry_digest =
        sha256_hex(&serde_json::to_vec(&sort_value(entry)).map_err(|e| e.to_string())?);
    Ok(LiveLedgerRoundEvidence {
        envelope,
        disposition,
        entry_digest,
        realized_cost_usd,
        provider_calls: if disposition == EffectReceiptDisposition::FailedBeforeSendNoEffect
            || disposition == EffectReceiptDisposition::KnownFailedEffect
        {
            0
        } else {
            1
        },
    })
}

pub(super) fn parse_ledger_round_timestamp_duration(
    claimed_at: &str,
    reconciled_at: &str,
) -> Option<u64> {
    let parse = |raw: &str| {
        chrono::DateTime::parse_from_rfc3339(raw.trim())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .ok()
    };
    let claimed = parse(claimed_at)?;
    let reconciled = parse(reconciled_at)?;
    let millis = reconciled.signed_duration_since(claimed).num_milliseconds();
    u64::try_from(millis).ok()
}

/// Render one bounded worker prompt from the fresh [`WorkerContext`]. The
/// context fields are already core-sanitized summaries; this rendering only
/// bounds total bytes so the managed pre-send reservation always holds. No
/// credential, transcript, or path-bearing payload is constructed here.
pub(super) fn render_live_ledger_worker_prompt(
    context: &WorkerContext,
    objective_sha256: &str,
    task_objective: &str,
    staged_context: Option<&str>,
    strategy_projection: Option<&str>,
    allowed_paths: &[String],
    package_id: &str,
) -> String {
    fn take_lines(values: &[String], max_items: usize, max_chars: usize) -> Vec<String> {
        values
            .iter()
            .take(max_items)
            .map(|v| v.chars().take(max_chars).collect())
            .collect()
    }
    let findings: Vec<String> = take_lines(
        &context
            .relevant_findings
            .iter()
            .map(|f| format!("{}: {}", f.id, f.summary))
            .collect::<Vec<_>>(),
        5,
        400,
    );
    let observations: Vec<String> = take_lines(
        &context
            .relevant_observations
            .iter()
            .map(|o| {
                format!(
                    "round {} {:?}: {}",
                    o.round, o.outcome, o.observation_summary
                )
            })
            .collect::<Vec<_>>(),
        5,
        400,
    );
    let objective_text: String = if task_objective.trim().is_empty() {
        format!("objective hash {objective_sha256}")
    } else {
        task_objective.chars().take(2000).collect()
    };
    let strategy_line = match strategy_projection {
        Some(s) if !s.trim().is_empty() => format!("\nStrategy projection: {s}"),
        _ => String::new(),
    };
    let staged_section = match staged_context {
        Some(ctx) if !ctx.trim().is_empty() => {
            format!(
                "\nRelevant workspace content:\n{}",
                ctx.chars().take(2000).collect::<String>()
            )
        }
        _ => String::new(),
    };
    let prompt = format!(
        "Ledger-orchestrated matrix cell work (package {package_id}).\n\
         Task objective: {objective_text}\n\
         Objective sha256: {objective_sha256}{strategy_line}{staged_section}\n\
         Contract digest: {contract_digest}\n\
         Plan: {plan_summary}\n\
         Task {task_id}: {task_description}\n\
         Attempt {attempt} of round {round}.\n\
         Admitted target paths: {allowed_paths}\n\
         Expected evidence: {expected_evidence}\n\
         Relevant findings: {findings}\n\
         Prior verification observations: {observations}\n\
         If workspace modifications are required, output exactly one JSON action object (bare or in ```json ... ```):\n\
         {{\"schema_version\": \"managed_workspace_action.v1\", \"path\": \"<relative path>\", \"action\": \"replace_text\", \"old_text\": \"<exact existing text>\", \"new_text\": \"<replacement text>\"}}\n\
         Otherwise respond with a single bounded implementation proposal as plain prose \
         (no credentials, no transcripts, no absolute paths).",
        contract_digest = context.contract_digest,
        plan_summary = context.plan_summary.chars().take(800).collect::<String>(),
        task_id = context.selected_task.id,
        task_description = context.selected_task.description,
        attempt = context
            .execution_metadata
            .get("attempt")
            .map(String::as_str)
            .unwrap_or("?"),
        round = context
            .execution_metadata
            .get("round")
            .map(String::as_str)
            .unwrap_or("?"),
        allowed_paths = allowed_paths
            .join(",")
            .chars()
            .take(800)
            .collect::<String>(),
        expected_evidence = context.expected_evidence,
        findings = findings.join(" | "),
        observations = observations.join(" | "),
    );
    if prompt.len() > LIVE_LEDGER_MAX_PROMPT_BYTES {
        prompt.chars().take(LIVE_LEDGER_MAX_PROMPT_BYTES).collect()
    } else {
        prompt
    }
}

/// Live worker for ledger-orchestrated cells: exactly one delegated
/// implementer call per round through the existing managed provider
/// authority (delegation, proposal, prepare, approval, one-use spend, attempt
/// lease, persisted-authority single call with store claim/reconcile). The
/// worker never mints usage or receipts: both derive from the reconciled
/// store journal entry read back after the call. Workspace modifications,
/// when present in implementer output, apply through the store-owned
/// `apply_managed_workspace_action` boundary, strictly within allowed paths.
pub struct LiveLedgerCellWorker {
    store: std::sync::Arc<LocalProductStore>,
    operator_principal: AuthenticatedPrincipal,
    executor_principal: AuthenticatedPrincipal,
    product_task_id: String,
    delegation_id_prefix: String,
    binding: ProductHarnessMatrixBinding,
    objective_sha256: String,
    task_objective: String,
    preferred_prompt_path: Option<String>,
    allowed_paths: Vec<String>,
    max_provider_calls: u64,
    cell_cost_ceiling_usd: f64,
    transport: Option<std::sync::Arc<dyn crate::provider::transport::HttpTransport>>,
    package_id: String,
    provider_binding: crate::rwe::campaign_package::FrozenProviderExecutionBinding,
    spent_usd: f64,
    provider_calls_made: u64,
    rounds: std::sync::Arc<std::sync::Mutex<Vec<LiveLedgerRoundRef>>>,
    target_repo_path: Option<std::path::PathBuf>,
    provider_adapter: Option<std::sync::Arc<dyn LiveLedgerProviderAdapter>>,
}

impl LiveLedgerCellWorker {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        store: std::sync::Arc<LocalProductStore>,
        operator_principal: AuthenticatedPrincipal,
        executor_principal: AuthenticatedPrincipal,
        product_task_id: String,
        delegation_id_prefix: String,
        binding: ProductHarnessMatrixBinding,
        objective_sha256: String,
        task_objective: String,
        preferred_prompt_path: Option<String>,
        allowed_paths: Vec<String>,
        max_provider_calls: u64,
        cell_cost_ceiling_usd: f64,
        transport: Option<std::sync::Arc<dyn crate::provider::transport::HttpTransport>>,
        package_id: String,
        provider_binding: crate::rwe::campaign_package::FrozenProviderExecutionBinding,
        rounds: std::sync::Arc<std::sync::Mutex<Vec<LiveLedgerRoundRef>>>,
        target_repo_path: Option<std::path::PathBuf>,
        provider_adapter: Option<std::sync::Arc<dyn LiveLedgerProviderAdapter>>,
    ) -> Self {
        Self {
            store,
            operator_principal,
            executor_principal,
            product_task_id,
            delegation_id_prefix,
            binding,
            objective_sha256,
            task_objective,
            preferred_prompt_path,
            allowed_paths,
            max_provider_calls,
            cell_cost_ceiling_usd,
            transport,
            package_id,
            provider_binding,
            spent_usd: 0.0,
            provider_calls_made: 0,
            rounds,
            target_repo_path,
            provider_adapter,
        }
    }

    fn blocked_result(
        context: &WorkerContext,
        reason: &str,
    ) -> Result<WorkerResult, OrchestrationError> {
        Ok(WorkerResult {
            task_id: context.selected_task.id.clone(),
            attempt_id: context
                .execution_metadata
                .get("attempt_id")
                .cloned()
                .unwrap_or_default(),
            status: WorkerOutcomeStatus::Blocked,
            effect_free: false,
            output_digest: None,
            partial_summary: None,
            artifact_refs: vec![],
            findings: vec![],
            failure_reason: Some(reason.to_string()),
            usage: None,
            effect_receipt: None,
        })
    }

    fn worker_error(context_task: &str, detail: &str) -> OrchestrationError {
        OrchestrationError::WorkerExecutionError(format!(
            "live ledger worker round for {context_task} failed: {detail}"
        ))
    }

    fn validate_adapter_matches_binding(
        &self,
        adapter: &dyn LiveLedgerProviderAdapter,
        requested_model: &str,
    ) -> Result<(), String> {
        let binding = &self.provider_binding;
        if adapter.provider_id() != binding.provider_identity
            || adapter.provider_kind() != binding.provider_kind
            || adapter.base_url() != binding.base_url
            || adapter.endpoint_path() != binding.endpoint_path
            || adapter.credential_reference() != binding.credential_reference
            || adapter
                .requested_model()
                .is_some_and(|model| model != requested_model)
        {
            return Err(
                "provider adapter does not match the frozen provider execution binding".into(),
            );
        }
        Ok(())
    }

    /// Authorize one round attempt under existing delegated owners (contract,
    /// proposal, prepare, approval, one-use spend, attempt lease). This
    /// mirrors the H0 armed path's steps 1-5 exactly; it stops before run
    /// activation because ledger rounds drive single managed calls, not the
    /// H0 3-node tick lifecycle, and never produce Draft PR output.
    fn authorize_round_attempt(
        &self,
        round_index: usize,
        now: &str,
        expires_at: &str,
    ) -> Result<(String, String, String, String, Value), String> {
        let delegation_id = format!("{}-r{:03}", self.delegation_id_prefix, round_index);
        let attempt_input = format!(
            "{}|{}|{round_index}",
            self.product_task_id, self.binding.cell_id
        );
        let delegated_attempt_id = format!(
            "rwe-att-ledger-{}-{:03}",
            &sha256_hex(attempt_input.as_bytes())[..32],
            round_index
        );
        let (max_files, max_lines) =
            crate::rwe::frozen_rwe_bindings::frozen_rwe_max_patch_limits()?;
        // Each round delegation carries the frozen cell ceiling; the bridge
        // additionally fences cumulative cell spend before every round, so
        // the per-round one-use spend can never exceed the cell budget.
        let union_paths = crate::rwe::frozen_rwe_bindings::frozen_rwe_union_allowed_paths()?;
        let role_model = self.provider_binding.admitted_model.clone();
        let role_models = json!({
            "planner": role_model.clone(),
            "implementer": role_model.clone(),
            "reviewer": role_model,
        });
        self.provider_binding.validate()?;
        let contract = DelegationContract {
            schema_version: DELEGATION_SCHEMA_VERSION.into(),
            delegation_id: delegation_id.clone(),
            created_at: now.to_string(),
            expires_at: expires_at.to_string(),
            executions: 1,
            repositories: vec![OPERATOR_TARGET_REPO.into()],
            task_classes: vec!["rwe".into()],
            allowed_paths: union_paths,
            max_changed_files: max_files,
            max_changed_lines: max_lines,
            max_cost_usd_per_run: self.cell_cost_ceiling_usd,
            max_total_cost_usd: self.cell_cost_ceiling_usd,
            protocol: "openai_compatible".into(),
            models: role_models.clone(),
            output: json!({
                "draft_pr_only": true,
                "target_main_write": false,
                "merge": false,
                "auto_merge": false
            }),
            forbidden: vec![
                "credential changes".into(),
                "authentication or permission changes".into(),
                "schema or database migrations".into(),
                "dependency changes".into(),
                "executable or workflow changes".into(),
                "destructive operations".into(),
                "release".into(),
                "deployment".into(),
            ],
        };
        self.store.persist_delegation_for_product_task(
            &self.operator_principal,
            &self.product_task_id,
            &contract,
        )?;
        let mut proposal = json!({
            "schema_version": "managed_proposal_manifest.v1",
            "target_repository": OPERATOR_TARGET_REPO,
            "target_main_sha": FROZEN_RWE_TARGET_MAIN_SHA,
            "mutable_paths": self.allowed_paths,
            "max_cost_usd": Value::Null,
            "verifier": crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY,
            "campaign_package_id": self.package_id,
            "matrix_model_descriptor_id": self.binding.model_id,
            "provider_execution_binding": self.provider_binding.to_json(),
            "provider": {
                "execution_binding": self.provider_binding.to_json(),
                "price_profile": crate::provider::managed_deepseek::DeepSeekPriceProfile::default(),
            },
        });
        let proposal_sha = compute_attempt_manifest_sha256(&proposal)?;
        proposal["manifest_sha256"] = json!(proposal_sha);
        self.store.persist_approved_delegated_proposal(
            &delegation_id,
            &proposal,
            proposal["manifest_sha256"].as_str().unwrap(),
        )?;
        let prepared = self
            .store
            .prepare_delegated_managed_product_task(
                &self.product_task_id,
                "executor",
                &["managed_deepseek".into()],
                &proposal,
                &contract,
                &delegated_attempt_id,
            )
            .map_err(|e| {
                format!("ledger round delegated prepare failed for {delegated_attempt_id}: {e}")
            })?;
        let manifest = prepared
            .get("final_manifest")
            .cloned()
            .ok_or("ledger round delegated prepare final_manifest missing")?;
        let approval = self.store.approve_delegated_manifest(
            &self.operator_principal,
            &delegation_id,
            &manifest,
        )?;
        let approval_receipt_sha256 = approval
            .get("approval_receipt_sha256")
            .and_then(Value::as_str)
            .ok_or("ledger round delegated manifest approval receipt missing")?;
        let spend = self.store.issue_delegated_spend(
            &self.operator_principal,
            &delegation_id,
            approval_receipt_sha256,
            &manifest,
        )?;
        let spend_authorization_id = spend
            .get("spend_authorization_id")
            .and_then(Value::as_str)
            .ok_or("ledger round delegated spend authorization id missing")?;
        let lease = self.store.admit_delegated_attempt(
            &self.executor_principal,
            &delegation_id,
            &delegated_attempt_id,
            &manifest,
        )?;
        let attempt_lease_id = lease
            .get("attempt_lease_id")
            .and_then(Value::as_str)
            .ok_or("ledger round delegated attempt lease id missing")?;
        Ok((
            delegation_id,
            delegated_attempt_id,
            spend_authorization_id.to_string(),
            attempt_lease_id.to_string(),
            manifest,
        ))
    }

    /// Read back the reconciled journal entry for one round attempt from the
    /// store-owned projection. The worker never trusts its in-hand response
    /// for accounting: usage and receipt derive from this read-back.
    fn read_round_journal_entry(
        &self,
        delegated_attempt_id: &str,
        node_id: &str,
    ) -> Result<Value, String> {
        let projection = self
            .store
            .project_rwe_cell_store_evidence(&self.product_task_id, delegated_attempt_id)?;
        let journal = projection
            .get("provider_request_journal")
            .and_then(Value::as_array)
            .ok_or("ledger round store evidence lacks the provider request journal")?;
        let matches: Vec<&Value> = journal
            .iter()
            .filter(|entry| entry.get("node_id").and_then(Value::as_str) == Some(node_id))
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "ledger round journal must hold exactly one entry for node {node_id}"
            ));
        }
        Ok(matches[0].clone())
    }
}

impl LedgerWorker for LiveLedgerCellWorker {
    fn execute_task(
        &mut self,
        context: &WorkerContext,
    ) -> Result<WorkerResult, OrchestrationError> {
        let ledger_attempt_id = context
            .execution_metadata
            .get("attempt_id")
            .cloned()
            .unwrap_or_default();
        let round_index = self.rounds.lock().map(|r| r.len()).unwrap_or(0);
        // Frozen cell budget fence before any provider effect.
        if self.provider_calls_made >= self.max_provider_calls {
            return Self::blocked_result(
                context,
                "ledger cell frozen provider-call budget exhausted",
            );
        }
        let now = self
            .store
            .require_now()
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?;
        let created = chrono::DateTime::parse_from_rfc3339(&now)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|_| {
                Self::worker_error(
                    &context.selected_task.id,
                    "store clock must be canonical RFC3339/UTC",
                )
            })?;
        let expires_at = (created + chrono::Duration::hours(24))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let (
            delegation_id,
            delegated_attempt_id,
            spend_authorization_id,
            attempt_lease_id,
            manifest,
        ) = match self.authorize_round_attempt(round_index, &now, &expires_at) {
            Ok(v) => v,
            Err(e) => {
                return Err(Self::worker_error(&context.selected_task.id, &e));
            }
        };
        // The implementer position of the round manifest's exact four-stage
        // route is read from the store-owned manifest itself, never assumed.
        let node_ids = manifest
            .pointer("/execution/workflow_node_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                Self::worker_error(
                    &context.selected_task.id,
                    "ledger round manifest workflow_node_ids missing",
                )
            })?;
        if node_ids.len() != 4 {
            return Err(Self::worker_error(
                &context.selected_task.id,
                "ledger round manifest must bind the exact four-stage route",
            ));
        }
        let node_id = node_ids[1]
            .as_str()
            .ok_or_else(|| {
                Self::worker_error(
                    &context.selected_task.id,
                    "ledger round manifest implementer node is missing",
                )
            })?
            .to_string();
        let workflow_id = manifest
            .pointer("/execution/workflow_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Self::worker_error(
                    &context.selected_task.id,
                    "ledger round manifest workflow_id missing",
                )
            })?
            .to_string();
        let serving_transport = self.transport.as_ref().map(|tx| {
            std::sync::Arc::new(crate::provider::transport::InjectedTransportBoundary(
                std::sync::Arc::clone(tx),
            )) as std::sync::Arc<dyn crate::provider::transport::HttpTransport>
        });
        let adapter: std::sync::Arc<dyn LiveLedgerProviderAdapter> = match &self.provider_adapter {
            Some(a) => std::sync::Arc::clone(a),
            None => {
                let transport_for_provider: std::sync::Arc<
                    dyn crate::provider::transport::HttpTransport,
                > = match serving_transport {
                    Some(tx) => tx,
                    None => {
                        std::sync::Arc::new(crate::provider::transport::ReqwestTransport::new())
                    }
                };
                std::sync::Arc::new(OpenAiCompatibleProviderAdapter {
                    provider_id: self.provider_binding.provider_identity.clone(),
                    provider_kind: self.provider_binding.provider_kind.clone(),
                    base_url: self.provider_binding.base_url.clone(),
                    endpoint_path: self.provider_binding.endpoint_path.clone(),
                    credential_reference: self.provider_binding.credential_reference.clone(),
                    provider_subject: "provider:deepseek".to_string(),
                    transport: transport_for_provider,
                })
            }
        };
        let requested_model = manifest
            .pointer("/models/implementer")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Self::worker_error(
                    &context.selected_task.id,
                    "ledger round manifest implementer model missing",
                )
            })?
            .to_string();
        // The request envelope must equal the persisted manifest envelope
        // exactly, or the persisted authority check fails closed.
        let manifest_limits = ManagedCallLimits {
            max_requests: manifest
                .pointer("/limits/max_provider_requests")
                .and_then(Value::as_u64)
                .unwrap_or(3),
            max_retries: manifest
                .pointer("/limits/max_retries")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            max_input_tokens: manifest
                .pointer("/limits/max_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(12_000),
            max_output_tokens: manifest
                .pointer("/limits/max_output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(4_000),
            max_cumulative_tokens: manifest
                .pointer("/limits/max_cumulative_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(16_000),
            timeout_ms: manifest
                .pointer("/limits/timeout_ms")
                .and_then(Value::as_u64)
                .unwrap_or(900_000),
            max_cost_usd: manifest
                .pointer("/limits/max_cost_usd")
                .and_then(Value::as_f64),
        };
        let manifest_price_profile = serde_json::from_value(
            manifest
                .pointer("/provider/price_profile")
                .cloned()
                .ok_or_else(|| {
                    Self::worker_error(
                        &context.selected_task.id,
                        "ledger round manifest price profile missing",
                    )
                })?,
        )
        .map_err(|_| {
            Self::worker_error(
                &context.selected_task.id,
                "ledger round manifest price profile malformed",
            )
        })?;
        let binding = ManagedCallBinding {
            product_task_id: self.product_task_id.clone(),
            workflow_id,
            node_id: node_id.clone(),
            attempt_id: delegated_attempt_id.clone(),
            spend_authorization_id,
            attempt_lease_id,
        };
        let stage_context_metadata = json!({
            "managed_deepseek": {"stage": "implementation"}
        });
        let initial_stage_context = self
            .store
            .stage_context(&binding, &stage_context_metadata)
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?
            .ok_or_else(|| {
                Self::worker_error(
                    &context.selected_task.id,
                    "store returned no implementation context",
                )
            })?;
        let allowed_file_paths = initial_stage_context
            .get("allowed_file_paths")
            .and_then(Value::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let prompt_path = self
            .preferred_prompt_path
            .as_ref()
            .filter(|path| {
                allowed_file_paths
                    .iter()
                    .any(|candidate| candidate == *path)
            })
            .cloned()
            .or_else(|| allowed_file_paths.first().cloned());
        let stage_context = if let Some(prompt_path) = prompt_path {
            let mut metadata = stage_context_metadata;
            metadata["managed_deepseek"]["prompt_path"] = json!(prompt_path);
            self.store
                .stage_context(&binding, &metadata)
                .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?
                .ok_or_else(|| {
                    Self::worker_error(
                        &context.selected_task.id,
                        "store returned no selected implementation context",
                    )
                })?
        } else {
            initial_stage_context
        };
        let staged_content = serde_json::to_string(&sort_value(&stage_context)).map_err(|e| {
            Self::worker_error(
                &context.selected_task.id,
                &format!("store stage context serialization failed: {e}"),
            )
        })?;
        let strategy_projection = resolve_strategy_projection_for_cell(&self.binding)
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?;
        let prompt = render_live_ledger_worker_prompt(
            context,
            &self.objective_sha256,
            &self.task_objective,
            Some(&staged_content),
            strategy_projection.as_deref(),
            &self.allowed_paths,
            &self.package_id,
        );
        let mut request = ManagedProviderCallRequest::for_role(
            ManagedModelRole::Implementer,
            DeepSeekProtocol::OpenAiCompatible,
            binding.clone(),
        );
        request.provider_identity = self.provider_binding.provider_identity.clone();
        request.provider_kind = self.provider_binding.provider_kind.clone();
        request.host = self.provider_binding.host.clone();
        request.base_url = self.provider_binding.base_url.clone();
        request.endpoint_path = self.provider_binding.endpoint_path.clone();
        request.credential_reference = self.provider_binding.credential_reference.clone();
        request.response_schema_version = self.provider_binding.response_schema_version.clone();
        request.usage_parser_version = self.provider_binding.usage_parser_version.clone();
        request.requested_model = manifest
            .pointer("/models/implementer")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Self::worker_error(
                    &context.selected_task.id,
                    "ledger round manifest implementer model missing",
                )
            })?
            .to_string();
        request.single_model_plan = true;
        if request.requested_model != self.provider_binding.admitted_model {
            return Err(Self::worker_error(
                &context.selected_task.id,
                "matrix model binding does not match the persisted provider binding",
            ));
        }
        if let Some(adapter_model) = adapter.requested_model() {
            if adapter_model != requested_model {
                return Err(Self::worker_error(
                    &context.selected_task.id,
                    "provider adapter model does not match the persisted manifest",
                ));
            }
        }
        request.limits = manifest_limits;
        request.price_profile = manifest_price_profile;
        request.messages = vec![ManagedMessage::text("user", &prompt)];
        request.max_output_tokens = request.limits.max_output_tokens;
        request.transport_provenance =
            crate::provider::transport::production_transport_provenance(adapter.transport());
        request
            .validate()
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?;
        self.validate_adapter_matches_binding(adapter.as_ref(), &request.requested_model)
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?;
        // Frozen cell spend fence on the store-reserved cost before any
        // provider effect: cumulative realized spend plus this round's
        // conservative reservation must stay within the cell ceiling.
        let reservation = request
            .conservative_reserved_cost_usd()
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?;
        if self.spent_usd + reservation > self.cell_cost_ceiling_usd + 1e-12 {
            return Self::blocked_result(context, "ledger cell frozen cost ceiling exhausted");
        }
        let authority = std::sync::Arc::new(
            ManagedProviderCallAuthority::new(
                std::sync::Arc::clone(&self.store) as std::sync::Arc<dyn ManagedAuthoritySource>,
                request.limits.clone(),
            )
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?,
        );
        let invoke_result = adapter.invoke(&authority, &request);
        // The usage envelope and effect receipt derive from the reconciled
        // store journal entry read back after the call, never from the
        // in-hand response or the worker's own status claim.
        let journal_entry = self
            .read_round_journal_entry(&delegated_attempt_id, &node_id)
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?;
        let mut round = map_live_ledger_journal_entry(&journal_entry)
            .map_err(|e| Self::worker_error(&context.selected_task.id, &e))?;
        self.spent_usd += round.realized_cost_usd;
        self.provider_calls_made += round.provider_calls;
        let mut action_receipt: Option<Value> = None;
        match invoke_result {
            Ok(response) => {
                if round.disposition != EffectReceiptDisposition::Success {
                    // A successful provider response reconciled to a
                    // non-success journal entry is contradictory; fence
                    // unknown rather than trusting either side alone.
                    return Err(Self::worker_error(
                        &context.selected_task.id,
                        "provider response conflicts with the reconciled journal entry",
                    ));
                }
                if let Ok(action_val) =
                    LocalProductStore::parse_managed_workspace_action(&response.output_text)
                {
                    if action_val.get("schema_version").and_then(Value::as_str)
                        == Some("managed_workspace_action.v1")
                    {
                        if let Some(target_repo_path) = &self.target_repo_path {
                            let node_metadata = json!({
                                "workspace_path": target_repo_path.to_string_lossy(),
                                "workspace_root": target_repo_path.to_string_lossy(),
                                "allowed_paths": self.allowed_paths,
                            });
                            match self.store.apply_managed_workspace_action(
                                &binding,
                                &node_metadata,
                                &response.output_text,
                            ) {
                                Ok(act) => {
                                    action_receipt = Some(act);
                                    let persisted_entry = self
                                        .read_round_journal_entry(&delegated_attempt_id, &node_id)
                                        .map_err(|e| {
                                            Self::worker_error(&context.selected_task.id, &e)
                                        })?;
                                    round = map_live_ledger_journal_entry(&persisted_entry)
                                        .map_err(|e| {
                                            Self::worker_error(&context.selected_task.id, &e)
                                        })?;
                                }
                                Err(error) => {
                                    // B5: Preserve provider success and usage while recording a
                                    // separate Store-owned action failure. The provider journal
                                    // remains succeeded, so the manager sees a successful effect
                                    // with a terminal worker failure and cannot replay it.
                                    let failure_evidence_digest = self
                                        .store
                                        .record_managed_workspace_action_failure(
                                            &binding,
                                            "workspace_action_failed",
                                        )
                                        .map_err(|err| {
                                            Self::worker_error(
                                                &context.selected_task.id,
                                                &format!(
                                                    "workspace action failure evidence persist failed: {err}"
                                                ),
                                            )
                                        })?;
                                    let persisted_entry = self
                                        .read_round_journal_entry(&delegated_attempt_id, &node_id)
                                        .map_err(|e| {
                                            Self::worker_error(&context.selected_task.id, &e)
                                        })?;
                                    round = map_live_ledger_journal_entry(&persisted_entry)
                                        .map_err(|e| {
                                            Self::worker_error(&context.selected_task.id, &e)
                                        })?;
                                    if let Ok(mut rounds) = self.rounds.lock() {
                                        rounds.push(LiveLedgerRoundRef {
                                            delegated_attempt_id: delegated_attempt_id.clone(),
                                            node_id: node_id.clone(),
                                            action_receipt: None,
                                        });
                                    }
                                    let receipt = StoreEffectReceipt {
                                        attempt_id: ledger_attempt_id.clone(),
                                        disposition: EffectReceiptDisposition::Success,
                                        receipt_evidence_digest: Some(round.entry_digest.clone()),
                                        store_evidence_ref: Some(format!(
                                            "delegation:{delegation_id}:attempt:{delegated_attempt_id}:workspace_action_failure_sha256:{failure_evidence_digest}"
                                        )),
                                    };
                                    return Ok(WorkerResult {
                                        task_id: context.selected_task.id.clone(),
                                        attempt_id: ledger_attempt_id,
                                        status: WorkerOutcomeStatus::Failed,
                                        effect_free: false,
                                        output_digest: Some(sha256_hex(
                                            response.output_text.as_bytes(),
                                        )),
                                        partial_summary: None,
                                        artifact_refs: vec![],
                                        findings: vec![],
                                        failure_reason: Some(format!(
                                            "managed workspace action failed: {error}"
                                        )),
                                        usage: Some(round.envelope),
                                        effect_receipt: Some(receipt),
                                    });
                                }
                            }
                        }
                    }
                }
                if let Ok(mut rounds) = self.rounds.lock() {
                    rounds.push(LiveLedgerRoundRef {
                        delegated_attempt_id: delegated_attempt_id.clone(),
                        node_id: node_id.clone(),
                        action_receipt: action_receipt.clone(),
                    });
                }
                let mut evidence_ref =
                    format!("delegation:{delegation_id}:attempt:{delegated_attempt_id}");
                if let Some(ref act) = action_receipt {
                    if let Some(after_sha) = act.get("after_sha256").and_then(Value::as_str) {
                        evidence_ref = format!("{evidence_ref}:action_sha:{after_sha}");
                    }
                }
                let receipt = StoreEffectReceipt {
                    attempt_id: ledger_attempt_id.clone(),
                    disposition: round.disposition,
                    receipt_evidence_digest: Some(round.entry_digest.clone()),
                    store_evidence_ref: Some(evidence_ref),
                };
                Ok(WorkerResult {
                    task_id: context.selected_task.id.clone(),
                    attempt_id: ledger_attempt_id,
                    status: WorkerOutcomeStatus::Completed,
                    effect_free: false,
                    output_digest: Some(sha256_hex(response.output_text.as_bytes())),
                    partial_summary: None,
                    artifact_refs: vec![],
                    findings: vec![],
                    failure_reason: None,
                    usage: Some(round.envelope),
                    effect_receipt: Some(receipt),
                })
            }
            Err(call_failure) => {
                if let Ok(mut rounds) = self.rounds.lock() {
                    rounds.push(LiveLedgerRoundRef {
                        delegated_attempt_id: delegated_attempt_id.clone(),
                        node_id: node_id.clone(),
                        action_receipt: None,
                    });
                }
                let receipt = StoreEffectReceipt {
                    attempt_id: ledger_attempt_id.clone(),
                    disposition: round.disposition,
                    receipt_evidence_digest: Some(round.entry_digest.clone()),
                    store_evidence_ref: Some(format!(
                        "delegation:{delegation_id}:attempt:{delegated_attempt_id}"
                    )),
                };
                let domain = call_failure
                    .split(':')
                    .next()
                    .unwrap_or("provider_call")
                    .trim()
                    .to_string();
                Ok(WorkerResult {
                    task_id: context.selected_task.id.clone(),
                    attempt_id: ledger_attempt_id,
                    status: WorkerOutcomeStatus::Failed,
                    effect_free: round.disposition
                        == EffectReceiptDisposition::FailedBeforeSendNoEffect,
                    output_digest: None,
                    partial_summary: None,
                    artifact_refs: vec![],
                    findings: vec![],
                    failure_reason: Some(format!("managed provider call failed: {domain}")),
                    usage: Some(round.envelope),
                    effect_receipt: Some(receipt),
                })
            }
        }
    }
}

/// Live verifier for ledger-orchestrated cells: executes the exact frozen
/// verification command (strict-parser admitted shapes only) locally against
/// the cell target and judges by the accepted process-outcome basis
/// (`boundary_mapping().is_known_success()`, the same predicate the
/// store-owned verification owner applies). The observation lives in the
/// ledger with digest evidence only — raw command output never enters ledger
/// fields — and ledger verification never replaces the sealed research
/// evaluator. Timeouts and spawn failures are `Inconclusive` (bounded retry,
/// never a pass); nonzero exits are `Fail`.
pub struct LiveLedgerCellVerifier {
    target_path: std::path::PathBuf,
    verification_command: String,
    timeout_ms: u64,
}

impl LiveLedgerCellVerifier {
    pub fn new(
        target_path: std::path::PathBuf,
        verification_command: String,
        timeout_ms: u64,
    ) -> Result<Self, String> {
        crate::product_golden_path::parse_strict_product_verification_command(
            &verification_command,
        )?;
        Ok(Self {
            target_path,
            verification_command,
            timeout_ms: timeout_ms.clamp(1, 900_000),
        })
    }

    fn bounded_command_output(
        &self,
        env: &[(String, String)],
        argv: &[String],
    ) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
        use std::io::Read;
        use std::time::{Duration, Instant};
        let mut cmd = std::process::Command::new(&argv[0]);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .args(&argv[1..])
            .current_dir(&self.target_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("frozen verifier spawn failed: {e}"))?;
        let mut stdout_capped = child
            .stdout
            .take()
            .map(|s| s.take(LIVE_LEDGER_MAX_VERIFIER_OUTPUT_BYTES as u64));
        let mut stderr_capped = child.stderr.take().map(|s| s.take(4096));
        let (stdout_tx, stdout_rx) = std::sync::mpsc::channel();
        let (stderr_tx, stderr_rx) = std::sync::mpsc::channel();
        if let Some(mut capped) = stdout_capped.take() {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = capped.read_to_end(&mut buf);
                let _ = stdout_tx.send(buf);
            });
        } else {
            let _ = stdout_tx.send(Vec::new());
        }
        if let Some(mut capped) = stderr_capped.take() {
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = capped.read_to_end(&mut buf);
                let _ = stderr_tx.send(buf);
            });
        } else {
            let _ = stderr_tx.send(Vec::new());
        }
        let deadline = Instant::now() + Duration::from_millis(self.timeout_ms);
        loop {
            match child
                .try_wait()
                .map_err(|e| format!("frozen verifier wait failed: {e}"))?
            {
                Some(status) => {
                    let mut output = stdout_rx.recv().unwrap_or_default();
                    let stderr = stderr_rx.recv().unwrap_or_default();
                    // Keep the digest input bounded but complete for the
                    // captured stdout; stderr only breaks ties in identity.
                    output.extend_from_slice(&stderr);
                    return Ok((status, output));
                }
                None => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("frozen verifier timed out".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }
    }
}

impl LedgerVerifier for LiveLedgerCellVerifier {
    fn verify_task(
        &mut self,
        _ledger: &WorkingLedger,
        task: &LedgerTaskRecord,
        result: &WorkerResult,
    ) -> Result<VerificationReport, OrchestrationError> {
        if result.output_digest.is_none() {
            return Ok(VerificationReport {
                outcome: VerificationOutcome::Fail,
                observation_summary: format!(
                    "live ledger worker produced no output digest for {}",
                    task.id
                ),
                evidence_digest: None,
            });
        }
        let (env, argv) = crate::product_golden_path::parse_strict_product_verification_command(
            &self.verification_command,
        )
        .map_err(|e| {
            OrchestrationError::VerificationError(format!(
                "live ledger verification command is invalid: {e}"
            ))
        })?;
        let output_sha256 = result.output_digest.clone().unwrap_or_default();
        match self.bounded_command_output(&env, &argv) {
            Ok((status, output)) => {
                let code = status.code().unwrap_or(-1);
                let process_outcome =
                    crate::node_executor::process_outcome_from_exit_status(&status);
                let output_digest = sha256_hex(&output);
                let evidence_digest = Some(sha256_hex(
                    format!(
                        "live-ledger-verification|{}|{code}|{output_digest}|{output_sha256}",
                        self.verification_command
                    )
                    .as_bytes(),
                ));
                if process_outcome.boundary_mapping().is_known_success() {
                    Ok(VerificationReport {
                        outcome: VerificationOutcome::Pass,
                        observation_summary: format!(
                            "frozen verifier passed for {} (exit 0)",
                            task.id
                        ),
                        evidence_digest,
                    })
                } else {
                    Ok(VerificationReport {
                        outcome: VerificationOutcome::Fail,
                        observation_summary: format!(
                            "frozen verifier exited {code} for {}",
                            task.id
                        ),
                        evidence_digest,
                    })
                }
            }
            Err(timeout) => Ok(VerificationReport {
                outcome: VerificationOutcome::Inconclusive,
                observation_summary: format!(
                    "frozen verifier did not complete for {} ({timeout})",
                    task.id
                ),
                evidence_digest: None,
            }),
        }
    }
}

/// Evidence handles for one live ledger loop: the store-owned round attempts
/// the terminal projection re-reads for its cross-boundary conservation
/// proof.
#[derive(Debug, Clone, Default)]
pub(crate) struct LiveLedgerLoopEvidence {
    pub rounds: Vec<LiveLedgerRoundRef>,
}

/// Run the live ledger-orchestrated cell loop: the real bounded
/// manager/worker/verifier orchestration where every worker round is one
/// delegated implementer call through the store-owned provider journal. No
/// provider POST happens before the caller's live authorization gates pass;
/// this function assumes the bridge already enforced them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_live_ledger_cell_loop(
    store: &std::sync::Arc<LocalProductStore>,
    operator_principal: &AuthenticatedPrincipal,
    executor_principal: &AuthenticatedPrincipal,
    binding: &ProductHarnessMatrixBinding,
    task: &RweTaskDefinition,
    ids: &CellIdentities,
    product_task_id: &str,
    transport: &Option<std::sync::Arc<dyn crate::provider::transport::HttpTransport>>,
    package_id: &str,
    target_repo_path: &std::path::Path,
    cell_cost_ceiling_usd: f64,
) -> Result<(LedgerTerminalRecord, LiveLedgerLoopEvidence), String> {
    run_live_ledger_cell_loop_with_adapter(
        store,
        operator_principal,
        executor_principal,
        binding,
        task,
        ids,
        product_task_id,
        transport,
        package_id,
        target_repo_path,
        cell_cost_ceiling_usd,
        None,
    )
}

pub(super) fn preferred_stage_prompt_path(
    verification_command: &str,
    allowed_paths: &[String],
) -> Option<String> {
    let (_, argv) =
        crate::product_golden_path::parse_strict_product_verification_command(verification_command)
            .ok()?;
    argv.into_iter().find(|argument| {
        !argument.starts_with('-')
            && allowed_paths.iter().any(|allowed| {
                argument == allowed
                    || argument.starts_with(&format!("{}/", allowed.trim_end_matches('/')))
            })
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_live_ledger_cell_loop_with_adapter(
    store: &std::sync::Arc<LocalProductStore>,
    operator_principal: &AuthenticatedPrincipal,
    executor_principal: &AuthenticatedPrincipal,
    binding: &ProductHarnessMatrixBinding,
    task: &RweTaskDefinition,
    ids: &CellIdentities,
    product_task_id: &str,
    transport: &Option<std::sync::Arc<dyn crate::provider::transport::HttpTransport>>,
    package_id: &str,
    _target_repo_path: &std::path::Path,
    cell_cost_ceiling_usd: f64,
    adapter: Option<std::sync::Arc<dyn LiveLedgerProviderAdapter>>,
) -> Result<(LedgerTerminalRecord, LiveLedgerLoopEvidence), String> {
    let package = crate::rwe::campaign_package::resolve_frozen_campaign_package(package_id)?;
    package.validate()?;
    let matrix_model = resolve_frozen_matrix_model(binding)?;
    let provider_binding =
        package.provider_execution_binding_for_model(&matrix_model.resolved_model_id)?;
    let verification_command = task
        .expected_verification_commands
        .first()
        .cloned()
        .ok_or("live ledger cell frozen task missing verification command")?;
    crate::product_golden_path::parse_strict_product_verification_command(&verification_command)?;
    let ledger_task_id = format!("mx1-cell-{}", &sha256_hex(binding.cell_id.as_bytes())[..32]);
    let contract_text = format!(
        "ledger-cell|{}|{}|{}",
        binding.plan_id, binding.cell_id, binding.cell_descriptor_sha256
    );
    let mut ledger = WorkingLedger::new(
        contract_text,
        format!(
            "execute frozen matrix cell {} task {}",
            binding.cell_id, binding.task_id
        ),
    );
    ledger
        .add_task(
            &ledger_task_id,
            &format!(
                "execute frozen matrix cell {} (objective hash {})",
                binding.cell_id, task.objective_sha256
            ),
            &LedgerOrchestratorConfig::default(),
        )
        .map_err(|e| format!("live ledger cell add task failed: {e}"))?;
    let task_val = store
        .get_product_task(product_task_id)
        .map_err(|e| format!("live ledger cell get product task {product_task_id} failed: {e}"))?
        .ok_or_else(|| format!("live ledger cell product task {product_task_id} not found"))?;
    let execution_workspace = task_val
        .pointer("/workspace_binding/workspace_path")
        .and_then(Value::as_str)
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_dir())
        .ok_or_else(|| {
            format!(
                "live ledger cell product task {product_task_id} missing or invalid execution workspace binding"
            )
        })?;

    let rounds = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let worker = LiveLedgerCellWorker::new(
        std::sync::Arc::clone(store),
        operator_principal.clone(),
        executor_principal.clone(),
        product_task_id.to_string(),
        format!("rwe-del-ledger-{}", ids.delegated_attempt_id),
        binding.clone(),
        task.objective_sha256.clone(),
        task.objective.clone(),
        preferred_stage_prompt_path(&verification_command, &task.allowed_mutable_paths),
        task.allowed_mutable_paths.clone(),
        task.per_task_max_provider_requests.max(1),
        cell_cost_ceiling_usd,
        transport.clone(),
        package_id.to_string(),
        provider_binding,
        std::sync::Arc::clone(&rounds),
        Some(execution_workspace.clone()),
        adapter,
    );
    let verifier = LiveLedgerCellVerifier::new(
        execution_workspace.clone(),
        verification_command,
        task.timeout_ms,
    )?;
    let mut orchestrator = LedgerOrchestrator::new(
        LedgerOrchestratorConfig::default(),
        ledger,
        LiveLedgerCellController,
        worker,
        verifier,
    )
    .map_err(|e| format!("live ledger cell orchestrator init failed: {e}"))?;
    let record = orchestrator.run_bounded();
    let evidence = LiveLedgerLoopEvidence {
        rounds: rounds.lock().map(|r| r.clone()).unwrap_or_default(),
    };

    Ok((record, evidence))
}

/// Project one live ledger terminal record into a [`CellOutcome`] from
/// store-owned evidence alone. Every round journal is re-read from the store
/// and folded with the accepted RWE projection semantics; the folded sums
/// must equal the terminal record's lifecycle totals (exact on token/call/
/// duration integers, microunit-tolerant on summed float costs), or the cell
/// fails closed as unknown. Transport unanimity reuses
/// [`store_evidence_transport_provenance`], so only every-entry-`external`
/// journals can report a live provider request, and injected-transport runs
/// can never seal.
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_live_ledger_cell_outcome(
    store: &LocalProductStore,
    product_task_id: &str,
    rounds: &[LiveLedgerRoundRef],
    record: &LedgerTerminalRecord,
    package_id: &str,
    package_sha: &str,
    run_id: &str,
    ids: &CellIdentities,
) -> CellOutcome {
    fn sums_from_rounds(
        store: &LocalProductStore,
        product_task_id: &str,
        rounds: &[LiveLedgerRoundRef],
    ) -> Option<LiveLedgerJournalSums> {
        let mut entries = Vec::with_capacity(rounds.len());
        let mut requests = 0u64;
        let mut input = 0u64;
        let mut output = 0u64;
        let mut total = 0u64;
        let mut cost = 0.0f64;
        let mut cost_missing = false;
        let mut duration = 0u64;
        let mut duration_missing = false;
        let mut prompt_complete = true;
        for round in rounds {
            let projection = store
                .project_rwe_cell_store_evidence(product_task_id, &round.delegated_attempt_id)
                .ok()?;
            let journal = projection.get("provider_request_journal")?.as_array()?;
            let entry = journal.iter().find(|e| {
                e.get("node_id").and_then(Value::as_str) == Some(round.node_id.as_str())
            })?;
            let mapped = map_live_ledger_journal_entry(entry).ok()?;
            entries.push(entry.clone());
            let envelope = mapped.envelope;
            match mapped.disposition {
                EffectReceiptDisposition::Success | EffectReceiptDisposition::OutcomeUnknown => {
                    requests = requests.saturating_add(1);
                }
                EffectReceiptDisposition::FailedBeforeSendNoEffect
                | EffectReceiptDisposition::KnownFailedEffect => {}
            }
            match (envelope.prompt_tokens, envelope.completion_tokens) {
                (Some(p), Some(c)) => {
                    input = input.saturating_add(p);
                    output = output.saturating_add(c);
                }
                _ => prompt_complete = false,
            }
            if let Some(t) = envelope.total_tokens {
                total = total.saturating_add(t);
            } else {
                prompt_complete = false;
            }
            if mapped.disposition == EffectReceiptDisposition::Success {
                cost += mapped.realized_cost_usd;
            }
            if envelope.cost_usd_micros.is_none()
                || mapped.disposition == EffectReceiptDisposition::OutcomeUnknown
            {
                cost_missing = true;
            }
            match envelope.duration_ms {
                Some(d) => duration = duration.saturating_add(d),
                None => duration_missing = true,
            }
        }
        Some(LiveLedgerJournalSums {
            entries,
            requests,
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            realized_cost_usd: cost,
            cost_missing,
            duration_ms: duration,
            duration_missing,
            prompt_complete,
        })
    }

    let mut classification = match record.disposition {
        LedgerTerminalDisposition::Completed => "ledger_completed_pending_provenance",
        LedgerTerminalDisposition::OutcomeUnknown => "outcome_unknown",
        LedgerTerminalDisposition::Cancelled => "cancelled",
        _ => "controlled_failure",
    }
    .to_string();
    let mut cost_unknown = true;
    let mut live_provider_request = false;
    let mut provenance = "none".to_string();
    let mut provider_requests = 0u64;
    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut total_tokens = 0u64;
    let mut latency_ms = 0u64;
    let mut monetary_cost: Option<f64> = None;
    let mut divergence: Option<String> = None;

    if let Some(sums) = sums_from_rounds(store, product_task_id, rounds) {
        let entries = sums.entries;
        let requests = sums.requests;
        let input = sums.input_tokens;
        let output = sums.output_tokens;
        let total = sums.total_tokens;
        let realized_cost = sums.realized_cost_usd;
        let journal_cost_missing = sums.cost_missing;
        let journal_duration = sums.duration_ms;
        let journal_duration_missing = sums.duration_missing;
        let journal_prompt_complete = sums.prompt_complete;
        let synthetic = json!({
            "provider_execution": Value::Null,
            "provider_request_journal": entries,
        });
        let unanimous_provenance = store_evidence_transport_provenance(&synthetic).ok();
        let journal_has_unknown = entries
            .iter()
            .any(|entry| entry.get("status").and_then(Value::as_str) == Some("outcome_unknown"));
        // Cross-boundary conservation: the store re-read must equal the
        // terminal lifecycle totals (exact integers; summed float costs may
        // drift by at most one microunit per succeeded round).
        let metrics = &record.metrics;
        let cost_micros_observed = (realized_cost * 1_000_000.0).round() as u64;
        let cost_matches = match (metrics.cost_usd_micros, journal_cost_missing) {
            (None, true) => true,
            (Some(expected), false) => {
                (expected as i64 - cost_micros_observed as i64).abs() <= requests as i64
            }
            _ => false,
        };
        let conserved = metrics.provider_calls == Some(requests)
            && metrics.total_tokens == Some(total)
            && metrics.duration_ms
                == if journal_duration_missing {
                    None
                } else {
                    Some(journal_duration)
                }
            && cost_matches
            && (!journal_prompt_complete
                || (metrics.prompt_tokens == Some(input)
                    && metrics.completion_tokens == Some(output)));
        if !conserved {
            divergence =
                Some("store journal re-read disagrees with ledger lifecycle totals".into());
        }
        provenance = unanimous_provenance.unwrap_or_else(|| "none".to_string());
        if provenance != "external" && provenance != "injected" {
            provenance = "none".to_string();
        }
        provider_requests = requests;
        input_tokens = input;
        output_tokens = output;
        total_tokens = total;
        latency_ms = if journal_duration_missing {
            0
        } else {
            journal_duration
        };
        cost_unknown = journal_cost_missing || divergence.is_some();
        monetary_cost = if cost_unknown {
            None
        } else {
            Some(realized_cost)
        };
        let conserved_external_live = provenance == "external"
            && divergence.is_none()
            && !journal_has_unknown
            && requests > 0;
        live_provider_request = conserved_external_live;
        classification = if journal_has_unknown || divergence.is_some() {
            "outcome_unknown".to_string()
        } else {
            match record.disposition {
                LedgerTerminalDisposition::Completed => {
                    if provenance == "external" {
                        "success".to_string()
                    } else if provenance == "injected" {
                        "fixture_success".to_string()
                    } else {
                        "controlled_failure".to_string()
                    }
                }
                LedgerTerminalDisposition::OutcomeUnknown => "outcome_unknown".to_string(),
                LedgerTerminalDisposition::Cancelled => "cancelled".to_string(),
                _ => "controlled_failure".to_string(),
            }
        };
    } else if record.disposition == LedgerTerminalDisposition::Completed {
        // A completed loop with unreadable round journals proves nothing
        // about its effects: never report success without store evidence.
        classification = "outcome_unknown".to_string();
    }

    let verification = match record.disposition {
        LedgerTerminalDisposition::Completed => ("evidence_recorded", true),
        _ => ("not_run", false),
    };
    let mut note = format!(
        "live ledger cell loop executed through {RWE_LEDGER_CELL_COMPOSITION_SEAM} ({RWE_LIVE_LEDGER_CELL_COMPOSITION}) for run {run_id}; \
         ledger_terminal_disposition={} ledger_terminal_state={:?} ledger_final_hash={} \
         manager_semantics={RWE_LEDGER_MANAGER_SEMANTICS} \
         ledger_manager_calls={} ledger_worker_calls={} ledger_verifications={} ledger_rounds={} \
         usage_conservation={} store_journal_conserved={} provider_transport_provenance={provenance} \
         missing_evidence={} package_id={package_id} package_sha={package_sha} cell={}",
        record.disposition.as_str(),
        record.terminal_state,
        record.final_ledger_hash,
        record.metrics.manager_call_count,
        record.metrics.worker_call_count,
        record.metrics.verification_count,
        rounds.len(),
        record.usage_conservation_verified,
        divergence.is_none(),
        record.missing_evidence.join(","),
        ids.cell_id,
    );
    if let Some(reason) = divergence {
        note.push_str(&format!("; JOURNAL DIVERGENCE: {reason}"));
    }
    CellOutcome {
        classification,
        provider_requests,
        input_tokens,
        output_tokens,
        total_tokens,
        latency_ms,
        monetary_cost,
        cost_unknown,
        live_provider_request,
        provider_transport_provenance: provenance,
        evidence_source: "product_golden_path_owner".into(),
        verification_status: verification.0.into(),
        verification_trustworthy: verification.1,
        approval_id: None,
        output_draft_pr: None,
        terminal_evidence_id: None,
        terminal_content_sha256: Some(record.final_ledger_hash.clone()),
        cleanup_status: "not_required".into(),
        product_task_id: product_task_id.to_string(),
        workflow_id: ids.workflow_id.clone(),
        node_id: ids.node_id.clone(),
        delegated_attempt_id: ids.delegated_attempt_id.clone(),
        workspace_id: ids.worktree_id.clone(),
        note,
    }
}

/// Run the provider-free ledger-orchestrated cell loop: the real
/// manager/worker/verifier orchestration with effect-free attestations. No
/// provider POST, no target write, and no seal claim. The ledger task
/// identity is deterministically derived from the matrix cell; the matrix
/// binding itself travels in the ledger contract digest and the admitted
/// ProductTask intake.
pub(crate) fn run_provider_free_ledger_cell_loop(
    binding: &ProductHarnessMatrixBinding,
    objective_sha256: &str,
) -> Result<LedgerTerminalRecord, String> {
    let ledger_task_id = format!("mx1-cell-{}", &sha256_hex(binding.cell_id.as_bytes())[..32]);
    let contract_text = format!(
        "ledger-cell|{}|{}|{}",
        binding.plan_id, binding.cell_id, binding.cell_descriptor_sha256
    );
    let mut ledger = WorkingLedger::new(
        contract_text,
        format!(
            "execute frozen matrix cell {} task {}",
            binding.cell_id, binding.task_id
        ),
    );
    ledger
        .add_task(
            &ledger_task_id,
            &format!(
                "execute frozen matrix cell {} (objective hash {})",
                binding.cell_id, objective_sha256
            ),
            &LedgerOrchestratorConfig::default(),
        )
        .map_err(|e| format!("ledger cell task admission failed: {e}"))?;
    let mut orchestrator = LedgerOrchestrator::new(
        LedgerOrchestratorConfig::default(),
        ledger,
        ProviderFreeLedgerCellController,
        EffectFreeLedgerCellWorker,
        ProviderFreeLedgerCellVerifier,
    )
    .map_err(|e| format!("ledger cell orchestrator init failed: {e}"))?;
    Ok(orchestrator.run_bounded())
}
