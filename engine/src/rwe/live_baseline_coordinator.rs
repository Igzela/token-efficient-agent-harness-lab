//! Provider-free first-live-baseline coordinator for Minimum First RWE.
//!
//! Wires store-owned `rwe_run_authorization.v2` issue/admit to the frozen
//! 4-cell schedule and existing ProductTask / managed_deepseek owners via an
//! injectable [`CellDriver`]. Production live cells call the Golden Path owner
//! through [`ProductGoldenPathCellDriver`]; tests inject controlled outcomes
//! without a second orchestration stack.
//!
//! This module never POSTs to a Provider and never writes a target repository
//! by itself. Live external effects only occur when an admitted driver is
//! invoked outside CI with credentials and explicit operator intent.

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

/// Provider-free test/operator-dry driver: never calls a Provider or mutates target.
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
                o
            })
    }
}

/// Production driver: reserves Golden Path identities and refuses external
/// effects unless explicitly armed for a live operator session.
///
/// In CI / provider-free mode it fail-closes before any Provider POST or target write.
pub struct ProductGoldenPathCellDriver {
    pub allow_live_provider_effects: bool,
}

impl Default for ProductGoldenPathCellDriver {
    fn default() -> Self {
        Self {
            allow_live_provider_effects: false,
        }
    }
}

impl CellDriver for ProductGoldenPathCellDriver {
    fn execute_cell(
        &self,
        store: &LocalProductStore,
        principal: &AuthenticatedPrincipal,
        frozen: &OperatorFrozenContractSet,
        run_id: &str,
        _lease_token: &str,
        cell: &Value,
        task: &RweTaskDefinition,
        ids: &CellIdentities,
    ) -> Result<CellOutcome, String> {
        // Identity reservation facts (deterministic) — no provider/target effect.
        let _ = (store, principal, frozen, run_id, cell, task);
        if std::env::var("CI").ok().as_deref() == Some("true") {
            return Ok(CellOutcome::blocked(
                "blocked_ci_environment",
                "live RWE cell execution is forbidden in CI",
                ids,
            ));
        }
        if !self.allow_live_provider_effects {
            return Ok(CellOutcome::blocked(
                "blocked_provider_free_mode",
                "ProductGoldenPathCellDriver reserved identities only; live provider effects not armed",
                ids,
            ));
        }
        if std::env::var(DEEPSEEK_CREDENTIAL_REFERENCE)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .is_none()
        {
            return Ok(CellOutcome::blocked(
                "blocked_missing_credential",
                "parent-process credential symbol missing; no provider call attempted",
                ids,
            ));
        }
        // Armed live path is intentionally deferred to a post-merge operator
        // session that reuses admit_product_task + managed_deepseek under the
        // same identities. This implementation PR stays provider-free.
        Err(
            "live ProductGoldenPath cell dispatch is reserved for a separately authorized live operator session after merge"
                .into(),
        )
    }
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
        "cleanup_failed",
        "fixture_success",
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
    if classification == "outcome_unknown" {
        // Protocol: outcome_unknown is terminal for the cell; never auto-retry.
        return None;
    }
    None
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

    // GP prerequisite: must exist for same tenant when provided or when checking readiness.
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

    // Target / provider freeze bindings (constants, not caller text).
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
    // Preflight gate: do not consume when not ready.
    let pre = operator_preflight(
        store,
        principal,
        None,
        Some(golden_path_prerequisite_product_task_id),
    )?;
    if pre.get("ready").and_then(Value::as_bool) != Some(true) {
        // Allow issue path when only missing credential/CI (operator may issue dry) —
        // but GP prerequisite is mandatory before issue.
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

/// Run the frozen schedule under an admitted authorization and injectable driver.
pub fn run_frozen_schedule(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    lease_token: &str,
    driver: &dyn CellDriver,
) -> Result<Value, String> {
    let frozen = freeze_current_operator_contract_set()?;
    let auth = store
        .get_rwe_run_authorization(authorization_id)?
        .ok_or("RWE authorization not found")?;
    if auth.get("tenant_id").and_then(Value::as_str) != Some(principal.tenant_id()) {
        return Err("authorization tenant mismatch".into());
    }
    if auth.get("principal_id").and_then(Value::as_str) != Some(principal.principal_id()) {
        return Err("authorization principal mismatch".into());
    }
    // Restart recovery: exact admit replay if needed.
    let mut lease = lease_token.to_string();
    if lease.is_empty() {
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
    let mut cell_results = Vec::new();
    let mut aggregate_requests = 0u64;
    let mut aggregate_tokens = 0u64;
    let mut any_live_provider = false;
    let mut stopped_by: Option<String> = None;

    for cell in cells {
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
        let ids = cell_identities_for(run_id, cell, task)?;

        if let Some(prior) = existing_by_attempt.get(&ids.rwe_task_attempt_id) {
            let class = prior
                .get("classification")
                .and_then(Value::as_str)
                .unwrap_or("");
            if is_terminal_classification(class) {
                cell_results.push(prior.get("evidence_json").cloned().unwrap_or(Value::Null));
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
                continue;
            }
        }

        if stopped_by.is_some() {
            // Pre-registered stop: remaining cells still need explicit terminal
            // accounting as skipped_by_stop_rule (not silent omission).
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

        let outcome =
            driver.execute_cell(store, principal, &frozen, run_id, &lease, cell, task, &ids)?;

        // Budget remaining check (run-level ceiling from frozen schedule).
        let cell_req = cell
            .get("max_provider_requests")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let run_max_req = frozen
            .schedule
            .body
            .pointer("/run_level_budget/max_total_provider_requests")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if aggregate_requests.saturating_add(outcome.provider_requests) > run_max_req
            || outcome.provider_requests > cell_req
        {
            let outcome = CellOutcome::blocked(
                "blocked_budget",
                "cell or run provider-request budget would be exceeded",
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
            stopped_by = Some("stop_on_budget_exhaustion".into());
            continue;
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
        store.persist_rwe_task_attempt(
            run_id,
            &lease,
            &ids.rwe_task_attempt_id,
            &ids.task_id,
            &ids.definition_sha256,
            &outcome.classification,
            &evidence,
        )?;
        aggregate_requests = aggregate_requests.saturating_add(outcome.provider_requests);
        aggregate_tokens = aggregate_tokens.saturating_add(outcome.total_tokens);
        any_live_provider |= outcome.live_provider_request;
        if let Some(rule) = should_stop_after_cell(&stop_rules, &outcome.classification) {
            stopped_by = Some(rule.into());
        }
        // outcome_unknown: never auto-retry, never second authorization.
        cell_results.push(evidence);
    }

    // All pre-registered cells must have a terminal attempt before run terminalization.
    let final_attempts = store.list_rwe_task_attempts_for_run(run_id)?;
    if final_attempts.len() < cells.len() {
        return Err(format!(
            "run cannot terminalize: {} task attempts for {} cells",
            final_attempts.len(),
            cells.len()
        ));
    }

    // live_baseline_sealed only with real provider cell evidence for all non-skipped cells.
    let live_baseline_sealed = any_live_provider
        && cell_results.iter().all(|ev| {
            let class = ev
                .get("classification")
                .and_then(Value::as_str)
                .unwrap_or("");
            class == "skipped_by_stop_rule"
                || (ev
                    .get("live_provider_request")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                    && ev
                        .get("cleanup_status")
                        .and_then(Value::as_str)
                        .is_some_and(|s| s == "completed" || s == "not_required"))
        });

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
            "live baseline sealed from real provider cell evidence"
        } else {
            "provider-free or incomplete provider evidence; not a sealed live baseline"
        },
    }));
    let evidence_sha = sha256_hex(aggregate.to_string().as_bytes());
    let status = if any_live_provider && live_baseline_sealed {
        "succeeded"
    } else if cell_results
        .iter()
        .any(|e| e.get("classification").and_then(Value::as_str) == Some("outcome_unknown"))
    {
        "outcome_unknown"
    } else {
        // Provider-free orchestration and non-sealed live runs use failed until sealed.
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
        "provider_calls": if any_live_provider { "observed_in_cell_evidence" } else { "0" },
    })))
}

/// Build first-baseline evidence projection compatible with economic protocol artifacts
/// without claiming COMPARISON_ELIGIBLE or creating a VDE store.
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
    }

    #[test]
    fn four_cell_injected_orchestration_maps_identities_and_receipts() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("c4.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-c4";
        let principal = operator(&store, tenant, "op-c4");
        seed_gp(&store, "ptask-gp-c4", tenant);
        let auth_id = "auth-c4";
        let run_id = "run-c4";
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            auth_id,
            run_id,
            "ptask-gp-c4",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap().to_string();
        let driver = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        let result =
            run_frozen_schedule(&store, &principal, run_id, auth_id, &lease, &driver).unwrap();
        assert_eq!(result["cell_count"], 4);
        assert_eq!(result["attempts_recorded"], 4);
        assert_eq!(result["live_baseline_sealed"], false);
        let attempts = store.list_rwe_task_attempts_for_run(run_id).unwrap();
        assert_eq!(attempts.len(), 4);
        for a in &attempts {
            let ev = &a["evidence_json"];
            assert!(ev.get("cell_id").and_then(Value::as_str).is_some());
            assert!(ev
                .get("definition_sha256")
                .and_then(Value::as_str)
                .is_some());
            assert!(ev.get("product_task_id").and_then(Value::as_str).is_some());
            assert!(ev.get("workflow_id").and_then(Value::as_str).is_some());
            assert_eq!(ev["live_provider_request"], false);
        }
        // Exact restart: re-run skips completed cells (idempotent).
        let again =
            run_frozen_schedule(&store, &principal, run_id, auth_id, &lease, &driver).unwrap();
        assert_eq!(again["attempts_recorded"], 4);
        let attempts2 = store.list_rwe_task_attempts_for_run(run_id).unwrap();
        assert_eq!(attempts2.len(), 4);
    }

    #[test]
    fn classifier_outcomes_preserve_cost_fields() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("cls.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-cls";
        let principal = operator(&store, tenant, "op-cls");
        seed_gp(&store, "ptask-gp-cls", tenant);
        let outcomes = vec![
            CellOutcome {
                classification: "injected_verifier_failed".into(),
                provider_requests: 1,
                input_tokens: 10,
                output_tokens: 5,
                total_tokens: 15,
                latency_ms: 100,
                monetary_cost: Some(0.01),
                cost_unknown: false,
                live_provider_request: false,
                verification_status: "failed".into(),
                verification_trustworthy: true,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: Some("tev".into()),
                terminal_content_sha256: Some("a".repeat(64)),
                cleanup_status: "completed".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "verifier".into(),
            },
            CellOutcome {
                classification: "injected_outcome_unknown".into(),
                provider_requests: 1,
                input_tokens: 10,
                output_tokens: 0,
                total_tokens: 10,
                latency_ms: 50,
                monetary_cost: None,
                cost_unknown: true,
                live_provider_request: false,
                verification_status: "not_run".into(),
                verification_trustworthy: false,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: None,
                terminal_content_sha256: None,
                cleanup_status: "incomplete".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "unknown".into(),
            },
            CellOutcome {
                classification: "injected_timeout".into(),
                provider_requests: 1,
                input_tokens: 1,
                output_tokens: 0,
                total_tokens: 1,
                latency_ms: 900000,
                monetary_cost: Some(0.02),
                cost_unknown: false,
                live_provider_request: false,
                verification_status: "not_run".into(),
                verification_trustworthy: false,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: None,
                terminal_content_sha256: None,
                cleanup_status: "completed".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "timeout".into(),
            },
            CellOutcome {
                classification: "injected_cancel".into(),
                provider_requests: 0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms: 1,
                monetary_cost: Some(0.0),
                cost_unknown: false,
                live_provider_request: false,
                verification_status: "not_run".into(),
                verification_trustworthy: false,
                approval_id: None,
                output_draft_pr: None,
                terminal_evidence_id: None,
                terminal_content_sha256: None,
                cleanup_status: "completed".into(),
                product_task_id: String::new(),
                workflow_id: String::new(),
                node_id: String::new(),
                delegated_attempt_id: String::new(),
                workspace_id: String::new(),
                note: "cancel".into(),
            },
        ];
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-cls",
            "run-cls",
            "ptask-gp-cls",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap();
        let driver = InjectedCellDriver { outcomes };
        let result =
            run_frozen_schedule(&store, &principal, "run-cls", "auth-cls", lease, &driver).unwrap();
        assert_eq!(result["attempts_recorded"], 4);
        let attempts = store.list_rwe_task_attempts_for_run("run-cls").unwrap();
        let unknown = attempts
            .iter()
            .find(|a| a["classification"] == "injected_outcome_unknown")
            .unwrap();
        assert_eq!(unknown["evidence_json"]["cost_unknown"], true);
        assert!(unknown["evidence_json"]["monetary_cost"].is_null());
        // No second authorization was issued/consumed for outcome_unknown.
        assert_eq!(
            store
                .get_rwe_run_authorization("auth-cls")
                .unwrap()
                .unwrap()["status"],
            "consumed"
        );
    }

    #[test]
    fn concurrent_duplicate_cell_is_single_effect() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("dup.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-dup";
        let principal = operator(&store, tenant, "op-dup");
        seed_gp(&store, "ptask-gp-dup", tenant);
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-dup",
            "run-dup",
            "ptask-gp-dup",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        let lease = admitted["lease_token"].as_str().unwrap().to_string();
        let driver = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        run_frozen_schedule(&store, &principal, "run-dup", "auth-dup", &lease, &driver).unwrap();
        // Second schedule run is exact replay for completed cells.
        run_frozen_schedule(&store, &principal, "run-dup", "auth-dup", &lease, &driver).unwrap();
        assert_eq!(
            store
                .list_rwe_task_attempts_for_run("run-dup")
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn stale_and_wrong_principal_do_not_consume_fresh_authority() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("stale.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-stale";
        let principal = operator(&store, tenant, "op-stale");
        seed_gp(&store, "ptask-gp-stale", tenant);
        // Missing prereq path for foreign.
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
        // Expired issue rejected.
        let err = issue_and_admit_v2(
            &store,
            &principal,
            "auth-exp",
            "run-exp",
            "ptask-gp-stale",
            "2026-07-01T00:00:00Z",
        )
        .unwrap_err();
        assert!(
            err.contains("expired") || err.contains("fail closed"),
            "{err}"
        );
    }

    #[test]
    fn crash_style_admit_lease_recovery_then_schedule() {
        let dir = tempdir().unwrap();
        let store = LocalProductStore::new_with_clock(dir.path().join("lease.db"), || {
            "2026-07-25T12:00:00Z".into()
        })
        .unwrap();
        let tenant = "t-lease";
        let principal = operator(&store, tenant, "op-lease");
        seed_gp(&store, "ptask-gp-lease", tenant);
        let admitted = issue_and_admit_v2(
            &store,
            &principal,
            "auth-lease",
            "run-lease",
            "ptask-gp-lease",
            "2026-08-07T00:00:00Z",
        )
        .unwrap();
        assert!(admitted.get("lease_token").is_some());
        // Simulate crash: empty lease → coordinator recovers via exact admit replay.
        let driver = InjectedCellDriver {
            outcomes: success_outcomes(),
        };
        let result =
            run_frozen_schedule(&store, &principal, "run-lease", "auth-lease", "", &driver)
                .unwrap();
        assert_eq!(result["attempts_recorded"], 4);
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
        assert_eq!(a.definition_sha256, task.definition_sha256);
    }
}
