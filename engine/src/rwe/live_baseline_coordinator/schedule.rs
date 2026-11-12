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
use super::worker::*;
use super::{
    CODEX_PREREQUISITE_DEFAULT_MAX_RECOVERY_GENERATIONS, CODEX_PREREQUISITE_FALLBACK_TASK_ID,
    CODEX_PREREQUISITE_PRIMARY_TASK_ID, LIVE_LEDGER_MAX_PROMPT_BYTES,
    LIVE_LEDGER_MAX_VERIFIER_OUTPUT_BYTES, RWE_CELL_ATTEMPT_EVIDENCE_SCHEMA,
    RWE_CODEX_PREREQUISITE_MAX_RECOVERY_GENERATIONS_TOKEN, RWE_LEDGER_CELL_COMPOSITION_SEAM,
    RWE_LEDGER_MANAGER_SEMANTICS, RWE_LIVE_BASELINE_COORDINATOR_SCHEMA,
    RWE_LIVE_CELL_COMPOSITION_SEAM, RWE_LIVE_LEDGER_CELL_COMPOSITION, RWE_OPERATOR_LIVE_RUN_TOKEN,
};

pub(super) fn codex_prerequisite_recovery_available(recovery_generation: u8) -> bool {
    let limit = std::env::var(RWE_CODEX_PREREQUISITE_MAX_RECOVERY_GENERATIONS_TOKEN)
        .ok()
        .and_then(|val| val.parse::<u8>().ok())
        .unwrap_or(CODEX_PREREQUISITE_DEFAULT_MAX_RECOVERY_GENERATIONS);
    recovery_generation < limit
}

pub(super) fn recovered_failure_terminal_receipt(
    durable_terminal: Option<Value>,
    finalized: &Value,
) -> Option<Value> {
    durable_terminal.or_else(|| {
        finalized
            .pointer("/delegated_terminal/failure_terminal_evidence")
            .cloned()
    })
}

pub(super) fn sort_value(value: &Value) -> Value {
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

pub(super) fn codex_prerequisite_candidate_indices(
    frozen: &OperatorFrozenContractSet,
) -> Result<Vec<usize>, String> {
    let primary_index = frozen
        .corpus
        .tasks
        .iter()
        .position(|task| task.task_id == CODEX_PREREQUISITE_PRIMARY_TASK_ID)
        .ok_or("frozen RWE corpus is missing the Codex prerequisite primary task")?;
    let mut candidates = vec![primary_index];
    if let Some(fallback_index) = frozen
        .corpus
        .tasks
        .iter()
        .position(|task| task.task_id == CODEX_PREREQUISITE_FALLBACK_TASK_ID)
    {
        if fallback_index != primary_index {
            candidates.push(fallback_index);
        }
    }
    Ok(candidates)
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn terminal_classifications() -> &'static [&'static str] {
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

pub(super) fn is_terminal_classification(c: &str) -> bool {
    terminal_classifications().contains(&c)
}

/// Honest run terminal status. `succeeded` is reachable only through a sealed
/// live baseline or a fixture where EVERY required cell classifies as
/// `fixture_success`. Merely terminalizing (controlled_failure, verifier_failed,
/// timeout, cancelled, blocked_*, cleanup_failed, injected_* classes) is a
/// completed-but-failed run; `outcome_unknown` stays outcome_unknown.
pub(super) fn run_terminal_status(
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

pub(super) fn stop_rules_from_schedule(frozen: &OperatorFrozenContractSet) -> Vec<String> {
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

pub(super) fn should_stop_after_cell(
    stop_rules: &[String],
    classification: &str,
) -> Option<&'static str> {
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
    let package = body
        .get("campaign_package_id")
        .and_then(Value::as_str)
        .map(crate::rwe::campaign_package::resolve_frozen_campaign_package)
        .transpose()?
        .unwrap_or(crate::rwe::campaign_package::canonical_deepseek_v2_package()?);
    package.validate()?;
    let provider_binding = package
        .provider_execution_binding
        .clone()
        .ok_or("stored v2 authorization package lacks a provider binding")?;
    provider_binding.validate()?;
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
            Some(provider_binding.provider_kind.as_str()),
        ),
        (
            "provider_base_url",
            body.get("provider_base_url").and_then(Value::as_str),
            Some(provider_binding.base_url.as_str()),
        ),
        (
            "provider_path",
            body.get("provider_path").and_then(Value::as_str),
            Some(provider_binding.endpoint_path.as_str()),
        ),
        (
            "model_identity",
            body.get("model_identity").and_then(Value::as_str),
            Some(package.admitted_model.as_str()),
        ),
        (
            "binary_path",
            body.get("binary_path").and_then(Value::as_str),
            Some(package.admitted_binary_path.as_str()),
        ),
        (
            "binary_version",
            body.get("binary_version").and_then(Value::as_str),
            Some(package.admitted_binary_version.as_str()),
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
    let schedule_expansion = package.matrix_schedule_expansion_factor();
    let run_max_req = frozen
        .schedule
        .body
        .pointer("/run_level_budget/max_total_provider_requests")
        .and_then(Value::as_u64)
        .and_then(|value| value.checked_mul(schedule_expansion));
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
        .and_then(Value::as_u64)
        .and_then(|value| value.checked_mul(schedule_expansion));
    if body.get("max_total_tokens").and_then(Value::as_u64) != run_max_tok {
        return Err("stored v2 authorization max_total_tokens mismatch".into());
    }
    let run_max_wall_ms = frozen
        .schedule
        .body
        .pointer("/run_level_budget/max_wall_time_ms")
        .and_then(Value::as_u64)
        .and_then(|value| value.checked_mul(schedule_expansion));
    if body.get("max_wall_time_ms").and_then(Value::as_u64) != run_max_wall_ms {
        return Err("stored v2 authorization max_wall_time_ms mismatch".into());
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

pub(super) fn operator_preflight_with_credential_readiness(
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
    issue_and_admit_v2_with_package(
        store,
        principal,
        authorization_id,
        run_id,
        golden_path_prerequisite_product_task_id,
        expires_at,
        None,
    )
}

/// Issue/admit the existing v2 run envelope for one frozen campaign package.
/// The package argument only selects an already-registered binding; all
/// authorization, budget, corpus, and prerequisite state remains Store-owned.
pub fn issue_and_admit_v2_with_package(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    authorization_id: &str,
    run_id: &str,
    golden_path_prerequisite_product_task_id: &str,
    expires_at: &str,
    campaign_package_id: Option<&str>,
) -> Result<Value, String> {
    if let Some(package_id) = campaign_package_id {
        let package = crate::rwe::campaign_package::resolve_frozen_campaign_package(package_id)?;
        package.validate()?;
        if package.provider_kind
            == crate::cli::codex_budget_authority::CODEX_PROVIDER_KIND_CHATGPT_SUBSCRIPTION
        {
            if std::env::var("CI").ok().as_deref() == Some("true") {
                return Err("live RWE is forbidden in CI".into());
            }
            if std::env::var(RWE_OPERATOR_LIVE_RUN_TOKEN).ok().as_deref() != Some("1") {
                return Err(format!(
                    "live RWE requires the operator live-run token {RWE_OPERATOR_LIVE_RUN_TOKEN}=1"
                ));
            }
            if CodexUpstreamAuth::resolve_parent_chatgpt_subscription().is_none() {
                return Err("ChatGPT subscription session is unavailable".into());
            }
            rwe_composition_seam_ready()?;
            let prereq = store
                .get_product_task_for_tenant(
                    golden_path_prerequisite_product_task_id,
                    principal.tenant_id(),
                )?
                .ok_or("Golden Path prerequisite is missing for the authenticated tenant")?;
            let _ = prereq;
            let evidence = store
                .get_product_task_terminal_evidence(golden_path_prerequisite_product_task_id)?;
            if evidence.get("task_status").and_then(Value::as_str) != Some("completed") {
                return Err("Golden Path prerequisite lacks a completed terminal seal".into());
            }
        }
    } else {
        let pre = operator_preflight(
            store,
            principal,
            None,
            Some(golden_path_prerequisite_product_task_id),
        )?;
        if pre.get("ready").and_then(Value::as_bool) != Some(true) {
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
            campaign_package_id: campaign_package_id.map(str::to_string),
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

pub(super) fn build_cell_evidence(
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
            "model": cell
                .get("matrix_model_id")
                .or_else(|| cell.get("model_identity"))
                .cloned()
                .unwrap_or_else(|| json!(OPERATOR_ADMITTED_MODEL)),
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

pub(super) fn evaluate_store_owned_live_baseline_seal(
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
    evaluate_store_owned_live_baseline_seal_for_cells(
        store,
        principal,
        frozen,
        run_id,
        stopped_by,
        cells,
        cell_results,
    )
}

pub(super) fn evaluate_store_owned_live_baseline_seal_for_cells(
    store: &LocalProductStore,
    principal: &AuthenticatedPrincipal,
    frozen: &OperatorFrozenContractSet,
    run_id: &str,
    stopped_by: &Option<String>,
    cells: &[Value],
    cell_results: &[Value],
) -> bool {
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
pub(super) fn changed_file_is_evaluator_surface(changed: &str) -> bool {
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
pub(super) fn classify_execution_error(err: &str) -> &'static str {
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

pub(super) fn mark_cleanup_failed(outcome: &mut CellOutcome, detail: impl Into<String>) {
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
pub(super) fn validate_prerequisite_provider_identity(
    projection: &Value,
    manifest_identity: &Value,
    expected_binding: &crate::rwe::campaign_package::FrozenProviderExecutionBinding,
    expected_package_id: &str,
    product_task_id: &str,
    attempt_id: &str,
) -> Result<(), String> {
    let persisted_binding = manifest_identity
        .get("provider_execution_binding")
        .ok_or_else(|| {
            "completed prerequisite manifest lacks provider execution binding".to_string()
        })
        .and_then(crate::rwe::campaign_package::FrozenProviderExecutionBinding::from_json)?;
    if &persisted_binding != expected_binding
        || manifest_identity
            .get("campaign_package_id")
            .and_then(Value::as_str)
            != Some(expected_package_id)
        || manifest_identity
            .pointer("/target/main_sha")
            .and_then(Value::as_str)
            != Some(FROZEN_RWE_TARGET_MAIN_SHA)
        || manifest_identity
            .pointer("/execution/product_task_id")
            .and_then(Value::as_str)
            != Some(product_task_id)
        || manifest_identity
            .pointer("/execution/attempt_id")
            .and_then(Value::as_str)
            != Some(attempt_id)
        || manifest_identity.get("models")
            != Some(&json!({
                "planner": expected_binding.admitted_model,
                "implementer": expected_binding.admitted_model,
                "reviewer": expected_binding.admitted_model,
            }))
        || manifest_identity.get("output")
            != Some(&json!({
                "draft_pr_only": true,
                "target_main_write": false,
                "merge": false,
                "auto_merge": false,
            }))
    {
        return Err(
            "completed prerequisite manifest does not match the exact frozen Codex binding".into(),
        );
    }
    let provider_execution = projection
        .get("provider_execution")
        .ok_or("completed prerequisite provider execution evidence is missing")?;
    if provider_execution
        .get("schema_version")
        .and_then(Value::as_str)
        != Some("managed_deepseek_execution_evidence.v1")
        || provider_execution
            .get("provider_identity")
            .and_then(Value::as_str)
            != Some(expected_binding.provider_identity.as_str())
        || provider_execution
            .get("provider_kind")
            .and_then(Value::as_str)
            != Some(expected_binding.provider_kind.as_str())
    {
        return Err("completed prerequisite provider execution identity is not exact".into());
    }
    let requests = projection
        .pointer("/provider_execution/requests")
        .and_then(Value::as_array)
        .filter(|requests| !requests.is_empty())
        .ok_or("completed prerequisite lacks confirmed provider requests")?;
    if requests.len() != 3 {
        return Err("completed prerequisite provider request route is not exact".into());
    }
    for request in requests {
        if request.get("provider_kind").and_then(Value::as_str)
            != Some(expected_binding.provider_kind.as_str())
            || !matches!(
                request.get("protocol").and_then(Value::as_str),
                Some("openai_compatible" | "open_ai_compatible")
            )
            || request.get("requested_model").and_then(Value::as_str)
                != Some(expected_binding.admitted_model.as_str())
            || request.get("resolved_model").and_then(Value::as_str)
                != Some(expected_binding.admitted_model.as_str())
            || request.pointer("/usage/model").and_then(Value::as_str)
                != Some(expected_binding.admitted_model.as_str())
        {
            return Err(
                "completed prerequisite provider route or model differs from selected freeze"
                    .into(),
            );
        }
    }
    Ok(())
}

pub(super) fn store_evidence_transport_provenance(projection: &Value) -> Result<String, String> {
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
pub(super) fn provider_execution_from_journal(projection: &Value) -> Option<Value> {
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
pub(super) fn couple_usage_to_store_journal(store: &LocalProductStore, outcome: &mut CellOutcome) {
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

pub(super) fn reconstruct_stopped_by(existing: &[Value], stop_rules: &[String]) -> Option<String> {
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

pub(super) fn cell_reservation_limits(cell: &Value) -> Result<RweCellBudgetEnvelope, String> {
    RweCellBudgetEnvelope::from_schedule_cell(cell)
}

pub(super) fn frozen_codex_mx1_cells(
    frozen: &OperatorFrozenContractSet,
    rung: crate::harness_evolution::Mx1MatrixRung,
) -> Result<Vec<Value>, String> {
    let base_cells = frozen
        .schedule
        .body
        .get("cells")
        .and_then(Value::as_array)
        .filter(|cells| !cells.is_empty())
        .ok_or("frozen schedule has no budget cells for MX1")?;
    let manifest =
        crate::harness_evolution::sample_mx1_descriptor_manifest_with_codex_subscription_models();
    crate::harness_evolution::validate_mx1_descriptor_manifest(&manifest)
        .map_err(|error| format!("Codex MX1 manifest is invalid: {}", error.message))?;
    let factor_cells_per_schedule_cell = match rung {
        crate::harness_evolution::Mx1MatrixRung::OneByOneByOne => 1,
        crate::harness_evolution::Mx1MatrixRung::OneByOneByThree => 3,
        crate::harness_evolution::Mx1MatrixRung::OneByTwoByOne => 2,
        _ => {
            return Err(format!(
                "Codex operator runner does not admit MX1 rung {}",
                rung.as_str()
            ))
        }
    };
    let comparison = current_comparison_manifest()?.to_json();
    let mut cells = Vec::with_capacity(base_cells.len() * factor_cells_per_schedule_cell);
    for base_cell in base_cells {
        let task_id = base_cell
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or("MX1 schedule cell task identity is missing")?;
        let task = frozen
            .corpus
            .tasks
            .iter()
            .find(|task| task.task_id == task_id)
            .ok_or("MX1 schedule cell task is absent from the frozen corpus")?;
        let repetition = base_cell
            .get("repetition")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0)
            .ok_or("MX1 schedule cell repetition is invalid")?;
        let common_basis = sort_value(&json!({
            "schema_version": "mx1_common_basis.v1",
            "corpus_sha256": frozen.corpus.corpus_sha256,
            "protocol_sha256": frozen.protocol.body_sha256,
            "schedule_sha256": frozen.schedule.schedule_sha256,
            "task_id": task.task_id,
            "repetition": repetition,
            "seed": base_cell.get("seed"),
            "budget_point_id": base_cell.get("budget_point_id"),
            "verifier": crate::rwe::frozen_rwe_bindings::FROZEN_RWE_VERIFIER_IDENTITY,
            "allowed_paths": task.allowed_mutable_paths,
            "comparison": comparison,
        }));
        let common_basis_sha256 = sha256_hex(common_basis.to_string().as_bytes());
        let plan = crate::harness_evolution::build_mx1_matrix_plan(
            &manifest,
            rung,
            task_id,
            repetition,
            &common_basis_sha256,
        )
        .map_err(|error| format!("Codex MX1 plan construction failed: {}", error.message))?;
        crate::harness_evolution::validate_mx1_matrix_plan(&manifest, &plan)
            .map_err(|error| format!("Codex MX1 plan validation failed: {}", error.message))?;
        if plan.cells.len() != factor_cells_per_schedule_cell {
            return Err(format!(
                "Codex MX1 {} factor plan must contain {} cells, got {}",
                rung.as_str(),
                factor_cells_per_schedule_cell,
                plan.cells.len()
            ));
        }
        for matrix_cell in &plan.cells {
            if matrix_cell.disposition
                != crate::harness_evolution::Mx1MatrixCellDisposition::Admitted
            {
                return Err(format!(
                    "Codex MX1 {} selected an incomparable cell: {}",
                    rung.as_str(),
                    matrix_cell.cell_id
                ));
            }
            let mut cell = base_cell.clone();
            cell["cell_id"] = json!(matrix_cell.cell_id);
            cell["task_id"] = json!(matrix_cell.identity.task_id);
            cell["harness_id"] = json!(matrix_cell.identity.harness_id);
            cell["strategy_id"] = json!(matrix_cell.identity.strategy_id);
            cell["matrix_plan_id"] = json!(plan.plan_id);
            cell["matrix_manifest_sha256"] = json!(plan.manifest_sha256);
            cell["matrix_common_basis_sha256"] = json!(plan.common_basis_sha256);
            cell["matrix_rung"] = json!(plan.rung.as_str());
            cell["matrix_repetition"] = json!(plan.repetition);
            cell["matrix_cell_descriptor_sha256"] = json!(matrix_cell.descriptor_digest);
            cell["matrix_model_id"] = json!(matrix_cell.identity.model_id);
            cell["matrix_strategy_id"] = json!(matrix_cell.identity.strategy_id);
            cell["sequential_order"] = json!((cells.len() + 1) as u64);
            cells.push(cell);
        }
    }
    let expected_cells = base_cells.len() * factor_cells_per_schedule_cell;
    if cells.len() != expected_cells {
        return Err(format!(
            "Codex MX1 {} must materialize exactly {} cells, got {}",
            rung.as_str(),
            expected_cells,
            cells.len()
        ));
    }
    Ok(cells)
}

pub(super) fn authorization_campaign_package_id(auth: &Value) -> Result<&str, String> {
    auth.get("body_json")
        .and_then(|body| body.get("campaign_package_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("stored RWE authorization lacks a campaign package identity".into())
}

pub(super) fn required_campaign_package_id_for_rung(
    rung: Option<crate::harness_evolution::Mx1MatrixRung>,
) -> Result<&'static str, String> {
    match rung {
        None => Ok(crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID),
        Some(crate::harness_evolution::Mx1MatrixRung::OneByTwoByOne) => {
            Ok(crate::rwe::campaign_package::RWE_CODEX_SUBSCRIPTION_V1_PACKAGE_ID)
        }
        Some(crate::harness_evolution::Mx1MatrixRung::OneByOneByOne) => {
            Ok(crate::rwe::campaign_package::RWE_CODEX_LUNA_XHIGH_V2_PACKAGE_ID)
        }
        Some(crate::harness_evolution::Mx1MatrixRung::OneByOneByThree) => {
            Ok(crate::rwe::campaign_package::RWE_CODEX_LUNA_XHIGH_V3_STRATEGY_PACKAGE_ID)
        }
        Some(other) => Err(format!(
            "operator runner does not admit MX1 rung {}",
            other.as_str()
        )),
    }
}

pub(super) fn require_driver_campaign_package(
    driver: &dyn CellDriver,
    expected_package_id: &str,
) -> Result<crate::rwe::campaign_package::FrozenCampaignPackage, String> {
    let expected =
        crate::rwe::campaign_package::resolve_frozen_campaign_package(expected_package_id)?;
    let observed = match driver.campaign_package() {
        Some(package) => package.clone(),
        None if expected_package_id == crate::rwe::campaign_package::RWE_DEEPSEEK_V2_PACKAGE_ID => {
            crate::rwe::campaign_package::canonical_deepseek_v2_package()?
        }
        None => {
            return Err(format!(
                "campaign package {expected_package_id} requires an exact driver package binding"
            ))
        }
    };
    observed.validate()?;
    if observed != expected {
        return Err(format!(
            "driver campaign package does not exactly match authorization package {expected_package_id}"
        ));
    }
    Ok(expected)
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
    let frozen = freeze_current_operator_contract_set()?;
    let auth = revalidate_stored_v2_authorization(store, principal, authorization_id, &frozen)?;
    let package_id = authorization_campaign_package_id(&auth)?;
    let required_package_id = required_campaign_package_id_for_rung(None)?;
    if package_id != required_package_id {
        return Err("the generic frozen schedule accepts only the canonical DeepSeek package; Codex packages require an exact MX1 rung".into());
    }
    require_driver_campaign_package(driver, package_id)?;
    driver.ensure_effects_ready()?;
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
    run_frozen_cells(
        store,
        principal,
        &frozen,
        run_id,
        authorization_id,
        lease_token,
        cells.to_vec(),
        driver,
    )
}

/// Execute the minimal real Codex subscription matrix rung: one admitted H0
/// harness, two exact subscription Models, one frozen baseline Strategy, one
/// repetition. It reuses the normal RWE coordinator and Store owners.
pub fn run_frozen_mx1_1x2x1(
    store: &std::sync::Arc<LocalProductStore>,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    lease_token: &str,
    driver: &dyn CellDriver,
) -> Result<Value, String> {
    let frozen = freeze_current_operator_contract_set()?;
    let auth = revalidate_stored_v2_authorization(store, principal, authorization_id, &frozen)?;
    let required_package_id = required_campaign_package_id_for_rung(Some(
        crate::harness_evolution::Mx1MatrixRung::OneByTwoByOne,
    ))?;
    if authorization_campaign_package_id(&auth)? != required_package_id {
        return Err("MX1 1x2x1 requires the frozen Codex subscription campaign package".into());
    }
    require_driver_campaign_package(driver, required_package_id)?;
    driver.ensure_effects_ready()?;
    let cells = frozen_codex_mx1_cells(
        &frozen,
        crate::harness_evolution::Mx1MatrixRung::OneByTwoByOne,
    )?;
    run_frozen_cells(
        store,
        principal,
        &frozen,
        run_id,
        authorization_id,
        lease_token,
        cells,
        driver,
    )
}

pub(super) fn run_frozen_codex_single_model_rung(
    store: &std::sync::Arc<LocalProductStore>,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    lease_token: &str,
    driver: &dyn CellDriver,
    rung: crate::harness_evolution::Mx1MatrixRung,
) -> Result<Value, String> {
    let frozen = freeze_current_operator_contract_set()?;
    let auth = revalidate_stored_v2_authorization(store, principal, authorization_id, &frozen)?;
    let required_package_id = required_campaign_package_id_for_rung(Some(rung))?;
    if authorization_campaign_package_id(&auth)? != required_package_id {
        return Err(format!(
            "MX1 {} requires frozen campaign package {required_package_id}",
            rung.as_str(),
        ));
    }
    require_driver_campaign_package(driver, required_package_id)?;
    driver.ensure_effects_ready()?;
    let cells = frozen_codex_mx1_cells(&frozen, rung)?;
    let package =
        crate::rwe::campaign_package::resolve_frozen_campaign_package(required_package_id)?;
    if cells.len() != package.cell_count {
        return Err(format!(
            "MX1 {} materialized {} cells but frozen package {} requires {}",
            rung.as_str(),
            cells.len(),
            required_package_id,
            package.cell_count
        ));
    }
    run_frozen_cells(
        store,
        principal,
        &frozen,
        run_id,
        authorization_id,
        lease_token,
        cells,
        driver,
    )
}

/// Execute the Luna-only Codex subscription matrix rung: one H0 Harness, one
/// exact Luna Model, and one baseline Strategy.
pub fn run_frozen_mx1_1x1x1(
    store: &std::sync::Arc<LocalProductStore>,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    lease_token: &str,
    driver: &dyn CellDriver,
) -> Result<Value, String> {
    run_frozen_codex_single_model_rung(
        store,
        principal,
        run_id,
        authorization_id,
        lease_token,
        driver,
        crate::harness_evolution::Mx1MatrixRung::OneByOneByOne,
    )
}

/// Execute the three-Strategy extension only after the Luna-only baseline has
/// been admitted by the same Store-owned run path.
pub fn run_frozen_mx1_1x1x3(
    store: &std::sync::Arc<LocalProductStore>,
    principal: &AuthenticatedPrincipal,
    run_id: &str,
    authorization_id: &str,
    lease_token: &str,
    driver: &dyn CellDriver,
) -> Result<Value, String> {
    run_frozen_codex_single_model_rung(
        store,
        principal,
        run_id,
        authorization_id,
        lease_token,
        driver,
        crate::harness_evolution::Mx1MatrixRung::OneByOneByThree,
    )
}

pub(super) fn run_frozen_cells(
    store: &std::sync::Arc<LocalProductStore>,
    principal: &AuthenticatedPrincipal,
    frozen: &OperatorFrozenContractSet,
    run_id: &str,
    authorization_id: &str,
    lease_token: &str,
    cells: Vec<Value>,
    driver: &dyn CellDriver,
) -> Result<Value, String> {
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

    let existing = store.list_rwe_task_attempts_for_run(run_id)?;
    let mut existing_by_attempt: std::collections::BTreeMap<String, Value> =
        std::collections::BTreeMap::new();
    for row in &existing {
        if let Some(id) = row.get("task_attempt_id").and_then(Value::as_str) {
            existing_by_attempt.insert(id.to_string(), row.clone());
        }
    }

    let stop_rules = stop_rules_from_schedule(frozen);
    let mut stopped_by = reconstruct_stopped_by(&existing, &stop_rules);

    let mut cell_results = Vec::new();
    let mut aggregate_requests = 0u64;
    let mut aggregate_tokens = 0u64;
    let mut any_live_provider = false;
    let mut any_injected_provider = false;

    for cell in &cells {
        // Revalidate bindings before every cell effect.
        revalidate_stored_v2_authorization(store, principal, authorization_id, frozen)?;

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
            let evidence =
                build_cell_evidence(run_id, authorization_id, frozen, cell, task, &ids, &outcome);
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
                    frozen,
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
                    frozen,
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
            .execute_cell(store, principal, frozen, run_id, &lease, cell, task, &ids)
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

        let evidence =
            build_cell_evidence(run_id, authorization_id, frozen, cell, task, &ids, &outcome);
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

    let live_baseline_sealed = if cells
        .first()
        .and_then(|cell| cell.get("matrix_plan_id"))
        .is_some()
    {
        evaluate_store_owned_live_baseline_seal_for_cells(
            store,
            principal,
            frozen,
            run_id,
            &stopped_by,
            &cells,
            &cell_results,
        )
    } else {
        evaluate_store_owned_live_baseline_seal(
            store,
            principal,
            frozen,
            run_id,
            &stopped_by,
            &cell_results,
        )
    };

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
pub(super) fn execute_armed_delegated_rwe_cell(
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
    matrix_binding: Option<&ProductHarnessMatrixBinding>,
    campaign_package: Option<&crate::rwe::campaign_package::FrozenCampaignPackage>,
    target_repo_path: &std::path::Path,
    transport: &Option<std::sync::Arc<dyn crate::provider::transport::HttpTransport>>,
) -> Result<CellOutcome, String> {
    let product_task_id = product_task_id.to_string();
    // Validate the frozen output contract before issuing spend or activating
    // any provider stage. Discovering this after three calls is too late.
    let product_task = store
        .get_product_task(&product_task_id)?
        .ok_or("delegated ProductTask is missing")?;
    if product_task.get("output_intent").and_then(Value::as_str) != Some("draft_pr") {
        return Err("delegated RWE execution requires the frozen draft_pr output contract".into());
    }
    if let Some(workspace_path_str) = product_task
        .pointer("/workspace_binding/workspace_path")
        .and_then(Value::as_str)
    {
        crate::rwe::frozen_rwe_bindings::ensure_frozen_rwe_workspace_active_baseline(
            std::path::Path::new(workspace_path_str),
            Some(target_repo_path),
        )?;
    }
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
    let package = match (matrix_binding, campaign_package) {
        (Some(binding), Some(package)) => {
            if !matches!(binding.rung.as_str(), "1x1x1" | "1x1x3" | "1x2x1")
                || binding.harness_id != crate::harness_evolution::MX1_ARM_ZERO_HARNESS_ID
            {
                return Err(
                    "Codex live H0 route requires an admitted single-Harness frozen MX1 cell"
                        .into(),
                );
            }
            package.clone()
        }
        (Some(binding), None) => {
            if binding.rung != "1x2x1"
                || binding.harness_id != crate::harness_evolution::MX1_ARM_ZERO_HARNESS_ID
                || binding.strategy_id != crate::harness_evolution::MX1_NO_PROJECTION_STRATEGY_ID
            {
                return Err(
                    "Codex live H0 route requires the legacy frozen MX1 1x2x1 arm-zero/no-projection cell"
                        .into(),
                );
            }
            crate::rwe::campaign_package::canonical_codex_subscription_v1_package()?
        }
        (None, Some(package)) => package.clone(),
        (None, None) => crate::rwe::campaign_package::canonical_deepseek_v2_package()?,
    };
    package.validate()?;
    let is_codex = package.provider_kind
        == crate::cli::codex_budget_authority::CODEX_PROVIDER_KIND_CHATGPT_SUBSCRIPTION;
    let role_model = if let Some(binding) = matrix_binding {
        resolve_frozen_matrix_model(binding)?.resolved_model_id
    } else if is_codex {
        package.admitted_model.clone()
    } else {
        OPERATOR_ADMITTED_MODEL.to_string()
    };
    let provider_binding = package.provider_execution_binding_for_model(&role_model)?;
    let role_models = if is_codex {
        json!({
            "planner": role_model,
            "implementer": role_model,
            "reviewer": role_model,
        })
    } else {
        json!({
            "planner": OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL,
            "implementer": OPERATOR_ADMITTED_MODEL,
            "reviewer": OPERATOR_ADMITTED_PLANNER_REVIEWER_MODEL,
        })
    };

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
        protocol: provider_binding.protocol.clone(),
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
        "provider_execution_binding": provider_binding.to_json(),
        "campaign_package_id": if is_codex {
            json!(package.package_id)
        } else {
            Value::Null
        },
        "matrix_model_descriptor_id": if is_codex {
            matrix_binding.map(|binding| binding.model_id.clone())
        } else {
            None
        },
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
    let manifest_price_profile = match manifest.pointer("/provider/price_profile") {
        Some(Value::Null) if is_codex => {
            crate::provider::managed_deepseek::DeepSeekPriceProfile::default()
        }
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| "delegated manifest price profile malformed")?,
        None => return Err("delegated manifest price profile missing".into()),
    };
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
    let executor = if is_codex {
        if serving_transport.is_some() {
            return Err(
                "live Codex subscription execution cannot use an injected transport".into(),
            );
        }
        let adapter: std::sync::Arc<dyn ManagedNodeProvider> =
            std::sync::Arc::new(CodexSubscriptionProviderAdapter::new(
                std::sync::Arc::clone(store),
                &provider_binding,
                std::sync::Arc::new(crate::provider::transport::ReqwestTransport::new()),
                target_repo_path.to_path_buf(),
                package.package_id.clone(),
            ));
        ManagedDeepSeekNodeExecutor::new_with_adapters(
            std::sync::Arc::clone(&adapter),
            std::sync::Arc::clone(&adapter),
            adapter,
            source,
            ManagedDeepSeekExecutorConfig {
                protocol: DeepSeekProtocol::OpenAiCompatible,
                limits: manifest_limits,
                price_profile: manifest_price_profile,
            },
            vec![role_model.clone()],
        )?
    } else {
        match serving_transport {
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

    // This campaign requires the canonical Draft PR output contract.
    let output_intent = store
        .get_product_task(&product_task_id)?
        .and_then(|task| task.get("output_intent").cloned())
        .and_then(|value| value.as_str().map(str::to_string))
        .ok_or("delegated ProductTask output intent is missing")?;
    if output_intent != "draft_pr" {
        return Err(format!(
            "delegated ProductTask output intent is not supported: {output_intent}"
        ));
    }

    // 8. Genuine output under the operator-authorized live-run environment: the
    // store plans and claims the draft_pr operation, pushes the approved branch
    // to the credential-free https origin, and records the pushed commit; the
    // coordinator then creates the real GitHub Draft PR through the existing
    // GitHub owner and completes the store-owned operation, which transitions
    // the ProductTask to completed with a Draft PR terminal record. Network
    // effects here require ACP_PRODUCT_GOLDEN_PATH_ALLOW_NETWORK_OUTPUT=1,
    // ACP_ENABLE_GITHUB_PR_OUTPUT=1, and populated token references; without
    // them the store returns a planned/blocked output and this fails closed.
    let terminal = complete_delegated_draft_pr_output_and_terminal(
        store,
        &product_task_id,
        task_version,
        approval_id,
        &delegation_id,
        &attempt_id,
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

pub(crate) fn complete_delegated_draft_pr_output_and_terminal(
    store: &std::sync::Arc<LocalProductStore>,
    product_task_id: &str,
    task_version: u64,
    approval_id: &str,
    delegation_id: &str,
    attempt_id: &str,
) -> Result<Value, String> {
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
            product_task_id,
            "executor",
            task_version,
            Some(approval_id),
            true,
        )
        .map_err(|e| format!("delegated output failed (task {product_task_id}): {e}"))?;
    let output_status = output.pointer("/output/status").and_then(Value::as_str);
    if !matches!(
        output_status,
        Some("pr_create_pending" | "operation_in_progress")
    ) {
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
        product_task_id,
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
    store.complete_delegated_product_task_terminal(
        delegation_id,
        attempt_id,
        product_task_id,
        "executor",
    )
}

/// Recover or create the exact-revision Product Golden Path prerequisite used
/// by live Codex RWE authorization.
///
/// This is an ordinary Store-owned ProductTask using the first frozen workload
/// definition and verifier, with `draft_pr` output. It exercises real
/// Luna subscription calls, workspace actions, deterministic verification,
/// independent confirmation, Draft PR output, and terminal evidence. It does
/// not merge the target default branch and is never an RWE matrix result.
pub fn recover_or_create_codex_subscription_golden_path_prerequisite(
    store: &std::sync::Arc<LocalProductStore>,
    principal: &AuthenticatedPrincipal,
    target_repo_path: &std::path::Path,
    cell_executor_key_id: &str,
    cell_confirmer_key_id: &str,
) -> Result<Value, String> {
    use crate::product_golden_path::{
        validate_intake, ProductExecutorPolicy, ProductTaskBudget, ProductTaskIntakeRequest,
        ProductVerificationCommand,
    };

    if !target_repo_path.is_dir() {
        return Err("Codex prerequisite target_repo_path must be an existing directory".into());
    }
    let target_repo_path = target_repo_path
        .canonicalize()
        .map_err(|_| "Codex prerequisite target_repo_path identity is unavailable".to_string())?;
    let (_default_branch, origin_remote, observed_sha) =
        crate::target_repo_output::inspect_git_source_identity(
            &crate::target_repo_output::TargetRepoOutputConfig::from_env(),
            &target_repo_path,
        )?;
    let repository = crate::target_repo_output::parse_github_repository_url(&origin_remote)
        .map_err(|_| {
            "Codex prerequisite target origin is not the frozen GitHub repository".to_string()
        })?;
    if format!("{}/{}", repository.owner, repository.repository) != OPERATOR_TARGET_REPO {
        return Err("Codex prerequisite target origin does not match the frozen repository".into());
    }
    if observed_sha != FROZEN_RWE_TARGET_MAIN_SHA {
        return Err(format!(
            "Codex prerequisite target revision mismatch: expected {FROZEN_RWE_TARGET_MAIN_SHA}"
        ));
    }

    let package = crate::rwe::campaign_package::canonical_codex_luna_xhigh_v2_package()?;
    package.validate()?;
    if package.admitted_model != crate::rwe::campaign_package::CODEX_SUBSCRIPTION_LUNA_MODEL
        || package.planner_reviewer_model
            != crate::rwe::campaign_package::CODEX_SUBSCRIPTION_LUNA_MODEL
        || package
            .provider_execution_binding
            .as_ref()
            .and_then(|binding| binding.reasoning_effort.as_deref())
            != Some("xhigh")
    {
        return Err("Codex prerequisite requires the exact Luna xhigh package".into());
    }
    let driver = ProductGoldenPathCellDriver {
        target_repo_path: Some(target_repo_path.clone()),
        allow_live_provider_effects: true,
        fake_transport: None,
        cell_executor_key_id: Some(cell_executor_key_id.to_string()),
        cell_confirmer_key_id: Some(cell_confirmer_key_id.to_string()),
        campaign_package: Some(package.clone()),
    };
    let executor_principal = store.authenticate_managed_acceptance_principal(
        principal.tenant_id(),
        cell_executor_key_id,
        None,
    )?;
    let confirmer_principal = store.authenticate_managed_acceptance_principal(
        principal.tenant_id(),
        cell_confirmer_key_id,
        None,
    )?;
    if executor_principal.principal_id() == principal.principal_id()
        || confirmer_principal.principal_id() == principal.principal_id()
        || executor_principal.principal_id() == confirmer_principal.principal_id()
    {
        return Err(
            "Codex prerequisite requires three distinct operator, executor, and confirmer identities"
                .into(),
        );
    }

    let frozen = freeze_current_operator_contract_set()?;
    // A prior attempt may have failed after Store admission but before the
    // provider request. Reconcile that exact task through the existing
    // ProductTask finalizer before making one bounded recovery admission. A
    // recovered attempt gets fresh immutable delegation/task identities; the
    // failed history remains in Store and is never overwritten or replayed.
    //
    // If the first frozen task has an unreconciled provider outcome, preserve
    // that task and move to a different frozen task instead of replaying its
    // objective. This keeps the prerequisite finite and real while respecting
    // the no-replay boundary for an outcome-unknown provider effect.
    let prerequisite_task_indices = codex_prerequisite_candidate_indices(&frozen)?;
    let primary_task_index = prerequisite_task_indices[0];
    let mut prerequisite_candidate_position = 0usize;
    let mut recovery_generation = 0u8;
    let (
        product_task_id,
        mut persisted_task,
        ids,
        selected_task,
        selected_schedule_cell,
        execution_run_id,
    ) = loop {
        let prerequisite_task_index = prerequisite_task_indices[prerequisite_candidate_position];
        let is_primary_prerequisite = prerequisite_task_index == primary_task_index;
        let frozen_task = frozen
            .corpus
            .tasks
            .get(prerequisite_task_index)
            .cloned()
            .ok_or("frozen RWE corpus has no remaining prerequisite task")?;
        if frozen_task.source_commit != FROZEN_RWE_TARGET_MAIN_SHA {
            return Err("frozen prerequisite task source revision is not exact".into());
        }
        let verifier = frozen_task
            .expected_verification_commands
            .first()
            .cloned()
            .ok_or("frozen prerequisite task verifier is missing")?;
        let schedule_cell = frozen
            .schedule
            .body
            .get("cells")
            .and_then(Value::as_array)
            .and_then(|cells| {
                cells.iter().find(|cell| {
                    cell.get("task_id").and_then(Value::as_str)
                        == Some(frozen_task.task_id.as_str())
                })
            })
            .cloned()
            .ok_or("frozen schedule has no cell budget for prerequisite task")?;
        let execution_run_id = if is_primary_prerequisite {
            if recovery_generation == 0 {
                "codex-prerequisite".to_string()
            } else {
                format!("codex-prerequisite-recovery-{recovery_generation}")
            }
        } else {
            let base = format!("codex-prerequisite-{}", frozen_task.task_id);
            if recovery_generation == 0 {
                base
            } else {
                format!("{base}-recovery-{recovery_generation}")
            }
        };
        let ids = cell_identities_for(&execution_run_id, &schedule_cell, &frozen_task)?;
        let idempotency_key = if is_primary_prerequisite {
            format!(
                "rwe-golden-prerequisite-codex-luna-xhigh-v{}",
                recovery_generation + 1
            )
        } else {
            format!(
                "rwe-golden-prerequisite-codex-luna-xhigh-{}-v{}",
                frozen_task.task_id,
                recovery_generation + 1
            )
        };
        let workspace_id = idempotency_key.clone();
        let intake = ProductTaskIntakeRequest {
            objective: frozen_task.objective.clone(),
            target_id: "alters-lab".into(),
            target_repo_path: target_repo_path.to_string_lossy().into_owned(),
            source_kind: Some("git_repository".into()),
            source_revision: frozen_task.source_commit.clone(),
            source_tree_hash: Some(frozen_task.source_tree_hash.clone()),
            allowed_paths: frozen_task.allowed_mutable_paths.clone(),
            verification_commands: vec![ProductVerificationCommand {
                command: verifier.clone(),
                timeout_ms: frozen_task.timeout_ms.clamp(1, 900_000),
            }],
            output_intent: "draft_pr".into(),
            executor_policy: ProductExecutorPolicy {
                allowed_executors: vec!["managed_deepseek".into()],
                prefer: Some("managed_deepseek".into()),
            },
            budget: Some(ProductTaskBudget {
                total_tokens: Some(frozen_task.per_task_max_total_tokens),
                total_calls: Some(frozen_task.per_task_max_provider_requests),
                total_elapsed_ms: Some(frozen_task.timeout_ms),
                max_retries: Some(frozen_task.per_task_max_retries),
                max_repairs: Some(0),
                max_concurrency: Some(1),
                stage_budgets: None,
            }),
            risk_class: FROZEN_RWE_RISK_CLASS.into(),
            approval_required: true,
            confirm_execution: Some(true),
            confirm_output: Some(true),
            idempotency_key: idempotency_key.clone(),
            expected_version: None,
            tenant_id: Some(principal.tenant_id().into()),
            workspace_id: Some(workspace_id.clone()),
            workspace_mode: Some("git_worktree".into()),
            matrix_binding: None,
        };
        let validated = validate_intake(&intake, principal.tenant_id(), &workspace_id)?;
        // Read history before admission: a changed output contract must not
        // overwrite the old intake or hide its unsettled provider attempt.
        let admitted = match store.get_product_task_by_idempotency(
            principal.tenant_id(),
            &workspace_id,
            &idempotency_key,
        )? {
            Some(existing) => existing,
            None => {
                // Historical reconciliation/reuse does not require a fresh
                // provider session. New task admission still checks readiness
                // before any workspace acquisition or execution.
                driver.ensure_effects_ready()?;
                store.admit_product_task(&validated, principal.principal_id())?
            }
        };
        let product_task_id = admitted
            .get("task_id")
            .and_then(Value::as_str)
            .ok_or("Codex prerequisite ProductTask identity is missing")?
            .to_string();
        let mut persisted_task = store
            .get_product_task(&product_task_id)?
            .ok_or("Codex prerequisite ProductTask disappeared after admission")?;
        let status = persisted_task.get("status").and_then(Value::as_str);
        let historical_evidence =
            store.project_rwe_cell_store_evidence(&product_task_id, &ids.delegated_attempt_id)?;
        let has_uncertain_provider_effect = historical_evidence
            .get("provider_request_journal")
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries.iter().any(|entry| {
                    matches!(
                        entry.get("status").and_then(Value::as_str),
                        Some("sending" | "outcome_unknown")
                    )
                })
            });
        if has_uncertain_provider_effect {
            let run_status = persisted_task
                .get("run_id")
                .and_then(Value::as_str)
                .map(|run_id| store.get_workflow_run(run_id))
                .transpose()?
                .flatten()
                .and_then(|run| {
                    run.get("status")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            if !matches!(
                run_status.as_deref(),
                Some("failed" | "cancelled" | "killed")
            ) {
                return Err(format!(
                    "prerequisite attempt requires provider reconciliation; preserved task {product_task_id}"
                ));
            }
            // Close the failed run through the existing Store owner before
            // selecting any fresh candidate. This records conservative
            // outcome-unknown terminal evidence, closes its lease, and never
            // replays or declares the old effect absent.
            store
                .finalize_product_task_after_execution(&product_task_id, "recovery-owner")
                .map_err(|error| {
                    format!(
                        "failed prerequisite unknown outcome could not be reconciled through Store: {error}"
                    )
                })?;
            if prerequisite_candidate_position + 1 < prerequisite_task_indices.len() {
                prerequisite_candidate_position += 1;
                recovery_generation = 0;
                continue;
            }
            if !codex_prerequisite_recovery_available(recovery_generation) {
                return Err(format!(
                    "prerequisite attempt requires provider reconciliation; preserved task {product_task_id}"
                ));
            }
            // The unknown attempt remains preserved and unreplayable. A new
            // ProductTask identity is permitted only as a bounded acquisition
            // of this same frozen prerequisite definition.
            recovery_generation += 1;
            continue;
        }
        let intake_contract_matches = persisted_task
            .get("intake_contract_sha256")
            .and_then(Value::as_str)
            == Some(validated.intake_contract_sha256.as_str());
        if !intake_contract_matches
            && !matches!(
                status,
                Some("failed" | "cancelled" | "killed" | "outcome_unknown")
            )
        {
            return Err(
                "persisted prerequisite ProductTask does not match the exact frozen intake contract"
                    .into(),
            );
        }
        if status == Some("admitted") {
            // A prior bounded admission can leave an admitted row before its
            // workspace-preparation owner resumes (for example after a gate
            // failure immediately before the prepare call). Re-enter the
            // existing Store-owned recovery path; it performs no provider
            // effect and returns the same task only after workspace binding
            // or an auditable terminal preparation result.
            store
                .recover_product_task_workspace(&product_task_id, "recovery-owner")
                .map_err(|error| {
                    format!("admitted prerequisite ProductTask workspace recovery failed: {error}")
                })?;
            persisted_task = store
                .get_product_task(&product_task_id)?
                .ok_or("Codex prerequisite ProductTask disappeared after workspace recovery")?;
        }
        let status = persisted_task.get("status").and_then(Value::as_str);
        if matches!(
            status,
            Some("completed" | "workspace_bound" | "output_pending")
        ) {
            if persisted_task.get("output_intent").and_then(Value::as_str) != Some("draft_pr")
                || persisted_task
                    .get("source_revision")
                    .and_then(Value::as_str)
                    != Some(FROZEN_RWE_TARGET_MAIN_SHA)
            {
                return Err(
                    "historical prerequisite does not match the frozen revision/output contract"
                        .into(),
                );
            }
            break (
                product_task_id,
                persisted_task,
                ids,
                frozen_task,
                schedule_cell,
                execution_run_id,
            );
        }

        let run_status = persisted_task
            .get("run_id")
            .and_then(Value::as_str)
            .map(|run_id| store.get_workflow_run(run_id))
            .transpose()?
            .flatten()
            .and_then(|run| {
                run.get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        if matches!(
            run_status.as_deref(),
            Some("failed" | "cancelled" | "killed")
        ) {
            let finalized = store
                .finalize_product_task_after_execution(&product_task_id, "recovery-owner")
                .map_err(|error| {
                    format!(
                        "failed prerequisite recovery could not be reconciled through Store: {error}"
                    )
                })?;
            let delegation_id = format!("rwe-del:{execution_run_id}:{}", ids.cell_id);
            store.require_delegation_product_task(
                &delegation_id,
                principal.tenant_id(),
                &product_task_id,
            )?;
            let authority_state = store.delegated_authority_state(&delegation_id)?;
            let durable_terminal = if authority_state
                .get("attempt_lease_state")
                .and_then(Value::as_str)
                == Some("closed")
                && authority_state
                    .get("delegation_state")
                    .and_then(Value::as_str)
                    == Some("expired")
                && authority_state
                    .get("spend_authorization_state")
                    .and_then(Value::as_str)
                    == Some("expired")
            {
                authority_state
                    .get("terminal_evidence")
                    .filter(|value| value.is_object())
                    .cloned()
            } else {
                None
            };
            // The finalizer may reuse a task whose delegation was already
            // closed. In that case its transient `delegated_terminal` is
            // absent, while the same Store owner still has the durable
            // terminal receipt. Prefer that durable state after every
            // finalizer call rather than treating an omitted transient value
            // as missing reconciliation evidence.
            let failure_receipt = recovered_failure_terminal_receipt(durable_terminal, &finalized);
            let terminal_class = failure_receipt
                .as_ref()
                .and_then(|receipt| receipt.get("terminal_class"))
                .and_then(Value::as_str)
                .or_else(|| {
                    finalized
                        .pointer("/delegated_terminal/terminal/terminal_class")
                        .and_then(Value::as_str)
                });
            // An absent journal/receipt does not prove reconciliation. Require
            // the Store finalizer's affirmative terminal evidence before any
            // recovery admission; historical replay needs the durable receipt.
            let reconciled_before_provider = failure_receipt.as_ref().is_some_and(|receipt| {
                receipt.get("schema_version").and_then(Value::as_str)
                    == Some("managed_delegated_failure_terminal_evidence.v1")
                    && receipt.get("product_task_id").and_then(Value::as_str)
                        == Some(product_task_id.as_str())
                    && receipt
                        .get("provider_request_count")
                        .and_then(Value::as_u64)
                        == Some(0)
                    && matches!(
                        receipt.get("cost_evidence").and_then(Value::as_str),
                        Some("reconciled" | "unavailable")
                    )
                    && receipt.get("cleanup_status").and_then(Value::as_str) == Some("cleaned")
            });
            let reconciled_known_failure_after_provider =
                failure_receipt.as_ref().is_some_and(|receipt| {
                    receipt.get("schema_version").and_then(Value::as_str)
                        == Some("managed_delegated_failure_terminal_evidence.v1")
                        && receipt.get("product_task_id").and_then(Value::as_str)
                            == Some(product_task_id.as_str())
                        && receipt
                            .get("provider_request_states")
                            .and_then(Value::as_array)
                            .is_some_and(|states| {
                                states.iter().any(|state| {
                                    state.get("status").and_then(Value::as_str) == Some("succeeded")
                                })
                            })
                        && matches!(
                            receipt.get("cost_evidence").and_then(Value::as_str),
                            Some("reconciled" | "unavailable")
                        )
                        && receipt.get("cleanup_status").and_then(Value::as_str) == Some("cleaned")
                });
            if terminal_class != Some("failed")
                || (!reconciled_before_provider && !reconciled_known_failure_after_provider)
            {
                return Err(
                    "failed prerequisite recovery did not produce a reconciled non-effect failure terminal"
                        .into(),
                );
            }
            if reconciled_known_failure_after_provider {
                // This attempt already has a known provider effect.  It is
                // therefore never replayable, even though its later stage
                // failed before send.  Consume the existing explicit corpus
                // fallback instead; it has a fresh ProductTask/delegation
                // identity and remains within the frozen prerequisite scope.
                if prerequisite_candidate_position + 1 >= prerequisite_task_indices.len() {
                    // Both explicit frozen candidates may themselves have
                    // terminal, known failures after a composition defect.
                    // Permit the next bounded fresh ProductTask for the same
                    // frozen fallback definition. This is not a replay:
                    // it gets new Store/delegation/lease identities, while
                    // every prior effect and terminal receipt remains closed
                    // and immutable. Keep acquisition bounded by the shared
                    // finite recovery-generation ceiling.
                    if !codex_prerequisite_recovery_available(recovery_generation) {
                        return Err(
                            "known prerequisite provider effect has no remaining bounded recovery"
                                .into(),
                        );
                    }
                    recovery_generation += 1;
                    continue;
                }
                prerequisite_candidate_position += 1;
                recovery_generation = 0;
                continue;
            }
            if recovery_generation >= 1 {
                return Err("bounded Codex prerequisite recovery attempts exhausted".into());
            }
            recovery_generation += 1;
            continue;
        }
        return Err(format!(
            "Codex prerequisite requires workspace_bound task, got {}",
            status.unwrap_or("unknown")
        ));
    };

    if persisted_task.get("status").and_then(Value::as_str) == Some("output_pending") {
        let task_version = persisted_task
            .get("version")
            .and_then(Value::as_u64)
            .ok_or("prerequisite task version missing")?;
        let run_id = persisted_task
            .get("run_id")
            .and_then(Value::as_str)
            .ok_or("prerequisite task run_id missing")?;
        let approvals = store.workflow_run_approvals(run_id, 1000)?;
        let approval = approvals
            .into_iter()
            .find(|candidate| candidate.get("decision").and_then(Value::as_str) == Some("approved"))
            .ok_or("prerequisite task approval not found")?;
        let approval_id = approval
            .get("approval_id")
            .and_then(Value::as_str)
            .ok_or("prerequisite task approval_id missing")?;
        let delegation_id = format!("rwe-del:{execution_run_id}:{}", ids.cell_id);
        let attempt_id = &ids.delegated_attempt_id;

        driver.ensure_effects_ready()?;
        complete_delegated_draft_pr_output_and_terminal(
            store,
            &product_task_id,
            task_version,
            approval_id,
            &delegation_id,
            attempt_id,
        )?;
        persisted_task = store
            .get_product_task(&product_task_id)?
            .ok_or("prerequisite task missing after output completion")?;
    }

    // Idempotent recovery: only a pre-existing Store-owned completed seal can
    // satisfy this branch; no synthetic terminal row is ever created.
    if persisted_task.get("status").and_then(Value::as_str) == Some("completed") {
        let delegation_id = format!("rwe-del:{execution_run_id}:{}", ids.cell_id);
        let attempt_id = &ids.delegated_attempt_id;
        if let Ok(authority_state) = store.delegated_authority_state(&delegation_id) {
            if authority_state
                .get("delegation_state")
                .and_then(Value::as_str)
                == Some("active")
            {
                store.complete_delegated_product_task_terminal(
                    &delegation_id,
                    attempt_id,
                    &product_task_id,
                    "executor",
                )?;
            }
        }
        let evidence = store.validated_rwe_prerequisite_evidence(
            principal,
            &product_task_id,
            FROZEN_RWE_TARGET_MAIN_SHA,
        )?;
        let projection =
            store.project_rwe_cell_store_evidence(&product_task_id, &ids.delegated_attempt_id)?;
        if store_evidence_transport_provenance(&projection)? != "external" {
            return Err("completed prerequisite lacks external provider provenance".into());
        }
        let manifest_identity =
            store.project_delegated_execution_identity_for_attempt(&ids.delegated_attempt_id)?;
        let expected_binding =
            package.provider_execution_binding_for_model(&package.admitted_model)?;
        validate_prerequisite_provider_identity(
            &projection,
            &manifest_identity,
            &expected_binding,
            &package.package_id,
            &product_task_id,
            &ids.delegated_attempt_id,
        )?;
        return Ok(sort_value(&json!({
            "schema_version": "codex_golden_path_prerequisite.v1",
            "scientific_result": false,
            "reused": true,
            "status": "completed",
            "product_task_id": product_task_id,
            "source_revision": FROZEN_RWE_TARGET_MAIN_SHA,
            "expected_provider_kind": package.provider_kind,
            "expected_model_identity": package.admitted_model,
            "expected_reasoning_effort": "xhigh",
            "managed_executor_identity": evidence.pointer("/node/managed_executor_identity"),
            "provider_execution": projection.get("provider_execution"),
            "terminal_evidence_id": evidence.get("evidence_id"),
            "terminal_content_sha256": evidence.get("content_sha256"),
            "note": "reused Store-owned prerequisite; not a scientific matrix result"
        })));
    }
    if persisted_task.get("status").and_then(Value::as_str) != Some("workspace_bound") {
        return Err(format!(
            "Codex prerequisite requires workspace_bound task, got {}",
            persisted_task
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ));
    }
    let workspace_path = persisted_task
        .pointer("/workspace_binding/workspace_path")
        .and_then(Value::as_str)
        .ok_or("Codex prerequisite workspace binding path is missing")?
        .to_string();
    crate::rwe::frozen_rwe_bindings::ensure_frozen_rwe_workspace_active_baseline(
        std::path::Path::new(&workspace_path),
        Some(&target_repo_path),
    )?;
    let prerequisite_run_id = ids
        .workflow_id
        .strip_prefix("rwe-wf:")
        .and_then(|value| value.split_once(':').map(|(run_id, _)| run_id))
        .ok_or("Codex prerequisite run identity is malformed")?;
    driver.ensure_effects_ready()?;
    let outcome = execute_armed_delegated_rwe_cell(
        store,
        principal,
        &executor_principal,
        &confirmer_principal,
        &frozen,
        prerequisite_run_id,
        "",
        &selected_schedule_cell,
        &selected_task,
        &ids,
        &product_task_id,
        None,
        Some(&package),
        &target_repo_path,
        &None,
    )?;
    let evidence = store.get_product_task_terminal_evidence(&product_task_id)?;
    if evidence.get("task_status").and_then(Value::as_str) != Some("completed") {
        return Err(
            "Codex prerequisite execution returned without a completed terminal seal".into(),
        );
    }
    Ok(sort_value(&json!({
        "schema_version": "codex_golden_path_prerequisite.v1",
        "scientific_result": false,
        "reused": false,
        "status": "completed",
        "product_task_id": product_task_id,
        "source_revision": FROZEN_RWE_TARGET_MAIN_SHA,
        "provider_kind": package.provider_kind,
        "provider_identity": package.provider_execution_binding.as_ref().map(|b| b.provider_identity.clone()),
        "model_identity": package.admitted_model,
        "reasoning_effort": "xhigh",
        "provider_transport_provenance": outcome.provider_transport_provenance,
        "provider_requests": outcome.provider_requests,
        "total_tokens": outcome.total_tokens,
        "verification_status": outcome.verification_status,
        "verification_trustworthy": outcome.verification_trustworthy,
        "terminal_evidence_id": evidence.get("evidence_id"),
        "terminal_content_sha256": evidence.get("content_sha256"),
        "output_draft_pr": outcome.output_draft_pr,
        "note": "real Store-owned Product Golden Path prerequisite; not a scientific matrix result"
    })))
}
