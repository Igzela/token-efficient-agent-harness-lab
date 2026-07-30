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
use crate::node_executor::{NodeExecutionInput, NodeExecutionOutput, NodeExecutor};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const MANAGED_DEEPSEEK_EXECUTOR_TYPE: &str = "managed_deepseek";
const MANAGED_DEEPSEEK_NODE_METADATA: &str = "managed_deepseek";

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
                max_requests: 4,
                max_retries: 0,
                max_input_tokens: 8_000,
                max_output_tokens: 4_000,
                max_cumulative_tokens: 12_000,
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
            "2026-07-30T00:00:00Z",
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
                "2026-07-30T00:00:00Z",
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
            effect: super::managed_deepseek::ManagedFailureEffect::NoExternalEffect,
        })?
    }

    fn output(
        &self,
        request: &ManagedProviderCallRequest,
        response: &ManagedProviderResponse,
    ) -> String {
        json!({
            "schema_version": "managed_deepseek_node_output.v1",
            "provider_kind": response.provider_kind,
            "protocol": response.protocol,
            "requested_model": response.requested_model,
            "resolved_model": response.resolved_model,
            "request_id": response.request_id,
            "output_sha256": hex::encode(Sha256::digest(response.output_text.as_bytes())),
            "output_bytes": response.output_text.len(),
            "route_stage": request.role,
        })
        .to_string()
    }
}

impl NodeExecutor for ManagedDeepSeekNodeExecutor {
    fn executor_type_name(&self) -> &str {
        MANAGED_DEEPSEEK_EXECUTOR_TYPE
    }

    fn execute_node(&self, input: &NodeExecutionInput) -> NodeExecutionOutput {
        let started = std::time::Instant::now();
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
            Ok(response) => NodeExecutionOutput {
                status: "completed".to_string(),
                executor_type: MANAGED_DEEPSEEK_EXECUTOR_TYPE.to_string(),
                output: Some(self.output(&request, &response)),
                error_domain: None,
                error_message: None,
                input_tokens: i64::try_from(response.usage.input_tokens).ok(),
                output_tokens: i64::try_from(response.usage.output_tokens).ok(),
                estimated_cost: response.estimated_cost_usd,
                latency_ms: Some(started.elapsed().as_millis() as i64),
                process_outcome: None,
                resolved_model: Some(response.resolved_model),
            },
            Err(error) => failed(error.to_string(), started.elapsed().as_millis() as i64),
        }
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
        PersistedAuthoritySnapshot, DEEPSEEK_CREDENTIAL_REFERENCE,
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
        NodeExecutionInput {
            node_id: binding.node_id.clone(),
            task_type: MANAGED_DEEPSEEK_EXECUTOR_TYPE.to_string(),
            run_id: binding.workflow_id.clone(),
            workflow_id: binding.workflow_id.clone(),
            node_metadata: json!({
                "managed_deepseek": {
                    "stage": stage,
                    "role": "planner",
                    "binding": binding,
                    "prompt": "bounded planning request"
                }
            }),
        }
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
}
