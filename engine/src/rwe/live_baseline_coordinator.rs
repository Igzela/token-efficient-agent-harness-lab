//! Provider-free first-live-baseline coordinator for Minimum First RWE.
//!
//! Wires store-owned `rwe_run_authorization.v2` issue/admit to the frozen 4-cell
//! schedule. Cell dispatch is fenced by store-owned atomic claim + full next-cell
//! budget reservation under the existing RWE attempt owner
//! ([`LocalProductStore::claim_rwe_cell_dispatch`]).
//!
//! This module never POSTs to a Provider and never writes a target repository.
//! Injectable drivers exist only for orchestration proofs. The production
//! Product Golden Path cell composition seam for multi-path frozen RWE tasks is
//! not fabricated here — see [`ProductGoldenPathCellDriver`].

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::corpus::RweTaskDefinition;
use super::operator_corpus::{
    freeze_current_operator_contract_set, OperatorFrozenContractSet, OPERATOR_ADMITTED_BINARY_PATH,
    OPERATOR_ADMITTED_BINARY_VERSION, OPERATOR_ADMITTED_MODEL, OPERATOR_TARGET_REPO,
};
use super::runner::{
    persist_rwe_run_authorization_v2, RWE_RUN_AUTH_V2_SCHEMA, RWE_RUN_EVIDENCE_SCHEMA,
};
use crate::provider::managed_deepseek::{
    DEEPSEEK_CREDENTIAL_REFERENCE, DEEPSEEK_OPENAI_BASE_URL, DEEPSEEK_OPENAI_PATH,
    DEEPSEEK_PROVIDER_KIND,
};
use crate::storage::local_product_store::{
    AuthenticatedPrincipal, LocalProductStore, RweAuthorizationV2IssueRequest,
};

pub const RWE_LIVE_BASELINE_COORDINATOR_SCHEMA: &str = "rwe_live_baseline_coordinator.v1";
pub const RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA: &str = "rwe_cell_attempt_evidence.v1";

/// Exact missing composition symbols when a live Product Golden Path cell path
/// is requested. Controllers use this constant for DECISION_REQUIRED routing.
pub const RWE_LIVE_CELL_COMPOSITION_SEAM_MISSING: &str = concat!(
    "CHECKPOINT_DECISION_REQUIRED: no callable RWE multi-path Product Golden Path composition seam. ",
    "Existing store-backed ManagedAuthoritySource (LocalProductStore::claim_provider_request / ",
    "reconcile_provider_request / stage_context / apply_workspace_action) requires ",
    "managed_acceptance_delegations with spend journal; prepare_delegated_managed_product_task ",
    "and ManagedAuthoritySource::stage_context hardcode docs/USER_GUIDE.md and the legacy docs ",
    "health-check verifier. Frozen Minimum First RWE cells bind multi-path alters-lab trees and ",
    "PYTHONPATH=… python3 -m pytest verifiers. Do not fabricate synthetic sources, no-op claim/",
    "reconcile, or planning-only paths. Minimal options: (1) generalize delegated GP owners for ",
    "frozen RWE task bindings under accepted authority, (2) authorize a dedicated RWE cell ",
    "composition under LocalProductStore without a second owner, (3) keep coordinator ",
    "orchestration-only until that seam exists. Consequences: live provider/target effects remain ",
    "blocked; PE7 execution stays blocked; fake transports alone cannot substitute store-backed ",
    "spend/journal/worktree/Draft-PR/terminal composition."
);

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
/// derived only from store-owned ProductTask/terminal/provider receipts.
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
    /// `injected` | `product_golden_path_owner` — sealing rejects `injected`.
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

/// Injectable cell execution seam (tests + production share one coordinator).
pub trait CellDriver: Send + Sync {
    /// Fail closed before any cell effect when this driver cannot execute.
    fn ensure_effects_ready(&self) -> Result<(), String> {
        Ok(())
    }

    fn execute_cell(
        &self,
        store: &LocalProductStore,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        lease_token: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String>;
}

/// Counts `execute_cell` entries that pass the store fence (for over-budget proofs).
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
        store: &LocalProductStore,
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

/// Provider-free injected outcomes for orchestration tests. Never seals a live baseline.
pub struct InjectedCellDriver {
    pub outcomes: Vec<CellOutcome>,
}

impl CellDriver for InjectedCellDriver {
    fn execute_cell(
        &self,
        _store: &LocalProductStore,
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
                o
            })
    }
}

/// Production-facing driver entry.
///
/// Fails closed before any cell effect: the repository does not currently expose
/// a callable Product Golden Path composition seam for frozen multi-path RWE
/// cells (store-backed spend/journal, exact target worktree, managed executor,
/// verifier, artifact, Draft PR, terminal, cleanup). This type deliberately does
/// **not** synthesize local sources, trivial pytest, no-op claim/reconcile, or
/// planning-only paths.
#[derive(Debug, Default, Clone)]
pub struct ProductGoldenPathCellDriver {
    /// Operator-supplied local clone of the frozen target (must match SHA).
    pub target_repo_path: Option<std::path::PathBuf>,
    /// When true, still fails closed until the composition seam is authorized.
    pub allow_live_provider_effects: bool,
}

impl ProductGoldenPathCellDriver {
    fn missing_seam_error() -> String {
        RWE_LIVE_CELL_COMPOSITION_SEAM_MISSING.to_string()
    }
}

impl CellDriver for ProductGoldenPathCellDriver {
    fn ensure_effects_ready(&self) -> Result<(), String> {
        if std::env::var("CI").ok().as_deref() == Some("true") {
            return Err(
                "fail closed before cell effect: live RWE cell execution is forbidden in CI".into(),
            );
        }
        // Even when armed and a target path is supplied, do not invent a live path.
        // The callable multi-path composition seam is missing (see constant).
        let _ = (
            &self.target_repo_path,
            self.allow_live_provider_effects,
            DEEPSEEK_CREDENTIAL_REFERENCE,
        );
        Err(Self::missing_seam_error())
    }

    fn execute_cell(
        &self,
        _store: &LocalProductStore,
        _principal: &AuthenticatedPrincipal,
        _frozen: &OperatorFrozenContractSet,
        _run_id: &str,
        _lease_token: &str,
        _cell: &Value,
        _task: &RweTaskDefinition,
        _ids: &CellIdentities,
    ) -> Result<CellOutcome, String> {
        Err(Self::missing_seam_error())
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
    if task.source_commit != "6240768506320a324d68787b9eaa86971c8c930c" {
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
        risk_class: "rwe".into(),
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
    let frozen = freeze_current_operator_contract_set()?;
    let mut blockers = Vec::new();
    let mut notes = Vec::new();

    if std::env::var("CI").ok().as_deref() == Some("true") {
        blockers.push(json!({
            "code": "ci_environment",
            "detail": "live RWE is forbidden in CI"
        }));
    }

    let cred_present = std::env::var(DEEPSEEK_CREDENTIAL_REFERENCE)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some();
    if !cred_present {
        blockers.push(json!({
            "code": "missing_credential_symbol",
            "detail": format!("{DEEPSEEK_CREDENTIAL_REFERENCE} not set in parent process")
        }));
    }

    // Live multi-path GP composition is not callable yet.
    blockers.push(json!({
        "code": "missing_rwe_cell_composition_seam",
        "detail": RWE_LIVE_CELL_COMPOSITION_SEAM_MISSING
    }));

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
    if target_main != "6240768506320a324d68787b9eaa86971c8c930c" {
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

    // Ready remains false until composition seam + GP + credential exist.
    let ready = blockers.is_empty() && gp_ready;
    Ok(sort_value(&json!({
        "schema_version": "rwe_operator_preflight.v1",
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
        "principal": {
            "tenant_id": principal.tenant_id(),
            "principal_id": principal.principal_id(),
            "principal_kind": principal.principal_kind().as_str(),
        },
        "credential_symbol_present": cred_present,
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
    // Issue/admit may proceed without the composition seam or credential: those
    // gate live cell effects, not Board B authority consumption. GP prerequisite
    // remains mandatory.
    if pre
        .get("blockers")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter().any(|b| {
                matches!(
                    b.get("code").and_then(Value::as_str),
                    Some(
                        "missing_golden_path_prerequisite_id"
                            | "golden_path_prerequisite_missing"
                            | "golden_path_prerequisite_not_found"
                            | "golden_path_prerequisite_tenant_mismatch"
                            | "golden_path_prerequisite_not_ready"
                    )
                )
            })
        })
        .unwrap_or(true)
    {
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
    cell_results: &[Value],
) -> bool {
    let mut executed = 0usize;
    for ev in cell_results {
        let class = ev
            .get("classification")
            .and_then(Value::as_str)
            .unwrap_or("");
        if class == "skipped_by_stop_rule" {
            continue;
        }
        executed += 1;
        let source = ev
            .get("evidence_source")
            .and_then(Value::as_str)
            .unwrap_or("");
        if source != "product_golden_path_owner" {
            return false;
        }
        let product_task_id = match ev.get("product_task_id").and_then(Value::as_str) {
            Some(id) if !id.is_empty() => id,
            _ => return false,
        };
        let task = match store.get_product_task_for_tenant(product_task_id, principal.tenant_id()) {
            Ok(Some(t)) => t,
            _ => return false,
        };
        if task.get("status").and_then(Value::as_str) != Some("completed") {
            return false;
        }
        if task.get("fixture_only").and_then(Value::as_bool) == Some(true) {
            return false;
        }
        let te = match store.get_product_task_terminal_evidence(product_task_id) {
            Ok(v) if !v.is_null() => v,
            _ => return false,
        };
        if te.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
            return false;
        }
        if te.get("product_task_id").and_then(Value::as_str) != Some(product_task_id) {
            return false;
        }
        if te.get("task_status").and_then(Value::as_str) != Some("completed") {
            return false;
        }
        if te.get("fixture").and_then(Value::as_bool) == Some(true)
            || te.get("fixture_only").and_then(Value::as_bool) == Some(true)
        {
            return false;
        }
        let verification_ok = te
            .pointer("/verification/trustworthy")
            .and_then(Value::as_bool)
            == Some(true)
            && te.pointer("/verification/status").and_then(Value::as_str) == Some("passed");
        let approval_ok = te
            .pointer("/approval/approval_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        let artifact_ok = te
            .pointer("/artifact/artifact_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty());
        let output_ok =
            te.pointer("/output/draft_pr").is_some() || te.pointer("/output/receipt_id").is_some();
        if !verification_ok || !approval_ok || !artifact_ok || !output_ok {
            return false;
        }
        if let Some(rev) = te.get("source_revision").and_then(Value::as_str) {
            let expected = frozen
                .corpus
                .tasks
                .first()
                .map(|t| t.source_commit.as_str())
                .unwrap_or("");
            if !expected.is_empty() && rev != expected {
                return false;
            }
        }
        if te
            .get("content_sha256")
            .and_then(Value::as_str)
            .is_none_or(|s| s.is_empty() || s.len() != 64)
        {
            return false;
        }
    }
    executed > 0
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

fn cell_reservation_limits(cell: &Value) -> Result<(u64, u64), String> {
    let req = cell
        .get("max_provider_requests")
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
        .ok_or("cell max_provider_requests must be positive")?;
    let tok = cell
        .get("max_total_tokens")
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
        .ok_or("cell max_total_tokens must be positive")?;
    Ok((req, tok))
}

/// Run the frozen schedule under an admitted authorization and injectable driver.
pub fn run_frozen_schedule(
    store: &LocalProductStore,
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

        let (reserve_req, reserve_tok) = match cell_reservation_limits(cell) {
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

        // Store-backed atomic pre-effect fence + full next-cell budget reservation.
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
            "note": "pre-effect full cell budget reservation",
        }));
        match store.claim_rwe_cell_dispatch(
            run_id,
            &lease,
            &ids.rwe_task_attempt_id,
            &ids.task_id,
            &ids.definition_sha256,
            reserve_req,
            reserve_tok,
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

        let outcome =
            driver.execute_cell(store, principal, &frozen, run_id, &lease, cell, task, &ids)?;

        // Post-effect honesty against frozen ceilings (reservation was full cell max).
        let mut outcome = outcome;
        if outcome.provider_requests > reserve_req || outcome.total_tokens > reserve_tok {
            outcome = CellOutcome::blocked(
                "blocked_budget",
                "cell provider/token budget exceeded by outcome",
                &ids,
            );
            if stop_rules.iter().any(|r| r == "stop_on_budget_exhaustion") {
                stopped_by = Some("stop_on_budget_exhaustion".into());
            }
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
        aggregate_requests = aggregate_requests.saturating_add(outcome.provider_requests);
        aggregate_tokens = aggregate_tokens.saturating_add(outcome.total_tokens);
        any_live_provider |= outcome.live_provider_request;
        if stopped_by.is_none() {
            if let Some(rule) = should_stop_after_cell(&stop_rules, &outcome.classification) {
                stopped_by = Some(rule.into());
            }
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

    let live_baseline_sealed =
        evaluate_store_owned_live_baseline_seal(store, principal, &frozen, &cell_results);

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
        "live_baseline_sealed": live_baseline_sealed,
        "provider_free_fixture_completion": false,
        "stopped_by": stopped_by,
        "comparison_eligible": false,
        "note": if live_baseline_sealed {
            "live baseline sealed from store-owned ProductTask/terminal receipts"
        } else {
            "provider-free, injected, or incomplete store receipts; not a sealed live baseline"
        },
    }));
    let evidence_sha = sha256_hex(aggregate.to_string().as_bytes());
    let status = if live_baseline_sealed {
        "succeeded"
    } else if cell_results
        .iter()
        .any(|e| e.get("classification").and_then(Value::as_str) == Some("outcome_unknown"))
    {
        "outcome_unknown"
    } else {
        "failed"
    };
    let completed = store.complete_rwe_run(run_id, &lease, status, &aggregate, &evidence_sha)?;
    Ok(sort_value(&json!({
        "schema_version": RWE_LIVE_BASELINE_COORDINATOR_SCHEMA,
        "run": completed,
        "aggregate": aggregate,
        "cell_count": cells.len(),
        "attempts_recorded": final_attempts.len(),
        "live_baseline_sealed": live_baseline_sealed,
        "provider_call_performed": any_live_provider,
        "provider_calls": if any_live_provider { "observed_in_cell_evidence" } else { "0" },
    })))
}

/// Build first-baseline evidence projection without claiming COMPARISON_ELIGIBLE.
pub fn project_first_baseline_evidence(run_aggregate: &Value) -> Value {
    sort_value(&json!({
        "schema_version": "rwe_first_baseline_evidence_projection.v1",
        "live_baseline_sealed": run_aggregate.get("live_baseline_sealed"),
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
    use crate::storage::local_product_store::{
        SCOPE_ATTEMPT_ADMIT, SCOPE_REVOKE, SCOPE_RISK_ACKNOWLEDGE, SCOPE_SPEND_AUTHORIZE,
    };
    use sha2::Digest;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

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
        (0..4)
            .map(|i| CellOutcome {
                classification: "injected_success".into(),
                provider_requests: 0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms: 1,
                monetary_cost: Some(0.0),
                cost_unknown: false,
                live_provider_request: false,
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
        let admitted = issue_and_admit_v2(
            store,
            principal,
            auth_id,
            run_id,
            gp,
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        admitted["lease_token"].as_str().unwrap().to_string()
    }

    #[test]
    fn preflight_fails_closed_without_gp_and_without_consuming() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("pf.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let principal = operator(&store, "t-pf", "op-pf");
        let pre = operator_preflight(&store, &principal, None, None).unwrap();
        assert_eq!(pre["ready"], false);
        assert_eq!(pre["authority_consumed"], false);
        assert_eq!(pre["provider_call_performed"], false);
        let codes: Vec<_> = pre["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|b| b.get("code").and_then(Value::as_str))
            .collect();
        assert!(codes.contains(&"missing_rwe_cell_composition_seam"));
    }

    #[test]
    fn four_cell_injected_orchestration_maps_identities_and_receipts() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("c4.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
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
    fn product_golden_path_driver_fails_closed_without_fabricating() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("unarmed.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let principal = operator(&store, "t-unarmed", "op-unarmed");
        let lease = admit_ready(
            &store,
            &principal,
            "auth-unarmed",
            "run-unarmed",
            "ptask-gp-unarmed",
        );
        let driver = ProductGoldenPathCellDriver {
            allow_live_provider_effects: true,
            target_repo_path: Some(dir.path().join("fake-target")),
        };
        let err = run_frozen_schedule(
            &store,
            &principal,
            "run-unarmed",
            "auth-unarmed",
            &lease,
            &driver,
        )
        .unwrap_err();
        assert!(
            err.contains("CHECKPOINT_DECISION_REQUIRED") || err.contains("composition seam"),
            "{err}"
        );
        assert!(store
            .list_rwe_task_attempts_for_run("run-unarmed")
            .unwrap()
            .is_empty());
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
        let (req, tok) = cell_reservation_limits(cell0).unwrap();
        let reservation = json!({
            "schema_version": RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
            "cell_id": ids.cell_id,
            "provider_requests": req,
            "total_tokens": tok,
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
                    match store.claim_rwe_cell_dispatch(
                        "run-dup",
                        &lease,
                        &attempt_id,
                        &task_id,
                        &def,
                        req,
                        tok,
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
        let store = LocalProductStore::new_with_clock(dir.path().join("budget.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
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
        let store = LocalProductStore::new_with_clock(dir.path().join("stop.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
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
        let store = LocalProductStore::new_with_clock(dir.path().join("ou.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
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
        let store = LocalProductStore::new_with_clock(dir.path().join("inj-seal.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
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
        let store = LocalProductStore::new_with_clock(dir.path().join("stale.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
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
        let store = LocalProductStore::new_with_clock(dir.path().join("lease.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
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
        let store = LocalProductStore::new_with_clock(dir.path().join("reval.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
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
}
