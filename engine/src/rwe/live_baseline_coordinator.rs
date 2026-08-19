//! Provider-free first-live-baseline coordinator for Minimum First RWE.
//!
//! Wires store-owned `rwe_run_authorization.v2` issue/admit to the frozen 4-cell
//! schedule under existing Product Golden Path and `LocalProductStore` owners for
//! the exact frozen RWE bindings. Cell dispatch is fenced by store-owned atomic
//! claim (run↔authorization binding + full next-cell budget envelope).
//!
//! Production composition uses [`ProductGoldenPathCellDriver`]. Injectable
//! drivers are test-only and cannot seal. Sealing requires immutable store-owned
//! ProductTask/terminal/provider-journal evidence. Provider POSTs and target
//! writes remain off until controller live authorization after merge; tests use
//! fake transports only.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::corpus::RweTaskDefinition;
use super::frozen_rwe_bindings::{
    current_comparison_manifest, rwe_composition_seam_ready, RweCellBudgetEnvelope,
    FROZEN_RWE_RISK_CLASS, FROZEN_RWE_TARGET_MAIN_SHA,
};
use super::operator_corpus::{
    freeze_current_operator_contract_set, OperatorFrozenContractSet, OPERATOR_ADMITTED_BINARY_PATH,
    OPERATOR_ADMITTED_BINARY_VERSION, OPERATOR_ADMITTED_MODEL,
    OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL, OPERATOR_TARGET_REPO,
};
use super::runner::{
    persist_rwe_run_authorization_v2, RWE_RUN_AUTH_V2_SCHEMA, RWE_RUN_EVIDENCE_SCHEMA,
};
use crate::provider::config::{CredentialRef, ProviderConfig};
use crate::provider::credential::CredentialBoundary;
use crate::provider::managed_deepseek::{
    DeepSeekProtocol, ManagedCallLimits, ManagedDeepSeekProvider, DEEPSEEK_CREDENTIAL_REFERENCE,
    DEEPSEEK_OPENAI_BASE_URL, DEEPSEEK_OPENAI_PATH, DEEPSEEK_PROVIDER_KIND,
};
use crate::provider::managed_deepseek_executor::{
    ManagedDeepSeekExecutorConfig, ManagedDeepSeekNodeExecutor,
};
use crate::storage::local_product_store::{
    compute_attempt_manifest_sha256, AuthenticatedPrincipal, DelegationContract, LocalProductStore,
    RweAuthorizationV2IssueRequest, DELEGATION_SCHEMA_VERSION,
};

pub const RWE_LIVE_BASELINE_COORDINATOR_SCHEMA: &str = "rwe_live_baseline_coordinator.v1";
pub const RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA: &str = "rwe_cell_attempt_evidence.v1";

/// Composition seam is callable for exact frozen RWE bindings under existing owners.
pub const RWE_LIVE_CELL_COMPOSITION_SEAM: &str =
    "rwe_cell_composition:product_golden_path+local_product_store:frozen_rwe_bindings.v1";

/// Operator live-run token: live provider POSTs and target writes require the
/// explicit `=1` symbol in the parent process (parity with the armed fixture's
/// `ACP_RWE_ARMED_LIVE_RUN` gate; CI never sets either).
pub const RWE_OPERATOR_LIVE_RUN_TOKEN: &str = "ACP_RWE_OPERATOR_LIVE_RUN";

fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = map.get(&k) {
                    out.insert(k, sort_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

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
}

impl Clone for ProductGoldenPathCellDriver {
    fn clone(&self) -> Self {
        Self {
            target_repo_path: self.target_repo_path.clone(),
            allow_live_provider_effects: self.allow_live_provider_effects,
            fake_transport: self.fake_transport.clone(),
            cell_executor_key_id: self.cell_executor_key_id.clone(),
            cell_confirmer_key_id: self.cell_confirmer_key_id.clone(),
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
            .finish()
    }
}

impl CellDriver for ProductGoldenPathCellDriver {
    fn ensure_effects_ready(&self) -> Result<(), String> {
        if std::env::var("CI").ok().as_deref() == Some("true") {
            return Err(
                "fail closed before cell effect: live RWE cell execution is forbidden in CI".into(),
            );
        }
        rwe_composition_seam_ready()?;
        if self.allow_live_provider_effects {
            // Operator live-run token: parity with the armed integration
            // fixture's ACP_RWE_ARMED_LIVE_RUN=1 gate. Live provider POSTs and
            // target writes require the explicit operator authorization symbol.
            if std::env::var(RWE_OPERATOR_LIVE_RUN_TOKEN).ok().as_deref() != Some("1") {
                return Err(format!(
                    "live RWE cell requires the operator live-run token {RWE_OPERATOR_LIVE_RUN_TOKEN}=1"
                ));
            }
            let cred = std::env::var(DEEPSEEK_CREDENTIAL_REFERENCE)
                .ok()
                .filter(|v| !v.trim().is_empty());
            if cred.is_none() && self.fake_transport.is_none() {
                return Err(format!(
                    "live RWE cell requires {DEEPSEEK_CREDENTIAL_REFERENCE} or an injected fake transport"
                ));
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
        let intake = build_rwe_cell_product_intake(principal, frozen, task, ids, target)?;
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
                    "store-owned ProductTask admitted under {}; live provider/target effects deferred until controller authorization; seam={RWE_LIVE_CELL_COMPOSITION_SEAM}; cell={}",
                    principal.tenant_id(),
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
            &self.fake_transport,
        )
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
    };
    validate_intake(&request, principal.tenant_id(), &ids.worktree_id)
}

fn terminal_classifications() -> &'static [&'static str] {
    &[
        "success",
        "controlled_failure",
        "verifier_failed",
        "provider_known_failure",
        "timeout",
        "cancelled",
        "outcome_unknown",
        "blocked_ci_environment",
        "blocked_provider_free_mode",
        "blocked_missing_credential",
        "blocked_budget",
        "blocked_authority",
        "blocked_live_session_incomplete",
        "cleanup_failed",
        "fixture_success",
        "skipped_by_stop_rule",
        "injected_success",
        "injected_verifier_failed",
        "injected_provider_failure",
        "injected_timeout",
        "injected_cancel",
        "injected_outcome_unknown",
    ]
}

fn is_terminal_classification(c: &str) -> bool {
    terminal_classifications().contains(&c)
}

/// Honest run terminal status. `succeeded` is reachable only through a sealed
/// live baseline or a fixture where EVERY required cell classifies as
/// `fixture_success`. Merely terminalizing (controlled_failure, verifier_failed,
/// timeout, cancelled, blocked_*, cleanup_failed, injected_* classes) is a
/// completed-but-failed run; `outcome_unknown` stays outcome_unknown.
fn run_terminal_status(
    live_baseline_sealed: bool,
    integration_fixture_succeeded: bool,
    cell_results: &[Value],
) -> &'static str {
    if live_baseline_sealed {
        "succeeded"
    } else if cell_results
        .iter()
        .any(|e| e.get("classification").and_then(Value::as_str) == Some("outcome_unknown"))
    {
        "outcome_unknown"
    } else if integration_fixture_succeeded {
        "succeeded"
    } else {
        "failed"
    }
}

fn stop_rules_from_schedule(frozen: &OperatorFrozenContractSet) -> Vec<String> {
    frozen
        .schedule
        .body
        .get("stop_rules")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn should_stop_after_cell(stop_rules: &[String], classification: &str) -> Option<&'static str> {
    if stop_rules.iter().any(|r| r == "stop_on_authority_failure")
        && matches!(
            classification,
            "blocked_authority" | "blocked_missing_credential" | "blocked_ci_environment"
        )
    {
        return Some("stop_on_authority_failure");
    }
    if stop_rules.iter().any(|r| r == "stop_on_budget_exhaustion")
        && classification == "blocked_budget"
    {
        return Some("stop_on_budget_exhaustion");
    }
    None
}

/// Revalidate stored v2 authorization against current freeze owners and principal
/// before any cell effect.
pub fn revalidate_stored_v2_authorization(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    authorization_id: &str,
    frozen: &OperatorFrozenContractSet,
) -> Result<Value, String> {
    let auth = store
        .get_rwe_run_authorization(authorization_id)?
        .ok_or("RWE authorization not found")?;
    if auth.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
        return Err("authorization tenant mismatch".into());
    }
    if auth.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id()) {
        return Err("authorization principal mismatch".into());
    }
    let status = auth.get("status").and_then(Value::as_str).unwrap_or("");
    if !matches!(status, "active" | "consumed") {
        return Err(format!("authorization status {status} is not runnable"));
    }
    let body = auth
        .get("body_json")
        .cloned()
        .ok_or("authorization body_json missing")?;
    if body.get("schema_version").and_then(Value::as_str) != Some(RWE_RUN_AUTH_V2_SCHEMA) {
        return Err("live baseline requires rwe_run_authorization.v2".into());
    }
    // Bindings must match current freeze owners (not caller text).
    let checks = [
        (
            "accepted_main_sha",
            body.get("accepted_main_sha").and_then(Value::as_str),
            Some(frozen.accepted_main_sha.as_str()),
        ),
        (
            "corpus_sha256",
            body.get("corpus_sha256").and_then(Value::as_str),
            Some(frozen.corpus.corpus_sha256.as_str()),
        ),
        (
            "protocol_sha256",
            body.get("protocol_sha256").and_then(Value::as_str),
            Some(frozen.protocol.body_sha256.as_str()),
        ),
        (
            "schedule_sha256",
            body.get("schedule_sha256").and_then(Value::as_str),
            Some(frozen.schedule.schedule_sha256.as_str()),
        ),
        (
            "target_repo",
            body.get("target_repo").and_then(Value::as_str),
            Some(OPERATOR_TARGET_REPO),
        ),
        (
            "provider_kind",
            body.get("provider_kind").and_then(Value::as_str),
            Some(DEEPSEEK_PROVIDER_KIND),
        ),
        (
            "provider_base_url",
            body.get("provider_base_url").and_then(Value::as_str),
            Some(DEEPSEEK_OPENAI_BASE_URL),
        ),
        (
            "provider_path",
            body.get("provider_path").and_then(Value::as_str),
            Some(DEEPSEEK_OPENAI_PATH),
        ),
        (
            "model_identity",
            body.get("model_identity").and_then(Value::as_str),
            Some(OPERATOR_ADMITTED_MODEL),
        ),
        (
            "binary_path",
            body.get("binary_path").and_then(Value::as_str),
            Some(OPERATOR_ADMITTED_BINARY_PATH),
        ),
        (
            "binary_version",
            body.get("binary_version").and_then(Value::as_str),
            Some(OPERATOR_ADMITTED_BINARY_VERSION),
        ),
        (
            "principal_id",
            body.get("principal_id").and_then(Value::as_str),
            Some(principal.principal_id()),
        ),
        (
            "tenant_id",
            body.get("tenant_id").and_then(Value::as_str),
            Some(principal.tenant_id()),
        ),
    ];
    for (field, got, expected) in checks {
        if got != expected {
            return Err(format!(
                "stored v2 authorization {field} binding mismatch (got {got:?}, expected {expected:?})"
            ));
        }
    }
    let target_main = frozen
        .corpus
        .tasks
        .first()
        .map(|t| t.source_commit.as_str())
        .unwrap_or("");
    if body.get("target_main_sha").and_then(Value::as_str) != Some(target_main) {
        return Err("stored v2 authorization target_main_sha mismatch".into());
    }
    let run_max_req = frozen
        .schedule
        .body
        .pointer("/run_level_budget/max_total_provider_requests")
        .and_then(Value::as_u64);
    if body
        .get("max_total_provider_requests")
        .and_then(Value::as_u64)
        != run_max_req
    {
        return Err("stored v2 authorization max_total_provider_requests mismatch".into());
    }
    let run_max_tok = frozen
        .schedule
        .body
        .pointer("/run_level_budget/max_total_tokens")
        .and_then(Value::as_u64);
    if body.get("max_total_tokens").and_then(Value::as_u64) != run_max_tok {
        return Err("stored v2 authorization max_total_tokens mismatch".into());
    }
    if body.get("fixture_only").and_then(Value::as_bool) != Some(false) {
        return Err("live baseline refuses fixture_only authorization".into());
    }
    if body.get("one_use").and_then(Value::as_bool) != Some(true) {
        return Err("live baseline requires one_use authorization".into());
    }
    Ok(auth)
}

/// Provider-free readiness: no auth consumption, no provider, no target write.
pub fn operator_preflight(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    authorization_id: Option<&str>,
    golden_path_prerequisite_product_task_id: Option<&str>,
) -> Result<Value, String> {
    let credential_present = std::env::var(DEEPSEEK_CREDENTIAL_REFERENCE)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_some();
    operator_preflight_with_credential_readiness(
        store,
        principal,
        authorization_id,
        golden_path_prerequisite_product_task_id,
        Some(credential_present),
    )
}

/// Redacted parent-process credential presence. Inspects only whether the
/// named environment symbol exists and is non-empty; it never decodes, logs,
/// or returns the secret value.
pub fn redacted_provider_credential_present() -> bool {
    std::env::var_os(DEEPSEEK_CREDENTIAL_REFERENCE)
        .map(|value| !value.is_empty())
        .unwrap_or(false)
}

/// Provider-free read-only readiness projection. Credential readiness uses the
/// redacted presence owner and never reads credential values.
pub fn operator_preflight_read_only(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    authorization_id: Option<&str>,
    golden_path_prerequisite_product_task_id: Option<&str>,
) -> Result<Value, String> {
    operator_preflight_with_credential_readiness(
        store,
        principal,
        authorization_id,
        golden_path_prerequisite_product_task_id,
        Some(redacted_provider_credential_present()),
    )
}

fn operator_preflight_with_credential_readiness(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    authorization_id: Option<&str>,
    golden_path_prerequisite_product_task_id: Option<&str>,
    credential_present: Option<bool>,
) -> Result<Value, String> {
    let observed_at = store.require_now()?;
    let frozen = freeze_current_operator_contract_set()?;
    let mut blockers = Vec::new();
    let mut notes = Vec::new();

    if std::env::var("CI").ok().as_deref() == Some("true") {
        blockers.push(json!({
            "code": "ci_environment",
            "detail": "live RWE is forbidden in CI"
        }));
    }

    match credential_present {
        Some(true) => {}
        Some(false) => blockers.push(json!({
            "code": "missing_credential_symbol",
            "detail": format!("{DEEPSEEK_CREDENTIAL_REFERENCE} not set in parent process")
        })),
        None => blockers.push(json!({
            "code": "credential_readiness_unavailable",
            "detail": "provider credential readiness is unavailable in the read-only provider-free preflight; no credential value was read"
        })),
    }

    let comparison = match current_comparison_manifest() {
        Ok(manifest) => manifest.to_json(),
        Err(error) => {
            blockers.push(json!({
                "code": "comparison_identity_invalid",
                "detail": error,
            }));
            Value::Null
        }
    };

    if let Err(e) = rwe_composition_seam_ready() {
        blockers.push(json!({
            "code": "composition_seam_not_ready",
            "detail": e
        }));
    }

    let prereq_id = golden_path_prerequisite_product_task_id.unwrap_or("");
    let mut gp_ready = false;
    if prereq_id.is_empty() {
        blockers.push(json!({
            "code": "missing_golden_path_prerequisite_id",
            "detail": "operator must supply same-tenant Golden Path prerequisite ProductTask id"
        }));
    } else {
        match store.get_product_task_for_tenant(prereq_id, principal.tenant_id()) {
            Ok(Some(_)) => match store.get_product_task_terminal_evidence(prereq_id) {
                Ok(ev) if !ev.is_null() => {
                    if ev.get("tenant_id").and_then(Value::as_str) == Some(principal.tenant_id())
                        && ev.get("task_status").and_then(Value::as_str) == Some("completed")
                    {
                        gp_ready = true;
                    } else {
                        blockers.push(json!({
                            "code": "golden_path_prerequisite_not_ready",
                            "detail": "terminal evidence missing completed same-tenant seal"
                        }));
                    }
                }
                _ => {
                    blockers.push(json!({
                        "code": "golden_path_prerequisite_missing",
                        "detail": "no terminal evidence for prerequisite ProductTask"
                    }));
                }
            },
            Ok(None) => {
                blockers.push(json!({
                    "code": "golden_path_prerequisite_not_found",
                    "detail": "ProductTask not found for authenticated tenant"
                }));
            }
            Err(e) => {
                blockers.push(json!({
                    "code": "golden_path_prerequisite_tenant_mismatch",
                    "detail": e
                }));
            }
        }
    }

    let mut auth_status = Value::Null;
    if let Some(auth_id) = authorization_id {
        match store.get_rwe_run_authorization(auth_id)? {
            Some(row) => {
                auth_status = json!({
                    "authorization_id": auth_id,
                    "status": row.get("status"),
                    "fixture_only": row.get("fixture_only"),
                    "expires_at": row.get("expires_at"),
                    "tenant_id": row.get("tenant_id"),
                });
                if row.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
                    blockers.push(json!({
                        "code": "authorization_tenant_mismatch",
                        "detail": "authorization tenant does not match principal"
                    }));
                }
                if row.get("status").and_then(Value::as_str) != Some("active") {
                    blockers.push(json!({
                        "code": "authorization_not_active",
                        "detail": row.get("status").cloned().unwrap_or(Value::Null)
                    }));
                }
                if row
                    .get("body_json")
                    .and_then(|b| b.get("schema_version"))
                    .and_then(Value::as_str)
                    != Some(RWE_RUN_AUTH_V2_SCHEMA)
                {
                    blockers.push(json!({
                        "code": "authorization_not_v2",
                        "detail": "live baseline requires rwe_run_authorization.v2"
                    }));
                }
            }
            None => {
                notes.push(json!(
                    "authorization_id not yet issued; operator may issue under Board B"
                ));
            }
        }
    } else {
        notes.push(json!(
            "no authorization_id supplied; preflight does not issue or consume"
        ));
    }

    let target_main = frozen
        .corpus
        .tasks
        .first()
        .map(|t| t.source_commit.as_str())
        .unwrap_or("");
    if target_main != FROZEN_RWE_TARGET_MAIN_SHA {
        blockers.push(json!({
            "code": "frozen_target_sha_mismatch",
            "detail": target_main
        }));
    }
    if frozen.corpus.disposable_target_repo != OPERATOR_TARGET_REPO {
        blockers.push(json!({
            "code": "frozen_target_repo_mismatch",
            "detail": frozen.corpus.disposable_target_repo
        }));
    }

    store.ensure_read_only_snapshot_stable()?;

    // Ready only when composition seam, GP prereq, and other pre-effect gates pass.
    let ready = blockers.is_empty() && gp_ready;
    Ok(sort_value(&json!({
        "schema_version": "rwe_operator_preflight.v1",
        "observed_at": observed_at,
        "ready": ready,
        "live_baseline_sealed": false,
        "provider_call_performed": false,
        "target_write_performed": false,
        "authority_consumed": false,
        "frozen": {
            "accepted_main_sha": frozen.accepted_main_sha,
            "corpus_sha256": frozen.corpus.corpus_sha256,
            "protocol_sha256": frozen.protocol.body_sha256,
            "schedule_sha256": frozen.schedule.schedule_sha256,
            "cell_count": frozen.schedule.body.get("cells").and_then(Value::as_array).map(|a| a.len()),
            "target_repo": OPERATOR_TARGET_REPO,
            "target_main_sha": target_main,
            "provider_kind": DEEPSEEK_PROVIDER_KIND,
            "provider_base_url": DEEPSEEK_OPENAI_BASE_URL,
            "provider_path": DEEPSEEK_OPENAI_PATH,
            "model": OPERATOR_ADMITTED_MODEL,
            "binary_path": OPERATOR_ADMITTED_BINARY_PATH,
            "binary_version": OPERATOR_ADMITTED_BINARY_VERSION,
        },
        "comparison": comparison,
        "principal": {
            "tenant_id": principal.tenant_id(),
            "principal_id": principal.principal_id(),
            "principal_kind": principal.principal_kind().as_str(),
        },
        "credential_symbol_present": credential_present,
        "credential_readiness": match credential_present {
            Some(true) => "present",
            Some(false) => "missing",
            None => "unavailable",
        },
        "credential_reference": DEEPSEEK_CREDENTIAL_REFERENCE,
        "golden_path_prerequisite_ready": gp_ready,
        "authorization": auth_status,
        "blockers": blockers,
        "notes": notes,
    })))
}

/// Issue (if needed) and admit a v2 authorization, returning lease for coordination.
pub fn issue_and_admit_v2(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    authorization_id: &str,
    run_id: &str,
    golden_path_prerequisite_product_task_id: &str,
    expires_at: &str,
) -> Result<Value, String> {
    let pre = operator_preflight(
        store,
        principal,
        None,
        Some(golden_path_prerequisite_product_task_id),
    )?;
    // One-use live authority must not be consumed unless the complete runnable
    // seam and all pre-effect prerequisites are ready: composition seam, Golden
    // Path prerequisite, non-CI environment, and credential symbol present.
    // A run that cannot genuinely execute cells never consumes the authority.
    if pre.get("ready").and_then(Value::as_bool) != Some(true) {
        return Err(format!(
            "fail closed before RWE authority consumption: {}",
            pre.get("blockers").cloned().unwrap_or(json!([]))
        ));
    }

    let _issued = persist_rwe_run_authorization_v2(
        store,
        principal,
        &RweAuthorizationV2IssueRequest {
            authorization_id: authorization_id.into(),
            golden_path_prerequisite_product_task_id: golden_path_prerequisite_product_task_id
                .into(),
            expires_at: expires_at.into(),
        },
    )?;

    let auth = store
        .get_rwe_run_authorization(authorization_id)?
        .ok_or("issued authorization missing")?;
    let auth_body = auth.get("body_json").cloned().unwrap_or(Value::Null);
    let mut run_body = auth_body.clone();
    if let Value::Object(ref mut m) = run_body {
        m.insert("run_id".into(), json!(run_id));
        m.insert("authorization_id".into(), json!(authorization_id));
        m.insert("provider_free_fixture".into(), json!(false));
        m.insert("schema_version".into(), json!("rwe_run_body.v2"));
    }
    let admitted = store.admit_rwe_run(principal, run_id, authorization_id, &run_body, false)?;
    Ok(admitted)
}

fn build_cell_evidence(
    run_id: &str,
    authorization_id: &str,
    frozen: &OperatorFrozenContractSet,
    cell: &Value,
    task: &RweTaskDefinition,
    ids: &CellIdentities,
    outcome: &CellOutcome,
) -> Value {
    sort_value(&json!({
        "schema_version": RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
        "run_id": run_id,
        "authorization_id": authorization_id,
        "cell_id": ids.cell_id,
        "task_id": ids.task_id,
        "definition_sha256": ids.definition_sha256,
        "objective_sha256": task.objective_sha256,
        "repetition": ids.repetition,
        "seed": ids.seed,
        "budget_point_id": ids.budget_point_id,
        "sequential_order": ids.sequential_order,
        "product_task_id": outcome.product_task_id,
        "workflow_id": outcome.workflow_id,
        "node_id": outcome.node_id,
        "delegated_attempt_id": outcome.delegated_attempt_id,
        "workspace_id": outcome.workspace_id,
        "branch_name": ids.branch_name,
        "classification": outcome.classification,
        "evidence_source": outcome.evidence_source,
        "provider_requests": outcome.provider_requests,
        "input_tokens": outcome.input_tokens,
        "output_tokens": outcome.output_tokens,
        "total_tokens": outcome.total_tokens,
        "latency_ms": outcome.latency_ms,
        "monetary_cost": outcome.monetary_cost,
        "cost_unknown": outcome.cost_unknown,
        "live_provider_request": outcome.live_provider_request,
        "provider_transport_provenance": outcome.provider_transport_provenance,
        "verification": {
            "status": outcome.verification_status,
            "trustworthy": outcome.verification_trustworthy,
            "commands": task.expected_verification_commands,
        },
        "approval_id": outcome.approval_id,
        "output": {
            "intent": "draft_pr",
            "draft_pr": outcome.output_draft_pr,
        },
        "terminal_evidence_id": outcome.terminal_evidence_id,
        "terminal_content_sha256": outcome.terminal_content_sha256,
        "cleanup_status": outcome.cleanup_status,
        "note": outcome.note,
        "frozen_bindings": {
            "corpus_sha256": frozen.corpus.corpus_sha256,
            "protocol_sha256": frozen.protocol.body_sha256,
            "schedule_sha256": frozen.schedule.schedule_sha256,
            "model": OPERATOR_ADMITTED_MODEL,
            "target_repo": OPERATOR_TARGET_REPO,
            "cell": cell,
        },
        "schedule_cell_budget": {
            "max_provider_requests": cell.get("max_provider_requests"),
            "max_total_tokens": cell.get("max_total_tokens"),
            "max_wall_time_ms": cell.get("max_wall_time_ms"),
            "max_cost": cell.get("max_cost"),
        },
    }))
}

fn evaluate_store_owned_live_baseline_seal(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    frozen: &OperatorFrozenContractSet,
    run_id: &str,
    stopped_by: &Option<String>,
    cell_results: &[Value],
) -> bool {
    // Never trust mutable driver/evidence_json strings for seal decisions.
    // Derive scheduled identities, then compare immutable store rows.
    let cells = match frozen.schedule.body.get("cells").and_then(Value::as_array) {
        Some(c) if !c.is_empty() => c,
        _ => return false,
    };
    let stop_rules = stop_rules_from_schedule(frozen);
    let mut executed = 0usize;
    let mut any_skipped = false;

    for cell in cells {
        let task_id = match cell.get("task_id").and_then(Value::as_str) {
            Some(t) => t,
            None => return false,
        };
        let task_def = match frozen.corpus.tasks.iter().find(|t| t.task_id == task_id) {
            Some(t) => t,
            None => return false,
        };
        let ids = match cell_identities_for(run_id, cell, task_def) {
            Ok(i) => i,
            Err(_) => return false,
        };
        // Lookup ProductTask by deterministic schedule-bound idempotency key only.
        let product_task = match store.get_product_task_by_idempotency(
            principal.tenant_id(),
            &ids.worktree_id,
            &ids.product_task_id,
        ) {
            Ok(Some(t)) => t,
            _ => {
                // Skipped cells have no ProductTask.
                if cell_results.iter().any(|ev| {
                    ev.get("cell_id").and_then(Value::as_str) == Some(ids.cell_id.as_str())
                        && ev.get("classification").and_then(Value::as_str)
                            == Some("skipped_by_stop_rule")
                }) {
                    any_skipped = true;
                    continue;
                }
                return false;
            }
        };
        let product_task_id = match product_task.get("task_id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => return false,
        };
        executed += 1;

        // Project canonical store receipts (terminal.v2 + managed provider_execution).
        let attempt_id = ids.delegated_attempt_id.clone();
        let projection = match store.project_rwe_cell_store_evidence(&product_task_id, &attempt_id)
        {
            Ok(p) => p,
            Err(_) => return false,
        };
        if projection
            .pointer("/product_task/status")
            .and_then(Value::as_str)
            != Some("completed")
        {
            return false;
        }
        let te = match projection.get("terminal_evidence") {
            Some(v) if !v.is_null() => v,
            _ => return false,
        };
        if te.get("schema_version").and_then(Value::as_str)
            != Some("product_task_terminal_evidence.v2")
        {
            return false;
        }
        if te.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id())
            || te.get("product_task_id").and_then(Value::as_str) != Some(product_task_id.as_str())
            || te.get("task_status").and_then(Value::as_str) != Some("completed")
        {
            return false;
        }
        // Exact scheduled identity bindings vs store rows.
        if te.get("run_id").and_then(Value::as_str)
            != product_task.get("run_id").and_then(Value::as_str)
            || product_task.get("run_id").and_then(Value::as_str).is_none()
        {
            return false;
        }
        if te
            .pointer("/node/node_id")
            .and_then(Value::as_str)
            .is_none_or(|s| s.is_empty())
        {
            return false;
        }
        if te
            .get("workspace_record_id")
            .and_then(Value::as_str)
            .is_none_or(|s| s.is_empty())
        {
            return false;
        }
        if te.get("source_revision").and_then(Value::as_str) != Some(FROZEN_RWE_TARGET_MAIN_SHA) {
            return false;
        }
        // product_task_terminal_evidence.v2 verification uses evidence_recorded + trustworthy.
        let verification_ok = te
            .pointer("/verification/trustworthy")
            .and_then(Value::as_bool)
            == Some(true)
            && te.pointer("/verification/status").and_then(Value::as_str)
                == Some("evidence_recorded");
        let approval_ok = te
            .pointer("/approval/approval_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        let artifact_ok = te
            .pointer("/artifact/artifact_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        let output_ok = te.pointer("/output/draft_pr").is_some()
            || te.pointer("/output/receipt_id").is_some()
            || te.pointer("/output/operation_id").is_some();
        let terminal_hash_ok = te
            .get("content_sha256")
            .and_then(Value::as_str)
            .is_some_and(|s| s.len() == 64);
        if !verification_ok || !approval_ok || !artifact_ok || !output_ok || !terminal_hash_ok {
            return false;
        }
        // Canonical managed provider_execution lives on artifact confirmation, not terminal.v2.
        let pe = match projection.get("provider_execution") {
            Some(v) if !v.is_null() => v,
            _ => return false,
        };
        if pe.get("schema_version").and_then(Value::as_str)
            != Some("managed_deepseek_execution_evidence.v1")
        {
            return false;
        }
        let pe_count = pe
            .get("provider_request_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let pe_requests = pe
            .get("requests")
            .and_then(Value::as_array)
            .map(|a| a.len() as u64)
            .unwrap_or(0);
        if pe_count != 3 || pe_requests != 3 {
            return false;
        }
        let journal = projection
            .get("provider_request_journal")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        if journal != 3 {
            return false;
        }
        // Store-owned transport provenance gate: a live baseline seal requires
        // every durable journal claim and the aggregated execution evidence to
        // attest the production external transport. Injected or missing
        // provenance fails closed regardless of receipts or outcomes.
        if store_evidence_transport_provenance(&projection) != Ok("external".to_string()) {
            return false;
        }
        // Cleanup: workspace must be cleaned (store status) for seal honesty.
        // A missing or errored workspace record fails the seal closed.
        match product_task
            .get("workspace_record_id")
            .and_then(Value::as_str)
        {
            Some(ws_id) => match store.get_supervised_patch_workspace(ws_id) {
                Ok(Some(ws)) => {
                    if ws.get("status").and_then(Value::as_str) != Some("cleaned") {
                        return false;
                    }
                }
                Ok(None) | Err(_) => return false,
            },
            None => return false,
        }
    }
    if executed == 0 {
        return false;
    }
    // Exact preregistered stop trigger when any cells skipped.
    if any_skipped {
        let Some(rule) = stopped_by.as_deref() else {
            return false;
        };
        if !stop_rules.iter().any(|r| r == rule) {
            return false;
        }
    } else if cells.len() != executed {
        return false;
    }
    true
}

/// Sentinel for candidate modification of the evaluator surface: test
/// collection, verifier config, ignore/baseline, or the frozen suite fixture.
/// Cells whose artifact touches these files are never classified `success`.
fn changed_file_is_evaluator_surface(changed: &str) -> bool {
    const EVALUATOR_SURFACE_BASENAMES: &[&str] = &[
        "conftest.py",
        "pytest.ini",
        "tox.ini",
        "setup.cfg",
        ".coveragerc",
        "pyproject.toml",
    ];
    let basename = changed.rsplit('/').next().unwrap_or(changed);
    EVALUATOR_SURFACE_BASENAMES.contains(&basename)
}

/// Classify a post-fence execution error into a durable terminal classification.
fn classify_execution_error(err: &str) -> &'static str {
    let e = err.to_ascii_lowercase();
    if e.contains("timeout") || e.contains("timed out") {
        "timeout"
    } else if e.contains("cancel") {
        "cancelled"
    } else if e.contains("kill") {
        "killed"
    } else if e.contains("cleanup") {
        "cleanup_failed"
    } else if e.contains("outcome_unknown") || e.contains("unknown external") {
        "outcome_unknown"
    } else if e.contains("verif") {
        "verifier_failed"
    } else if e.contains("provider") {
        "provider_known_failure"
    } else {
        "controlled_failure"
    }
}

fn mark_cleanup_failed(outcome: &mut CellOutcome, detail: impl Into<String>) {
    if outcome.classification != "outcome_unknown" {
        outcome.classification = "cleanup_failed".into();
    }
    outcome.cleanup_status = "failed".into();
    outcome.note = format!(
        "{}; delegated failure cleanup unavailable: {}",
        outcome.note,
        detail.into()
    );
}

/// Store-owned transport provenance from the RWE cell evidence projection.
///
/// Returns `external` only when the aggregated execution evidence and every
/// durable journal claim attest the production external transport. Missing,
/// invalid, or mixed provenance fails closed with an error.
fn store_evidence_transport_provenance(projection: &Value) -> Result<String, String> {
    let journal_projection = provider_execution_from_journal(projection);
    let pe = projection
        .get("provider_execution")
        .filter(|value| !value.is_null())
        .or(journal_projection.as_ref())
        .ok_or("RWE cell store evidence lacks provider execution")?;
    let aggregate = pe
        .get("transport_provenance")
        .and_then(Value::as_str)
        .ok_or("RWE cell provider execution lacks transport provenance")?;
    if !matches!(aggregate, "external" | "injected") {
        return Err("RWE cell provider execution transport provenance is invalid".into());
    }
    let journal = projection
        .get("provider_request_journal")
        .and_then(Value::as_array)
        .ok_or("RWE cell store evidence lacks the provider request journal")?;
    if journal.is_empty() {
        return Err("RWE cell provider request journal is empty".into());
    }
    for entry in journal {
        let provenance = entry
            .get("transport_provenance")
            .and_then(Value::as_str)
            .ok_or("RWE cell journal claim lacks transport provenance")?;
        if provenance != aggregate {
            return Err("RWE cell provider transport provenance is mixed or conflicting".into());
        }
    }
    Ok(aggregate.to_string())
}

/// Failed delegated stages can stop before artifact confirmation, but the
/// existing managed-acceptance owner still durably records every provider
/// request in its journal. Reuse that receipt for accounting rather than
/// reporting a false zero. Incomplete journal entries stay unavailable.
fn provider_execution_from_journal(projection: &Value) -> Option<Value> {
    if projection
        .get("provider_execution")
        .is_some_and(|value| !value.is_null())
    {
        return None;
    }
    let journal = projection
        .get("provider_request_journal")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty())?;
    let mut provenance: Option<&str> = None;
    let mut cumulative_tokens = 0_u64;
    let mut realized_cost_usd = 0.0_f64;
    let mut cost_unknown = false;
    let mut requests = Vec::with_capacity(journal.len());
    for entry in journal {
        let status = entry.get("status").and_then(Value::as_str)?;
        if !matches!(
            status,
            "succeeded" | "failed_before_send" | "failed_known_outcome" | "outcome_unknown"
        ) {
            return None;
        }
        let entry_provenance = entry.get("transport_provenance").and_then(Value::as_str)?;
        if !matches!(entry_provenance, "external" | "injected")
            || provenance.is_some_and(|seen| seen != entry_provenance)
        {
            return None;
        }
        provenance = Some(entry_provenance);
        let cost = entry.get("effective_cost_usd").and_then(Value::as_f64)?;
        if !cost.is_finite() || cost < 0.0 {
            return None;
        }
        let effective_tokens = entry
            .get("effective_tokens")
            .or_else(|| entry.pointer("/usage/cumulative_tokens"))
            .and_then(Value::as_u64)?;
        if status == "succeeded" {
            entry
                .pointer("/usage/input_tokens")
                .and_then(Value::as_u64)?;
            entry
                .pointer("/usage/output_tokens")
                .and_then(Value::as_u64)?;
        }
        if matches!(status, "succeeded" | "outcome_unknown") {
            cumulative_tokens = cumulative_tokens.checked_add(effective_tokens)?;
            if status == "succeeded" {
                let next_cost = realized_cost_usd + cost;
                if !next_cost.is_finite() {
                    return None;
                }
                realized_cost_usd = next_cost;
            } else {
                cost_unknown = true;
            }
            requests.push(entry.clone());
        }
    }
    Some(json!({
        "schema_version": "managed_deepseek_execution_evidence.v1",
        "provider_request_count": requests.len(),
        "transport_provenance": provenance?,
        "requests": requests,
        "cumulative_tokens": cumulative_tokens,
        "realized_cost_usd": realized_cost_usd,
        "cost_unknown": cost_unknown,
    }))
}

/// Couple final usage to canonical managed provider_execution / journal on store.
/// Never trust driver-supplied usage for accounting or seal.
fn couple_usage_to_store_journal(store: &LocalProductStore, outcome: &mut CellOutcome) {
    if outcome.evidence_source == "injected" || outcome.product_task_id.is_empty() {
        return;
    }
    let attempt = if outcome.delegated_attempt_id.is_empty() {
        return;
    } else {
        outcome.delegated_attempt_id.as_str()
    };
    let Ok(proj) = store.project_rwe_cell_store_evidence(&outcome.product_task_id, attempt) else {
        return;
    };
    let journal_has_outcome_unknown = proj
        .get("provider_request_journal")
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries
                .iter()
                .any(|entry| entry.get("status").and_then(Value::as_str) == Some("outcome_unknown"))
        });
    if journal_has_outcome_unknown {
        outcome.classification = "outcome_unknown".into();
        outcome.cost_unknown = true;
        outcome.monetary_cost = None;
    }
    // Store-owned provenance gate: live provider claims require the durable
    // journal to attest the external transport; anything else stays non-live.
    match store_evidence_transport_provenance(&proj) {
        Ok(provenance) if provenance == "external" => {
            outcome.provider_transport_provenance = "external".into();
        }
        Ok(provenance) => {
            outcome.provider_transport_provenance = provenance;
            outcome.live_provider_request = false;
        }
        Err(_) => {
            outcome.provider_transport_provenance = "none".into();
            outcome.live_provider_request = false;
            return;
        }
    }
    let journal_projection = provider_execution_from_journal(&proj);
    let pe = proj
        .get("provider_execution")
        .filter(|value| !value.is_null())
        .or(journal_projection.as_ref());
    if let Some(count) = pe
        .and_then(|p| p.get("provider_request_count"))
        .and_then(Value::as_u64)
    {
        outcome.provider_requests = count;
        outcome.live_provider_request =
            count > 0 && outcome.provider_transport_provenance == "external";
    } else if let Some(arr) = pe.and_then(|p| p.get("requests")).and_then(Value::as_array) {
        outcome.provider_requests = arr.len() as u64;
        outcome.live_provider_request =
            !arr.is_empty() && outcome.provider_transport_provenance == "external";
    }
    if let Some(tok) = pe
        .and_then(|p| p.get("cumulative_tokens"))
        .and_then(Value::as_u64)
    {
        outcome.total_tokens = tok;
    }
    if let Some(requests) = pe.and_then(|p| p.get("requests")).and_then(Value::as_array) {
        outcome.input_tokens = requests
            .iter()
            .filter_map(|request| {
                request
                    .pointer("/usage/input_tokens")
                    .and_then(Value::as_u64)
            })
            .fold(0_u64, u64::saturating_add);
        outcome.output_tokens = requests
            .iter()
            .filter_map(|request| {
                request
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64)
            })
            .fold(0_u64, u64::saturating_add);
    }
    if pe
        .and_then(|p| p.get("cost_unknown"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        outcome.cost_unknown = true;
        outcome.monetary_cost = None;
    } else if !outcome.cost_unknown {
        if let Some(cost) = pe
            .and_then(|p| p.get("realized_cost_usd"))
            .and_then(Value::as_f64)
        {
            outcome.monetary_cost = Some(cost);
        }
    }
    if let Ok(te) = store.get_product_task_terminal_evidence(&outcome.product_task_id) {
        if let Some(id) = te.get("evidence_id").and_then(Value::as_str) {
            outcome.terminal_evidence_id = Some(id.into());
        }
        if let Some(h) = te.get("content_sha256").and_then(Value::as_str) {
            outcome.terminal_content_sha256 = Some(h.into());
        }
    }
}

fn reconstruct_stopped_by(existing: &[Value], stop_rules: &[String]) -> Option<String> {
    for row in existing {
        let class = row
            .get("classification")
            .and_then(Value::as_str)
            .unwrap_or("");
        if class == "dispatched" {
            continue;
        }
        if class == "skipped_by_stop_rule" {
            if let Some(note) = row.pointer("/evidence_json/note").and_then(Value::as_str) {
                if note.starts_with("stop_on_") {
                    return Some(note.to_string());
                }
            }
            return Some("stop_rule_reconstructed".into());
        }
        if let Some(rule) = should_stop_after_cell(stop_rules, class) {
            return Some(rule.into());
        }
    }
    None
}

fn cell_reservation_limits(cell: &Value) -> Result<RweCellBudgetEnvelope, String> {
    RweCellBudgetEnvelope::from_schedule_cell(cell)
}

/// Run the frozen schedule under an admitted authorization and injectable driver.
pub fn run_frozen_schedule(
    store: &std::sync::Arc<LocalProductStore>,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    lease_token: &str,
    driver: &dyn CellDriver,
) -> Result<Value, String> {
    driver.ensure_effects_ready()?;

    let frozen = freeze_current_operator_contract_set()?;
    revalidate_stored_v2_authorization(store, principal, authorization_id, &frozen)?;

    let mut lease = lease_token.to_string();
    if lease.is_empty() {
        let auth = store
            .get_rwe_run_authorization(authorization_id)?
            .ok_or("RWE authorization not found")?;
        let auth_body = auth.get("body_json").cloned().unwrap_or(Value::Null);
        let mut run_body = auth_body;
        if let Value::Object(ref mut m) = run_body {
            m.insert("run_id".into(), json!(run_id));
            m.insert("authorization_id".into(), json!(authorization_id));
            m.insert("provider_free_fixture".into(), json!(false));
            m.insert("schema_version".into(), json!("rwe_run_body.v2"));
        }
        let replay = store.admit_rwe_run(principal, run_id, authorization_id, &run_body, false)?;
        lease = replay
            .get("lease_token")
            .and_then(Value::as_str)
            .ok_or("admit/replay missing lease_token for restart recovery")?
            .to_string();
    }

    let cells = frozen
        .schedule
        .body
        .get("cells")
        .and_then(Value::as_array)
        .ok_or("frozen schedule cells missing")?;
    if cells.len() != 4 {
        return Err(format!(
            "Minimum First RWE schedule must have exactly 4 cells, got {}",
            cells.len()
        ));
    }

    let existing = store.list_rwe_task_attempts_for_run(run_id)?;
    let mut existing_by_attempt: std::collections::BTreeMap<String, Value> =
        std::collections::BTreeMap::new();
    for row in &existing {
        if let Some(id) = row.get("task_attempt_id").and_then(Value::as_str) {
            existing_by_attempt.insert(id.to_string(), row.clone());
        }
    }

    let stop_rules = stop_rules_from_schedule(&frozen);
    let mut stopped_by = reconstruct_stopped_by(&existing, &stop_rules);

    let mut cell_results = Vec::new();
    let mut aggregate_requests = 0u64;
    let mut aggregate_tokens = 0u64;
    let mut any_live_provider = false;
    let mut any_injected_provider = false;

    for cell in cells {
        // Revalidate bindings before every cell effect.
        revalidate_stored_v2_authorization(store, principal, authorization_id, &frozen)?;

        let task_id = cell
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or("cell task_id missing")?;
        let task = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == task_id)
            .ok_or_else(|| format!("frozen corpus missing task {task_id}"))?;
        // Frozen verifier shape must remain admitted.
        for cmd in &task.expected_verification_commands {
            crate::product_golden_path::parse_strict_product_verification_command(cmd)?;
        }
        let ids = cell_identities_for(run_id, cell, task)?;

        if let Some(prior) = existing_by_attempt.get(&ids.rwe_task_attempt_id) {
            let class = prior
                .get("classification")
                .and_then(Value::as_str)
                .unwrap_or("");
            if is_terminal_classification(class) {
                let evidence = prior.get("evidence_json").cloned().unwrap_or(Value::Null);
                if let Some(ev) = prior.get("evidence_json") {
                    aggregate_requests = aggregate_requests.saturating_add(
                        ev.get("provider_requests")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    );
                    aggregate_tokens = aggregate_tokens.saturating_add(
                        ev.get("total_tokens").and_then(Value::as_u64).unwrap_or(0),
                    );
                    any_live_provider |= ev
                        .get("live_provider_request")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    any_injected_provider |= ev
                        .get("provider_transport_provenance")
                        .and_then(Value::as_str)
                        == Some("injected");
                }
                cell_results.push(evidence);
                if stopped_by.is_none() {
                    if let Some(rule) = should_stop_after_cell(&stop_rules, class) {
                        stopped_by = Some(rule.into());
                    }
                }
                continue;
            }
            // Stale in-flight dispatch without terminalization is not auto-replayed as success.
            if class == "dispatched" {
                return Err(format!(
                    "cell {} has an open dispatched fence; refuse ambiguous restart (manual recovery required)",
                    ids.cell_id
                ));
            }
        }

        if stopped_by.is_some() {
            let outcome = CellOutcome::blocked(
                "skipped_by_stop_rule",
                stopped_by.as_deref().unwrap_or("stop"),
                &ids,
            );
            let evidence = build_cell_evidence(
                run_id,
                authorization_id,
                &frozen,
                cell,
                task,
                &ids,
                &outcome,
            );
            // Skipped cells do not reserve budget; terminal accounting only.
            store.persist_rwe_task_attempt(
                run_id,
                &lease,
                &ids.rwe_task_attempt_id,
                &ids.task_id,
                &ids.definition_sha256,
                &outcome.classification,
                &evidence,
            )?;
            cell_results.push(evidence);
            continue;
        }

        let envelope = match cell_reservation_limits(cell) {
            Ok(v) => v,
            Err(budget_err) => {
                let outcome = CellOutcome::blocked(
                    "blocked_budget",
                    &format!("pre-effect budget refusal: {budget_err}"),
                    &ids,
                );
                let evidence = build_cell_evidence(
                    run_id,
                    authorization_id,
                    &frozen,
                    cell,
                    task,
                    &ids,
                    &outcome,
                );
                store.persist_rwe_task_attempt(
                    run_id,
                    &lease,
                    &ids.rwe_task_attempt_id,
                    &ids.task_id,
                    &ids.definition_sha256,
                    &outcome.classification,
                    &evidence,
                )?;
                cell_results.push(evidence);
                if stop_rules.iter().any(|r| r == "stop_on_budget_exhaustion") {
                    stopped_by = Some("stop_on_budget_exhaustion".into());
                }
                continue;
            }
        };
        let reserve_req = envelope.max_provider_requests;
        let reserve_tok = envelope.max_total_tokens;

        // Store-backed atomic pre-effect fence: run↔auth binding + full envelope.
        let reservation_evidence = sort_value(&json!({
            "schema_version": RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
            "run_id": run_id,
            "authorization_id": authorization_id,
            "cell_id": ids.cell_id,
            "task_id": ids.task_id,
            "definition_sha256": ids.definition_sha256,
            "classification": "dispatched",
            "provider_requests": reserve_req,
            "total_tokens": reserve_tok,
            "budget_envelope": envelope.to_json(),
            "note": "pre-effect full cell budget reservation",
        }));
        match store.claim_rwe_cell_dispatch(
            principal,
            run_id,
            authorization_id,
            &lease,
            &ids.rwe_task_attempt_id,
            &ids.task_id,
            &ids.definition_sha256,
            &envelope,
            &reservation_evidence,
        ) {
            Ok(claim) => {
                if claim.get("idempotent_replay").and_then(Value::as_bool) == Some(true) {
                    return Err(format!(
                        "cell {} dispatch claim is already held; refuse concurrent/double dispatch",
                        ids.cell_id
                    ));
                }
            }
            Err(e) if e.contains("budget reservation refused") || e.contains("budget") => {
                let outcome = CellOutcome::blocked(
                    "blocked_budget",
                    &format!("pre-effect budget refusal: {e}"),
                    &ids,
                );
                let evidence = build_cell_evidence(
                    run_id,
                    authorization_id,
                    &frozen,
                    cell,
                    task,
                    &ids,
                    &outcome,
                );
                store.persist_rwe_task_attempt(
                    run_id,
                    &lease,
                    &ids.rwe_task_attempt_id,
                    &ids.task_id,
                    &ids.definition_sha256,
                    &outcome.classification,
                    &evidence,
                )?;
                cell_results.push(evidence);
                if stop_rules.iter().any(|r| r == "stop_on_budget_exhaustion") {
                    stopped_by = Some("stop_on_budget_exhaustion".into());
                }
                continue;
            }
            Err(e) if e.contains("duplicate RWE cell dispatch refused") => {
                return Err(format!(
                    "duplicate cell dispatch refused for {}: {e}",
                    ids.cell_id
                ));
            }
            Err(e) => return Err(e),
        }

        // After fence: catch execution errors and terminalize correctly.
        // outcome_unknown must not auto-retry or consume another authorization.
        let mut outcome = match driver
            .execute_cell(store, principal, &frozen, run_id, &lease, cell, task, &ids)
        {
            Ok(o) => o,
            Err(e) => {
                let class = classify_execution_error(&e);
                let mut o = CellOutcome::blocked(class, &e, &ids);
                o.evidence_source = "product_golden_path_owner".into();
                // The schedule identity is the ProductTask idempotency key;
                // after admission, the store owns a distinct generated task
                // id. Resolve it before coupling failure usage to the durable
                // delegated journal.
                let task = match store.get_product_task_by_idempotency(
                    principal.tenant_id(),
                    &ids.worktree_id,
                    &ids.product_task_id,
                ) {
                    Ok(Some(task)) => task,
                    Ok(None) => {
                        mark_cleanup_failed(&mut o, "admitted ProductTask identity was not found");
                        Value::Null
                    }
                    Err(error) => {
                        mark_cleanup_failed(
                            &mut o,
                            format!("ProductTask identity lookup failed: {error}"),
                        );
                        Value::Null
                    }
                };
                if let Some(task_id) = task.get("task_id").and_then(Value::as_str) {
                    o.product_task_id = task_id.to_string();
                    let workspace_id = task
                        .get("workspace_record_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    match store.finalize_product_task_after_execution(task_id, "rwe-live-baseline")
                    {
                        Ok(finalized) => {
                            let phase = finalized.get("phase").and_then(Value::as_str);
                            let terminal_phase = matches!(
                                phase,
                                Some(
                                    "terminal_failure"
                                        | "execution_failed"
                                        | "verification_authority_lost"
                                        | "verification_failed"
                                        | "verification_outcome_unknown"
                                )
                            );
                            let workspace_cleaned = workspace_id.as_deref().is_some_and(|id| {
                                store
                                    .get_supervised_patch_workspace(id)
                                    .ok()
                                    .flatten()
                                    .and_then(|workspace| {
                                        workspace
                                            .get("status")
                                            .and_then(Value::as_str)
                                            .map(|status| status == "cleaned")
                                    })
                                    .unwrap_or(false)
                            });
                            if terminal_phase && workspace_cleaned {
                                o.cleanup_status = "cleaned".into();
                            } else {
                                mark_cleanup_failed(
                                    &mut o,
                                    format!(
                                        "cleanup owner returned phase {} with workspace cleanup unproven",
                                        phase.unwrap_or("missing")
                                    ),
                                );
                            }
                        }
                        Err(cleanup_error) => {
                            mark_cleanup_failed(&mut o, cleanup_error);
                        }
                    }
                } else if !task.is_null() {
                    mark_cleanup_failed(&mut o, "ProductTask identity has no task_id");
                }
                if class == "outcome_unknown" {
                    o.cost_unknown = true;
                    o.monetary_cost = None;
                }
                o
            }
        };

        // Couple usage to store journal; do not trust driver-supplied usage for seal.
        couple_usage_to_store_journal(store, &mut outcome);

        // Post-effect honesty against reserved full cell envelope.
        if outcome.provider_requests > reserve_req || outcome.total_tokens > reserve_tok {
            // Preserve failed-attempt cost (do not zero reserved consumption).
            outcome.classification = "blocked_budget".into();
            outcome.note = format!(
                "{}; cell provider/token budget exceeded by journal/outcome",
                outcome.note
            );
            if stop_rules.iter().any(|r| r == "stop_on_budget_exhaustion") {
                stopped_by = Some("stop_on_budget_exhaustion".into());
            }
        }
        if outcome.input_tokens > envelope.max_input_tokens
            || outcome.output_tokens > envelope.max_output_tokens
        {
            outcome.classification = "blocked_budget".into();
            outcome.note = format!(
                "{}; cell input/output token dimension exceeded",
                outcome.note
            );
        }
        if let Some(max_cost) = envelope.max_cost {
            if let Some(cost) = outcome.monetary_cost {
                if cost > max_cost {
                    outcome.classification = "blocked_budget".into();
                    outcome.note = format!("{}; cell monetary ceiling exceeded", outcome.note);
                }
            }
            // When cost unavailable on a live provider run, keep cost_unknown; do not invent.
        } else if outcome.live_provider_request && outcome.monetary_cost.is_none() {
            outcome.cost_unknown = true;
        }

        let evidence = build_cell_evidence(
            run_id,
            authorization_id,
            &frozen,
            cell,
            task,
            &ids,
            &outcome,
        );
        store.finalize_rwe_cell_dispatch(
            run_id,
            &lease,
            &ids.rwe_task_attempt_id,
            &outcome.classification,
            &evidence,
        )?;
        // Preserve failed-attempt cost in aggregates (actuals after journal couple).
        aggregate_requests = aggregate_requests.saturating_add(outcome.provider_requests);
        aggregate_tokens = aggregate_tokens.saturating_add(outcome.total_tokens);
        any_live_provider |= outcome.live_provider_request;
        any_injected_provider |= outcome.provider_transport_provenance == "injected";
        if stopped_by.is_none() {
            if let Some(rule) = should_stop_after_cell(&stop_rules, &outcome.classification) {
                stopped_by = Some(rule.into());
            }
        }
        // outcome_unknown: terminal no-retry — stop schedule advancement for remaining cells.
        if outcome.classification == "outcome_unknown" && stopped_by.is_none() {
            stopped_by = Some("outcome_unknown_no_retry".into());
        }
        cell_results.push(evidence);
    }

    let final_attempts = store.list_rwe_task_attempts_for_run(run_id)?;
    if final_attempts.len() < cells.len() {
        return Err(format!(
            "run cannot terminalize: {} task attempts for {} cells",
            final_attempts.len(),
            cells.len()
        ));
    }
    if final_attempts
        .iter()
        .any(|a| a.get("classification").and_then(Value::as_str) == Some("dispatched"))
    {
        return Err("run cannot terminalize while a cell dispatch fence is open".into());
    }

    let live_baseline_sealed = evaluate_store_owned_live_baseline_seal(
        store,
        principal,
        &frozen,
        run_id,
        &stopped_by,
        &cell_results,
    );

    // Store-owned transport provenance classification for the whole run.
    // `external` requires every journaled provider request to have been served
    // by the production transport; `injected` marks the integration-fixture
    // path; `none` means provider-free execution.
    let provider_transport_provenance = if any_live_provider && live_baseline_sealed {
        "external"
    } else if any_injected_provider {
        "injected"
    } else if any_live_provider {
        "external"
    } else {
        "none"
    };
    let integration_fixture_completed = any_injected_provider
        && !any_live_provider
        && !live_baseline_sealed
        && cell_results.len() == cells.len()
        && cell_results.iter().all(|e| {
            let class = e
                .get("classification")
                .and_then(Value::as_str)
                .unwrap_or("");
            is_terminal_classification(class) && class != "outcome_unknown"
        });
    // Fixture SUCCESS is a strictly stronger claim than fixture completion:
    // every required fixture cell must classify as fixture_success. Merely
    // terminating with controlled_failure / verifier_failed / timeout /
    // cancelled / blocked_* / cleanup_failed is completion, never success.
    let integration_fixture_succeeded = integration_fixture_completed
        && cell_results
            .iter()
            .all(|e| e.get("classification").and_then(Value::as_str) == Some("fixture_success"));

    let aggregate = sort_value(&json!({
        "schema_version": RWE_RUN_EVIDENCE_SCHEMA,
        "coordinator_schema": RWE_LIVE_BASELINE_COORDINATOR_SCHEMA,
        "run_id": run_id,
        "authorization_id": authorization_id,
        "corpus_sha256": frozen.corpus.corpus_sha256,
        "protocol_sha256": frozen.protocol.body_sha256,
        "schedule_sha256": frozen.schedule.schedule_sha256,
        "accepted_main_sha": frozen.accepted_main_sha,
        "cell_results": cell_results,
        "aggregate_provider_requests": aggregate_requests,
        "aggregate_total_tokens": aggregate_tokens,
        "live_provider_request": any_live_provider,
        "provider_transport_provenance": provider_transport_provenance,
        "injected_provider_call_performed": any_injected_provider,
        "integration_fixture_completed": integration_fixture_completed,
        "integration_fixture_succeeded": integration_fixture_succeeded,
        "live_baseline_sealed": live_baseline_sealed,
        "provider_free_fixture_completion": false,
        "stopped_by": stopped_by,
        "comparison_eligible": false,
        "note": if live_baseline_sealed {
            "live baseline sealed from store-owned ProductTask/terminal receipts with external provider transport provenance"
        } else if any_injected_provider {
            if integration_fixture_succeeded {
                "integration fixture completed and succeeded through the injected transport; not a sealed live baseline"
            } else {
                "integration fixture terminated without success through the injected transport; not a sealed live baseline"
            }
        } else {
            "provider-free, injected, or incomplete store receipts; not a sealed live baseline"
        },
    }));
    let evidence_sha = sha256_hex(aggregate.to_string().as_bytes());
    let status = run_terminal_status(
        live_baseline_sealed,
        integration_fixture_succeeded,
        &cell_results,
    );
    let completed = store.complete_rwe_run(run_id, &lease, status, &aggregate, &evidence_sha)?;
    Ok(sort_value(&json!({
        "schema_version": RWE_LIVE_BASELINE_COORDINATOR_SCHEMA,
        "run": completed,
        "aggregate": aggregate,
        "cell_count": cells.len(),
        "attempts_recorded": final_attempts.len(),
        "live_baseline_sealed": live_baseline_sealed,
        "provider_call_performed": any_live_provider && provider_transport_provenance == "external",
        "provider_transport_provenance": provider_transport_provenance,
        "injected_provider_call_performed": any_injected_provider,
        "integration_fixture_completed": integration_fixture_completed,
        "integration_fixture_succeeded": integration_fixture_succeeded,
        "provider_calls": if any_live_provider && provider_transport_provenance == "external" {
            "observed_in_cell_evidence"
        } else {
            "0"
        },
    })))
}

/// Genuine delegated lifecycle for one armed RWE cell through existing owners.
///
/// Mirrors the accepted Golden Path delegated route: contract → approved
/// proposal → prepare → manifest approval → one-use spend → attempt lease →
/// activation → managed executor ticks → finalize → artifact confirmation →
/// genuine Draft PR output (branch push + GitHub Draft PR under the
/// operator-authorized live-run environment) → terminal receipt + cleanup.
/// The frozen pytest verifier runs against the
/// application-owned worktree; the store persists every provider request in the
/// delegation journal, so sealing never trusts driver-supplied usage.
fn execute_armed_delegated_rwe_cell(
    store: &std::sync::Arc<LocalProductStore>,
    principal: &AuthenticatedPrincipal,
    executor_principal: &AuthenticatedPrincipal,
    confirmer_principal: &AuthenticatedPrincipal,
    _frozen: &OperatorFrozenContractSet,
    run_id: &str,
    _lease_token: &str,
    cell: &Value,
    task: &RweTaskDefinition,
    ids: &CellIdentities,
    product_task_id: &str,
    transport: &Option<std::sync::Arc<dyn crate::provider::transport::HttpTransport>>,
) -> Result<CellOutcome, String> {
    let product_task_id = product_task_id.to_string();
    let delegation_id = format!("rwe-del:{run_id}:{}", ids.cell_id);
    let attempt_id = ids.delegated_attempt_id.clone();
    let now = store.require_now()?;
    let created = chrono::DateTime::parse_from_rfc3339(&now)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|_| "store clock must be canonical RFC3339/UTC".to_string())?;
    let expires_at = (created + chrono::Duration::hours(24))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let (max_files, max_lines) = crate::rwe::frozen_rwe_bindings::frozen_rwe_max_patch_limits()?;
    let cell_cost = crate::rwe::frozen_rwe_bindings::frozen_schedule_cell_max_cost(cell)?
        .ok_or("frozen RWE cell monetary ceiling required for delegated spend")?;
    let union_paths = crate::rwe::frozen_rwe_bindings::frozen_rwe_union_allowed_paths()?;
    let role_models = json!({
        "planner": OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL,
        "implementer": OPERATOR_ADMITTED_MODEL,
        "reviewer": OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL,
    });

    // 1. Delegation contract under the existing delegated authority owner.
    let contract = DelegationContract {
        schema_version: DELEGATION_SCHEMA_VERSION.into(),
        delegation_id: delegation_id.clone(),
        created_at: now.clone(),
        expires_at: expires_at.clone(),
        executions: 1,
        repositories: vec![OPERATOR_TARGET_REPO.into()],
        task_classes: vec!["rwe".into()],
        allowed_paths: union_paths,
        max_changed_files: max_files,
        max_changed_lines: max_lines,
        max_cost_usd_per_run: cell_cost,
        max_total_cost_usd: cell_cost,
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
    store.persist_delegation_for_product_task(principal, &product_task_id, &contract)?;

    // 2. Operator-approved proposal pinned to the frozen target identity.
    let mut proposal = json!({
        "schema_version": "managed_proposal_manifest.v1",
        "target_repository": OPERATOR_TARGET_REPO,
        "target_main_sha": FROZEN_RWE_TARGET_MAIN_SHA,
        "mutable_paths": task.allowed_mutable_paths,
        "max_cost_usd": Value::Null,
        "verifier": crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY,
    });
    let proposal_sha = compute_attempt_manifest_sha256(&proposal)?;
    proposal["manifest_sha256"] = json!(proposal_sha);
    store.persist_approved_delegated_proposal(
        &delegation_id,
        &proposal,
        proposal["manifest_sha256"].as_str().unwrap(),
    )?;

    // 3. Prepare the delegated route (plan + final manifest) under the store owner.
    let prepared = store
        .prepare_delegated_managed_product_task(
            &product_task_id,
            "executor",
            &["managed_deepseek".into()],
            &proposal,
            &contract,
            &attempt_id,
        )
        .map_err(|e| {
            format!("delegated prepare failed for admitted task {product_task_id}: {e}")
        })?;
    let manifest = prepared
        .get("final_manifest")
        .cloned()
        .ok_or("delegated prepare final_manifest missing")?;

    // 4. Manifest approval + one-use delegated spend by the operator principal.
    let approval = store.approve_delegated_manifest(principal, &delegation_id, &manifest)?;
    let approval_receipt_sha256 = approval
        .get("approval_receipt_sha256")
        .and_then(Value::as_str)
        .ok_or("delegated manifest approval receipt missing")?;
    let spend = store.issue_delegated_spend(
        principal,
        &delegation_id,
        approval_receipt_sha256,
        &manifest,
    )?;
    let spend_authorization_id = spend
        .get("spend_authorization_id")
        .and_then(Value::as_str)
        .ok_or("delegated spend authorization id missing")?;

    // 5. Role-separated attempt lease + activation.
    let lease = store.admit_delegated_attempt(
        executor_principal,
        &delegation_id,
        &attempt_id,
        &manifest,
    )?;
    let attempt_lease_id = lease
        .get("attempt_lease_id")
        .and_then(Value::as_str)
        .ok_or("delegated attempt lease id missing")?;
    let activated = store.activate_delegated_managed_product_task(
        &product_task_id,
        "executor",
        &manifest,
        spend_authorization_id,
        attempt_lease_id,
    )?;
    let run_id_cell = activated
        .get("run")
        .and_then(|r| r.get("run_id"))
        .and_then(Value::as_str)
        .ok_or("delegated activation run missing")?;

    // 6. Managed executor through the injected fake transport (provider-free) or
    // the production credential boundary when no fake transport is supplied.
    // Executor envelope must equal the persisted manifest envelope so the
    // store-owned authority contract matches the request exactly on BOTH
    // branches; the live path must never fall back to the from_env default
    // envelope (docs limits), which would break the frozen cell budget.
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
            .ok_or("delegated manifest price profile missing")?,
    )
    .map_err(|_| "delegated manifest price profile malformed")?;
    let source: std::sync::Arc<dyn crate::provider::managed_deepseek::ManagedAuthoritySource> =
        store.clone();
    // The injectable seam can never be the canonical production boundary: any
    // transport passed through the fake slot is wrapped in
    // InjectedTransportBoundary, so even a real ReqwestTransport placed in the
    // fake slot is Injected by construction. Provenance is minted by concrete
    // type (production_transport_provenance), never self-declared by the
    // transport object; the None branch constructs the canonical
    // ReqwestTransport and is the only External path.
    let serving_transport = transport.as_ref().map(|tx| {
        std::sync::Arc::new(crate::provider::transport::InjectedTransportBoundary(
            std::sync::Arc::clone(tx),
        )) as std::sync::Arc<dyn crate::provider::transport::HttpTransport>
    });
    let executor = match serving_transport {
        Some(tx) => {
            let mk = |model: &str| -> Result<std::sync::Arc<ManagedDeepSeekProvider>, String> {
                let config = ProviderConfig::new(
                    "deepseek-managed-rwe",
                    "openai_compatible",
                    DEEPSEEK_OPENAI_BASE_URL,
                    model,
                    DEEPSEEK_CREDENTIAL_REFERENCE,
                    "2026-07-30T00:00:00Z",
                );
                let credential = CredentialRef::new(
                    DEEPSEEK_CREDENTIAL_REFERENCE,
                    "env",
                    "***",
                    "provider:deepseek",
                    "2026-07-30T00:00:00Z",
                );
                Ok(std::sync::Arc::new(ManagedDeepSeekProvider::new_openai(
                    config,
                    CredentialBoundary::new("env")
                        .map_err(|e| format!("managed credential boundary failed: {e}"))?,
                    credential,
                    std::sync::Arc::clone(&tx),
                )))
            };
            ManagedDeepSeekNodeExecutor::new(
                mk(OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL)?,
                mk(OPERATOR_ADMITTED_MODEL)?,
                mk(OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL)?,
                source,
                ManagedDeepSeekExecutorConfig {
                    protocol: DeepSeekProtocol::OpenAiCompatible,
                    limits: manifest_limits,
                    price_profile: manifest_price_profile,
                },
            )?
        }
        None => {
            let mk = |model: &str| -> Result<std::sync::Arc<ManagedDeepSeekProvider>, String> {
                let config = ProviderConfig::new(
                    "deepseek-managed-rwe",
                    "openai_compatible",
                    DEEPSEEK_OPENAI_BASE_URL,
                    model,
                    DEEPSEEK_CREDENTIAL_REFERENCE,
                    "2026-07-30T00:00:00Z",
                );
                let credential = CredentialRef::new(
                    DEEPSEEK_CREDENTIAL_REFERENCE,
                    "env",
                    "***",
                    "provider:deepseek",
                    "2026-07-30T00:00:00Z",
                );
                Ok(std::sync::Arc::new(ManagedDeepSeekProvider::new_openai(
                    config,
                    CredentialBoundary::new("env")
                        .map_err(|e| format!("managed credential boundary failed: {e}"))?,
                    credential,
                    std::sync::Arc::new(crate::provider::transport::ReqwestTransport::new()),
                )))
            };
            ManagedDeepSeekNodeExecutor::new(
                mk(OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL)?,
                mk(OPERATOR_ADMITTED_MODEL)?,
                mk(OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL)?,
                source,
                ManagedDeepSeekExecutorConfig {
                    protocol: DeepSeekProtocol::OpenAiCompatible,
                    limits: manifest_limits,
                    price_profile: manifest_price_profile,
                },
            )?
        }
    };
    let mut terminal_reached = false;
    let mut last_tick = Value::Null;
    for _ in 0..16 {
        let tick = store.tick_with_executor(run_id_cell, "executor", 0, &executor)?;
        last_tick = tick.clone();
        if tick
            .pointer("/run/status")
            .and_then(Value::as_str)
            .is_some_and(|status| matches!(status, "completed" | "failed" | "cancelled" | "killed"))
        {
            terminal_reached = true;
            break;
        }
    }
    if !terminal_reached {
        return Err("delegated cell run did not reach a terminal status".into());
    }
    let run_status = last_tick
        .pointer("/run/status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    if run_status != "completed" {
        return Err(format!(
            "delegated cell run failed: {run_status} (run {run_id_cell}, task {product_task_id})"
        ));
    }

    // 7. Finalize under the same owner, then artifact confirmation.
    let finalized = store.finalize_product_task_after_execution(&product_task_id, "executor")?;
    let task_version = finalized
        .pointer("/task/version")
        .and_then(Value::as_u64)
        .ok_or("delegated finalize task version missing")?;
    let approval_evidence = store
        .approve_delegated_product_task(
            confirmer_principal,
            &product_task_id,
            "artifact-confirmer",
            task_version,
            &delegation_id,
            &manifest,
            FROZEN_RWE_TARGET_MAIN_SHA,
        )
        .map_err(|e| {
            format!(
                "delegated product-task approval failed (task {product_task_id}, version {task_version}): {e}"
            )
        })?;
    let approval_id = approval_evidence
        .pointer("/approval/approval_id")
        .and_then(Value::as_str)
        .ok_or("delegated product-task approval identity missing")?;

    // 8. Genuine output under the operator-authorized live-run environment: the
    // store plans and claims the draft_pr operation, pushes the approved branch
    // to the credential-free https origin, and records the pushed commit; the
    // coordinator then creates the real GitHub Draft PR through the existing
    // GitHub owner and completes the store-owned operation, which transitions
    // the ProductTask to completed with a Draft PR terminal record. Network
    // effects here require ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT=1,
    // ACP_ENABLE_GITHUB_PR_OUTPUT=1, and populated token references; without
    // them the store returns a planned/blocked output and this fails closed.
    let output = store
        .output_product_task(
            &product_task_id,
            "executor",
            task_version,
            Some(approval_id),
            true,
        )
        .map_err(|e| format!("delegated output failed (task {product_task_id}): {e}"))?;
    let output_status = output.pointer("/output/status").and_then(Value::as_str);
    if output_status != Some("pr_create_pending") {
        let reason = output
            .pointer("/output/reason")
            .and_then(Value::as_str)
            .unwrap_or("output operation did not claim Draft PR creation");
        return Err(format!(
            "delegated output did not reach Draft PR creation (status {output_status:?}): {reason}"
        ));
    }
    let operation = output
        .pointer("/output/operation")
        .cloned()
        .ok_or("delegated output operation missing")?;
    let request = operation
        .get("request")
        .cloned()
        .ok_or("delegated output request missing")?;
    let target_repository = request
        .get("target_repository")
        .and_then(Value::as_str)
        .ok_or("delegated output target repository missing")?;
    let (owner, repository) = target_repository
        .split_once('/')
        .ok_or("delegated output target repository identity invalid")?;
    let artifact_id = operation
        .get("artifact_id")
        .and_then(Value::as_str)
        .ok_or("delegated output artifact identity missing")?;
    let operation_id = operation
        .get("operation_id")
        .and_then(Value::as_str)
        .ok_or("delegated output operation identity missing")?;
    let operation_version = operation
        .get("current_version")
        .and_then(Value::as_u64)
        .ok_or("delegated output operation version missing")?;
    let completion_task_version = output
        .pointer("/task/version")
        .and_then(Value::as_u64)
        .ok_or("delegated output task version missing")?;
    let pull_request_request = crate::target_repo_output::GitHubPullRequestRequest {
        repository: crate::target_repo_output::GitHubRepository {
            host: request
                .get("repository_host")
                .and_then(Value::as_str)
                .unwrap_or("github.com")
                .to_string(),
            owner: owner.to_string(),
            repository: repository.to_string(),
        },
        head_branch: request
            .get("head_branch")
            .and_then(Value::as_str)
            .ok_or("delegated output head branch missing")?
            .to_string(),
        base_branch: request
            .get("base_branch")
            .and_then(Value::as_str)
            .ok_or("delegated output base branch missing")?
            .to_string(),
        title: request
            .get("pr_title")
            .and_then(Value::as_str)
            .ok_or("delegated output PR title missing")?
            .to_string(),
        body: request
            .get("pr_body")
            .and_then(Value::as_str)
            .ok_or("delegated output PR body missing")?
            .to_string(),
        expected_base_sha: operation
            .get("source_revision")
            .and_then(Value::as_str)
            .map(str::to_string),
        expected_head_sha: operation
            .pointer("/branch_push/commit_sha")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("delegated Draft PR runtime failed: {e}"))?;
    let pull_request = runtime
        .block_on(
            crate::target_repo_output::create_or_reuse_github_pull_request(
                &crate::target_repo_output::GitHubPullRequestConfig::from_env(),
                &pull_request_request,
            ),
        )
        .map_err(|e| format!("delegated Draft PR creation failed: {e}"))?;
    let pull_request = serde_json::to_value(pull_request).map_err(|e| e.to_string())?;
    let completed_output = store.complete_product_task_draft_pr_output(
        &product_task_id,
        artifact_id,
        operation_id,
        operation_version,
        completion_task_version,
        &pull_request,
        "executor",
    )?;
    if completed_output
        .pointer("/task/status")
        .and_then(Value::as_str)
        != Some("completed")
    {
        return Err("delegated output completion did not complete the ProductTask".into());
    }

    // 9. Terminal closeout: store-owned receipt + cleanup + attempt terminal.
    let terminal = store.complete_delegated_product_task_terminal(
        &delegation_id,
        &attempt_id,
        &product_task_id,
        "executor",
    )?;
    let terminal_evidence = terminal
        .get("product_terminal_evidence")
        .cloned()
        .unwrap_or(Value::Null);
    let confirmation = terminal
        .get("artifact_confirmation")
        .cloned()
        .unwrap_or(Value::Null);
    let draft_pr = terminal_evidence
        .pointer("/output/draft_pr")
        .cloned()
        .unwrap_or(Value::Null);
    let approval_id = terminal_evidence
        .pointer("/approval/approval_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let terminal_evidence_id = terminal_evidence
        .get("evidence_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let terminal_content_sha256 = terminal_evidence
        .get("content_sha256")
        .and_then(Value::as_str)
        .map(str::to_string);
    let realized_cost = confirmation
        .get("realized_cost_usd")
        .and_then(Value::as_f64);
    // The delegated attempt terminal receipt reports `status: closed` with the
    // exact `terminal_class` ("succeeded" | "controlled_failure" | ...); only
    // the store-owned class authorizes a success classification.
    let terminal_class = terminal
        .pointer("/terminal/terminal_class")
        .and_then(Value::as_str)
        .unwrap_or("");
    // Evaluator-surface sentinel: modifying test collection, verifier config,
    // ignore/baseline, or the frozen suite fixture is never real task success.
    // The injected fixture's own conftest skip patch is recorded here as the
    // fixture technique; on the external transport it is verifier tampering.
    // The changed files are read from the store-owned artifact record
    // (canonical change prefixes stripped); the terminal evidence approval
    // binding only carries the approval identity, not the file list.
    let artifact_changed_files = match store.get_product_task(&product_task_id) {
        Ok(Some(product_task)) => {
            let run_id = product_task
                .get("run_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let workspace_record_id = product_task
                .get("workspace_record_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let source_revision = product_task
                .get("source_revision")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            store
                .current_product_task_artifact(
                    &product_task_id,
                    &run_id,
                    &workspace_record_id,
                    &source_revision,
                )
                .map(|artifact| {
                    artifact
                        .get("changed_files")
                        .and_then(Value::as_array)
                        .map(|files| {
                            files
                                .iter()
                                .filter_map(Value::as_str)
                                .map(|path| {
                                    path.strip_prefix(['+', '~', '-'])
                                        .unwrap_or(path)
                                        .to_string()
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };
    let evaluator_surface_tampered = artifact_changed_files
        .iter()
        .any(|changed| changed_file_is_evaluator_surface(changed));
    // Integration fixtures never record `success`: the injected-transport path
    // is a lifecycle fixture proof, not accepted task delivery.
    let classification = if terminal_class == "succeeded" && !evaluator_surface_tampered {
        if transport.is_some() {
            "fixture_success"
        } else {
            "success"
        }
    } else if terminal_class == "succeeded" {
        // Succeeded store terminal but the artifact modified the evaluator
        // surface: never `success`. Injected fixtures keep their fixture
        // classification; external runs are verifier tampering.
        if transport.is_some() {
            "fixture_success"
        } else {
            "verifier_failed"
        }
    } else {
        "controlled_failure"
    };
    let verifier_receipt = terminal_evidence
        .pointer("/verification/receipts")
        .and_then(Value::as_array)
        .and_then(|attempts| attempts.last().cloned())
        .unwrap_or(Value::Null);
    let verification_outcome = json!({
        "status": terminal_evidence.pointer("/verification/status").and_then(Value::as_str).unwrap_or(""),
        "trustworthy": terminal_evidence.pointer("/verification/trustworthy").and_then(Value::as_bool).unwrap_or(false),
        "output_sha256": verifier_receipt.get("output_sha256").and_then(Value::as_str).unwrap_or(""),
        "exit_code": verifier_receipt.pointer("/process_outcome/exit_code").and_then(Value::as_i64).unwrap_or(-1),
    });
    let fixture_note = if transport.is_some() {
        if evaluator_surface_tampered {
            format!(
                "; integration fixture applied an evaluator-surface skip patch (changed files include {}); fixture evidence only, never accepted delivery",
                artifact_changed_files.join(", ")
            )
        } else {
            "; integration fixture executed through the injected transport; fixture evidence only, never accepted delivery"
                .to_string()
        }
    } else {
        String::new()
    };

    Ok(CellOutcome {
        classification: classification.into(),
        provider_requests: 0,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        latency_ms: 0,
        monetary_cost: realized_cost,
        cost_unknown: realized_cost.is_none(),
        live_provider_request: false,
        provider_transport_provenance: "none".into(),
        evidence_source: "product_golden_path_owner".into(),
        verification_status: "evidence_recorded".into(),
        verification_trustworthy: true,
        approval_id,
        output_draft_pr: if draft_pr.is_null() { None } else { Some(draft_pr) },
        terminal_evidence_id,
        terminal_content_sha256,
        cleanup_status: "completed".into(),
        product_task_id,
        workflow_id: ids.workflow_id.clone(),
        node_id: ids.node_id.clone(),
        delegated_attempt_id: attempt_id,
        workspace_id: ids.worktree_id.clone(),
        note: format!(
            "delegated cell lifecycle executed through store owners; seam={RWE_LIVE_CELL_COMPOSITION_SEAM}; cell={}; verifier_result={}; verifier_output_sha256={}; verifier_exit_code={}{}",
            cell.get("cell_id").and_then(Value::as_str).unwrap_or(""),
            verification_outcome["status"].as_str().unwrap_or(""),
            verification_outcome["output_sha256"].as_str().unwrap_or(""),
            verification_outcome["exit_code"],
            fixture_note,
        ),
    })
}

/// Build first-baseline evidence projection without claiming COMPARISON_ELIGIBLE.
pub fn project_first_baseline_evidence(run_aggregate: &Value) -> Value {
    sort_value(&json!({
        "schema_version": "rwe_first_baseline_evidence_projection.v1",
        "live_baseline_sealed": run_aggregate.get("live_baseline_sealed"),
        "provider_transport_provenance": run_aggregate.get("provider_transport_provenance"),
        "injected_provider_call_performed": run_aggregate.get("injected_provider_call_performed"),
        "integration_fixture_completed": run_aggregate.get("integration_fixture_completed"),
        "integration_fixture_succeeded": run_aggregate.get("integration_fixture_succeeded"),
        "comparison_eligible": false,
        "aggregate_provider_requests": run_aggregate.get("aggregate_provider_requests"),
        "aggregate_total_tokens": run_aggregate.get("aggregate_total_tokens"),
        "cell_results": run_aggregate.get("cell_results"),
        "corpus_sha256": run_aggregate.get("corpus_sha256"),
        "protocol_sha256": run_aggregate.get("protocol_sha256"),
        "schedule_sha256": run_aggregate.get("schedule_sha256"),
        "note": "projection only; not a VDE store and not comparison-eligible",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::transport::HttpResponse;
    use crate::storage::local_product_store::{
        SCOPE_ATTEMPT_ADMIT, SCOPE_DELEGATED_ARTIFACT_CONFIRM, SCOPE_DELEGATED_AUTONOMY,
        SCOPE_DELEGATED_EXECUTE, SCOPE_DELEGATED_MANIFEST_APPROVE, SCOPE_REVOKE,
        SCOPE_RISK_ACKNOWLEDGE, SCOPE_SPEND_AUTHORIZE,
    };
    use sha2::Digest;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{channel, sync_channel};
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn failed_cell_journal_usage_is_projected_without_false_zeroes() {
        let projection = json!({
            "provider_execution": null,
            "provider_request_journal": [
                {
                    "status": "succeeded",
                    "effective_tokens": 422,
                    "effective_cost_usd": 0.000120118,
                    "transport_provenance": "external",
                    "usage": {"input_tokens": 314, "output_tokens": 108}
                },
                {
                    "status": "succeeded",
                    "effective_tokens": 4349,
                    "effective_cost_usd": 0.0011337368,
                    "transport_provenance": "external",
                    "usage": {"input_tokens": 349, "output_tokens": 4000}
                }
            ]
        });

        let execution = provider_execution_from_journal(&projection).unwrap();
        assert_eq!(execution["provider_request_count"], 2);
        assert_eq!(execution["cumulative_tokens"], 4771);
        assert_eq!(execution["transport_provenance"], "external");
        assert_eq!(
            store_evidence_transport_provenance(&projection),
            Ok("external".to_string())
        );
    }

    #[test]
    fn failed_provider_journal_entries_do_not_become_live_requests() {
        let projection = json!({
            "provider_execution": null,
            "provider_request_journal": [{
                "status": "failed_before_send",
                "effective_tokens": 0,
                "effective_cost_usd": 0.0,
                "transport_provenance": "external"
            }]
        });
        let execution = provider_execution_from_journal(&projection).unwrap();
        assert_eq!(execution["provider_request_count"], 0);
        assert_eq!(execution["realized_cost_usd"], 0.0);
        assert_eq!(
            store_evidence_transport_provenance(&projection),
            Ok("external".to_string())
        );
    }

    #[test]
    fn outcome_unknown_journal_entries_preserve_unknown_effect_evidence() {
        let projection = json!({
            "provider_execution": null,
            "provider_request_journal": [{
                "status": "outcome_unknown",
                "effective_tokens": 512,
                "effective_cost_usd": 0.0002,
                "transport_provenance": "external"
            }]
        });
        let execution = provider_execution_from_journal(&projection).unwrap();
        assert_eq!(execution["provider_request_count"], 1);
        assert_eq!(execution["cumulative_tokens"], 512);
        assert_eq!(execution["cost_unknown"], true);
    }

    #[test]
    fn journal_cost_overflow_stays_unavailable() {
        let projection = json!({
            "provider_execution": null,
            "provider_request_journal": [
                {
                    "status": "succeeded",
                    "effective_tokens": 1,
                    "effective_cost_usd": 1.0e308,
                    "transport_provenance": "external",
                    "usage": {"input_tokens": 1, "output_tokens": 0}
                },
                {
                    "status": "succeeded",
                    "effective_tokens": 1,
                    "effective_cost_usd": 1.0e308,
                    "transport_provenance": "external",
                    "usage": {"input_tokens": 1, "output_tokens": 0}
                }
            ]
        });
        assert!(provider_execution_from_journal(&projection).is_none());
    }

    #[test]
    fn incomplete_cell_journal_stays_unavailable() {
        let projection = json!({
            "provider_execution": null,
            "provider_request_journal": [{
                "transport_provenance": "external",
                "usage": {"input_tokens": 10, "output_tokens": 5}
            }]
        });
        assert!(provider_execution_from_journal(&projection).is_none());
        assert!(store_evidence_transport_provenance(&projection).is_err());
    }

    fn operator(store: &LocalProductStore, tenant: &str, key: &str) -> AuthenticatedPrincipal {
        store
            .record_api_key_metadata_for_tenant(
                tenant,
                key,
                "operator-user",
                "operator",
                &[
                    SCOPE_RISK_ACKNOWLEDGE.to_string(),
                    SCOPE_SPEND_AUTHORIZE.to_string(),
                    SCOPE_ATTEMPT_ADMIT.to_string(),
                    SCOPE_REVOKE.to_string(),
                    SCOPE_DELEGATED_AUTONOMY.to_string(),
                    SCOPE_DELEGATED_MANIFEST_APPROVE.to_string(),
                    SCOPE_DELEGATED_ARTIFACT_CONFIRM.to_string(),
                ],
                "test",
            )
            .unwrap();
        store
            .authenticate_managed_acceptance_principal(tenant, key, None)
            .unwrap()
    }

    /// Role-separated delegated attempt activator key (never the manifest approver).
    fn cell_executor(store: &LocalProductStore, tenant: &str, key: &str) -> AuthenticatedPrincipal {
        store
            .record_api_key_metadata_for_tenant(
                tenant,
                key,
                "executor-user",
                "executor",
                &[
                    SCOPE_DELEGATED_EXECUTE.to_string(),
                    SCOPE_ATTEMPT_ADMIT.to_string(),
                    SCOPE_RISK_ACKNOWLEDGE.to_string(),
                ],
                "test",
            )
            .unwrap();
        store
            .authenticate_managed_acceptance_principal(tenant, key, None)
            .unwrap()
    }

    /// Role-separated delegated artifact confirmer key, distinct from both the
    /// manifest approver and the attempt activator.
    fn cell_confirmer(
        store: &LocalProductStore,
        tenant: &str,
        key: &str,
    ) -> AuthenticatedPrincipal {
        store
            .record_api_key_metadata_for_tenant(
                tenant,
                key,
                "reviewer-user",
                "reviewer",
                &[
                    SCOPE_RISK_ACKNOWLEDGE.to_string(),
                    SCOPE_DELEGATED_ARTIFACT_CONFIRM.to_string(),
                ],
                "test",
            )
            .unwrap();
        store
            .authenticate_managed_acceptance_principal(tenant, key, None)
            .unwrap()
    }

    fn seed_gp(store: &LocalProductStore, product_task_id: &str, tenant_id: &str) {
        let mut evidence = json!({
            "schema_version": "product_task_terminal_evidence.v2",
            "evidence_id": format!("ev-{product_task_id}"),
            "product_task_id": product_task_id,
            "tenant_id": tenant_id,
            "workspace_scope_id": "ws-gp",
            "task_version": 1,
            "task_status": "completed",
            "node": {"executor_class": "managed_coding"},
            "source_revision": "c".repeat(40),
            "verification": {"trustworthy": true, "status": "passed"},
            "approval": {"approval_id": "ap-1"},
            "artifact": {"artifact_id": "art-1"},
            "output": {
                "intent": "draft_pr",
                "result_sha256": "d".repeat(64),
                "operation_id": "op-1",
                "receipt_id": "rcpt-1",
                "draft_pr": {
                    "number": 1,
                    "repository": "Igzela/alters-lab",
                    "base_branch": "main",
                    "head_branch": "acp/gp",
                    "head_sha": "e".repeat(40),
                    "draft": true
                }
            },
            "audit_reference": {"audit_id": 1, "action": "product_task.terminal_evidence_committed"},
            "created_at": "2026-07-25T12:00:00Z",
            "created_by": "test",
            "content_sha256": Value::Null,
        });
        let hash = hex::encode(Sha256::digest(serde_json::to_vec(&evidence).unwrap()));
        evidence["content_sha256"] = json!(hash);
        store
            .insert_product_task_terminal_evidence_for_tests(&evidence)
            .unwrap();
    }

    fn success_outcomes() -> Vec<CellOutcome> {
        outcomes_with_classification("injected_success")
    }

    fn outcomes_with_classification(class: &str) -> Vec<CellOutcome> {
        (0..4)
            .map(|i| CellOutcome {
                classification: class.into(),
                provider_requests: 3,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms: 1,
                monetary_cost: Some(0.0),
                cost_unknown: false,
                live_provider_request: false,
                provider_transport_provenance: "injected".into(),
                evidence_source: "injected".into(),
                verification_status: "passed".into(),
                verification_trustworthy: true,
                approval_id: Some(format!("ap-{i}")),
                output_draft_pr: None,
                terminal_evidence_id: Some(format!("tev-{i}")),
                terminal_content_sha256: Some("f".repeat(64)),
                cleanup_status: "completed".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "injected".into(),
            })
            .collect()
    }

    fn admit_ready(
        store: &LocalProductStore,
        principal: &AuthenticatedPrincipal,
        auth_id: &str,
        run_id: &str,
        gp: &str,
    ) -> String {
        seed_gp(store, gp, principal.tenant_id());
        // The one-use authority may only be consumed when the complete runnable
        // seam is ready (credential present, non-CI). Tests simulate the operator
        // environment around the admit call under the shared env lock.
        let _lock = crate::cli::config::cli_env_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let had_cred = std::env::var_os(DEEPSEEK_CREDENTIAL_REFERENCE);
        let had_ci = std::env::var_os("CI");
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, "test-operator-credential");
        std::env::remove_var("CI");
        let admitted = issue_and_admit_v2(
            store,
            principal,
            auth_id,
            run_id,
            gp,
            "2026-08-07T00:00:00Z",
        );
        match had_cred {
            Some(v) => std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, v),
            None => std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE),
        }
        match had_ci {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
        let admitted = admitted.unwrap();
        admitted["lease_token"].as_str().unwrap().to_string()
    }

    #[test]
    fn preflight_fails_closed_without_gp_and_without_consuming() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("pf.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-pf", "op-pf");
        let pre = operator_preflight(&store, &principal, None, None).unwrap();
        assert_eq!(pre["ready"], false);
        assert_eq!(pre["observed_at"], "2026-07-25T12:00:00Z");
        assert_eq!(pre["authority_consumed"], false);
        assert_eq!(pre["provider_call_performed"], false);
        let codes: Vec<_> = pre["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|b| b.get("code").and_then(Value::as_str))
            .collect();
        assert!(
            codes.contains(&"missing_golden_path_prerequisite_id")
                || codes
                    .iter()
                    .any(|c| c.contains("golden_path") || c.contains("composition"))
        );
    }

    #[test]
    fn viability_preflight_reports_redacted_credential_presence_without_value() {
        let dir = tempdir().unwrap();
        let store = Arc::new(LocalProductStore::new(dir.path().join("pf-ready.db")).unwrap());
        let principal = operator(&store, "t-pf-ready", "op-pf-ready");
        seed_gp(&store, "ptask-gp-pf-ready", principal.tenant_id());
        let had_cred = std::env::var_os(DEEPSEEK_CREDENTIAL_REFERENCE);
        std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
        let pre = operator_preflight_read_only(&store, &principal, None, Some("ptask-gp-pf-ready"));
        match had_cred {
            Some(v) => std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, v),
            None => std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE),
        }
        let pre = pre.unwrap();
        assert_eq!(pre["ready"], false);
        assert_eq!(pre["authority_consumed"], false);
        assert_eq!(pre["provider_call_performed"], false);
        assert_eq!(pre["target_write_performed"], false);
        assert_eq!(pre["live_baseline_sealed"], false);
        assert_eq!(pre["credential_readiness"], "missing");
        assert_eq!(pre["credential_symbol_present"], false);
        assert_eq!(
            pre["comparison"]["window"],
            "single_randomized_interleaved_window"
        );
        assert_eq!(pre["comparison"]["authorizations_issued"], false);
        assert_eq!(pre["comparison"]["unissued_authorization_packages"], 2);
        assert!(pre["blockers"].as_array().unwrap().iter().any(|blocker| {
            blocker.get("code").and_then(Value::as_str) == Some("missing_credential_symbol")
        }));
        assert!(!pre["blockers"].as_array().unwrap().iter().any(|blocker| {
            blocker.get("code").and_then(Value::as_str) == Some("credential_readiness_unavailable")
        }));
        let observed = pre["observed_at"].as_str().unwrap();
        assert!(
            chrono::DateTime::parse_from_rfc3339(observed).is_ok(),
            "{observed}"
        );
        let frozen = freeze_current_operator_contract_set().unwrap();
        assert_eq!(
            frozen.accepted_main_sha,
            crate::rwe::operator_corpus::OPERATOR_ARTIFACTS_FROZEN_AT_MAIN_SHA
        );
        assert_eq!(
            frozen.corpus.corpus_sha256,
            crate::rwe::operator_corpus::OPERATOR_V2_CORPUS_SHA256
        );
        assert_eq!(
            frozen.protocol.body_sha256,
            crate::rwe::operator_corpus::OPERATOR_V2_PROTOCOL_SHA256
        );
        assert_eq!(
            frozen.schedule.schedule_sha256,
            crate::rwe::operator_corpus::OPERATOR_V2_SCHEDULE_SHA256
        );
        let request = sort_value(&json!({
            "schema_version": "rwe_run_authorization_v2_request.v1",
            "authorization_id": "unissued",
            "expires_at": "caller-supplied-finite",
            "issued": false,
            "admitted": false,
            "accepted_main_sha": frozen.accepted_main_sha,
            "corpus_sha256": frozen.corpus.corpus_sha256,
            "protocol_sha256": frozen.protocol.body_sha256,
            "schedule_sha256": frozen.schedule.schedule_sha256,
        }));
        let request_sha256 = sha256_hex(serde_json::to_vec(&request).unwrap().as_slice());
        assert_eq!(
            request_sha256,
            "015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a"
        );
        assert!(store
            .get_rwe_run_authorization("unissued")
            .unwrap()
            .is_none());
    }

    #[test]
    fn viability_preflight_is_read_only_without_store_creation_or_auth_touch() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("existing?reserved#percent%.db");
        let store = LocalProductStore::new(&db_path).unwrap();
        store
            .record_api_key_metadata_for_tenant(
                "t-read-only",
                "op-read-only",
                "operator-user",
                "operator",
                &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                "test",
            )
            .unwrap();
        let before = store
            .get_api_key_metadata_for_tenant("op-read-only", "t-read-only")
            .unwrap()
            .unwrap();
        assert!(before["last_used_at"].is_null());
        drop(store);
        let directory_entries = || {
            let mut entries: Vec<_> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| {
                    let entry = entry.unwrap();
                    let metadata = entry.metadata().unwrap();
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        metadata.len(),
                        metadata.modified().ok(),
                    )
                })
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            entries
        };
        let before_directory = directory_entries();
        let before_file = std::fs::metadata(&db_path).unwrap();
        let before_size = before_file.len();
        let before_modified = before_file.modified().ok();

        let read_only = Arc::new(LocalProductStore::open_existing_read_only(&db_path).unwrap());
        let principal = read_only
            .authenticate_managed_acceptance_principal_read_only(
                "t-read-only",
                "op-read-only",
                Some(1.0),
            )
            .unwrap();
        let pre = operator_preflight_read_only(&read_only, &principal, None, None).unwrap();
        assert_eq!(pre["ready"], false);
        assert_eq!(pre["authority_consumed"], false);
        assert_eq!(pre["provider_call_performed"], false);
        assert_eq!(pre["target_write_performed"], false);
        assert!(pre["credential_symbol_present"].is_boolean());
        assert_ne!(pre["credential_readiness"], "unavailable");
        let mutation = read_only.record_api_key_metadata(
            "should-not-write",
            "operator-user",
            "operator",
            &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
            "read-only-regression",
        );
        assert!(mutation.is_err(), "read-only store accepted a mutation");
        let checkpoint = read_only.checkpoint_wal();
        assert!(checkpoint
            .expect_err("read-only store checkpointed its WAL")
            .contains("read-only"));
        let transaction = read_only.with_transaction(|_| Ok::<(), String>(()));
        assert!(transaction
            .expect_err("read-only store accepted a transaction")
            .contains("read-only"));
        let restore = read_only
            .restore_verified_sqlite_backup(std::path::Path::new("missing-verified-backup.db"));
        assert!(restore
            .expect_err("read-only store accepted a backup restore")
            .contains("read-only"));
        let after = read_only
            .get_api_key_metadata_for_tenant("op-read-only", "t-read-only")
            .unwrap()
            .unwrap();
        assert!(after["last_used_at"].is_null());
        assert_eq!(before, after);
        drop(read_only);

        let after_file = std::fs::metadata(&db_path).unwrap();
        assert_eq!(after_file.len(), before_size);
        assert_eq!(after_file.modified().ok(), before_modified);
        assert_eq!(directory_entries(), before_directory);

        let missing_parent = dir.path().join("missing-parent");
        let missing_path = missing_parent.join("missing.db");
        assert!(LocalProductStore::open_existing_read_only(&missing_path).is_err());
        assert!(!missing_parent.exists());
    }

    #[test]
    fn read_only_snapshot_serializes_same_process_writer() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("snapshot.db");
        let store = LocalProductStore::new(&db_path).unwrap();
        drop(store);

        let read_only = LocalProductStore::open_existing_read_only(&db_path).unwrap();
        let writer_path = db_path.clone();
        let (started_sender, started_receiver) = sync_channel(0);
        let (finished_sender, finished_receiver) = channel();
        let writer = std::thread::spawn(move || {
            started_sender.send(()).unwrap();
            let mutating_store = LocalProductStore::new(&writer_path).unwrap();
            mutating_store
                .record_api_key_metadata_for_tenant(
                    "t-snapshot",
                    "op-snapshot",
                    "operator-user",
                    "operator",
                    &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                    "snapshot-mutation",
                )
                .unwrap();
            finished_sender.send(()).unwrap();
        });
        started_receiver.recv().unwrap();
        assert!(finished_receiver
            .recv_timeout(Duration::from_millis(100))
            .is_err());
        drop(read_only);
        assert!(finished_receiver
            .recv_timeout(Duration::from_secs(2))
            .is_ok());
        writer.join().unwrap();
    }

    #[test]
    fn read_only_snapshot_rejects_same_bytes_path_replacement() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("same-bytes-replacement.db");
        let store = LocalProductStore::new(&db_path).unwrap();
        store
            .record_api_key_metadata_for_tenant(
                "t-same-bytes",
                "op-same-bytes",
                "operator-user",
                "operator",
                &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                "same-bytes-setup",
            )
            .unwrap();
        drop(store);

        let read_only = LocalProductStore::open_existing_read_only(&db_path).unwrap();
        let original_bytes = std::fs::read(&db_path).unwrap();
        std::fs::remove_file(&db_path).unwrap();
        std::fs::write(&db_path, original_bytes).unwrap();

        let error = read_only
            .get_api_key_metadata_for_tenant("op-same-bytes", "t-same-bytes")
            .expect_err("read-only store accepted a replacement inode");
        assert!(error.contains("identity changed"), "{error}");
    }

    #[test]
    fn read_only_open_reuses_encryption_configuration_and_verified_lock_anchor() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("encrypted-anchor.db");
        let store = LocalProductStore::new_with_encryption(
            &db_path,
            || "2026-08-18T00:00:00Z".to_string(),
            Some("test-encryption-key'with-specials"),
        )
        .unwrap();
        store
            .record_api_key_metadata_for_tenant(
                "t-encrypted",
                "op-encrypted",
                "operator-user",
                "operator",
                &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                "encryption-test",
            )
            .unwrap();
        drop(store);

        let read_only = LocalProductStore::open_existing_read_only_with_encryption(
            &db_path,
            Some("test-encryption-key'with-specials"),
        )
        .unwrap();
        assert!(read_only.is_encrypted());
        assert!(read_only
            .get_api_key_metadata_for_tenant("op-encrypted", "t-encrypted")
            .unwrap()
            .is_some());
        assert!(read_only
            .record_api_key_metadata(
                "op-encrypted-new",
                "operator-user",
                "operator",
                &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                "encryption-read-only-test",
            )
            .is_err());
    }

    #[test]
    fn read_only_encrypted_store_reports_unavailable_without_key() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("encrypted-unavailable.db");
        let store = LocalProductStore::new_with_encryption(
            &db_path,
            || "2026-08-18T00:00:00Z".to_string(),
            Some("test-encryption-key"),
        )
        .unwrap();
        drop(store);

        let companion_path = |suffix: &str| {
            let mut path = db_path.as_os_str().to_os_string();
            path.push(suffix);
            PathBuf::from(path)
        };
        let before = std::fs::read(&db_path).unwrap();
        for suffix in ["-wal", "-shm", "-journal"] {
            assert!(!companion_path(suffix).exists());
        }

        let error = match LocalProductStore::open_existing_read_only(&db_path) {
            Ok(_) => panic!("encrypted store opened without redacted key readiness"),
            Err(error) => error,
        };
        assert_eq!(error, "encryption_readiness_unavailable");
        assert_eq!(std::fs::read(&db_path).unwrap(), before);
        for suffix in ["-wal", "-shm", "-journal"] {
            assert!(!companion_path(suffix).exists());
        }
    }

    #[test]
    fn writable_store_fails_closed_on_path_replacement_but_keeps_verified_unlinked_inode() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("anchor.db");
        let store = LocalProductStore::new(&db_path).unwrap();
        std::fs::remove_file(&db_path).unwrap();
        store
            .record_api_key_metadata(
                "unlinked-key",
                "operator-user",
                "operator",
                &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                "unlinked-test",
            )
            .unwrap();

        std::fs::write(&db_path, b"replacement").unwrap();
        let error = store
            .record_api_key_metadata(
                "replacement-key",
                "operator-user",
                "operator",
                &[SCOPE_RISK_ACKNOWLEDGE.to_string()],
                "replacement-test",
            )
            .unwrap_err();
        assert!(
            error.contains("identity changed"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn read_only_open_rejects_pending_sqlite_sidecars() {
        for (suffix, label) in [
            ("-wal", "WAL"),
            ("-shm", "SHM"),
            ("-journal", "rollback journal"),
        ] {
            let dir = tempdir().unwrap();
            let db_path = dir.path().join("pending-sidecar.db");
            let store = LocalProductStore::new(&db_path).unwrap();
            drop(store);
            let mut sidecar = db_path.as_os_str().to_os_string();
            sidecar.push(suffix);
            std::fs::write(PathBuf::from(sidecar), b"pending").unwrap();
            let error = match LocalProductStore::open_existing_read_only(&db_path) {
                Ok(_) => panic!("read-only open accepted pending {label} companion"),
                Err(error) => error,
            };
            assert!(
                error.contains(label),
                "unexpected error for {label}: {error}"
            );
        }
    }

    #[test]
    fn preflight_fails_closed_without_parseable_store_clock() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("pf-bad-clock.db"), || {
                "not-a-timestamp".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-pf-clock", "op-pf-clock");
        let err = operator_preflight(&store, &principal, None, None).unwrap_err();
        assert!(
            err.contains("store clock must be canonical RFC3339/UTC"),
            "{err}"
        );
    }

    #[test]
    fn four_cell_injected_orchestration_maps_identities_and_receipts() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("c4.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-c4", "op-c4");
        let lease = admit_ready(&store, &principal, "auth-c4", "run-c4", "ptask-gp-c4");
        let driver = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        let result =
            run_frozen_schedule(&store, &principal, "run-c4", "auth-c4", &lease, &driver).unwrap();
        assert_eq!(result["cell_count"], 4);
        assert_eq!(result["attempts_recorded"], 4);
        assert_eq!(result["live_baseline_sealed"], false);
        let attempts = store.list_rwe_task_attempts_for_run("run-c4").unwrap();
        assert_eq!(attempts.len(), 4);
        for a in &attempts {
            assert_ne!(a["classification"], "dispatched");
            assert!(a["evidence_json"].get("cell_id").is_some());
            assert!(a["evidence_json"].get("product_task_id").is_some());
        }
        let again =
            run_frozen_schedule(&store, &principal, "run-c4", "auth-c4", &lease, &driver).unwrap();
        assert_eq!(again["attempts_recorded"], 4);
    }

    #[test]
    fn fixture_run_with_failures_never_reports_succeeded() {
        // Completion and success are separate: a fixture that merely
        // terminalizes with failure classes must record status "failed" (or
        // "outcome_unknown"), never "succeeded", and integration_fixture_succeeded
        // must stay false. Each scenario runs the full 4-cell schedule.
        let scenarios: &[(&str, &str)] = &[
            ("verifier_failed", "run-vf"),
            ("controlled_failure", "run-cf"),
            ("blocked_budget", "run-bb"),
        ];
        for (class, run_id) in scenarios {
            let dir = tempdir().unwrap();
            let store = Arc::new(
                LocalProductStore::new_with_clock(dir.path().join("fx.db"), || {
                    "2026-07-25T12:00:00Z".into()
                })
                .unwrap(),
            );
            let principal = operator(&store, "t-fx", "op-fx");
            let lease = admit_ready(&store, &principal, "auth-fx", run_id, "ptask-gp-fx");
            let driver = InjectedCellDriver {
                outcomes: outcomes_with_classification(class),
            };
            let result =
                run_frozen_schedule(&store, &principal, run_id, "auth-fx", &lease, &driver)
                    .unwrap();
            assert_eq!(
                result["integration_fixture_completed"], true,
                "{class} cells are terminal; the fixture completed: {result}"
            );
            assert_eq!(
                result["integration_fixture_succeeded"], false,
                "a fixture with {class} cells never succeeded: {result}"
            );
            assert_eq!(
                result["run"]["status"], "failed",
                "merely terminalized {class} cells must not report succeeded: {result}"
            );
            assert_eq!(result["live_baseline_sealed"], false);
        }
        // outcome_unknown stays outcome_unknown and never completes.
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("fxou.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-fxou", "op-fxou");
        let lease = admit_ready(&store, &principal, "auth-fxou", "run-fxou", "ptask-gp-fxou");
        let driver = InjectedCellDriver {
            outcomes: outcomes_with_classification("outcome_unknown"),
        };
        let result =
            run_frozen_schedule(&store, &principal, "run-fxou", "auth-fxou", &lease, &driver)
                .unwrap();
        assert_eq!(result["integration_fixture_completed"], false);
        assert_eq!(result["integration_fixture_succeeded"], false);
        assert_eq!(result["run"]["status"], "outcome_unknown");
        assert_eq!(
            result["aggregate"]["stopped_by"],
            "outcome_unknown_no_retry"
        );
    }

    #[test]
    fn fixture_run_with_all_fixture_success_cells_reports_succeeded() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("fxs.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-fxs", "op-fxs");
        let lease = admit_ready(&store, &principal, "auth-fxs", "run-fxs", "ptask-gp-fxs");
        let driver = InjectedCellDriver {
            outcomes: outcomes_with_classification("fixture_success"),
        };
        let result =
            run_frozen_schedule(&store, &principal, "run-fxs", "auth-fxs", &lease, &driver)
                .unwrap();
        assert_eq!(result["integration_fixture_completed"], true);
        assert_eq!(result["integration_fixture_succeeded"], true);
        assert_eq!(result["run"]["status"], "succeeded");
        assert_eq!(result["live_baseline_sealed"], false);
    }

    #[test]
    fn run_terminal_status_never_calls_merely_terminal_runs_succeeded() {
        let cell = |class: &str| json!({"classification": class});
        // Sealed live baseline -> succeeded.
        assert_eq!(
            run_terminal_status(true, false, &[cell("success"), cell("success")]),
            "succeeded"
        );
        // All required fixture cells fixture_success -> succeeded.
        assert_eq!(
            run_terminal_status(false, true, &[cell("fixture_success")]),
            "succeeded"
        );
        // outcome_unknown dominates.
        assert_eq!(
            run_terminal_status(
                false,
                false,
                &[cell("outcome_unknown"), cell("fixture_success")]
            ),
            "outcome_unknown"
        );
        // Every merely-terminal failure class -> failed, never succeeded.
        for class in [
            "controlled_failure",
            "verifier_failed",
            "provider_known_failure",
            "timeout",
            "cancelled",
            "blocked_ci_environment",
            "blocked_provider_free_mode",
            "blocked_missing_credential",
            "blocked_budget",
            "blocked_authority",
            "blocked_live_session_incomplete",
            "cleanup_failed",
            "injected_success",
            "injected_verifier_failed",
        ] {
            assert_eq!(
                run_terminal_status(false, false, &[cell(class)]),
                "failed",
                "{class} must map to failed, never succeeded"
            );
        }
    }

    #[test]
    fn product_golden_path_driver_composes_product_task_without_provider_or_seal() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("unarmed.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-unarmed", "op-unarmed");
        let lease = admit_ready(
            &store,
            &principal,
            "auth-unarmed",
            "run-unarmed",
            "ptask-gp-unarmed",
        );
        // Minimal target tree for intake validation (no live provider).
        let target = dir.path().join("fake-target");
        std::fs::create_dir_all(target.join("apps/api/src")).unwrap();
        std::fs::create_dir_all(target.join("apps/api/tests")).unwrap();
        std::fs::write(target.join("README.md"), "rwe\n").unwrap();
        let _gate = crate::product_golden_path::PRODUCT_TASK_GATE;
        let _lock = crate::cli::config::cli_env_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
        // The coordinator refuses to execute any cell effect while CI is set;
        // simulate the operator environment around this provider-free run.
        let had_ci = std::env::var_os("CI");
        std::env::remove_var("CI");
        let driver = ProductGoldenPathCellDriver {
            allow_live_provider_effects: false,
            target_repo_path: Some(target),
            fake_transport: None,
            cell_executor_key_id: None,
            cell_confirmer_key_id: None,
        };
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-unarmed",
            "auth-unarmed",
            &lease,
            &driver,
        )
        .unwrap();
        match had_ci {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
        assert_eq!(result["live_baseline_sealed"], false);
        assert_eq!(result["provider_call_performed"], false);
        let attempts = store.list_rwe_task_attempts_for_run("run-unarmed").unwrap();
        assert_eq!(attempts.len(), 4);
        for a in &attempts {
            assert_eq!(
                a["evidence_json"]["evidence_source"],
                "product_golden_path_owner"
            );
            assert_ne!(a["classification"], "dispatched");
        }
        std::env::remove_var(crate::product_golden_path::PRODUCT_TASK_GATE);
    }

    #[test]
    fn concurrent_duplicate_cell_dispatch_is_single_effect_sqlite() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("dup.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-dup", "op-dup");
        let lease = Arc::new(admit_ready(
            &store,
            &principal,
            "auth-dup",
            "run-dup",
            "ptask-gp-dup",
        ));
        let frozen = freeze_current_operator_contract_set().unwrap();
        let cell0 = &frozen.schedule.body["cells"][0];
        let task0 = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
            .unwrap();
        let ids = cell_identities_for("run-dup", cell0, task0).unwrap();
        let envelope0 = cell_reservation_limits(cell0).unwrap();
        let req = envelope0.max_provider_requests;
        let tok = envelope0.max_total_tokens;
        let reservation = json!({
            "schema_version": RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
            "cell_id": ids.cell_id,
            "provider_requests": req,
            "total_tokens": tok,
            "authorization_id": "auth-dup",
        });
        let wins = Arc::new(AtomicUsize::new(0));
        let losses = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                let store = Arc::clone(&store);
                let lease = Arc::clone(&lease);
                let wins = Arc::clone(&wins);
                let losses = Arc::clone(&losses);
                let attempt_id = ids.rwe_task_attempt_id.clone();
                let task_id = ids.task_id.clone();
                let def = ids.definition_sha256.clone();
                let reservation = reservation.clone();
                scope.spawn(move || {
                    let principal = store
                        .authenticate_managed_acceptance_principal("t-dup", "op-dup", None)
                        .unwrap();
                    let envelope = cell_reservation_limits(&json!({
                        "max_provider_requests": req,
                        "max_retries": 0,
                        "max_input_tokens": 12000,
                        "max_output_tokens": 4000,
                        "max_total_tokens": tok,
                        "max_wall_time_ms": 900000,
                        "max_cost": 0.2,
                    }))
                    .unwrap();
                    match store.claim_rwe_cell_dispatch(
                        &principal,
                        "run-dup",
                        "auth-dup",
                        &lease,
                        &attempt_id,
                        &task_id,
                        &def,
                        &envelope,
                        &reservation,
                    ) {
                        Ok(v)
                            if v.get("idempotent_replay").and_then(Value::as_bool)
                                != Some(true) =>
                        {
                            wins.fetch_add(1, Ordering::SeqCst);
                        }
                        Ok(_) | Err(_) => {
                            losses.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                });
            }
        });
        assert_eq!(
            wins.load(Ordering::SeqCst),
            1,
            "exactly one concurrent claim must win"
        );
        assert!(losses.load(Ordering::SeqCst) >= 1);
        let attempts = store.list_rwe_task_attempts_for_run("run-dup").unwrap();
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0]["classification"], "dispatched");
    }

    #[test]
    fn pre_effect_budget_refusal_invokes_driver_zero_times() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("budget.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-budget", "op-budget");
        let lease = admit_ready(
            &store,
            &principal,
            "auth-budget2",
            "run-budget2",
            "ptask-gp-budget",
        );
        let frozen = freeze_current_operator_contract_set().unwrap();
        let cell0 = &frozen.schedule.body["cells"][0];
        let task0 = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
            .unwrap();
        let ids0 = cell_identities_for("run-budget2", cell0, task0).unwrap();
        let mut seed_outcome = success_outcomes()[0].clone();
        seed_outcome.provider_requests = 12;
        seed_outcome.total_tokens = 1000;
        let evidence = build_cell_evidence(
            "run-budget2",
            "auth-budget2",
            &frozen,
            cell0,
            task0,
            &ids0,
            &seed_outcome,
        );
        store
            .persist_rwe_task_attempt(
                "run-budget2",
                &lease,
                &ids0.rwe_task_attempt_id,
                &ids0.task_id,
                &ids0.definition_sha256,
                "injected_success",
                &evidence,
            )
            .unwrap();
        let counter = Arc::new(AtomicUsize::new(0));
        let inner = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        let counting = CountingCellDriver {
            inner: &inner,
            invocations: Arc::clone(&counter),
        };
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-budget2",
            "auth-budget2",
            &lease,
            &counting,
        )
        .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        assert_eq!(result["attempts_recorded"], 4);
        let classes: Vec<_> = store
            .list_rwe_task_attempts_for_run("run-budget2")
            .unwrap()
            .iter()
            .map(|a| a["classification"].as_str().unwrap().to_string())
            .collect();
        assert!(
            classes.iter().any(|c| c == "blocked_budget"),
            "expected blocked_budget, got {classes:?}"
        );
    }

    #[test]
    fn stop_rule_restart_is_exact_and_never_redispatches_skipped_cells() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("stop.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-stop", "op-stop");
        let lease = admit_ready(&store, &principal, "auth-stop", "run-stop", "ptask-gp-stop");
        let mut outcomes = success_outcomes();
        outcomes[0].classification = "blocked_authority".into();
        let counter = Arc::new(AtomicUsize::new(0));
        let inner = InjectedCellDriver {
            outcomes: outcomes.clone(),
        };
        let counting = CountingCellDriver {
            inner: &inner,
            invocations: Arc::clone(&counter),
        };
        run_frozen_schedule(
            &store,
            &principal,
            "run-stop",
            "auth-stop",
            &lease,
            &counting,
        )
        .unwrap();
        let first = counter.load(Ordering::SeqCst);
        assert_eq!(first, 1);
        let skipped = store
            .list_rwe_task_attempts_for_run("run-stop")
            .unwrap()
            .iter()
            .filter(|a| a["classification"] == "skipped_by_stop_rule")
            .count();
        assert_eq!(skipped, 3);
        run_frozen_schedule(
            &store,
            &principal,
            "run-stop",
            "auth-stop",
            &lease,
            &counting,
        )
        .unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), first);
    }

    #[test]
    fn outcome_unknown_is_terminal_no_second_authorization() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("ou.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-ou", "op-ou");
        let lease = admit_ready(&store, &principal, "auth-ou", "run-ou", "ptask-gp-ou");
        let mut outcomes = success_outcomes();
        outcomes[0].classification = "outcome_unknown".into();
        outcomes[0].cost_unknown = true;
        outcomes[0].monetary_cost = None;
        let driver = InjectedCellDriver { outcomes };
        run_frozen_schedule(&store, &principal, "run-ou", "auth-ou", &lease, &driver).unwrap();
        assert!(store
            .get_rwe_run_authorization("auth-ou-2")
            .unwrap()
            .is_none());
        assert_eq!(
            store.get_rwe_run_authorization("auth-ou").unwrap().unwrap()["status"],
            "consumed"
        );
    }

    #[test]
    fn injected_outcome_cannot_seal_live_baseline() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("inj-seal.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-inj-seal", "op-inj-seal");
        let lease = admit_ready(
            &store,
            &principal,
            "auth-inj-seal",
            "run-inj-seal",
            "ptask-gp-inj-seal",
        );
        let mut outcomes = success_outcomes();
        for o in &mut outcomes {
            o.live_provider_request = true;
            o.classification = "success".into();
        }
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-inj-seal",
            "auth-inj-seal",
            &lease,
            &InjectedCellDriver { outcomes },
        )
        .unwrap();
        assert_eq!(result["live_baseline_sealed"], false);
    }

    #[test]
    fn cell_identities_are_deterministic() {
        let frozen = freeze_current_operator_contract_set().unwrap();
        let cell = &frozen.schedule.body["cells"][0];
        let task = &frozen.corpus.tasks[0];
        let a = cell_identities_for("run-x", cell, task).unwrap();
        let b = cell_identities_for("run-x", cell, task).unwrap();
        assert_eq!(a, b);
        assert!(a.branch_name.starts_with("acp/rwe/"));
    }

    #[test]
    fn stale_and_wrong_principal_do_not_consume_fresh_authority() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("stale.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-stale", "op-stale");
        seed_gp(&store, "ptask-gp-stale", principal.tenant_id());
        let foreign = operator(&store, "t-foreign", "op-foreign");
        let err = issue_and_admit_v2(
            &store,
            &foreign,
            "auth-foreign",
            "run-foreign",
            "ptask-gp-stale",
            "2026-08-07T00:00:00Z",
        )
        .unwrap_err();
        assert!(
            err.contains("fail closed") || err.contains("tenant") || err.contains("not found"),
            "{err}"
        );
        assert!(store
            .get_rwe_run_authorization("auth-foreign")
            .unwrap()
            .is_none());
    }

    #[test]
    fn crash_style_admit_lease_recovery_then_schedule() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("lease.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-lease", "op-lease");
        let _lease = admit_ready(
            &store,
            &principal,
            "auth-lease",
            "run-lease",
            "ptask-gp-lease",
        );
        let driver = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        let result =
            run_frozen_schedule(&store, &principal, "run-lease", "auth-lease", "", &driver)
                .unwrap();
        assert_eq!(result["attempts_recorded"], 4);
    }

    #[test]
    fn revalidate_rejects_tampered_binding_surface() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("reval.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-reval", "op-reval");
        let _lease = admit_ready(
            &store,
            &principal,
            "auth-reval",
            "run-reval",
            "ptask-gp-reval",
        );
        let frozen = freeze_current_operator_contract_set().unwrap();
        let ok = revalidate_stored_v2_authorization(&store, &principal, "auth-reval", &frozen);
        assert!(ok.is_ok());
    }

    #[test]
    fn claim_refuses_auth_mismatch_and_stale_lease() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("auth-mis.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-mis", "op-mis");
        let lease = admit_ready(&store, &principal, "auth-mis", "run-mis", "ptask-gp-mis");
        let frozen = freeze_current_operator_contract_set().unwrap();
        let cell0 = &frozen.schedule.body["cells"][0];
        let task0 = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
            .unwrap();
        let ids = cell_identities_for("run-mis", cell0, task0).unwrap();
        let envelope = cell_reservation_limits(cell0).unwrap();
        let reservation = json!({"cell_id": ids.cell_id});
        let wrong_auth = store
            .claim_rwe_cell_dispatch(
                &principal,
                "run-mis",
                "auth-NOT-BOUND",
                &lease,
                &ids.rwe_task_attempt_id,
                &ids.task_id,
                &ids.definition_sha256,
                &envelope,
                &reservation,
            )
            .unwrap_err();
        assert!(
            wrong_auth.contains("authorization mismatch")
                || wrong_auth.contains("not found")
                || wrong_auth.contains("no rows")
                || wrong_auth.contains("Query returned"),
            "{wrong_auth}"
        );
        let stale_lease = store
            .claim_rwe_cell_dispatch(
                &principal,
                "run-mis",
                "auth-mis",
                "rwe-lease-stale-token",
                &ids.rwe_task_attempt_id,
                &ids.task_id,
                &ids.definition_sha256,
                &envelope,
                &reservation,
            )
            .unwrap_err();
        assert!(
            stale_lease.contains("lease") || stale_lease.contains("owner"),
            "{stale_lease}"
        );
    }

    #[test]
    fn claim_refuses_budget_dimension_overflow_and_preserves_failed_cost() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("bdim.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-bdim", "op-bdim");
        let lease = admit_ready(&store, &principal, "auth-bdim", "run-bdim", "ptask-gp-bdim");
        let frozen = freeze_current_operator_contract_set().unwrap();
        let cell0 = &frozen.schedule.body["cells"][0];
        let task0 = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == cell0["task_id"].as_str().unwrap())
            .unwrap();
        let ids = cell_identities_for("run-bdim", cell0, task0).unwrap();
        // Exhaust run-level request ceiling with a full-reservation terminal attempt.
        let mut seed = success_outcomes()[0].clone();
        seed.provider_requests = 12;
        seed.total_tokens = 1000;
        let evidence =
            build_cell_evidence("run-bdim", "auth-bdim", &frozen, cell0, task0, &ids, &seed);
        store
            .persist_rwe_task_attempt(
                "run-bdim",
                &lease,
                &ids.rwe_task_attempt_id,
                &ids.task_id,
                &ids.definition_sha256,
                "controlled_failure",
                &evidence,
            )
            .unwrap();
        // Second cell must refuse reservation.
        let cell1 = &frozen.schedule.body["cells"][1];
        let task1 = frozen
            .corpus
            .tasks
            .iter()
            .find(|t| t.task_id == cell1["task_id"].as_str().unwrap())
            .unwrap();
        let ids1 = cell_identities_for("run-bdim", cell1, task1).unwrap();
        let envelope = cell_reservation_limits(cell1).unwrap();
        let err = store
            .claim_rwe_cell_dispatch(
                &principal,
                "run-bdim",
                "auth-bdim",
                &lease,
                &ids1.rwe_task_attempt_id,
                &ids1.task_id,
                &ids1.definition_sha256,
                &envelope,
                &json!({"cell_id": ids1.cell_id}),
            )
            .unwrap_err();
        assert!(err.contains("budget"), "{err}");
        // Failed-attempt cost preserved on first row.
        let rows = store.list_rwe_task_attempts_for_run("run-bdim").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["evidence_json"]["provider_requests"], 12);
    }

    #[test]
    fn provider_journal_mismatch_and_reused_receipt_cannot_seal() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("journal.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-jnl", "op-jnl");
        let lease = admit_ready(&store, &principal, "auth-jnl", "run-jnl", "ptask-gp-jnl");
        let mut outcomes = success_outcomes();
        for o in &mut outcomes {
            o.classification = "success".into();
            o.evidence_source = "product_golden_path_owner".into();
            o.live_provider_request = true;
            o.provider_requests = 3;
            o.total_tokens = 100;
        }
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-jnl",
            "auth-jnl",
            &lease,
            &InjectedCellDriver { outcomes },
        )
        .unwrap();
        // Injected driver forces evidence_source=injected and non-seal classifications.
        assert_eq!(result["live_baseline_sealed"], false);
    }

    #[test]
    fn production_driver_accepts_fake_transport_without_caller_seal_evidence() {
        use crate::provider::transport::{HttpResponse, MockTransport};
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("fake-tx.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-ftx", "op-ftx");
        let lease = admit_ready(&store, &principal, "auth-ftx", "run-ftx", "ptask-gp-ftx");
        let target = dir.path().join("target");
        std::fs::create_dir_all(target.join("apps/api/src")).unwrap();
        std::fs::create_dir_all(target.join("apps/api/tests")).unwrap();
        std::fs::write(target.join("README.md"), "rwe\n").unwrap();
        let _lock = crate::cli::config::cli_env_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
        // The coordinator refuses to execute any cell effect while CI is set;
        // simulate the operator environment around this fake-transport run.
        let had_ci = std::env::var_os("CI");
        std::env::remove_var("CI");
        let transport = std::sync::Arc::new(MockTransport::new(vec![Ok(HttpResponse {
            status: 200,
            body: br#"{"choices":[{"message":{"content":"{}"}}]}"#.to_vec(),
        })]));
        let driver = ProductGoldenPathCellDriver {
            allow_live_provider_effects: false,
            target_repo_path: Some(target),
            fake_transport: Some(transport),
            cell_executor_key_id: None,
            cell_confirmer_key_id: None,
        };
        let result =
            run_frozen_schedule(&store, &principal, "run-ftx", "auth-ftx", &lease, &driver)
                .unwrap();
        match had_ci {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
        assert_eq!(result["live_baseline_sealed"], false);
        assert_eq!(result["provider_call_performed"], false);
        assert_eq!(result["provider_transport_provenance"], "none");
        for a in store.list_rwe_task_attempts_for_run("run-ftx").unwrap() {
            assert_eq!(
                a["evidence_json"]["evidence_source"],
                "product_golden_path_owner"
            );
            assert_ne!(a["evidence_json"]["evidence_source"], "injected");
        }
        std::env::remove_var(crate::product_golden_path::PRODUCT_TASK_GATE);
    }

    /// A deliberately hostile transport that behaves like a production client
    /// but is not the canonical ReqwestTransport concrete type. The trait has
    /// no provenance surface, so it can never mint External; the fake slot is
    /// wrapped in InjectedTransportBoundary regardless.
    struct SpoofExternalTransport {
        responses: std::sync::Mutex<
            std::collections::VecDeque<
                Result<
                    crate::provider::transport::HttpResponse,
                    crate::provider::transport::HttpError,
                >,
            >,
        >,
    }

    #[async_trait::async_trait]
    impl crate::provider::transport::HttpTransport for SpoofExternalTransport {
        async fn send(
            &self,
            _request: &crate::provider::transport::HttpRequest,
        ) -> Result<crate::provider::transport::HttpResponse, crate::provider::transport::HttpError>
        {
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    Err(crate::provider::transport::HttpError::Connection(
                        "no spoof responses left".to_string(),
                    ))
                })
        }
    }

    #[test]
    fn fake_transport_slot_cannot_impersonate_external_provenance() {
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("spoof.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-spoof", "op-spoof");
        let lease = admit_ready(
            &store,
            &principal,
            "auth-spoof",
            "run-spoof",
            "ptask-gp-spoof",
        );
        let target = dir.path().join("target");
        std::fs::create_dir_all(target.join("apps/api/src")).unwrap();
        std::fs::create_dir_all(target.join("apps/api/tests")).unwrap();
        std::fs::write(target.join("README.md"), "rwe\n").unwrap();
        let _lock = crate::cli::config::cli_env_test_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
        let had_ci = std::env::var_os("CI");
        std::env::remove_var("CI");
        // 4 cells x 3 requests of plausible-looking responses: the spoof
        // transport completes the full lifecycle but must never mint External.
        let responses = (0..12)
            .map(|_| {
                Ok(HttpResponse {
                    status: 200,
                    body: br#"{"choices":[{"message":{"content":"{}"}}]}"#.to_vec(),
                })
            })
            .collect::<std::collections::VecDeque<_>>();
        let transport = std::sync::Arc::new(SpoofExternalTransport {
            responses: std::sync::Mutex::new(responses),
        });
        let driver = ProductGoldenPathCellDriver {
            allow_live_provider_effects: false,
            target_repo_path: Some(target),
            fake_transport: Some(transport),
            cell_executor_key_id: None,
            cell_confirmer_key_id: None,
        };
        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-spoof",
            "auth-spoof",
            &lease,
            &driver,
        )
        .unwrap();
        match had_ci {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
        // The spoof path can never become a live baseline: the run seals
        // nothing, performs no live provider call, and its provenance is
        // never external (fail-closed to none/injected).
        assert_eq!(result["live_baseline_sealed"], false);
        assert_eq!(result["provider_call_performed"], false);
        assert_ne!(
            result["provider_transport_provenance"], "external",
            "a custom transport cannot mint external provenance: {result}"
        );
        let attempts = store.list_rwe_task_attempts_for_run("run-spoof").unwrap();
        assert_eq!(attempts.len(), 4);
        for a in &attempts {
            assert_ne!(
                a["evidence_json"]["provider_transport_provenance"], "external",
                "durable attempt evidence must never carry external provenance for the spoof path: {a}"
            );
            assert_eq!(a["evidence_json"]["live_provider_request"], false);
        }
        // The store evidence gate (store_evidence_transport_provenance) is
        // exercised directly by store_evidence_transport_provenance_gate_is_fail_closed;
        // here the durable attempt evidence itself must never carry external
        // provenance for the spoof path.
        std::env::remove_var(crate::product_golden_path::PRODUCT_TASK_GATE);
    }

    #[test]
    fn store_evidence_transport_provenance_gate_is_fail_closed() {
        let external_entry = |status: &str| {
            json!({
                "schema_version": "managed_provider_request_claim.v1",
                "node_id": "n",
                "status": status,
                "transport_provenance": "external",
                "request_sha256": "a".repeat(64),
            })
        };
        let injected_entry = json!({
            "schema_version": "managed_provider_request_claim.v1",
            "node_id": "n",
            "status": "succeeded",
            "transport_provenance": "injected",
            "request_sha256": "b".repeat(64),
        });
        let external_projection = json!({
            "provider_execution": {
                "schema_version": "managed_deepseek_execution_evidence.v1",
                "provider_request_count": 1,
                "transport_provenance": "external",
                "requests": [json!({"node_id": "n", "transport_provenance": "external"})],
            },
            "provider_request_journal": [external_entry("succeeded")],
        });
        let injected_projection = json!({
            "provider_execution": {
                "schema_version": "managed_deepseek_execution_evidence.v1",
                "provider_request_count": 1,
                "transport_provenance": "injected",
                "requests": [json!({"node_id": "n", "transport_provenance": "injected"})],
            },
            "provider_request_journal": [injected_entry.clone()],
        });

        assert_eq!(
            store_evidence_transport_provenance(&external_projection).as_deref(),
            Ok("external")
        );
        assert_eq!(
            store_evidence_transport_provenance(&injected_projection).as_deref(),
            Ok("injected")
        );

        // Missing aggregate provenance fails closed.
        let mut missing_aggregate = external_projection.clone();
        missing_aggregate["provider_execution"]
            .as_object_mut()
            .unwrap()
            .remove("transport_provenance");
        assert!(store_evidence_transport_provenance(&missing_aggregate).is_err());

        // Missing journal-entry provenance fails closed.
        let mut missing_entry = external_projection.clone();
        missing_entry["provider_request_journal"][0]
            .as_object_mut()
            .unwrap()
            .remove("transport_provenance");
        assert!(store_evidence_transport_provenance(&missing_entry).is_err());

        // Mixed aggregate vs journal provenance fails closed.
        let mut mixed = external_projection.clone();
        mixed["provider_request_journal"] = json!([injected_entry.clone()]);
        assert!(store_evidence_transport_provenance(&mixed).is_err());

        // Empty journal fails closed even with an external aggregate claim.
        let mut empty_journal = external_projection.clone();
        empty_journal["provider_request_journal"] = json!([]);
        assert!(store_evidence_transport_provenance(&empty_journal).is_err());

        // Invalid provenance value fails closed.
        let mut invalid = external_projection.clone();
        invalid["provider_execution"]["transport_provenance"] = json!("fixture");
        assert!(store_evidence_transport_provenance(&invalid).is_err());

        // Null provider execution fails closed (provider-free is never a seal).
        let no_provider = json!({
            "provider_execution": Value::Null,
            "provider_request_journal": [],
        });
        assert!(store_evidence_transport_provenance(&no_provider).is_err());
    }

    #[test]
    fn injected_transport_cannot_seal_with_forged_receipts_or_outcomes() {
        // Even if every receipt and outcome were forged as external, the
        // durable journal's injected provenance blocks sealing.
        let forged_external_receipt = json!({
            "provider_execution": {
                "schema_version": "managed_deepseek_execution_evidence.v1",
                "provider_request_count": 3,
                "transport_provenance": "external",
                "requests": [json!({})],
            },
            "provider_request_journal": [injected_claim_entry(), injected_claim_entry(), injected_claim_entry()],
        });
        assert_ne!(
            store_evidence_transport_provenance(&forged_external_receipt).as_deref(),
            Ok("external"),
            "injected journal claims must defeat forged external receipts"
        );
    }

    #[test]
    fn provider_free_evidence_is_never_seal_eligible() {
        let provider_free = json!({
            "provider_execution": Value::Null,
            "provider_request_journal": [],
        });
        assert!(
            store_evidence_transport_provenance(&provider_free).is_err(),
            "provider-free execution has no transport provenance and cannot seal"
        );
    }

    fn injected_claim_entry() -> Value {
        json!({
            "schema_version": "managed_provider_request_claim.v1",
            "node_id": "n",
            "status": "succeeded",
            "transport_provenance": "injected",
            "request_sha256": "c".repeat(64),
        })
    }

    #[test]
    fn evaluator_surface_sentinel_rejects_verifier_tampering_paths() {
        // Test collection and verifier config files are never real delivery.
        assert!(changed_file_is_evaluator_surface(
            "apps/api/tests/conftest.py"
        ));
        assert!(changed_file_is_evaluator_surface(
            "apps/api/tests/pytest.ini"
        ));
        assert!(changed_file_is_evaluator_surface("apps/api/tests/tox.ini"));
        assert!(changed_file_is_evaluator_surface(
            "apps/api/tests/setup.cfg"
        ));
        assert!(changed_file_is_evaluator_surface("pyproject.toml"));
        // Ordinary product files are not evaluator surface.
        assert!(!changed_file_is_evaluator_surface("apps/api/src/main.py"));
        assert!(!changed_file_is_evaluator_surface(
            "apps/api/tests/test_flow.py"
        ));
        assert!(!changed_file_is_evaluator_surface("README.md"));
        assert!(!changed_file_is_evaluator_surface(
            "apps/api/tests/conftest_notes.md"
        ));
    }

    #[test]
    fn execution_error_after_fence_terminalizes_without_second_auth() {
        struct FailDriver;
        impl CellDriver for FailDriver {
            fn execute_cell(
                &self,
                _store: &std::sync::Arc<LocalProductStore>,
                _principal: &AuthenticatedPrincipal,
                _frozen: &OperatorFrozenContractSet,
                _run_id: &str,
                _lease_token: &str,
                _cell: &Value,
                _task: &RweTaskDefinition,
                _ids: &CellIdentities,
            ) -> Result<CellOutcome, String> {
                Err("provider timeout while awaiting response".into())
            }
        }
        let dir = tempdir().unwrap();
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("fence-err.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let principal = operator(&store, "t-ferr", "op-ferr");
        let lease = admit_ready(&store, &principal, "auth-ferr", "run-ferr", "ptask-gp-ferr");
        run_frozen_schedule(
            &store,
            &principal,
            "run-ferr",
            "auth-ferr",
            &lease,
            &FailDriver,
        )
        .unwrap();
        let rows = store.list_rwe_task_attempts_for_run("run-ferr").unwrap();
        assert!(!rows.is_empty());
        assert_eq!(rows[0]["classification"], "cleanup_failed");
        assert_ne!(rows[0]["classification"], "dispatched");
        // No second authorization consumed.
        assert!(store
            .get_rwe_run_authorization("auth-ferr-2")
            .unwrap()
            .is_none());
    }

    /// Clone the frozen target at the exact frozen SHA: local checkout when
    /// present (offline), otherwise a GitHub clone (CI). Returns None when the
    /// repository is unavailable so the test skips instead of faking evidence.
    /// Operator live-run gate for the armed Draft PR test: only an explicit
    /// `=1` authorizes the genuine remote push + GitHub Draft PR external
    /// effects. Normal CI never sets it, so the armed test SKIPs there.
    const ARMED_LIVE_RUN_GATE: &str = "ACP_RWE_ARMED_LIVE_RUN";

    fn frozen_target_repo(dir: &std::path::Path) -> Option<std::path::PathBuf> {
        use std::process::Command;
        let target = dir.join("frozen-target");
        let local = std::path::Path::new("/home/igzela/Projects/alters-lab");
        let live_run = std::env::var(ARMED_LIVE_RUN_GATE).is_ok_and(|value| value == "1");
        let clone_ok = if live_run {
            // Live-run mode requires the credential-free https origin so the
            // approved branch is genuinely pushed to the operator-authorized
            // remote and the Draft PR is created there. The operator proxy can
            // drop large pack transfers, so retry the clone a bounded number of
            // times; a persistent failure skips the fixture honestly.
            (0..3).any(|_| {
                Command::new("git")
                    .args([
                        "-c",
                        "http.lowSpeedLimit=1",
                        "-c",
                        "http.lowSpeedTime=30",
                        "clone",
                        "--no-checkout",
                        "https://github.com/Igzela/alters-lab.git",
                    ])
                    .arg(&target)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
                    || {
                        let _ = Command::new("rm").args(["-rf"]).arg(&target).status();
                        false
                    }
            })
        } else if local.is_dir() {
            Command::new("git")
                .args(["clone", "--no-checkout"])
                .arg(local)
                .arg(&target)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else {
            Command::new("git")
                .args([
                    "clone",
                    "--no-checkout",
                    "https://github.com/Igzela/alters-lab.git",
                ])
                .arg(&target)
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        if !clone_ok {
            return None;
        }
        let checkout_ok = Command::new("git")
            .args(["checkout", "-B", "main", FROZEN_RWE_TARGET_MAIN_SHA])
            .current_dir(&target)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !checkout_ok {
            return None;
        }
        for args in [
            vec!["config", "user.email", "rwe-test@example.invalid"],
            vec!["config", "user.name", "RWE Test"],
        ] {
            let _ = Command::new("git").args(args).current_dir(&target).status();
        }
        Some(target)
    }

    fn mock_rwe_openai_response(id: &str, model: &str, content: Value) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: json!({
                "id": id,
                "model": model,
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": content.to_string()
                    },
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 20, "completion_tokens": 10}
            })
            .to_string()
            .into_bytes(),
        }
    }

    /// Armed production driver: the four-cell frozen schedule genuinely executes
    /// the delegated lifecycle through the store owners (contract, proposal,
    /// manifest approval, one-use spend, attempt lease, activation, managed
    /// executor over the injected fake transport, frozen pytest verifier,
    /// artifact confirmation, genuine Draft PR creation against the exact
    /// operator-authorized remote, terminal receipt, cleanup) and the run seals
    /// from store-owned receipts alone.
    ///
    /// Requires the operator live-run gate `ACP_RWE_ARMED_LIVE_RUN=1` plus
    /// populated `ACP_GITHUB_TOKEN_ENV` / `ACP_TARGET_REPO_GIT_TOKEN_ENV`
    /// references; without it the test SKIPs (like `frozen_target_repo`), so
    /// normal CI never creates external effects.
    #[test]
    /// Armed operator-gated live Draft PR lifecycle integration fixture.
    ///
    /// With the operator live-run gate set, this test genuinely executes the
    /// complete delegated lifecycle (store owners, frozen pytest verifier in
    /// the app-owned worktree, branch push, real GitHub Draft PR on the frozen
    /// target) through the INJECTED MockTransport. It is an integration-fixture
    /// proof of the lifecycle and Draft PR output only: the injected transport
    /// can never yield a live baseline seal, and the cells are recorded as
    /// `fixture_success`, never `success`. Without the gate the test SKIPs and
    /// creates no external effect.
    fn armed_production_driver_executes_genuine_delegated_lifecycle_as_integration_fixture() {
        use crate::provider::transport::MockTransport;
        let dir = tempdir().unwrap();
        if !std::env::var(ARMED_LIVE_RUN_GATE).is_ok_and(|value| value == "1") {
            eprintln!(
                "SKIP: armed live Draft PR run requires {ARMED_LIVE_RUN_GATE}=1 with populated ACP_GITHUB_TOKEN_ENV and ACP_TARGET_REPO_GIT_TOKEN_ENV (operator live-run authorization)"
            );
            return;
        }
        let Some(target) = frozen_target_repo(dir.path()) else {
            eprintln!("SKIP: frozen target repository unavailable (offline without local clone)");
            return;
        };
        let store = Arc::new(
            LocalProductStore::new_with_clock(dir.path().join("armed.db"), || {
                "2026-07-25T12:00:00Z".into()
            })
            .unwrap(),
        );
        let tenant = "t-armed";
        let principal = operator(&store, tenant, "op-armed");
        let _executor = cell_executor(&store, tenant, "op-armed-exec");
        let _confirmer = cell_confirmer(&store, tenant, "op-armed-conf");
        let lease = admit_ready(
            &store,
            &principal,
            "auth-armed",
            "run-armed",
            "ptask-gp-armed",
        );

        // Env guard: product gate + target repo output + credential symbol stay
        // set for the whole run; CI is removed so the armed path is exercised.
        // The operator live-run gate above authorizes the genuine network
        // output: branch push + real GitHub Draft PR on the exact remote.
        let _lock = crate::cli::config::cli_env_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let had_cred = std::env::var_os(DEEPSEEK_CREDENTIAL_REFERENCE);
        let had_ci = std::env::var_os("CI");
        let had_live_token = std::env::var_os(RWE_OPERATOR_LIVE_RUN_TOKEN);
        let had_env = [
            crate::product_golden_path::PRODUCT_TASK_GATE,
            "ACP_ENABLE_TARGET_REPO_OUTPUT",
            "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH",
            "ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST",
            "ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT",
            "ACP_ENABLE_GITHUB_PR_OUTPUT",
            "ACP_GITHUB_REPOSITORY_ALLOWLIST",
        ]
        .map(|name| (name, std::env::var_os(name)));
        std::env::set_var(crate::product_golden_path::PRODUCT_TASK_GATE, "1");
        std::env::set_var("ACP_ENABLE_TARGET_REPO_OUTPUT", "1");
        std::env::set_var("ACP_TARGET_REPO_OUTPUT_KILL_SWITCH", "0");
        std::env::set_var("ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST", "github.com");
        std::env::set_var("ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT", "1");
        std::env::set_var("ACP_ENABLE_GITHUB_PR_OUTPUT", "1");
        std::env::set_var("ACP_GITHUB_REPOSITORY_ALLOWLIST", "Igzela/alters-lab");
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, "test-operator-credential");
        std::env::set_var(RWE_OPERATOR_LIVE_RUN_TOKEN, "1");
        std::env::remove_var("CI");

        // 4 cells × 3 managed requests: planning / implementation / review.
        let plan = json!({
            "schema_version": "managed_deepseek_plan.v1",
            "status": "planned",
            "path": "apps/api/tests/test_alters_persist.py",
            "intent": "bounded_product_task"
        });
        // The frozen pytest suite is green only when the operator runtime data
        // is present; the mock implementer therefore ships a self-contained
        // conftest hook that skips the data-dependent modules, so the frozen
        // verifier passes in the app-owned worktree at the exact frozen SHA.
        let implement = json!({
            "schema_version": "managed_workspace_action.v1",
            "action": "replace_text",
            "path": "apps/api/tests/conftest.py",
            "old_text": "from __future__ import annotations",
            "new_text": "from __future__ import annotations\n\nimport pytest\n\n_DATA_DEPENDENT_MODULES = {\n    \"test_cycle_summary_api.py\",\n    \"test_validate_active_yaml_cli.py\",\n    \"test_active_yaml_loader.py\",\n    \"test_provider_dialogue.py\",\n    \"test_alter_dialogue_api.py\",\n    \"test_alter_dialogue.py\",\n    \"test_generation_drafts_api.py\",\n    \"test_snapshot_persist_api.py\",\n    \"test_alter_rubric_baseline.py\",\n    \"test_day30_harness.py\",\n    \"test_p8_m2_e2e_validation.py\",\n}\n\n\n@pytest.hookimpl(tryfirst=True)\ndef pytest_collection_modifyitems(config, items):\n    for item in items:\n        module = item.nodeid.split(\"::\")[0].split(\"/\")[-1]\n        if module in _DATA_DEPENDENT_MODULES:\n            item.add_marker(\n                pytest.mark.skip(reason=\"frozen suite runtime data unavailable\")\n            )"
        });
        let review = json!({
            "schema_version": "managed_deepseek_review.v1",
            "status": "accepted",
            "material_objections": []
        });
        let mut responses = Vec::new();
        for i in 0..4 {
            responses.push(Ok(mock_rwe_openai_response(
                &format!("armed-plan-{i}"),
                OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL,
                plan.clone(),
            )));
            responses.push(Ok(mock_rwe_openai_response(
                &format!("armed-implement-{i}"),
                OPERATOR_ADMITTED_MODEL,
                implement.clone(),
            )));
            responses.push(Ok(mock_rwe_openai_response(
                &format!("armed-review-{i}"),
                OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL,
                review.clone(),
            )));
        }
        let transport = std::sync::Arc::new(MockTransport::new(responses));
        let driver = ProductGoldenPathCellDriver {
            allow_live_provider_effects: true,
            target_repo_path: Some(target),
            fake_transport: Some(transport),
            cell_executor_key_id: Some("op-armed-exec".into()),
            cell_confirmer_key_id: Some("op-armed-conf".into()),
        };

        let result = run_frozen_schedule(
            &store,
            &principal,
            "run-armed",
            "auth-armed",
            &lease,
            &driver,
        )
        .unwrap();

        match had_cred {
            Some(v) => std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, v),
            None => std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE),
        }
        match had_ci {
            Some(v) => std::env::set_var("CI", v),
            None => std::env::remove_var("CI"),
        }
        match had_live_token {
            Some(v) => std::env::set_var(RWE_OPERATOR_LIVE_RUN_TOKEN, v),
            None => std::env::remove_var(RWE_OPERATOR_LIVE_RUN_TOKEN),
        }
        for name in [
            crate::product_golden_path::PRODUCT_TASK_GATE,
            "ACP_ENABLE_TARGET_REPO_OUTPUT",
            "ACP_TARGET_REPO_OUTPUT_KILL_SWITCH",
            "ACP_TARGET_REPO_REMOTE_HOST_ALLOWLIST",
            "ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT",
            "ACP_ENABLE_GITHUB_PR_OUTPUT",
            "ACP_GITHUB_REPOSITORY_ALLOWLIST",
        ] {
            match had_env.iter().find(|(n, _)| *n == name) {
                Some((_, Some(v))) => std::env::set_var(name, v),
                Some((_, None)) | None => std::env::remove_var(name),
            }
        }

        assert_eq!(
            result["live_baseline_sealed"], false,
            "injected transport must never seal a live baseline: {result}"
        );
        assert_eq!(
            result["provider_call_performed"], false,
            "injected requests are not real provider calls: {result}"
        );
        assert_eq!(result["provider_transport_provenance"], "injected");
        assert_eq!(result["injected_provider_call_performed"], true);
        let cell_results = result["aggregate"]["cell_results"].as_array().unwrap();
        let outcome_unknown_cell = cell_results
            .iter()
            .any(|c| c["classification"].as_str() == Some("outcome_unknown"));
        assert_eq!(
            result["integration_fixture_completed"], !outcome_unknown_cell,
            "fixture completion requires every cell to reach a terminal class; an outcome_unknown cell fails the fixture closed: {result}"
        );
        let run_status = result["run"]["status"].as_str().unwrap_or("");
        if outcome_unknown_cell {
            assert_eq!(
                result["aggregate"]["stopped_by"], "outcome_unknown_no_retry",
                "outcome_unknown must stop the schedule, never retry or advance: {result}"
            );
            assert_eq!(run_status, "outcome_unknown");
        } else {
            assert!(
                result["aggregate"]["stopped_by"].is_null(),
                "a fully completed fixture run has no stop rule: {result}"
            );
            assert_eq!(run_status, "succeeded");
        }
        assert_eq!(result["cell_count"], 4);
        assert_eq!(result["attempts_recorded"], 4);
        let attempts = store.list_rwe_task_attempts_for_run("run-armed").unwrap();
        for a in &attempts {
            let class = a["classification"].as_str().unwrap_or("");
            let ev = a["evidence_json"].clone();
            assert_eq!(
                ev["live_provider_request"], false,
                "injected provider requests are never live provider requests: {ev}"
            );
            match class {
                "fixture_success" => {
                    assert_eq!(ev["evidence_source"], "product_golden_path_owner");
                    assert_eq!(ev["provider_transport_provenance"], "injected");
                    assert_eq!(ev["provider_requests"], 3);
                    // Fixture honesty: the note records the verifier's real result and
                    // the evaluator-surface skip patch as fixture evidence.
                    let note = ev["note"].as_str().unwrap_or("");
                    assert!(note.contains("verifier_result=evidence_recorded"), "{note}");
                    assert!(note.contains("verifier_output_sha256="), "{note}");
                    assert!(
                        note.contains("evaluator-surface skip patch"),
                        "fixture skip patch must be recorded in evidence: {note}"
                    );
                    assert!(
                        note.contains("fixture evidence only, never accepted delivery"),
                        "{note}"
                    );
                }
                "outcome_unknown" => {
                    assert_eq!(ev["evidence_source"], "product_golden_path_owner");
                    assert_eq!(ev["provider_transport_provenance"], "none");
                    assert_eq!(ev["provider_requests"], 0);
                    let note = ev["note"].as_str().unwrap_or("");
                    assert!(
                        note.contains("outcome_unknown") && note.contains("git push"),
                        "outcome_unknown evidence must record the real external failure: {note}"
                    );
                }
                "skipped_by_stop_rule" => {
                    assert_eq!(ev["provider_transport_provenance"], "none");
                    assert_eq!(ev["evidence_source"], "blocked");
                    assert_eq!(ev["note"], "outcome_unknown_no_retry");
                }
                other => panic!("unexpected fixture classification {other}"),
            }
        }
        // Store-owned terminal receipts per cell: provider execution journal of
        // exactly three managed requests with injected provenance, verification
        // recorded, cleanup done, and a real Draft PR record on the frozen target.
        let frozen = freeze_current_operator_contract_set().unwrap();
        let cells = frozen.schedule.body["cells"].as_array().unwrap().clone();
        for cell in &cells {
            let task_def = frozen
                .corpus
                .tasks
                .iter()
                .find(|t| t.task_id == cell["task_id"].as_str().unwrap())
                .unwrap();
            let ids = cell_identities_for("run-armed", cell, task_def).unwrap();
            let outcome = cell_results
                .iter()
                .find(|c| c["cell_id"].as_str() == Some(cell["cell_id"].as_str().unwrap()))
                .unwrap();
            let class = outcome["classification"].as_str().unwrap_or("");
            if class == "skipped_by_stop_rule" {
                continue;
            }
            let product_task = store
                .get_product_task_by_idempotency(tenant, &ids.worktree_id, &ids.product_task_id)
                .unwrap()
                .unwrap();
            if class == "outcome_unknown" {
                assert_ne!(product_task["status"], "completed");
                let projection = store
                    .project_rwe_cell_store_evidence(
                        product_task["task_id"].as_str().unwrap(),
                        &ids.delegated_attempt_id,
                    )
                    .unwrap();
                assert_eq!(
                    projection["provider_execution"]["provider_request_count"],
                    0
                );
                assert!(projection.pointer("/output/draft_pr").is_none());
                continue;
            }
            assert_eq!(product_task["status"], "completed");
            let projection = store
                .project_rwe_cell_store_evidence(
                    product_task["task_id"].as_str().unwrap(),
                    &ids.delegated_attempt_id,
                )
                .unwrap();
            assert_eq!(
                projection["provider_execution"]["provider_request_count"],
                3
            );
            assert_eq!(
                projection["provider_execution"]["transport_provenance"],
                "injected"
            );
            let journal = projection["provider_request_journal"]
                .as_array()
                .unwrap()
                .clone();
            assert_eq!(journal.len(), 3);
            for entry in &journal {
                assert_eq!(entry["transport_provenance"], "injected");
                assert_eq!(entry["status"], "succeeded");
            }
            let te = projection["terminal_evidence"].clone();
            assert_eq!(te["task_status"], "completed");
            assert_eq!(te["verification"]["status"], "evidence_recorded");
            assert_eq!(te["verification"]["trustworthy"], true);
            assert!(te.pointer("/output/draft_pr").is_some());
            let ws_id = product_task["workspace_record_id"].as_str().unwrap();
            let ws = store
                .get_supervised_patch_workspace(ws_id)
                .unwrap()
                .unwrap();
            assert_eq!(ws["status"], "cleaned");
        }
    }
}
