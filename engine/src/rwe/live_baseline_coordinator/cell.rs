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

use super::provider::*;
use super::schedule::*;
use super::worker::*;
use super::{
    CODEX_PREREQUISITE_DEFAULT_MAX_RECOVERY_GENERATIONS, CODEX_PREREQUISITE_FALLBACK_TASK_ID,
    CODEX_PREREQUISITE_PRIMARY_TASK_ID, LIVE_LEDGER_MAX_PROMPT_BYTES,
    LIVE_LEDGER_MAX_VERIFIER_OUTPUT_BYTES, RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
    RWE_CODEX_PREREQUISITE_MAX_RECOVERY_GENERATIONS_TOKEN, RWE_LEDGER_CELL_COMPOSITION_SEAM,
    RWE_LEDGER_MANAGER_SEMANTICS, RWE_LIVE_BASELINE_COORDINATOR_SCHEMA,
    RWE_LIVE_CELL_COMPOSITION_SEAM, RWE_LIVE_LEDGER_CELL_COMPOSITION, RWE_OPERATOR_LIVE_RUN_TOKEN,
};

/// Deterministic identity set for one schedule cell under one RWE run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellIdentities {
    pub cell_id: String,
    pub task_id: String,
    pub product_task_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub delegated_attempt_id: String,
    pub worktree_id: String,
    pub branch_name: String,
    pub rwe_task_attempt_id: String,
    pub definition_sha256: String,
    pub repetition: u64,
    pub seed: u64,
    pub budget_point_id: String,
    pub sequential_order: u64,
}

pub fn cell_identities_for(
    run_id: &str,
    cell: &Value,
    task: &RweTaskDefinition,
) -> Result<CellIdentities, String> {
    let cell_id = cell
        .get("cell_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("schedule cell_id required")?
        .to_string();
    let task_id = cell
        .get("task_id")
        .and_then(Value::as_str)
        .ok_or("schedule cell task_id required")?;
    if task_id != task.task_id {
        return Err(format!(
            "cell task_id {task_id} does not match frozen task {}",
            task.task_id
        ));
    }
    let product_task_id = format!("rwe-pt:{run_id}:{cell_id}");
    let workflow_id = format!("rwe-wf:{run_id}:{cell_id}");
    let node_id = format!("{workflow_id}-implementation");
    let delegated_attempt_id = format!("rwe-att:{run_id}:{cell_id}");
    let worktree_id = format!("rwe-ws:{run_id}:{cell_id}");
    let branch_name = format!("acp/rwe/{run_id}/{cell_id}");
    let rwe_task_attempt_id = format!("{run_id}:{cell_id}");
    Ok(CellIdentities {
        cell_id,
        task_id: task.task_id.clone(),
        product_task_id,
        workflow_id,
        node_id,
        delegated_attempt_id,
        worktree_id,
        branch_name,
        rwe_task_attempt_id,
        definition_sha256: task.definition_sha256.clone(),
        repetition: cell.get("repetition").and_then(Value::as_u64).unwrap_or(0),
        seed: cell.get("seed").and_then(Value::as_u64).unwrap_or(0),
        budget_point_id: cell
            .get("budget_point_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        sequential_order: cell
            .get("sequential_order")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

/// Outcome of one cell execution under an injectable driver.
///
/// Public/injected claims never authorize `live_baseline_sealed`. Sealing is
/// derived only from store-owned ProductTask/terminal/provider receipts, and
/// the durable provider journal additionally records the transport provenance
/// (`external` vs `injected`) of every request; a seal is impossible while any
/// journaled claim is not `external`. This is enforced by code in
/// `evaluate_store_owned_live_baseline_seal`, not by comments or presentation.
///
/// Store-owned receipt fields: after the driver returns, the coordinator
/// re-couples usage/cost/terminal evidence from the canonical store rows
/// (`couple_usage_to_store_journal`), and `evidence_source`,
/// `classification`, `provider_requests`, `live_provider_request`,
/// `provider_transport_provenance`, `total_tokens`, `monetary_cost`,
/// `cost_unknown`, `verification_status`, `verification_trustworthy`,
/// `approval_id`, `terminal_evidence_id`, `terminal_content_sha256` are
/// treated as driver-supplied presentation claims only — the seal recomputes
/// every decision from store rows and never trusts these fields (nor
/// `evidence_json` strings) to seal.
#[derive(Debug, Clone)]
pub struct CellOutcome {
    pub classification: String,
    pub provider_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub latency_ms: u64,
    pub monetary_cost: Option<f64>,
    pub cost_unknown: bool,
    pub live_provider_request: bool,
    /// `none` | `external` | `injected`. Derived from the store-owned journal
    /// via `couple_usage_to_store_journal`; never caller-authored. Presentation
    /// only — the seal reads the durable journal itself.
    pub provider_transport_provenance: String,
    /// `injected` | `product_golden_path_owner` — sealing rejects `injected`.
    /// Seal decisions are store-owned; this field is presentation only.
    pub evidence_source: String,
    pub verification_status: String,
    pub verification_trustworthy: bool,
    pub approval_id: Option<String>,
    pub output_draft_pr: Option<Value>,
    pub terminal_evidence_id: Option<String>,
    pub terminal_content_sha256: Option<String>,
    pub cleanup_status: String,
    pub product_task_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub delegated_attempt_id: String,
    pub workspace_id: String,
    pub note: String,
}

impl CellOutcome {
    pub fn blocked(classification: &str, note: &str, ids: &CellIdentities) -> Self {
        Self {
            classification: classification.into(),
            provider_requests: 0,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            latency_ms: 0,
            monetary_cost: None,
            cost_unknown: false,
            live_provider_request: false,
            provider_transport_provenance: "none".into(),
            evidence_source: "blocked".into(),
            verification_status: "not_run".into(),
            verification_trustworthy: false,
            approval_id: None,
            output_draft_pr: None,
            terminal_evidence_id: None,
            terminal_content_sha256: None,
            cleanup_status: "not_required".into(),
            product_task_id: ids.product_task_id.clone(),
            workflow_id: ids.workflow_id.clone(),
            node_id: ids.node_id.clone(),
            delegated_attempt_id: ids.delegated_attempt_id.clone(),
            workspace_id: ids.worktree_id.clone(),
            note: note.into(),
        }
    }
}

/// Cell execution seam shared by production and tests.
pub trait CellDriver: Send + Sync {
    /// Exact campaign package consumed by this driver. `None` is the legacy
    /// canonical DeepSeek default only; Codex runners always require an exact
    /// package value matching the Store authorization.
    fn campaign_package(&self) -> Option<&crate::rwe::campaign_package::FrozenCampaignPackage> {
        None
    }

    fn ensure_effects_ready(&self) -> Result<(), String> {
        Ok(())
    }

    fn execute_cell(
        &self,
        store: &std::sync::Arc<LocalProductStore>,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        lease_token: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String>;
}

/// Counts `execute_cell` entries that pass the store fence (orchestration proofs only).
pub struct CountingCellDriver<'a> {
    pub inner: &'a dyn CellDriver,
    pub invocations: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl CellDriver for CountingCellDriver<'_> {
    fn campaign_package(&self) -> Option<&crate::rwe::campaign_package::FrozenCampaignPackage> {
        self.inner.campaign_package()
    }

    fn ensure_effects_ready(&self) -> Result<(), String> {
        self.inner.ensure_effects_ready()
    }

    fn execute_cell(
        &self,
        store: &std::sync::Arc<LocalProductStore>,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        lease_token: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String> {
        self.invocations
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.execute_cell(
            store,
            principal,
            frozen,
            run_id,
            lease_token,
            cell,
            task,
            ids,
        )
    }
}

/// Provider-free injected outcomes for orchestration proofs only.
/// Always stamps `evidence_source=injected` and cannot seal a live baseline.
pub struct InjectedCellDriver {
    pub outcomes: Vec<CellOutcome>,
}

impl CellDriver for InjectedCellDriver {
    fn execute_cell(
        &self,
        _store: &std::sync::Arc<LocalProductStore>,
        _principal: &AuthenticatedPrincipal,
        _frozen: &OperatorFrozenContractSet,
        _run_id: &str,
        _lease_token: &str,
        cell: &Value,
        _task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String> {
        let order = cell
            .get("sequential_order")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize;
        let idx = order.saturating_sub(1);
        self.outcomes
            .get(idx)
            .cloned()
            .ok_or_else(|| {
                format!(
                    "injected cell driver missing outcome for sequential_order {order} (have {})",
                    self.outcomes.len()
                )
            })
            .map(|mut o| {
                o.product_task_id = ids.product_task_id.clone();
                o.workflow_id = ids.workflow_id.clone();
                o.node_id = ids.node_id.clone();
                o.delegated_attempt_id = ids.delegated_attempt_id.clone();
                o.workspace_id = ids.worktree_id.clone();
                o.evidence_source = "injected".into();
                o.live_provider_request = false;
                o.provider_transport_provenance = if o.provider_requests > 0 {
                    "injected".into()
                } else {
                    "none".into()
                };
                // Force non-seal classifications even if caller mutates success claims.
                if o.classification == "success" {
                    o.classification = "injected_success".into();
                }
                o
            })
    }
}

/// Production Product Golden Path cell composition for exact frozen RWE bindings.
///
/// Creates store-owned ProductTask intake under existing owners, maps frozen
/// verifier/paths/budgets, and refuses live provider POSTs / target writes unless
/// `allow_live_provider_effects` is set after controller live authorization.
/// Optional `fake_transport` is for provider-free tests only (MockTransport).
/// A fake/injected transport can never seal a live baseline: the durable
/// provider journal records `transport_provenance=injected` for every request
/// it serves, and `evaluate_store_owned_live_baseline_seal` fails closed unless
/// every journaled claim attests the external production transport.
#[derive(Default)]
pub struct ProductGoldenPathCellDriver {
    /// Operator-supplied local clone of the frozen target (must match SHA).
    pub target_repo_path: Option<std::path::PathBuf>,
    /// When false (default), no provider POST and no target write.
    pub allow_live_provider_effects: bool,
    /// Test-only injectable HTTP transport (never a production seal path alone).
    pub fake_transport: Option<std::sync::Arc<dyn crate::provider::transport::HttpTransport>>,
    /// Provisioned operator key for the role-separated delegated attempt activator.
    /// Required only when `allow_live_provider_effects` is true.
    pub cell_executor_key_id: Option<String>,
    /// Provisioned reviewer key for the role-separated delegated artifact
    /// confirmer (must be distinct from the manifest approver and the attempt
    /// activator; the store rejects shared identities). Required only when
    /// `allow_live_provider_effects` is true.
    pub cell_confirmer_key_id: Option<String>,
    /// Bound frozen campaign package (defaults to canonical DeepSeek v2 if None).
    pub campaign_package: Option<crate::rwe::campaign_package::FrozenCampaignPackage>,
}

impl Clone for ProductGoldenPathCellDriver {
    fn clone(&self) -> Self {
        Self {
            target_repo_path: self.target_repo_path.clone(),
            allow_live_provider_effects: self.allow_live_provider_effects,
            fake_transport: self.fake_transport.clone(),
            cell_executor_key_id: self.cell_executor_key_id.clone(),
            cell_confirmer_key_id: self.cell_confirmer_key_id.clone(),
            campaign_package: self.campaign_package.clone(),
        }
    }
}

impl std::fmt::Debug for ProductGoldenPathCellDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProductGoldenPathCellDriver")
            .field("target_repo_path", &self.target_repo_path)
            .field(
                "allow_live_provider_effects",
                &self.allow_live_provider_effects,
            )
            .field("fake_transport", &self.fake_transport.is_some())
            .field("cell_executor_key_id", &self.cell_executor_key_id)
            .field("cell_confirmer_key_id", &self.cell_confirmer_key_id)
            .field("campaign_package", &self.campaign_package)
            .finish()
    }
}

impl CellDriver for ProductGoldenPathCellDriver {
    fn campaign_package(&self) -> Option<&crate::rwe::campaign_package::FrozenCampaignPackage> {
        self.campaign_package.as_ref()
    }

    fn ensure_effects_ready(&self) -> Result<(), String> {
        if self.allow_live_provider_effects && std::env::var("CI").ok().as_deref() == Some("true") {
            return Err(
                "fail closed before cell effect: live RWE cell execution is forbidden in CI".into(),
            );
        }
        rwe_composition_seam_ready()?;
        let pkg = match &self.campaign_package {
            Some(p) => {
                p.validate()?;
                p.clone()
            }
            None => crate::rwe::campaign_package::canonical_deepseek_v2_package()?,
        };
        if self.allow_live_provider_effects {
            if pkg.requires_owner_approval {
                return Err(format!(
                    "campaign package {} requires explicit owner approval before live execution",
                    pkg.package_id
                ));
            }
            // Operator live-run token: parity with the armed integration
            // fixture's ACP_RWE_ARMED_LIVE_RUN=1 gate. Live provider POSTs and
            // target writes require the explicit operator authorization symbol.
            if std::env::var(RWE_OPERATOR_LIVE_RUN_TOKEN).ok().as_deref() != Some("1") {
                return Err(format!(
                    "live RWE cell requires the operator live-run token {RWE_OPERATOR_LIVE_RUN_TOKEN}=1"
                ));
            }
            if pkg.provider_kind == "managed_deepseek" {
                let cred = std::env::var(DEEPSEEK_CREDENTIAL_REFERENCE)
                    .ok()
                    .filter(|v| !v.trim().is_empty());
                if cred.is_none() && self.fake_transport.is_none() {
                    return Err(format!(
                        "live RWE cell requires {DEEPSEEK_CREDENTIAL_REFERENCE} or an injected fake transport"
                    ));
                }
            } else if pkg.provider_kind == "agy" {
                return Err(
                    "live RWE cell for agy provider requires explicit Store live authorization and owner approval".into(),
                );
            } else if pkg.provider_kind
                == crate::cli::codex_budget_authority::CODEX_PROVIDER_KIND_CHATGPT_SUBSCRIPTION
            {
                if self.fake_transport.is_some() {
                    return Err(
                        "live Codex subscription execution cannot use an injected transport".into(),
                    );
                }
                if crate::cli::codex_budget_authority::CodexUpstreamAuth::resolve_parent_chatgpt_subscription()
                    .is_none()
                {
                    return Err(
                        "live RWE cell requires a current ChatGPT subscription session".into(),
                    );
                }
            }
            if self.target_repo_path.is_none() {
                return Err("live RWE cell requires target_repo_path matching frozen SHA".into());
            }
        }
        Ok(())
    }

    fn execute_cell(
        &self,
        store: &std::sync::Arc<LocalProductStore>,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        _lease_token: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String> {
        let target = self
            .target_repo_path
            .as_ref()
            .ok_or("ProductGoldenPathCellDriver requires target_repo_path for composition")?;
        // Ledger-orchestrated matrix cells never take the default managed
        // 3-node path: they execute the admitted manager/worker/verifier
        // loop inside the store-owned envelope, or fail closed.
        if cell
            .get("harness_id")
            .and_then(Value::as_str)
            .is_some_and(|harness| harness == LEDGER_ORCHESTRATED_HARNESS_ID)
        {
            return self.execute_ledger_orchestrated_cell(
                store, principal, frozen, run_id, cell, task, ids, target,
            );
        }
        let matrix_binding = if cell.get("matrix_plan_id").is_some() {
            Some(matrix_binding_from_frozen_cell(cell, ids, task)?)
        } else {
            None
        };
        let intake = build_rwe_cell_product_intake_with_matrix(
            principal,
            frozen,
            task,
            ids,
            target,
            matrix_binding.clone(),
        )?;
        // Product gate must be on for store-owned intake.
        let admitted = store
            .admit_product_task(&intake, principal.principal_id())
            .map_err(|e| format!("product task admit failed: {e}"))?;
        let product_task_id = admitted
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or(&ids.product_task_id)
            .to_string();

        // Provider-free default: compose ProductTask + frozen binding evidence without
        // POSTing or writing the target. Live effects require allow_live_provider_effects
        // and are still blocked in CI. Fake transport proves composition without seal
        // claims from caller-authored receipts.
        if !self.allow_live_provider_effects {
            let pkg = match &self.campaign_package {
                Some(p) => p.clone(),
                None => crate::rwe::campaign_package::canonical_deepseek_v2_package()?,
            };
            let pkg_sha = pkg.canonical_sha256().unwrap_or_else(|_| "unknown".into());
            return Ok(CellOutcome {
                classification: "blocked_provider_free_mode".into(),
                provider_requests: 0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms: 0,
                monetary_cost: Some(0.0),
                cost_unknown: false,
                live_provider_request: false,
                provider_transport_provenance: "none".into(),
                evidence_source: "product_golden_path_owner".into(),
                verification_status: "not_run".into(),
                verification_trustworthy: false,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: None,
                terminal_content_sha256: None,
                cleanup_status: "not_required".into(),
                product_task_id,
                workflow_id: ids.workflow_id.clone(),
                node_id: ids.node_id.clone(),
                delegated_attempt_id: ids.delegated_attempt_id.clone(),
                workspace_id: ids.worktree_id.clone(),
                note: format!(
                    "store-owned ProductTask admitted under {}; live provider/target effects deferred until controller authorization; seam={RWE_LIVE_CELL_COMPOSITION_SEAM}; package_id={}; package_sha={}; cell={}",
                    principal.tenant_id(),
                    pkg.package_id,
                    pkg_sha,
                    cell.get("cell_id").and_then(Value::as_str).unwrap_or("")
                ),
            });
        }

        // Live-armed path: genuinely execute the delegated cell lifecycle through
        // existing store owners — delegation, manifest approval, one-use spend,
        // attempt lease, activation, managed executor, frozen verifier, artifact
        // confirmation, terminal receipt, cleanup, Draft PR record. Seal evidence
        // comes only from store rows, never from caller-authored outcomes.
        let executor_principal = match self.cell_executor_key_id.as_deref() {
            Some(kid) => store
                .authenticate_managed_acceptance_principal(principal.tenant_id(), kid, None)
                .map_err(|e| {
                    format!("armed RWE cell requires provisioned cell-executor key: {e}")
                })?,
            None => {
                return Err(
                    "armed RWE cell requires cell_executor_key_id for role-separated delegated activation"
                        .into(),
                )
            }
        };
        let confirmer_principal = match self.cell_confirmer_key_id.as_deref() {
            Some(kid) => store
                .authenticate_managed_acceptance_principal(principal.tenant_id(), kid, None)
                .map_err(|e| {
                    format!("armed RWE cell requires provisioned cell-confirmer key: {e}")
                })?,
            None => {
                return Err(
                    "armed RWE cell requires cell_confirmer_key_id for role-separated artifact confirmation"
                        .into(),
                )
            }
        };
        if confirmer_principal.principal_id() == principal.principal_id()
            || confirmer_principal.principal_id() == executor_principal.principal_id()
        {
            return Err(
                "armed RWE cell confirmer key must be distinct from approver and activator keys"
                    .into(),
            );
        }
        execute_armed_delegated_rwe_cell(
            store,
            principal,
            &executor_principal,
            &confirmer_principal,
            frozen,
            run_id,
            _lease_token,
            cell,
            task,
            ids,
            &product_task_id,
            matrix_binding.as_ref(),
            self.campaign_package.as_ref(),
            target,
            &self.fake_transport,
        )
    }
}

impl ProductGoldenPathCellDriver {
    /// Execute one live ledger-orchestrated matrix cell: the same admitted
    /// intake, then the live manager/worker/verifier loop where every worker
    /// round is one delegated implementer call through the store-owned
    /// provider journal. Genuine external blockers (absent credential,
    /// live-run token, approval, campaign package, cell budget, emergency
    /// stop) still fail before any provider effect. The ledger cell never
    /// takes the H0 3-node path and never produces Draft PR output. This
    /// introduces no second scheduler, store, provider authority, evaluator,
    /// budget, or approval owner.
    #[allow(clippy::too_many_arguments)]
    fn execute_live_ledger_cell(
        &self,
        store: &std::sync::Arc<LocalProductStore>,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
        target: &std::path::Path,
        binding: ProductHarnessMatrixBinding,
    ) -> Result<CellOutcome, String> {
        // Live authorization gates before any store state or provider effect.
        self.ensure_effects_ready()
            .map_err(|e| format!("live ledger cell effects not ready: {e}"))?;
        // Role-separated delegated attempt activator. No artifact confirmer
        // is required: ledger cells produce ledger terminal evidence, never
        // Draft PR output or artifact confirmation.
        let executor_principal = match self.cell_executor_key_id.as_deref() {
            Some(kid) => store
                .authenticate_managed_acceptance_principal(principal.tenant_id(), kid, None)
                .map_err(|e| {
                    format!("live ledger cell requires provisioned cell-executor key: {e}")
                })?,
            None => {
                return Err(
                    "live ledger cell requires cell_executor_key_id for role-separated delegated activation"
                        .into(),
                )
            }
        };
        let intake = build_rwe_cell_product_intake_with_matrix(
            principal,
            frozen,
            task,
            ids,
            target,
            Some(binding.clone()),
        )?;
        // Product gate must be on for store-owned intake.
        let admitted = store
            .admit_product_task(&intake, principal.principal_id())
            .map_err(|e| format!("live ledger cell product task admit failed: {e}"))?;
        let product_task_id = admitted
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or(&ids.product_task_id)
            .to_string();
        let pkg = match &self.campaign_package {
            Some(p) => p.clone(),
            None => crate::rwe::campaign_package::canonical_deepseek_v2_package()?,
        };
        let pkg_sha = pkg.canonical_sha256().unwrap_or_else(|_| "unknown".into());
        let cell_cost_ceiling_usd =
            crate::rwe::frozen_rwe_bindings::frozen_schedule_cell_max_cost(cell)?
                .ok_or("live ledger cell requires a frozen monetary ceiling for delegated spend")?;
        let (record, evidence) = run_live_ledger_cell_loop(
            store,
            principal,
            &executor_principal,
            &binding,
            task,
            ids,
            &product_task_id,
            &self.fake_transport,
            &pkg.package_id,
            target,
            cell_cost_ceiling_usd,
        )?;
        Ok(project_live_ledger_cell_outcome(
            store,
            &product_task_id,
            &evidence.rounds,
            &record,
            &pkg.package_id,
            &pkg_sha,
            run_id,
            ids,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_ledger_orchestrated_cell(
        &self,
        store: &std::sync::Arc<LocalProductStore>,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
        target: &std::path::Path,
    ) -> Result<CellOutcome, String> {
        let binding = matrix_binding_from_frozen_cell(cell, ids, task)?;
        if self.allow_live_provider_effects {
            return self.execute_live_ledger_cell(
                store, principal, frozen, run_id, cell, task, ids, target, binding,
            );
        }
        let intake = build_rwe_cell_product_intake_with_matrix(
            principal,
            frozen,
            task,
            ids,
            target,
            Some(binding.clone()),
        )?;
        // Product gate must be on for store-owned intake.
        let admitted = store
            .admit_product_task(&intake, principal.principal_id())
            .map_err(|e| format!("ledger cell product task admit failed: {e}"))?;
        let product_task_id = admitted
            .get("task_id")
            .and_then(Value::as_str)
            .unwrap_or(&ids.product_task_id)
            .to_string();

        // Provider-free composition: the real orchestration loop runs with
        // effect-free attestations (see [`run_provider_free_ledger_cell_loop`]).
        let record = run_provider_free_ledger_cell_loop(&binding, &task.objective_sha256)?;
        let pkg = match &self.campaign_package {
            Some(p) => p.clone(),
            None => crate::rwe::campaign_package::canonical_deepseek_v2_package()?,
        };
        let pkg_sha = pkg.canonical_sha256().unwrap_or_else(|_| "unknown".into());
        Ok(CellOutcome {
            classification: "blocked_provider_free_mode".into(),
            provider_requests: 0,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            latency_ms: 0,
            monetary_cost: Some(0.0),
            cost_unknown: false,
            live_provider_request: false,
            provider_transport_provenance: "none".into(),
            evidence_source: "product_golden_path_owner".into(),
            verification_status: "not_run".into(),
            verification_trustworthy: false,
            approval_id: None,
            output_draft_pr: None,
            terminal_evidence_id: None,
            terminal_content_sha256: None,
            cleanup_status: "not_required".into(),
            product_task_id,
            workflow_id: ids.workflow_id.clone(),
            node_id: ids.node_id.clone(),
            delegated_attempt_id: ids.delegated_attempt_id.clone(),
            workspace_id: ids.worktree_id.clone(),
            note: format!(
                "ledger cell loop executed through {RWE_LEDGER_CELL_COMPOSITION_SEAM} for run {run_id}; \
                 ledger_terminal_disposition={} ledger_terminal_state={:?} ledger_final_hash={} \
                 usage_conservation={} missing_evidence={} package_id={} package_sha={} cell={}",
                record.disposition.as_str(),
                record.terminal_state,
                record.final_ledger_hash,
                record.usage_conservation_verified,
                record.missing_evidence.join(","),
                pkg.package_id,
                pkg_sha,
                ids.cell_id,
            ),
        })
    }
}

/// Build the exact ProductTask intake mapping for one frozen RWE cell.
///
/// This is the intake half of the composition seam: git_worktree, draft_pr,
/// frozen source revision/tree hash, managed_deepseek, and the frozen verifier
/// command validated by the shared product verifier parser. It does not admit,
/// execute, or invent a target tree.
pub fn build_rwe_cell_product_intake(
    principal: &AuthenticatedPrincipal,
    frozen: &OperatorFrozenContractSet,
    task: &RweTaskDefinition,
    ids: &CellIdentities,
    target_repo_path: &std::path::Path,
) -> Result<crate::product_golden_path::ValidatedProductTaskIntake, String> {
    build_rwe_cell_product_intake_with_matrix(principal, frozen, task, ids, target_repo_path, None)
}

/// Read producer-owned matrix-cell provenance from a frozen schedule cell.
/// The schedule cell carries the frozen plan/cell owner fields under the
/// `matrix_*` keys; the schedule-validated `cell_id`/`task_id`/`repetition`
/// identities cross-check the binding before execution. A cell that is not
/// a well-formed matrix cell fails closed here: an unbound ledger cell can
/// never execute through the seam.
pub fn matrix_binding_from_frozen_cell(
    cell: &Value,
    ids: &CellIdentities,
    task: &RweTaskDefinition,
) -> Result<ProductHarnessMatrixBinding, String> {
    fn cell_str(cell: &Value, key: &str) -> Result<String, String> {
        cell.get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                format!("ledger cell is not a well-formed frozen matrix cell: missing {key}")
            })
    }
    let repetition = cell
        .get("matrix_repetition")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            "ledger cell is not a well-formed frozen matrix cell: missing matrix_repetition"
                .to_string()
        })?;
    let candidate = ProductHarnessMatrixBinding {
        plan_id: cell_str(cell, "matrix_plan_id")?,
        manifest_sha256: cell_str(cell, "matrix_manifest_sha256")?,
        rung: cell_str(cell, "matrix_rung")?,
        repetition: u32::try_from(repetition).map_err(|_| {
            "ledger cell matrix_repetition exceeds the admitted repetition bound".to_string()
        })?,
        cell_id: cell_str(cell, "cell_id")?,
        cell_descriptor_sha256: cell_str(cell, "matrix_cell_descriptor_sha256")?,
        harness_id: cell_str(cell, "harness_id")?,
        model_id: cell_str(cell, "matrix_model_id")?,
        strategy_id: cell_str(cell, "matrix_strategy_id")?,
        task_id: cell_str(cell, "task_id")?,
    };
    let binding =
        validate_matrix_binding(&candidate).map_err(|error| format!("ledger cell {error}"))?;
    // The binding must name exactly the schedule-validated cell being
    // executed; a binding for another cell/task/repetition is rejected.
    if binding.cell_id != ids.cell_id {
        return Err("ledger cell matrix binding names a different cell_id".to_string());
    }
    if binding.task_id != ids.task_id || binding.task_id != task.task_id {
        return Err("ledger cell matrix binding names a different task_id".to_string());
    }
    if u64::from(binding.repetition) != ids.repetition {
        return Err("ledger cell matrix binding names a different repetition".to_string());
    }
    Ok(binding)
}

pub fn build_rwe_cell_product_intake_with_matrix(
    principal: &AuthenticatedPrincipal,
    frozen: &OperatorFrozenContractSet,
    task: &RweTaskDefinition,
    ids: &CellIdentities,
    target_repo_path: &std::path::Path,
    matrix_binding: Option<ProductHarnessMatrixBinding>,
) -> Result<crate::product_golden_path::ValidatedProductTaskIntake, String> {
    use crate::product_golden_path::{
        validate_intake, ProductExecutorPolicy, ProductTaskBudget, ProductTaskIntakeRequest,
        ProductVerificationCommand,
    };

    if frozen.corpus.disposable_target_repo != OPERATOR_TARGET_REPO {
        return Err("frozen target repo mismatch".into());
    }
    if task.source_commit != FROZEN_RWE_TARGET_MAIN_SHA {
        return Err("frozen target main SHA mismatch".into());
    }
    let verifier = task
        .expected_verification_commands
        .first()
        .cloned()
        .ok_or("frozen task missing verification command")?;
    // Strict shared parser: reject non-admitted shapes before intake.
    crate::product_golden_path::parse_strict_product_verification_command(&verifier)?;

    let request = ProductTaskIntakeRequest {
        objective: format!(
            "RWE cell {} task {} (objective hash {})",
            ids.cell_id, task.task_id, task.objective_sha256
        ),
        target_id: format!(
            "rwe-{}",
            frozen.corpus.disposable_target_repo.replace('/', "-")
        ),
        target_repo_path: target_repo_path.to_string_lossy().into_owned(),
        source_kind: Some("git_repository".into()),
        source_revision: task.source_commit.clone(),
        source_tree_hash: Some(task.source_tree_hash.clone()),
        allowed_paths: task.allowed_mutable_paths.clone(),
        verification_commands: vec![ProductVerificationCommand {
            command: verifier,
            timeout_ms: task.timeout_ms.clamp(1, 900_000),
        }],
        output_intent: "draft_pr".into(),
        executor_policy: ProductExecutorPolicy {
            allowed_executors: vec!["managed_deepseek".into()],
            prefer: Some("managed_deepseek".into()),
        },
        budget: Some(ProductTaskBudget {
            total_tokens: Some(task.per_task_max_total_tokens),
            total_calls: Some(task.per_task_max_provider_requests),
            total_elapsed_ms: Some(task.timeout_ms),
            max_retries: Some(task.per_task_max_retries),
            max_repairs: Some(0),
            max_concurrency: Some(1),
            stage_budgets: None,
        }),
        risk_class: FROZEN_RWE_RISK_CLASS.into(),
        approval_required: true,
        confirm_execution: Some(true),
        confirm_output: Some(true),
        idempotency_key: ids.product_task_id.clone(),
        expected_version: None,
        tenant_id: Some(principal.tenant_id().into()),
        workspace_id: Some(ids.worktree_id.clone()),
        workspace_mode: Some("git_worktree".into()),
        matrix_binding,
    };
    validate_intake(&request, principal.tenant_id(), &ids.worktree_id)
}
