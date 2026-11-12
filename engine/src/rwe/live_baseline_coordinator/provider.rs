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

/// Resolve registered matrix Model identifier to the exact raw model name.
pub fn resolve_matrix_model_to_raw(model_id: &str) -> &str {
    model_id.split(':').next().unwrap_or(model_id)
}

pub(super) fn resolve_frozen_matrix_model(
    binding: &ProductHarnessMatrixBinding,
) -> Result<crate::harness_evolution::Mx1ModelPlanDescriptor, String> {
    let manifest = if matches!(
        binding.model_id.as_str(),
        crate::harness_evolution::MX1_CODEX_LUNA_MODEL_ID
            | crate::harness_evolution::MX1_CODEX_TERRA_MODEL_ID
    ) {
        crate::harness_evolution::sample_mx1_descriptor_manifest_with_codex_subscription_models()
    } else {
        crate::harness_evolution::sample_mx1_descriptor_manifest_with_ledger_harness()
    };
    crate::harness_evolution::validate_mx1_descriptor_manifest(&manifest).map_err(|error| {
        format!(
            "MX1 descriptor manifest validation failed for matrix model: {}",
            error.message
        )
    })?;
    let model = manifest
        .models
        .iter()
        .find(|descriptor| descriptor.descriptor_id == binding.model_id)
        .cloned()
        .ok_or_else(|| {
            format!(
                "matrix model binding is not an admitted frozen MX1 Model descriptor: {}",
                binding.model_id
            )
        })?;
    if !model
        .supported_harness_ids
        .iter()
        .any(|harness| harness == &binding.harness_id)
        || !model
            .supported_strategy_ids
            .iter()
            .any(|strategy| strategy == &binding.strategy_id)
        || model.requested_model_id != model.resolved_model_id
        || resolve_matrix_model_to_raw(&binding.model_id) != model.resolved_model_id
    {
        return Err("matrix model binding is not an exact admitted MX1 cell model".into());
    }
    Ok(model)
}

/// Legacy label resolver retained only for callers that need to distinguish
/// strategy arms. It deliberately returns no model-visible content: live
/// execution must use the cell-bound, descriptor-validated digest reference
/// produced by `resolve_strategy_projection_for_cell`.
pub fn resolve_strategy_projection(strategy_id: &str) -> Option<String> {
    let _ = strategy_id;
    None
}

pub(super) fn resolve_strategy_projection_for_cell(
    binding: &ProductHarnessMatrixBinding,
) -> Result<Option<String>, String> {
    use crate::harness_evolution::{
        validate_mx1_descriptor_manifest, Mx1CellIdentity, Mx1StrategyAdapter,
        MX1_MEMORY_ONLY_STRATEGY_ID, MX1_NO_PROJECTION_STRATEGY_ID, MX1_SKILL_ONLY_STRATEGY_ID,
    };
    let manifest = crate::harness_evolution::sample_mx1_descriptor_manifest_with_ledger_harness();
    validate_mx1_descriptor_manifest(&manifest).map_err(|error| {
        format!(
            "MX1 descriptor manifest validation failed: {}",
            error.message
        )
    })?;
    let strategy = manifest
        .strategies
        .iter()
        .find(|descriptor| descriptor.descriptor_id == binding.strategy_id)
        .cloned()
        .ok_or_else(|| "matrix strategy is not present in the sealed MX1 manifest".to_string())?;
    if !strategy.supported_harness_ids.contains(&binding.harness_id)
        || !strategy.supported_model_ids.contains(&binding.model_id)
    {
        return Err("matrix strategy is not admitted for the bound MX1 cell".into());
    }
    if strategy.descriptor_id == MX1_NO_PROJECTION_STRATEGY_ID {
        return Ok(None);
    }
    if !matches!(
        strategy.descriptor_id.as_str(),
        MX1_MEMORY_ONLY_STRATEGY_ID | MX1_SKILL_ONLY_STRATEGY_ID
    ) {
        return Err("matrix strategy is outside the admitted MX1 projection arms".into());
    }
    let adapter = Mx1StrategyAdapter::new(strategy)
        .map_err(|error| format!("MX1 strategy descriptor rejected: {}", error.message))?;
    let identity = Mx1CellIdentity {
        harness_id: binding.harness_id.clone(),
        model_id: binding.model_id.clone(),
        strategy_id: binding.strategy_id.clone(),
        task_id: binding.task_id.clone(),
    };
    let now_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock is before UNIX epoch".to_string())?
        .as_millis() as u64;
    let lease = adapter
        .prepare_projection(&identity, now_unix_ms)
        .map_err(|error| format!("MX1 projection preparation failed: {}", error.message))?
        .ok_or_else(|| "projection strategy unexpectedly has no projection".to_string())?;
    adapter
        .validate_projection(&lease, &identity, now_unix_ms)
        .map_err(|error| format!("MX1 projection validation failed: {}", error.message))?;
    let digest_ref = json!({
        "schema_version": "mx1_validated_projection_digest_ref.v1",
        "strategy_id": lease.strategy_id,
        "source_handle": lease.source_handle,
        "content_sha256": lease.content_sha256,
        "cell_binding_sha256": lease.cell_binding_sha256,
        "expires_at_unix_ms": lease.expires_at_unix_ms,
    });
    Ok(Some(sort_value(&digest_ref).to_string()))
}

/// Provider-neutral execution adapter for live ledger worker rounds.
///
/// Wraps provider wire invocation under `ManagedProviderCallAuthority`,
/// ensuring calls enter the store journal, rate limits, and spend authorization.
pub trait LiveLedgerProviderAdapter: Send + Sync {
    fn provider_id(&self) -> &str;
    fn provider_kind(&self) -> &str;
    fn base_url(&self) -> &str;
    fn endpoint_path(&self) -> &str;
    fn credential_reference(&self) -> &str;
    fn transport(&self) -> &std::sync::Arc<dyn crate::provider::transport::HttpTransport>;
    fn requested_model(&self) -> Option<&str> {
        None
    }
    fn invoke(
        &self,
        authority: &ManagedProviderCallAuthority,
        request: &ManagedProviderCallRequest,
    ) -> Result<crate::provider::managed_deepseek::ManagedProviderResponse, String>;
}

/// Minimal OpenAI-compatible provider adapter adhering strictly to the Chat
/// Completions protocol (`chat/completions`) with arbitrary base URLs (e.g.
/// OpenAI, OpenRouter, OpenCode Go, Codex proxies, or local test fixtures).
/// It does NOT claim generic Responses API support.
pub struct OpenAiCompatibleProviderAdapter {
    pub provider_id: String,
    pub provider_kind: String,
    pub base_url: String,
    pub endpoint_path: String,
    pub credential_reference: String,
    pub provider_subject: String,
    pub transport: std::sync::Arc<dyn crate::provider::transport::HttpTransport>,
}

impl OpenAiCompatibleProviderAdapter {
    pub fn new(
        provider_id: impl Into<String>,
        provider_kind: impl Into<String>,
        base_url: impl Into<String>,
        endpoint_path: impl Into<String>,
        credential_reference: impl Into<String>,
        provider_subject: impl Into<String>,
        transport: std::sync::Arc<dyn crate::provider::transport::HttpTransport>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            provider_kind: provider_kind.into(),
            base_url: base_url.into(),
            endpoint_path: endpoint_path.into(),
            credential_reference: credential_reference.into(),
            provider_subject: provider_subject.into(),
            transport,
        }
    }
}

impl LiveLedgerProviderAdapter for OpenAiCompatibleProviderAdapter {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }
    fn provider_kind(&self) -> &str {
        &self.provider_kind
    }
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn endpoint_path(&self) -> &str {
        &self.endpoint_path
    }
    fn credential_reference(&self) -> &str {
        &self.credential_reference
    }
    fn transport(&self) -> &std::sync::Arc<dyn crate::provider::transport::HttpTransport> {
        &self.transport
    }
    fn invoke(
        &self,
        authority: &ManagedProviderCallAuthority,
        request: &ManagedProviderCallRequest,
    ) -> Result<crate::provider::managed_deepseek::ManagedProviderResponse, String> {
        let mut effective_request = request.clone();
        authority
            .prepare_request_for_budget(&mut effective_request)
            .map_err(|error| format!("{}: {}", error.domain, error.message))?;
        let provider_config = ProviderConfig::new(
            &effective_request.provider_identity,
            &effective_request.provider_kind,
            &effective_request.base_url,
            &effective_request.requested_model,
            &effective_request.credential_reference,
            "2026-07-30T00:00:00Z",
        );
        let credential = CredentialRef::new(
            &effective_request.credential_reference,
            "env",
            "***",
            &self.provider_subject,
            "2026-07-30T00:00:00Z",
        );
        let boundary =
            CredentialBoundary::new("env").map_err(|e| format!("credential boundary: {e}"))?;
        let transport = std::sync::Arc::clone(&self.transport);
        let authority_clone = std::sync::Arc::new(authority.clone());
        let request_clone = effective_request;
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| format!("live ledger provider runtime unavailable: {error}"))?;
            runtime
                .block_on(authority_clone.invoke_with_retry(&request_clone, || {
                    crate::provider::managed_deepseek::invoke_openai_wire(
                        &provider_config,
                        &boundary,
                        &credential,
                        &transport,
                        &request_clone,
                    )
                }))
                .map_err(|error| format!("{}: {}", error.domain, error.message))
        })
        .join()
        .map_err(|_| "live ledger provider runtime thread panicked".to_string())?
    }
}

/// Store-bound adapter for the ChatGPT subscription Responses surface.
///
/// The existing `ManagedProviderCallAuthority` remains the sole ProductTask
/// claim/reconcile owner.  This adapter only translates the already-authorized
/// managed request to the loopback `CodexBudgetGateway`; the gateway owns the
/// parent subscription credential and its separate durable quota journal.
pub(super) struct CodexSubscriptionProviderAdapter {
    store: std::sync::Arc<LocalProductStore>,
    provider_id: String,
    provider_kind: String,
    base_url: String,
    endpoint_path: String,
    credential_reference: String,
    transport: std::sync::Arc<dyn HttpTransport>,
    worktree: std::path::PathBuf,
    package_id: String,
}

impl CodexSubscriptionProviderAdapter {
    pub(super) fn new(
        store: std::sync::Arc<LocalProductStore>,
        provider_binding: &crate::rwe::campaign_package::FrozenProviderExecutionBinding,
        transport: std::sync::Arc<dyn HttpTransport>,
        worktree: std::path::PathBuf,
        package_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            provider_id: provider_binding.provider_identity.clone(),
            provider_kind: provider_binding.provider_kind.clone(),
            base_url: provider_binding.base_url.clone(),
            endpoint_path: provider_binding.endpoint_path.clone(),
            credential_reference: provider_binding.credential_reference.clone(),
            transport,
            worktree,
            package_id: package_id.into(),
        }
    }
}

impl LiveLedgerProviderAdapter for CodexSubscriptionProviderAdapter {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn provider_kind(&self) -> &str {
        &self.provider_kind
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn endpoint_path(&self) -> &str {
        &self.endpoint_path
    }

    fn credential_reference(&self) -> &str {
        &self.credential_reference
    }

    fn transport(&self) -> &std::sync::Arc<dyn HttpTransport> {
        &self.transport
    }

    fn requested_model(&self) -> Option<&str> {
        None
    }

    fn invoke(
        &self,
        authority: &ManagedProviderCallAuthority,
        request: &ManagedProviderCallRequest,
    ) -> Result<ManagedProviderResponse, String> {
        if request.provider_kind
            != crate::cli::codex_budget_authority::CODEX_PROVIDER_KIND_CHATGPT_SUBSCRIPTION
        {
            return Err("Codex subscription adapter received a non-subscription request".into());
        }
        let mut effective_request = request.clone();
        authority
            .prepare_request_for_budget(&mut effective_request)
            .map_err(|error| format!("{}: {}", error.domain, error.message))?;
        let request = &effective_request;
        let permit = self
            .store
            .codex_gateway_start_permit_for_delegated_attempt(&request.binding)?;
        let execution_id = permit.execution_id().to_string();
        let package =
            crate::rwe::campaign_package::resolve_frozen_campaign_package(&self.package_id)?;
        package.validate()?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "system clock is before UNIX epoch".to_string())?
            .as_millis() as u64;
        let executable = CodexExecutableIdentity {
            binary_path: std::path::PathBuf::from(&package.admitted_binary_path),
            binary_version: package.admitted_binary_version.clone(),
            binary_sha256: package.admitted_binary_sha256.clone(),
        };
        let provider = CodexProviderIdentity::chatgpt_subscription(&request.base_url)?;
        let codex_authority = CodexBudgetAuthority {
            schema_version: CODEX_BUDGET_AUTHORITY_SCHEMA.into(),
            task_id: request.binding.product_task_id.clone(),
            workflow_node_id: request.binding.node_id.clone(),
            execution_id: execution_id.clone(),
            executable,
            provider,
            model: request.requested_model.clone(),
            // This gateway instance is scoped to exactly one manifest-bound
            // role call. The Store-owned ManagedProviderCallAuthority retains
            // the frozen three-call-per-cell budget across planner,
            // implementer, and reviewer.
            max_provider_requests: 1,
            max_retries: request.limits.max_retries,
            max_input_tokens_per_request: request.limits.max_input_tokens,
            max_output_tokens_per_request: request.max_output_tokens,
            max_cumulative_tokens: request.limits.max_cumulative_tokens,
            max_cost_usd: None,
            timeout_ms: request.limits.timeout_ms,
            worktree: self.worktree.clone(),
            expires_unix_ms: now_ms.saturating_add(24 * 60 * 60 * 1000),
        };
        let upstream_auth = CodexUpstreamAuth::resolve_parent_chatgpt_subscription().ok_or(
            "ChatGPT subscription credential is unavailable; API-key auth is not admitted",
        )?;
        let journal_path =
            crate::cli::codex_usage_journal::parent_owned_journal_path(&execution_id);
        let gateway = CodexBudgetGateway::start_with_auth(
            permit,
            codex_authority,
            &self.base_url,
            upstream_auth,
            journal_path,
        )?;
        let loopback_url = format!(
            "{}/{}",
            gateway.base_url().trim_end_matches('/'),
            request.endpoint_path.trim_start_matches('/')
        );
        let transport = std::sync::Arc::clone(&self.transport);
        let request_clone = request.clone();
        let authority_clone = std::sync::Arc::new(authority.clone());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("Codex subscription runtime unavailable: {error}"))?;
        let gateway_ref = &gateway;
        let gateway_result = runtime
            .block_on(authority_clone.invoke_with_retry(&request_clone, || {
                codex_responses_wire(gateway_ref, &transport, &loopback_url, &request_clone)
            }))
            .map_err(|error| format!("{}: {}", error.domain, error.message));
        let _gateway_usage = gateway.shutdown();
        gateway_result
    }
}

impl ManagedNodeProvider for CodexSubscriptionProviderAdapter {
    fn transport_provenance(&self) -> crate::provider::transport::ProviderTransportProvenance {
        crate::provider::transport::production_transport_provenance(&self.transport)
    }

    fn invoke_with_authority(
        &self,
        authority: &ManagedProviderCallAuthority,
        request: &ManagedProviderCallRequest,
    ) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
        LiveLedgerProviderAdapter::invoke(self, authority, request)
            .map_err(codex_error_from_adapter_message)
    }
}

pub(super) fn codex_error_from_adapter_message(error: String) -> ManagedProviderCallError {
    let (domain, message) = error.split_once(": ").map_or(
        ("adapter_pre_gateway", error.as_str()),
        |(domain, message)| (domain, message),
    );
    let pre_send = domain == "provider_pre_send";
    let no_external_effect = matches!(domain, "provider_request" | "gateway_pre_call");
    let provider_effect = domain.starts_with("provider_");
    codex_error(
        domain,
        message,
        pre_send,
        if pre_send {
            ManagedFailureEffect::PreSend
        } else if no_external_effect {
            ManagedFailureEffect::NoExternalEffect
        } else if provider_effect {
            ManagedFailureEffect::OutcomeUnknown
        } else {
            ManagedFailureEffect::NoExternalEffect
        },
    )
}

pub(super) fn codex_error(
    domain: &str,
    message: impl Into<String>,
    retryable: bool,
    effect: ManagedFailureEffect,
) -> ManagedProviderCallError {
    ManagedProviderCallError {
        domain: domain.into(),
        message: message.into(),
        retryable,
        effect,
    }
}

pub(super) fn codex_gateway_pre_call_code(reason: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(reason).ok()?;
    let code = value.pointer("/error/code").and_then(Value::as_str)?;
    matches!(
        code,
        "authority_expired"
            | "session_unbound"
            | "path_not_admitted"
            | "malformed_body"
            | "model_substitution"
            | "output_limit_required"
            | "output_limit_invalid"
            | "output_limit_increase"
            | "output_limit_unsupported"
            | "budget_lock_poisoned"
            | "journal_blocks_admit"
            | "request_budget_exhausted"
            | "retry_budget_exhausted"
            | "input_budget_exhausted"
            | "cumulative_budget_insufficient"
            | "journal_reserve_failed"
            | "journal_lock_poisoned"
            | "journal_halted"
    )
    .then(|| code.to_string())
}

pub(super) fn codex_http_error(error: HttpError) -> ManagedProviderCallError {
    match error {
        HttpError::PreSend(_) => codex_error(
            "provider_pre_send",
            "Codex subscription request was rejected before the loopback send",
            true,
            ManagedFailureEffect::PreSend,
        ),
        HttpError::Http { status, reason } => {
            if let Some(code) = codex_gateway_pre_call_code(&reason) {
                codex_error(
                    "gateway_pre_call",
                    code,
                    false,
                    ManagedFailureEffect::NoExternalEffect,
                )
            } else if status == 401 || status == 403 {
                codex_error(
                    "provider_auth",
                    "ChatGPT subscription authentication or route authorization failed",
                    false,
                    ManagedFailureEffect::OutcomeUnknown,
                )
            } else {
                codex_error(
                    "provider_error",
                    format!("ChatGPT subscription HTTP {status}"),
                    false,
                    ManagedFailureEffect::OutcomeUnknown,
                )
            }
        }
        HttpError::Timeout(_) => codex_error(
            "provider_timeout",
            "ChatGPT subscription response timed out; outcome is unknown",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        ),
        HttpError::Connection(_) => codex_error(
            "provider_connection",
            "ChatGPT subscription connection outcome is unknown",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        ),
        HttpError::Parse(_) => codex_error(
            "provider_response",
            "ChatGPT subscription response transport was malformed",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        ),
    }
}

pub(super) fn codex_sse_events(body: &[u8]) -> Vec<Value> {
    body.split(|byte| *byte == b'\n')
        .filter_map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let data = line.strip_prefix(b"data:")?.trim_ascii();
            if data.is_empty() || data == b"[DONE]" {
                None
            } else {
                serde_json::from_slice::<Value>(data).ok()
            }
        })
        .collect()
}

pub(super) fn codex_response_value(body: &[u8], events: &[Value]) -> Option<Value> {
    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        return Some(value);
    }
    events.iter().find_map(|event| {
        (event.get("type").and_then(Value::as_str) == Some("response.completed"))
            .then(|| event.get("response").cloned())
            .flatten()
    })
}

pub(super) fn append_codex_output_text(value: &Value, output: &mut String) {
    let Some(items) = value.get("output").and_then(Value::as_array) else {
        return;
    };
    for item in items {
        if item.get("type").and_then(Value::as_str) == Some("message") {
            if let Some(content) = item.get("content").and_then(Value::as_array) {
                for part in content {
                    if matches!(
                        part.get("type").and_then(Value::as_str),
                        Some("output_text") | Some("text")
                    ) {
                        if let Some(text) = part.get("text").and_then(Value::as_str) {
                            output.push_str(text);
                        }
                    }
                }
            }
        } else if item.get("type").and_then(Value::as_str) == Some("output_text") {
            if let Some(text) = item.get("text").and_then(Value::as_str) {
                output.push_str(text);
            }
        }
    }
}

pub(super) fn codex_output_text(body: &[u8], events: &[Value], response: &Value) -> String {
    let mut deltas = String::new();
    let mut completed_text = None;
    for event in events {
        match event.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                    deltas.push_str(delta);
                }
            }
            Some("response.output_text.done") => {
                completed_text = event
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            _ => {}
        }
    }
    if !deltas.is_empty() {
        return deltas;
    }
    if let Some(text) = completed_text {
        return text;
    }
    if let Some(text) = response.get("output_text").and_then(Value::as_str) {
        return text.to_string();
    }
    let mut output = String::new();
    append_codex_output_text(response, &mut output);
    if output.is_empty() && events.is_empty() {
        if let Ok(value) = serde_json::from_slice::<Value>(body) {
            append_codex_output_text(&value, &mut output);
        }
    }
    output
}

pub(super) fn parse_single_codex_tool_call(item: &Value) -> Option<ManagedToolCall> {
    if !matches!(
        item.get("type").and_then(Value::as_str),
        Some("function_call") | Some("function")
    ) {
        return None;
    }
    let id = item
        .get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("call_generated");
    let name = item
        .get("name")
        .or_else(|| item.pointer("/function/name"))
        .and_then(Value::as_str)?;
    let arguments_val = item
        .get("arguments")
        .or_else(|| item.pointer("/function/arguments"))?;
    let arguments = match arguments_val {
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).ok()?,
    };
    Some(ManagedToolCall {
        id: id.to_string(),
        call_type: "function".into(),
        function: ManagedFunctionCall {
            name: name.to_string(),
            arguments,
        },
    })
}

pub(super) fn codex_tool_calls(events: &[Value], response: &Value) -> Vec<ManagedToolCall> {
    let mut calls = response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(parse_single_codex_tool_call)
        .collect::<Vec<_>>();
    if calls.is_empty() {
        for event in events {
            if event.get("type").and_then(Value::as_str) == Some("response.output_item.done") {
                if let Some(item) = event.get("item") {
                    if let Some(call) = parse_single_codex_tool_call(item) {
                        calls.push(call);
                    }
                }
            }
        }
    }
    calls
}

pub(super) fn codex_responses_body(request: &ManagedProviderCallRequest) -> Value {
    let input = request
        .messages
        .iter()
        .map(|message| {
            let mut value = json!({
                "role": message.role,
                "content": message.content,
            });
            if let Some(tool_call_id) = &message.tool_call_id {
                value["call_id"] = json!(tool_call_id);
            }
            value
        })
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": request.requested_model,
        "input": input,
        "store": false,
        "stream": true,
    });
    if let Some(system) = &request.system {
        body["instructions"] = json!(system);
    }
    if request.thinking.mode == "enabled" {
        if let Some(effort) = &request.thinking.reasoning_effort {
            body["reasoning"] = json!({"effort": effort});
        }
    }
    if !request.tools.is_empty() {
        body["tools"] = serde_json::to_value(
            request
                .tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "name": tool.function.name,
                        "description": tool.function.description,
                        "parameters": tool.function.parameters,
                        "strict": tool.strict,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or(Value::Null);
    }
    if let Some(choice) = &request.tool_choice {
        // The managed route stores the Chat Completions-shaped choice for the
        // existing DeepSeek adapter. Responses uses the equivalent flat
        // function choice; translate only this wire representation at the
        // Codex boundary.
        if choice.get("type").and_then(Value::as_str) == Some("function") {
            if let Some(name) = choice.pointer("/function/name").and_then(Value::as_str) {
                body["tool_choice"] = json!({"type": "function", "name": name});
            } else {
                body["tool_choice"] = choice.clone();
            }
        } else {
            body["tool_choice"] = choice.clone();
        }
    }
    body
}

pub(super) async fn codex_responses_wire(
    gateway: &CodexBudgetGateway,
    transport: &std::sync::Arc<dyn HttpTransport>,
    url: &str,
    request: &ManagedProviderCallRequest,
) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
    let body = codex_responses_body(request);
    let response = transport
        .send(&HttpRequest {
            url: url.to_string(),
            method: "POST".into(),
            headers: vec![
                (
                    "Authorization".into(),
                    format!("Bearer {}", gateway.session_token()),
                ),
                ("Content-Type".into(), "application/json".into()),
                ("Accept".into(), "text/event-stream".into()),
            ],
            body: Some(serde_json::to_vec(&body).map_err(|_| {
                codex_error(
                    "provider_request",
                    "Codex subscription request could not be encoded",
                    false,
                    ManagedFailureEffect::NoExternalEffect,
                )
            })?),
            timeout_secs: Some(request.limits.timeout_ms as f64 / 1000.0),
        })
        .await
        .map_err(codex_http_error)?;
    let events = codex_sse_events(&response.body);
    let response_value = codex_response_value(&response.body, &events).ok_or_else(|| {
        codex_error(
            "provider_response",
            "Codex subscription response lacked a completed Responses object",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        )
    })?;
    if !events.is_empty()
        && !events
            .iter()
            .any(|event| event.get("type").and_then(Value::as_str) == Some("response.completed"))
    {
        return Err(codex_error(
            "provider_response",
            "Codex subscription SSE stream lacked response.completed",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        ));
    }
    let protocol_usage = if events.is_empty() {
        crate::execution_usage::protocol_usage::from_openai_responses_body(&response_value)
    } else {
        crate::execution_usage::protocol_usage::from_openai_responses_stream_events(&events)
    }
    .ok_or_else(|| {
        codex_error(
            "provider_response",
            "Codex subscription response lacked complete token usage",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        )
    })?;
    let model = protocol_usage.model.clone().ok_or_else(|| {
        codex_error(
            "provider_identity",
            "Codex subscription response model identity is missing",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        )
    })?;
    let request_id = protocol_usage
        .message_or_response_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| {
            codex_error(
                "provider_identity",
                "Codex subscription response id is missing",
                false,
                ManagedFailureEffect::OutcomeUnknown,
            )
        })?;
    if model != request.requested_model {
        return Err(codex_error(
            "provider_identity",
            "Codex subscription response model conflicts with the frozen request",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        ));
    }
    let buckets = protocol_usage.try_to_canonical_buckets().map_err(|_| {
        codex_error(
            "provider_response",
            "Codex subscription usage counters are contradictory",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        )
    })?;
    let usage = ManagedUsage {
        input_tokens: protocol_usage.input_tokens,
        output_tokens: protocol_usage.output_tokens,
        cache_read_tokens: protocol_usage.cache_read_tokens,
        cache_creation_tokens: protocol_usage.cache_creation_tokens,
        reasoning_output_tokens: protocol_usage.reasoning_output_tokens,
        fresh_input_tokens: buckets.fresh_input,
        cumulative_tokens: buckets.billable_token_total(),
        model: model.clone(),
        request_id: request_id.clone(),
    };
    let gateway_usage = gateway.usage();
    if gateway_usage.provider_requests != 1
        || gateway_usage.cumulative_input_tokens != usage.input_tokens
        || gateway_usage.cumulative_output_tokens != usage.output_tokens
    {
        return Err(codex_error(
            "provider_reconciliation",
            "Codex gateway usage disagrees with the Responses usage record",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        ));
    }
    let ownership = gateway.journal_ownership_facts().map_err(|_| {
        codex_error(
            "provider_reconciliation",
            "Codex gateway parent journal ownership could not be verified",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        )
    })?;
    if !ownership.parent_owned_for_attempt || !ownership.durable_and_current {
        return Err(codex_error(
            "provider_reconciliation",
            "Codex gateway parent journal is not current for this attempt",
            false,
            ManagedFailureEffect::OutcomeUnknown,
        ));
    }
    let stop_reason = response_value
        .get("status")
        .and_then(Value::as_str)
        .or_else(|| {
            response_value
                .pointer("/incomplete_details/reason")
                .and_then(Value::as_str)
        })
        .unwrap_or("completed")
        .to_string();
    Ok(ManagedProviderResponse {
        schema_version: request.response_schema_version.clone(),
        provider_identity: request.provider_identity.clone(),
        provider_kind: request.provider_kind.clone(),
        protocol: DeepSeekProtocol::OpenAiCompatible,
        requested_model: request.requested_model.clone(),
        resolved_model: model,
        request_id,
        output_text: codex_output_text(&response.body, &events, &response_value),
        tool_calls: codex_tool_calls(&events, &response_value),
        stop_reason,
        usage,
        estimated_cost_usd: None,
        stream: true,
    })
}
