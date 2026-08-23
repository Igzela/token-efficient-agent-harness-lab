//! ProductTask scheduler adapter for the managed DeepSeek provider.
//!
//! The executor is deliberately a thin adapter.  Persisted ProductTask,
//! attempt, spend, and lease authority remains in `LocalProductStore`; this
//! module only translates a typed node declaration into the existing
//! `ManagedProviderCallRequest` and invokes the provider through
//! `invoke_with_authority`.

use super::config::{CredentialRef, ProviderConfig};
use super::credential::CredentialBoundary;
use super::managed_deepseek::{
    DeepSeekPriceProfile, DeepSeekProtocol, ManagedAuthoritySource, ManagedCallBinding,
    ManagedCallLimits, ManagedDeepSeekProvider, ManagedMessage, ManagedModelRole,
    ManagedProviderCallAuthority, ManagedProviderCallError, ManagedProviderCallRequest,
    ManagedProviderResponse,
};
use super::transport::ReqwestTransport;
use crate::node_executor::{
    CommandNodeExecutor, NodeExecutionInput, NodeExecutionOutput, NodeExecutor,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const MANAGED_DEEPSEEK_EXECUTOR_TYPE: &str = "managed_deepseek";
const MANAGED_DEEPSEEK_NODE_METADATA: &str = "managed_deepseek";
const MANAGED_WORKSPACE_ACTION_TOOL: &str = "apply_workspace_action";

fn managed_workspace_action_tool() -> super::managed_deepseek::ManagedTool {
    super::managed_deepseek::ManagedTool {
        tool_type: "function".to_string(),
        function: super::managed_deepseek::ManagedToolFunction {
            name: MANAGED_WORKSPACE_ACTION_TOOL.to_string(),
            description: "Apply one bounded replacement in one concrete regular file from allowed_file_paths; never target a directory.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "schema_version": {"type": "string", "description": "Always managed_workspace_action.v1"},
                    "action": {"type": "string", "description": "Always replace_text"},
                    "path": {"type": "string"},
                    "old_text": {"type": "string"},
                    "new_text": {"type": "string"}
                },
                "required": ["schema_version", "action", "path", "old_text", "new_text"]
            }),
        },
        strict: true,
    }
}

#[derive(Clone)]
struct ManagedDeepSeekProviders {
    planner: Arc<ManagedDeepSeekProvider>,
    implementer: Arc<ManagedDeepSeekProvider>,
    reviewer: Arc<ManagedDeepSeekProvider>,
}

impl ManagedDeepSeekProviders {
    fn for_role(&self, role: ManagedModelRole) -> Arc<ManagedDeepSeekProvider> {
        match role {
            ManagedModelRole::Planner => Arc::clone(&self.planner),
            ManagedModelRole::Implementer => Arc::clone(&self.implementer),
            ManagedModelRole::Reviewer => Arc::clone(&self.reviewer),
        }
    }
}

/// Configuration shared by all stages of one ProductTask route.
#[derive(Debug, Clone, PartialEq)]
pub struct ManagedDeepSeekExecutorConfig {
    pub protocol: DeepSeekProtocol,
    pub limits: ManagedCallLimits,
    pub price_profile: DeepSeekPriceProfile,
}

impl Default for ManagedDeepSeekExecutorConfig {
    fn default() -> Self {
        Self {
            protocol: DeepSeekProtocol::OpenAiCompatible,
            limits: ManagedCallLimits {
                max_requests: 3,
                max_retries: 0,
                max_input_tokens: 8_000,
                max_output_tokens: 4_000,
                max_cumulative_tokens: 24_000,
                timeout_ms: 30_000,
                max_cost_usd: None,
            },
            price_profile: DeepSeekPriceProfile::default(),
        }
    }
}

/// The only scheduler-facing managed DeepSeek executor.
///
/// A budget coordinator is cached per ProductTask so the four route stages
/// share one in-process reservation ledger.  Persisted authority is still
/// checked by the store-backed source on every provider request.
pub struct ManagedDeepSeekNodeExecutor {
    providers: ManagedDeepSeekProviders,
    source: Arc<dyn ManagedAuthoritySource>,
    config: ManagedDeepSeekExecutorConfig,
    authorities: Mutex<HashMap<String, Arc<ManagedProviderCallAuthority>>>,
}

impl ManagedDeepSeekNodeExecutor {
    pub fn new(
        planner: Arc<ManagedDeepSeekProvider>,
        implementer: Arc<ManagedDeepSeekProvider>,
        reviewer: Arc<ManagedDeepSeekProvider>,
        source: Arc<dyn ManagedAuthoritySource>,
        config: ManagedDeepSeekExecutorConfig,
    ) -> Result<Self, String> {
        let _ = super::managed_deepseek::ManagedBudgetLedger::new(config.limits.clone())?;
        Ok(Self {
            providers: ManagedDeepSeekProviders {
                planner,
                implementer,
                reviewer,
            },
            source,
            config,
            authorities: Mutex::new(HashMap::new()),
        })
    }

    /// Build the production adapter without resolving the credential. The
    /// provider's credential boundary resolves `DEEPSEEK_API_KEY` only at the
    /// final managed request boundary.
    pub fn from_env(source: Arc<dyn ManagedAuthoritySource>) -> Result<Self, String> {
        let protocol = match std::env::var("ACP_MANAGED_DEEPSEEK_PROTOCOL")
            .unwrap_or_else(|_| "openai_compatible".to_string())
            .as_str()
        {
            "openai_compatible" => DeepSeekProtocol::OpenAiCompatible,
            "anthropic_compatible" => DeepSeekProtocol::AnthropicCompatible,
            other => return Err(format!("unsupported managed DeepSeek protocol: {other}")),
        };
        let mut config = ManagedDeepSeekExecutorConfig {
            protocol,
            ..Default::default()
        };
        if let Ok(value) = std::env::var("ACP_MANAGED_DEEPSEEK_MAX_COST_USD") {
            config.limits.max_cost_usd = Some(
                value
                    .parse::<f64>()
                    .map_err(|_| "ACP_MANAGED_DEEPSEEK_MAX_COST_USD is invalid".to_string())?,
            );
        }
        let boundary = CredentialBoundary::new("env")?;
        let transport = Arc::new(ReqwestTransport::new());
        let credential = CredentialRef::new(
            super::managed_deepseek::DEEPSEEK_CREDENTIAL_REFERENCE,
            "env",
            "***",
            "provider:deepseek",
            "2026-07-31T00:00:00Z",
        );
        let make = |role: ManagedModelRole| {
            let mut provider_config = ProviderConfig::new(
                "deepseek-managed",
                match protocol {
                    DeepSeekProtocol::OpenAiCompatible => "openai_compatible",
                    DeepSeekProtocol::AnthropicCompatible => "anthropic",
                },
                protocol.base_url(),
                role.default_model(),
                super::managed_deepseek::DEEPSEEK_CREDENTIAL_REFERENCE,
                "2026-07-31T00:00:00Z",
            );
            provider_config.timeout_ms = config.limits.timeout_ms as i64;
            provider_config.max_retries = config.limits.max_retries as i64;
            let provider = match protocol {
                DeepSeekProtocol::OpenAiCompatible => ManagedDeepSeekProvider::new_openai(
                    provider_config,
                    boundary_for_clone(&boundary),
                    credential.clone(),
                    Arc::clone(&transport) as Arc<dyn super::transport::HttpTransport>,
                ),
                DeepSeekProtocol::AnthropicCompatible => ManagedDeepSeekProvider::new_anthropic(
                    provider_config,
                    boundary_for_clone(&boundary),
                    credential.clone(),
                    Arc::clone(&transport) as Arc<dyn super::transport::HttpTransport>,
                ),
            };
            (role, Arc::new(provider))
        };
        let (planner_role, planner) = make(ManagedModelRole::Planner);
        let (implementer_role, implementer) = make(ManagedModelRole::Implementer);
        let (reviewer_role, reviewer) = make(ManagedModelRole::Reviewer);
        debug_assert_eq!(planner_role, ManagedModelRole::Planner);
        debug_assert_eq!(implementer_role, ManagedModelRole::Implementer);
        debug_assert_eq!(reviewer_role, ManagedModelRole::Reviewer);
        Self::new(planner, implementer, reviewer, source, config)
    }

    fn authority(
        &self,
        product_task_id: &str,
    ) -> Result<Arc<ManagedProviderCallAuthority>, String> {
        let mut authorities = self
            .authorities
            .lock()
            .map_err(|_| "managed DeepSeek authority cache poisoned".to_string())?;
        if let Some(authority) = authorities.get(product_task_id) {
            return Ok(Arc::clone(authority));
        }
        let authority = Arc::new(ManagedProviderCallAuthority::new(
            Arc::clone(&self.source),
            self.config.limits.clone(),
        )?);
        authorities.insert(product_task_id.to_string(), Arc::clone(&authority));
        Ok(authority)
    }

    fn request(
        &self,
        input: &NodeExecutionInput,
    ) -> Result<(ManagedProviderCallRequest, ManagedModelRole), String> {
        let declaration = input
            .node_metadata
            .get(MANAGED_DEEPSEEK_NODE_METADATA)
            .filter(|value| value.is_object())
            .ok_or_else(|| "managed DeepSeek node declaration is missing".to_string())?;
        let stage = declaration
            .get("stage")
            .and_then(Value::as_str)
            .ok_or_else(|| "managed DeepSeek stage is missing".to_string())?;
        let (expected_stage, role) = match stage {
            "planning" => ("planning", ManagedModelRole::Planner),
            "implementation" => ("implementation", ManagedModelRole::Implementer),
            "review" => ("review", ManagedModelRole::Reviewer),
            "deterministic_verification" => {
                return Err("deterministic verification is not a provider stage".to_string())
            }
            other => return Err(format!("unsupported managed DeepSeek stage: {other}")),
        };
        if declaration.get("stage").and_then(Value::as_str) != Some(expected_stage) {
            return Err("managed DeepSeek route stage is not canonical".to_string());
        }
        if declaration.get("role").and_then(Value::as_str)
            != Some(match role {
                ManagedModelRole::Planner => "planner",
                ManagedModelRole::Implementer => "implementer",
                ManagedModelRole::Reviewer => "reviewer",
            })
        {
            return Err("managed DeepSeek role does not match route stage".to_string());
        }
        if declaration.get("protocol").and_then(Value::as_str)
            != Some(match self.config.protocol {
                DeepSeekProtocol::OpenAiCompatible => "openai_compatible",
                DeepSeekProtocol::AnthropicCompatible => "anthropic_compatible",
            })
        {
            return Err("managed DeepSeek protocol does not match admitted executor".to_string());
        }
        let binding: ManagedCallBinding = serde_json::from_value(
            declaration
                .get("binding")
                .cloned()
                .ok_or_else(|| "managed DeepSeek binding is missing".to_string())?,
        )
        .map_err(|_| "managed DeepSeek binding is malformed".to_string())?;
        if binding.node_id != input.node_id || binding.workflow_id != input.workflow_id {
            return Err(
                "managed DeepSeek binding does not match scheduler node identity".to_string(),
            );
        }
        if let Some(product_task_id) = input
            .node_metadata
            .get("product_task_id")
            .and_then(Value::as_str)
        {
            if product_task_id != binding.product_task_id {
                return Err(
                    "managed DeepSeek binding does not match ProductTask identity".to_string(),
                );
            }
        }
        let mut request = ManagedProviderCallRequest::for_role(role, self.config.protocol, binding);
        request.limits = self.config.limits.clone();
        request.price_profile = self.config.price_profile.clone();
        // Execution-path provenance: derived from the transport object that
        // will actually serve this role's request, never caller-supplied.
        request.transport_provenance = self.providers.for_role(role).transport_provenance();
        request.max_output_tokens = declaration
            .get("max_output_tokens")
            .and_then(Value::as_u64)
            .unwrap_or(self.config.limits.max_output_tokens);
        request.system = declaration
            .get("system")
            .and_then(Value::as_str)
            .map(str::to_string);
        request.messages = declaration
            .get("messages")
            .cloned()
            .map(|value| {
                serde_json::from_value::<Vec<ManagedMessage>>(value)
                    .map_err(|_| "managed DeepSeek messages are malformed".to_string())
            })
            .transpose()?
            .unwrap_or_else(|| {
                vec![ManagedMessage::text(
                    "user",
                    declaration
                        .get("prompt")
                        .and_then(Value::as_str)
                        .unwrap_or("bounded ProductTask stage"),
                )]
            });
        if let Some(context) = self
            .source
            .stage_context(&request.binding, &input.node_metadata)?
        {
            request.messages.push(ManagedMessage::text(
                "user",
                &format!(
                    "Bound request-time context (do not reproduce secrets or unrelated content): {}",
                    context
                ),
            ));
        }
        if role == ManagedModelRole::Implementer {
            // Tool calling makes the implementation contract machine-readable
            // at the provider boundary. The store remains the sole authority:
            // it still validates the arguments against the exact workspace,
            // path, lease, and change bounds before writing anything.
            // DeepSeek V4 rejects forced tool_choice while thinking mode is
            // enabled, so this bounded implementation turn uses the provider's
            // non-thinking tool-call mode; planner and reviewer remain on the
            // admitted reasoning route.
            request.thinking.mode = "disabled".to_string();
            request.thinking.reasoning_effort = None;
            request.tools = vec![managed_workspace_action_tool()];
            request.tool_choice = Some(json!({
                "type": "function",
                "function": {"name": MANAGED_WORKSPACE_ACTION_TOOL}
            }));
        }
        request.validate()?;
        Ok((request, role))
    }

    fn execute_blocking(
        provider: Arc<ManagedDeepSeekProvider>,
        authority: Arc<ManagedProviderCallAuthority>,
        request: ManagedProviderCallRequest,
    ) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| ManagedProviderCallError {
                    domain: "provider_runtime".to_string(),
                    message: format!("managed DeepSeek runtime unavailable: {error}"),
                    retryable: false,
                    effect: super::managed_deepseek::ManagedFailureEffect::NoExternalEffect,
                })?;
            runtime.block_on(provider.invoke_with_authority(&authority, &request))
        })
        .join()
        .map_err(|_| ManagedProviderCallError {
            domain: "provider_runtime".to_string(),
            message: "managed DeepSeek runtime thread panicked".to_string(),
            retryable: false,
            effect: super::managed_deepseek::ManagedFailureEffect::OutcomeUnknown,
        })?
    }

    fn output(
        &self,
        request: &ManagedProviderCallRequest,
        response: &ManagedProviderResponse,
        action_receipt: Option<Value>,
    ) -> Result<String, String> {
        let stage_receipt = match request.role {
            ManagedModelRole::Planner => {
                let value: Value = serde_json::from_str(&response.output_text)
                    .map_err(|_| "managed planner output is not the required JSON object")?;
                let path = value.get("path").and_then(Value::as_str).unwrap_or("");
                let intent = value.get("intent").and_then(Value::as_str).unwrap_or("");
                let schema_ok = value.get("schema_version").and_then(Value::as_str)
                    == Some("managed_deepseek_plan.v1")
                    && value.get("status").and_then(Value::as_str) == Some("planned")
                    && !path.is_empty()
                    && !intent.is_empty();
                let docs_plan = path == "docs/USER_GUIDE.md"
                    && intent == "clarify_doctor_read_only_health_check";
                let rwe_plan = intent == "bounded_product_task"
                    && crate::rwe::frozen_rwe_bindings::path_under_allowed_paths(
                        path,
                        &crate::rwe::frozen_rwe_bindings::frozen_rwe_union_allowed_paths()
                            .unwrap_or_default(),
                    );
                if !schema_ok || (!docs_plan && !rwe_plan) {
                    return Err("managed planner output is outside the bounded plan schema".into());
                }
                Some(value)
            }
            ManagedModelRole::Implementer => action_receipt.clone(),
            ManagedModelRole::Reviewer => {
                let value: Value = serde_json::from_str(&response.output_text)
                    .map_err(|_| "managed reviewer output is not the required JSON object")?;
                let status = value.get("status").and_then(Value::as_str);
                let objections = value
                    .get("material_objections")
                    .and_then(Value::as_array)
                    .ok_or("managed reviewer material_objections must be an array")?;
                if value.get("schema_version").and_then(Value::as_str)
                    != Some("managed_deepseek_review.v1")
                    || !matches!(status, Some("accepted" | "rejected"))
                    || objections.len() > 10
                    || objections.iter().any(|item| {
                        item.as_str()
                            .is_none_or(|text| text.is_empty() || text.len() > 512)
                    })
                    || (status == Some("accepted") && !objections.is_empty())
                    || (status == Some("rejected") && objections.is_empty())
                {
                    return Err(
                        "managed reviewer output is outside the bounded review schema".into(),
                    );
                }
                let objections_sha256 = hex::encode(Sha256::digest(
                    serde_json::to_vec(objections)
                        .map_err(|_| "managed reviewer objections cannot be hashed")?,
                ));
                Some(json!({
                    "schema_version": "managed_deepseek_review_receipt.v1",
                    "status": status,
                    "material_objection_count": objections.len(),
                    "material_objections_sha256": objections_sha256,
                    "resolved_model": response.resolved_model,
                }))
            }
        };
        Ok(json!({
            "schema_version": "managed_deepseek_node_output.v1",
            "provider_kind": response.provider_kind,
            "protocol": response.protocol,
            "requested_model": response.requested_model,
            "resolved_model": response.resolved_model,
            "request_id": response.request_id,
            "usage": response.usage,
            "estimated_cost_usd": response.estimated_cost_usd,
            "output_sha256": hex::encode(Sha256::digest(response.output_text.as_bytes())),
            "output_bytes": response.output_text.len(),
            "route_stage": request.role,
            "workspace_action": action_receipt,
            "stage_receipt": stage_receipt,
        })
        .to_string())
    }
}

impl NodeExecutor for ManagedDeepSeekNodeExecutor {
    fn executor_type_name(&self) -> &str {
        MANAGED_DEEPSEEK_EXECUTOR_TYPE
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let started = std::time::Instant::now();
        if input.task_type == "command" {
            let command = input
                .node_metadata
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("");
            let docs_expected =
                "grep -E read-only[[:space:]]health[[:space:]]check docs/USER_GUIDE.md";
            let is_docs = command == docs_expected;
            let is_frozen_rwe =
                crate::rwe::frozen_rwe_bindings::is_exact_frozen_rwe_verifier_command(command);
            if input
                .node_metadata
                .get("executor_class")
                .and_then(Value::as_str)
                != Some("deterministic_verifier")
                || (!is_docs && !is_frozen_rwe)
                || input.node_metadata.get(MANAGED_DEEPSEEK_NODE_METADATA) != Some(&Value::Null)
            {
                return failed(
                    "managed DeepSeek route verifier is not the exact deterministic docs check or frozen RWE pytest"
                        .to_string(),
                    started.elapsed().as_millis() as i64,
                );
            }
            if is_docs {
                return CommandNodeExecutor {
                    timeout_ms: 5_000,
                    allowed_commands: vec!["grep".into()],
                    allowed_binaries: vec!["grep".into()],
                    env_vars: Vec::new(),
                }
                .execute_node(input);
            }
            // Exact frozen RWE pytest: share CommandNodeExecutor env parsing + python3.
            return CommandNodeExecutor {
                timeout_ms: 900_000,
                allowed_commands: vec!["python3".into()],
                allowed_binaries: vec!["python3".into()],
                env_vars: Vec::new(),
            }
            .execute_node(input);
        }
        let (request, role) = match self.request(input) {
            Ok(request) => request,
            Err(error) => return failed(error, started.elapsed().as_millis() as i64),
        };
        let authority = match self.authority(&request.binding.product_task_id) {
            Ok(authority) => authority,
            Err(error) => return failed(error, started.elapsed().as_millis() as i64),
        };
        let provider = self.providers.for_role(role);
        match Self::execute_blocking(provider, authority, request.clone()) {
            Ok(response) => {
                let action_receipt = if role == ManagedModelRole::Implementer {
                    let model_output = if response.tool_calls.len() == 1
                        && response.tool_calls[0].function.name == MANAGED_WORKSPACE_ACTION_TOOL
                    {
                        response.tool_calls[0].function.arguments.as_str()
                    } else if response.tool_calls.is_empty() {
                        response.output_text.as_str()
                    } else {
                        return failed(
                            "managed implementer returned an unexpected workspace tool call"
                                .to_string(),
                            started.elapsed().as_millis() as i64,
                        );
                    };
                    // The store-owned sink revalidates the exact ProductTask,
                    // workflow run, node lease generation, delegated attempt
                    // lease, and reconciled provider claim while holding the
                    // persistence lock through the workspace write.
                    match self.source.apply_workspace_action(
                        &request.binding,
                        &input.node_metadata,
                        model_output,
                    ) {
                        Ok(receipt) => Some(receipt),
                        Err(error) => {
                            return failed(
                                format!("managed workspace action rejected: {error}"),
                                started.elapsed().as_millis() as i64,
                            )
                        }
                    }
                } else {
                    None
                };
                let output = match self.output(&request, &response, action_receipt) {
                    Ok(output) => output,
                    Err(error) => return failed(error, started.elapsed().as_millis() as i64),
                };
                NodeExecutionOutput {
                    status: "completed".to_string(),
                    executor_type: MANAGED_DEEPSEEK_EXECUTOR_TYPE.to_string(),
                    output: Some(output),
                    error_domain: None,
                    error_message: None,
                    input_tokens: i64::try_from(response.usage.input_tokens).ok(),
                    output_tokens: i64::try_from(response.usage.output_tokens).ok(),
                    estimated_cost: response.estimated_cost_usd,
                    latency_ms: Some(started.elapsed().as_millis() as i64),
                    process_outcome: None,
                    resolved_model: Some(response.resolved_model),
                }
            }
            Err(error) => failed_provider(error, started.elapsed().as_millis() as i64),
        }
    }
}

fn failed_provider(error: ManagedProviderCallError, latency_ms: i64) -> NodeExecutionOutput {
    let error_domain = match error.effect {
        super::managed_deepseek::ManagedFailureEffect::OutcomeUnknown => "provider_outcome_unknown",
        super::managed_deepseek::ManagedFailureEffect::PreSend
        | super::managed_deepseek::ManagedFailureEffect::NoExternalEffect => {
            "provider_terminal_failure"
        }
    };
    NodeExecutionOutput {
        status: "failed".to_string(),
        executor_type: MANAGED_DEEPSEEK_EXECUTOR_TYPE.to_string(),
        output: None,
        error_domain: Some(error_domain.to_string()),
        error_message: Some(super::redaction::redact_sensitive_patterns(
            &error.to_string(),
        )),
        input_tokens: None,
        output_tokens: None,
        estimated_cost: None,
        latency_ms: Some(latency_ms),
        process_outcome: None,
        resolved_model: None,
    }
}

fn failed(message: String, latency_ms: i64) -> NodeExecutionOutput {
    NodeExecutionOutput {
        status: "failed".to_string(),
        executor_type: MANAGED_DEEPSEEK_EXECUTOR_TYPE.to_string(),
        output: None,
        error_domain: Some("managed_deepseek_execution".to_string()),
        error_message: Some(super::redaction::redact_sensitive_patterns(&message)),
        input_tokens: None,
        output_tokens: None,
        estimated_cost: None,
        latency_ms: Some(latency_ms),
        process_outcome: None,
        resolved_model: None,
    }
}

// CredentialBoundary owns no secret state and is intentionally reconstructed
// for each provider adapter; this avoids introducing a credential owner.
fn boundary_for_clone(boundary: &CredentialBoundary) -> CredentialBoundary {
    let _ = boundary;
    CredentialBoundary::new("env").expect("env credential backend")
}

#[cfg(test)]
mod tests {
    use super::super::managed_deepseek::{
        PersistedAuthoritySnapshot, PersistedManagedExecutionContract,
        DEEPSEEK_CREDENTIAL_REFERENCE, DEEPSEEK_OPENAI_BASE_URL, DEEPSEEK_OPENAI_PATH,
        DEEPSEEK_PROVIDER_KIND, DEEPSEEK_USAGE_PARSER_VERSION, MANAGED_PROVIDER_CALL_SCHEMA,
        MANAGED_PROVIDER_RESPONSE_SCHEMA,
    };
    use super::super::transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingTransport {
        sends: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl HttpTransport for CountingTransport {
        async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status: 200,
                body: br#"{"id":"response-1","model":"deepseek-v4-pro","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1}}"#.to_vec(),
            })
        }
    }

    struct StaticAuthority {
        snapshot: PersistedAuthoritySnapshot,
    }

    impl ManagedAuthoritySource for StaticAuthority {
        fn current_authority(
            &self,
            _binding: &ManagedCallBinding,
        ) -> Result<PersistedAuthoritySnapshot, String> {
            Ok(self.snapshot.clone())
        }

        fn claim_provider_request(
            &self,
            _request: &ManagedProviderCallRequest,
        ) -> Result<(), String> {
            Ok(())
        }

        fn reconcile_provider_request(
            &self,
            _request: &ManagedProviderCallRequest,
            _response: Option<&ManagedProviderResponse>,
            _effect: super::super::managed_deepseek::ManagedFailureEffect,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    fn binding() -> ManagedCallBinding {
        ManagedCallBinding {
            product_task_id: "product-task-1".to_string(),
            workflow_id: "workflow-1".to_string(),
            node_id: "plan-node".to_string(),
            attempt_id: "attempt-1".to_string(),
            spend_authorization_id: "spend-1".to_string(),
            attempt_lease_id: "lease-hash".to_string(),
        }
    }

    fn test_execution_contract() -> PersistedManagedExecutionContract {
        PersistedManagedExecutionContract {
            provider_kind: DEEPSEEK_PROVIDER_KIND.into(),
            protocol: DeepSeekProtocol::OpenAiCompatible,
            host: "api.deepseek.com".into(),
            base_url: DEEPSEEK_OPENAI_BASE_URL.into(),
            endpoint_path: DEEPSEEK_OPENAI_PATH.into(),
            request_schema_version: MANAGED_PROVIDER_CALL_SCHEMA.into(),
            response_schema_version: MANAGED_PROVIDER_RESPONSE_SCHEMA.into(),
            usage_parser_version: DEEPSEEK_USAGE_PARSER_VERSION.into(),
            requested_model: "deepseek-v4-pro".into(),
            limits: ManagedDeepSeekExecutorConfig::default().limits,
            price_profile: DeepSeekPriceProfile::default(),
        }
    }

    fn providers(transport: Arc<dyn HttpTransport>) -> ManagedDeepSeekProviders {
        let boundary = CredentialBoundary::new("env").unwrap();
        let credential = CredentialRef::new(
            DEEPSEEK_CREDENTIAL_REFERENCE,
            "env",
            "***",
            "provider:deepseek",
            "2026-07-30T00:00:00Z",
        );
        let make = |model: &str| {
            Arc::new(ManagedDeepSeekProvider::new_openai(
                ProviderConfig::new(
                    "deepseek-test",
                    "deepseek",
                    DeepSeekProtocol::OpenAiCompatible.base_url(),
                    model,
                    DEEPSEEK_CREDENTIAL_REFERENCE,
                    "2026-07-30T00:00:00Z",
                ),
                boundary_for_clone(&boundary),
                credential.clone(),
                Arc::clone(&transport),
            ))
        };
        ManagedDeepSeekProviders {
            planner: make("deepseek-v4-pro"),
            implementer: make("deepseek-v4-flash"),
            reviewer: make("deepseek-v4-pro"),
        }
    }

    fn input(stage: &str) -> NodeExecutionInput {
        let binding = binding();
        let role = match stage {
            "planning" => "planner",
            "implementation" => "implementer",
            "review" => "reviewer",
            _ => "planner",
        };
        NodeExecutionInput {
            node_id: binding.node_id.clone(),
            task_type: MANAGED_DEEPSEEK_EXECUTOR_TYPE.to_string(),
            run_id: binding.workflow_id.clone(),
            workflow_id: binding.workflow_id.clone(),
            node_metadata: json!({
                "managed_deepseek": {
                    "stage": stage,
                    "role": role,
                    "protocol": "openai_compatible",
                    "binding": binding,
                    "prompt": "bounded planning request"
                }
            }),
        }
    }

    #[test]
    fn implementer_request_requires_one_workspace_action_tool() {
        let source = Arc::new(StaticAuthority {
            snapshot: PersistedAuthoritySnapshot {
                product_task_id: binding().product_task_id,
                workflow_id: binding().workflow_id,
                node_id: binding().node_id,
                attempt_id: binding().attempt_id,
                spend_authorization_id: binding().spend_authorization_id,
                attempt_lease_id: binding().attempt_lease_id,
                spend_status: "consumed".to_string(),
                consumed_by_attempt_id: Some("attempt-1".to_string()),
                lease_status: "current".to_string(),
                execution_contract: Some(test_execution_contract()),
            },
        });
        let transport = Arc::new(CountingTransport {
            sends: Arc::new(AtomicUsize::new(0)),
        });
        let p = providers(transport);
        let executor = ManagedDeepSeekNodeExecutor::new(
            p.planner,
            p.implementer,
            p.reviewer,
            source,
            ManagedDeepSeekExecutorConfig::default(),
        )
        .unwrap();
        let (request, role) = executor.request(&input("implementation")).unwrap();
        assert_eq!(role, ManagedModelRole::Implementer);
        assert_eq!(request.thinking.mode, "disabled");
        assert_eq!(request.thinking.reasoning_effort, None);
        assert_eq!(request.tools.len(), 1);
        assert_eq!(
            request.tools[0].function.name,
            MANAGED_WORKSPACE_ACTION_TOOL
        );
        assert!(request.tools[0].strict);
        assert_eq!(
            request.tool_choice,
            Some(json!({
                "type": "function",
                "function": {"name": MANAGED_WORKSPACE_ACTION_TOOL}
            }))
        );
    }

    #[test]
    fn malformed_stage_fails_before_provider_request() {
        let sends = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(CountingTransport {
            sends: Arc::clone(&sends),
        });
        let source = Arc::new(StaticAuthority {
            snapshot: PersistedAuthoritySnapshot {
                product_task_id: binding().product_task_id,
                workflow_id: binding().workflow_id,
                node_id: binding().node_id,
                attempt_id: binding().attempt_id,
                spend_authorization_id: binding().spend_authorization_id,
                attempt_lease_id: binding().attempt_lease_id,
                spend_status: "consumed".to_string(),
                consumed_by_attempt_id: Some("attempt-1".to_string()),
                lease_status: "current".to_string(),
                execution_contract: Some(test_execution_contract()),
            },
        });
        let p = providers(transport);
        let executor = ManagedDeepSeekNodeExecutor::new(
            p.planner,
            p.implementer,
            p.reviewer,
            source,
            ManagedDeepSeekExecutorConfig::default(),
        )
        .unwrap();
        let output = executor.execute_node(&input("deterministic_verification"));
        assert_eq!(output.status, "failed");
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn persisted_authority_failure_is_fail_closed_before_provider_request() {
        let sends = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(CountingTransport {
            sends: Arc::clone(&sends),
        });
        let source = Arc::new(StaticAuthority {
            snapshot: PersistedAuthoritySnapshot {
                product_task_id: "different-task".to_string(),
                workflow_id: "workflow-1".to_string(),
                node_id: "plan-node".to_string(),
                attempt_id: "attempt-1".to_string(),
                spend_authorization_id: "spend-1".to_string(),
                attempt_lease_id: "lease-hash".to_string(),
                spend_status: "consumed".to_string(),
                consumed_by_attempt_id: Some("attempt-1".to_string()),
                lease_status: "current".to_string(),
                execution_contract: Some(test_execution_contract()),
            },
        });
        let p = providers(transport);
        let executor = ManagedDeepSeekNodeExecutor::new(
            p.planner,
            p.implementer,
            p.reviewer,
            source,
            ManagedDeepSeekExecutorConfig::default(),
        )
        .unwrap();
        let output = executor.execute_node(&input("planning"));
        assert_eq!(output.status, "failed");
        assert!(output.error_message.is_some());
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn immutable_execution_contract_drift_fails_before_provider_request() {
        let sends = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(CountingTransport {
            sends: Arc::clone(&sends),
        });
        let mut contract = test_execution_contract();
        contract.limits.timeout_ms -= 1;
        let source = Arc::new(StaticAuthority {
            snapshot: PersistedAuthoritySnapshot {
                product_task_id: binding().product_task_id,
                workflow_id: binding().workflow_id,
                node_id: binding().node_id,
                attempt_id: binding().attempt_id,
                spend_authorization_id: binding().spend_authorization_id,
                attempt_lease_id: binding().attempt_lease_id,
                spend_status: "consumed".to_string(),
                consumed_by_attempt_id: Some("attempt-1".to_string()),
                lease_status: "current".to_string(),
                execution_contract: Some(contract),
            },
        });
        let p = providers(transport);
        let executor = ManagedDeepSeekNodeExecutor::new(
            p.planner,
            p.implementer,
            p.reviewer,
            source,
            ManagedDeepSeekExecutorConfig::default(),
        )
        .unwrap();
        let output = executor.execute_node(&input("planning"));
        assert_eq!(output.status, "failed");
        assert!(output
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("stale or mismatched")));
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn binding_cannot_retarget_a_different_scheduler_node() {
        let sends = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(CountingTransport {
            sends: Arc::clone(&sends),
        });
        let source = Arc::new(StaticAuthority {
            snapshot: PersistedAuthoritySnapshot {
                product_task_id: binding().product_task_id,
                workflow_id: binding().workflow_id,
                node_id: binding().node_id,
                attempt_id: binding().attempt_id,
                spend_authorization_id: binding().spend_authorization_id,
                attempt_lease_id: binding().attempt_lease_id,
                spend_status: "consumed".to_string(),
                consumed_by_attempt_id: Some("attempt-1".to_string()),
                lease_status: "current".to_string(),
                execution_contract: Some(test_execution_contract()),
            },
        });
        let p = providers(transport);
        let executor = ManagedDeepSeekNodeExecutor::new(
            p.planner,
            p.implementer,
            p.reviewer,
            source,
            ManagedDeepSeekExecutorConfig::default(),
        )
        .unwrap();
        let mut node = input("planning");
        node.node_id = "different-node".to_string();
        let output = executor.execute_node(&node);
        assert_eq!(output.status, "failed");
        assert_eq!(sends.load(Ordering::SeqCst), 0);
    }

    fn planner_response_transport(path: &str) -> PlannerResponseTransport {
        PlannerResponseTransport {
            sends: Arc::new(AtomicUsize::new(0)),
            content: json!({
                "schema_version": "managed_deepseek_plan.v1",
                "status": "planned",
                "path": path,
                "intent": "bounded_product_task"
            })
            .to_string(),
        }
    }

    struct PlannerResponseTransport {
        sends: Arc<AtomicUsize>,
        content: String,
    }

    #[async_trait::async_trait]
    impl HttpTransport for PlannerResponseTransport {
        async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            let body = json!({
                "id": "response-1",
                "model": "deepseek-v4-pro",
                "choices": [{"message": {"role": "assistant", "content": self.content}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 2, "completion_tokens": 1}
            });
            Ok(HttpResponse {
                status: 200,
                body: body.to_string().into_bytes(),
            })
        }
    }

    #[test]
    fn planner_output_path_is_strictly_contained_in_frozen_allowed_paths() {
        // Reuses the frozen RWE allowed paths (apps/api/src, apps/api/tests,
        // README.md) via the same path gate the armed coordinator uses.
        let _lock = crate::cli::config::cli_env_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let had_credential = std::env::var_os(DEEPSEEK_CREDENTIAL_REFERENCE);
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, "test-credential");
        let run_planner = |path: &str| -> String {
            let transport = planner_response_transport(path);
            let sends = Arc::clone(&transport.sends);
            let source = Arc::new(StaticAuthority {
                snapshot: PersistedAuthoritySnapshot {
                    product_task_id: binding().product_task_id,
                    workflow_id: binding().workflow_id,
                    node_id: binding().node_id,
                    attempt_id: binding().attempt_id,
                    spend_authorization_id: binding().spend_authorization_id,
                    attempt_lease_id: binding().attempt_lease_id,
                    spend_status: "consumed".to_string(),
                    consumed_by_attempt_id: Some("attempt-1".to_string()),
                    lease_status: "current".to_string(),
                    execution_contract: Some(test_execution_contract()),
                },
            });
            let p = providers(Arc::new(transport));
            let executor = ManagedDeepSeekNodeExecutor::new(
                p.planner,
                p.implementer,
                p.reviewer,
                source,
                ManagedDeepSeekExecutorConfig::default(),
            )
            .unwrap();
            let output = executor.execute_node(&input("planning"));
            assert_eq!(
                sends.load(Ordering::SeqCst),
                1,
                "planner provider call observed"
            );
            format!(
                "{}|{}",
                output.status,
                output.error_message.clone().unwrap_or_default()
            )
        };
        // Children of allowed directories are admitted.
        let accepted = run_planner("apps/api/src/main.py");
        assert!(accepted.starts_with("completed"), "{accepted}");
        // Parent of an allowed entry is never admitted.
        let parent = run_planner("apps/api");
        assert!(
            parent.contains("outside the bounded plan schema"),
            "{parent}"
        );
        // Traversal escapes fail closed.
        let traversal = run_planner("apps/api/src/../../escape");
        assert!(
            traversal.contains("outside the bounded plan schema"),
            "{traversal}"
        );
        // Pseudo children of the file entry README.md fail closed.
        let pseudo_child = run_planner("README.md/child");
        assert!(
            pseudo_child.contains("outside the bounded plan schema"),
            "{pseudo_child}"
        );
        match had_credential {
            Some(value) => std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, value),
            None => std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE),
        }
    }
}
