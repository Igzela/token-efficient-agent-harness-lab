//! Provider-free, ProductTask-bound DeepSeek managed-call protocol boundary.
//!
//! This module owns protocol validation, wire parsing, usage normalization, and
//! conservative transient reservation.  It does not own persisted ProductTask
//! state, spend, leases, journal rows, approval, output, or audit authority;
//! those facts are supplied and revalidated by the existing store/runtime owner.
//!
//! Official protocol sources, verified 2026-07-31:
//! - https://api-docs.deepseek.com/api/create-chat-completion
//! - https://api-docs.deepseek.com/guides/anthropic_api
//! - https://api-docs.deepseek.com/quick_start/pricing/

use std::future::Future;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::execution_usage::endpoint_identity::ProviderEndpointKind;
use crate::execution_usage::protocol_usage::{
    aggregate_stream_usage, usage_from_body, ProtocolTokenUsage,
};

use super::anthropic::AnthropicProvider;
use super::config::{CredentialRef, ProviderConfig};
use super::credential::CredentialBoundary;
use super::openai::OpenAiProvider;
use super::transport::{HttpError, HttpRequest, HttpResponse, HttpTransport};

pub const MANAGED_DEEPSEEK_PROFILE_SCHEMA: &str = "managed_deepseek_profile.v1";
pub const MANAGED_PROVIDER_CALL_SCHEMA: &str = "managed_provider_call.v1";
pub const MANAGED_PROVIDER_RESPONSE_SCHEMA: &str = "managed_provider_response.v1";
pub const DEEPSEEK_USAGE_PARSER_VERSION: &str = "deepseek_usage_parser.v1";
pub const DEEPSEEK_PROVIDER_KIND: &str = "deepseek";
pub const DEEPSEEK_OPENAI_BASE_URL: &str = "https://api.deepseek.com";
pub const DEEPSEEK_ANTHROPIC_BASE_URL: &str = "https://api.deepseek.com/anthropic";
pub const DEEPSEEK_OPENAI_PATH: &str = "/chat/completions";
pub const DEEPSEEK_ANTHROPIC_PATH: &str = "/v1/messages";
pub const DEEPSEEK_CREDENTIAL_REFERENCE: &str = "DEEPSEEK_API_KEY";
pub const DEEPSEEK_PROFILE_VERIFIED_AT: &str = "2026-07-31";

pub const DEEPSEEK_PROTOCOL_SOURCES: &[&str] = &[
    "https://api-docs.deepseek.com/api/create-chat-completion",
    "https://api-docs.deepseek.com/guides/anthropic_api",
    "https://api-docs.deepseek.com/quick_start/pricing/",
];

pub const DEEPSEEK_MODELS: &[&str] = &["deepseek-v4-flash", "deepseek-v4-pro"];

/// Stable non-secret binding for the current store-issued attempt lease.
/// The lease token itself never enters a provider request or public evidence.
pub fn managed_attempt_lease_id(lease_token: &str) -> String {
    hex::encode(Sha256::digest(lease_token.as_bytes()))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeepSeekProtocol {
    OpenAiCompatible,
    AnthropicCompatible,
}

impl DeepSeekProtocol {
    pub fn base_url(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => DEEPSEEK_OPENAI_BASE_URL,
            Self::AnthropicCompatible => DEEPSEEK_ANTHROPIC_BASE_URL,
        }
    }

    pub fn endpoint_path(self) -> &'static str {
        match self {
            Self::OpenAiCompatible => DEEPSEEK_OPENAI_PATH,
            Self::AnthropicCompatible => DEEPSEEK_ANTHROPIC_PATH,
        }
    }

    fn endpoint_kind(self) -> ProviderEndpointKind {
        match self {
            Self::OpenAiCompatible => ProviderEndpointKind::OpenAiChatCompletions,
            Self::AnthropicCompatible => ProviderEndpointKind::AnthropicMessages,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagedModelRole {
    Planner,
    Implementer,
    Reviewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedRouteStage {
    Planning,
    Implementation,
    DeterministicVerification,
    Review,
}

pub fn default_managed_route() -> [(ManagedRouteStage, Option<ManagedModelRole>); 4] {
    [
        (ManagedRouteStage::Planning, Some(ManagedModelRole::Planner)),
        (
            ManagedRouteStage::Implementation,
            Some(ManagedModelRole::Implementer),
        ),
        (ManagedRouteStage::DeterministicVerification, None),
        (ManagedRouteStage::Review, Some(ManagedModelRole::Reviewer)),
    ]
}

impl ManagedModelRole {
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Planner | Self::Reviewer => "deepseek-v4-pro",
            Self::Implementer => "deepseek-v4-flash",
        }
    }
}

pub fn default_role_model(role: ManagedModelRole) -> &'static str {
    role.default_model()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeepSeekVersionedProfile {
    pub schema_version: String,
    pub provider_kind: String,
    pub models: Vec<String>,
    pub openai_base_url: String,
    pub openai_endpoint_path: String,
    pub anthropic_base_url: String,
    pub anthropic_endpoint_path: String,
    pub credential_reference: String,
    pub source_urls: Vec<String>,
    pub verified_at: String,
    pub price_profile_id: String,
    pub usage_parser_version: String,
}

impl Default for DeepSeekVersionedProfile {
    fn default() -> Self {
        Self {
            schema_version: MANAGED_DEEPSEEK_PROFILE_SCHEMA.to_string(),
            provider_kind: DEEPSEEK_PROVIDER_KIND.to_string(),
            models: DEEPSEEK_MODELS.iter().map(|v| (*v).to_string()).collect(),
            openai_base_url: DEEPSEEK_OPENAI_BASE_URL.to_string(),
            openai_endpoint_path: DEEPSEEK_OPENAI_PATH.to_string(),
            anthropic_base_url: DEEPSEEK_ANTHROPIC_BASE_URL.to_string(),
            anthropic_endpoint_path: DEEPSEEK_ANTHROPIC_PATH.to_string(),
            credential_reference: DEEPSEEK_CREDENTIAL_REFERENCE.to_string(),
            source_urls: DEEPSEEK_PROTOCOL_SOURCES
                .iter()
                .map(|v| (*v).to_string())
                .collect(),
            verified_at: DEEPSEEK_PROFILE_VERIFIED_AT.to_string(),
            price_profile_id: "deepseek-v4-usd-2026-07-31".to_string(),
            usage_parser_version: "protocol_usage.v1".to_string(),
        }
    }
}

impl DeepSeekVersionedProfile {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != MANAGED_DEEPSEEK_PROFILE_SCHEMA
            || self.provider_kind != DEEPSEEK_PROVIDER_KIND
            || self.credential_reference != DEEPSEEK_CREDENTIAL_REFERENCE
            || self.openai_base_url != DEEPSEEK_OPENAI_BASE_URL
            || self.openai_endpoint_path != DEEPSEEK_OPENAI_PATH
            || self.anthropic_base_url != DEEPSEEK_ANTHROPIC_BASE_URL
            || self.anthropic_endpoint_path != DEEPSEEK_ANTHROPIC_PATH
        {
            return Err("DeepSeek profile identity or route is not canonical".to_string());
        }
        if self.models
            != DEEPSEEK_MODELS
                .iter()
                .map(|v| (*v).to_string())
                .collect::<Vec<_>>()
        {
            return Err("DeepSeek profile model allowlist is not canonical".to_string());
        }
        if self.source_urls
            != DEEPSEEK_PROTOCOL_SOURCES
                .iter()
                .map(|v| (*v).to_string())
                .collect::<Vec<_>>()
        {
            return Err("DeepSeek profile source URLs are not canonical".to_string());
        }
        if self.verified_at != DEEPSEEK_PROFILE_VERIFIED_AT || self.price_profile_id.is_empty() {
            return Err(
                "DeepSeek profile verification or pricing provenance is missing".to_string(),
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagedMessage {
    pub role: String,
    pub content: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ManagedToolCall>>,
}

impl ManagedMessage {
    pub fn text(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: Value::String(content.to_string()),
            name: None,
            tool_call_id: None,
            tool_calls: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagedToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ManagedFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagedFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagedTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ManagedToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagedToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ThinkingConfiguration {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

impl Default for ThinkingConfiguration {
    fn default() -> Self {
        Self {
            mode: "enabled".to_string(),
            reasoning_effort: Some("high".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagedCallLimits {
    pub max_requests: u64,
    pub max_retries: u64,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_cumulative_tokens: u64,
    pub timeout_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_usd: Option<f64>,
}

impl Default for ManagedCallLimits {
    fn default() -> Self {
        Self {
            max_requests: 1,
            max_retries: 0,
            max_input_tokens: 7904,
            max_output_tokens: 4096,
            max_cumulative_tokens: 12_000,
            timeout_ms: 30_000,
            max_cost_usd: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedCallBinding {
    pub product_task_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub attempt_id: String,
    pub spend_authorization_id: String,
    pub attempt_lease_id: String,
}

impl ManagedCallBinding {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("product_task_id", &self.product_task_id),
            ("workflow_id", &self.workflow_id),
            ("node_id", &self.node_id),
            ("attempt_id", &self.attempt_id),
            ("spend_authorization_id", &self.spend_authorization_id),
            ("attempt_lease_id", &self.attempt_lease_id),
        ] {
            if value.trim().is_empty() {
                return Err(format!("managed provider binding requires {name}"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeepSeekPriceProfile {
    pub profile_id: String,
    pub source_url: String,
    pub verified_at: String,
    pub current: bool,
    pub flash_cache_hit_per_million_usd: f64,
    pub flash_cache_miss_per_million_usd: f64,
    pub flash_output_per_million_usd: f64,
    pub pro_cache_hit_per_million_usd: f64,
    pub pro_cache_miss_per_million_usd: f64,
    pub pro_output_per_million_usd: f64,
}

impl Default for DeepSeekPriceProfile {
    fn default() -> Self {
        Self {
            profile_id: "deepseek-v4-usd-2026-07-31".to_string(),
            source_url: "https://api-docs.deepseek.com/quick_start/pricing/".to_string(),
            verified_at: DEEPSEEK_PROFILE_VERIFIED_AT.to_string(),
            current: true,
            flash_cache_hit_per_million_usd: 0.0028,
            flash_cache_miss_per_million_usd: 0.14,
            flash_output_per_million_usd: 0.28,
            pro_cache_hit_per_million_usd: 0.003625,
            pro_cache_miss_per_million_usd: 0.435,
            pro_output_per_million_usd: 0.87,
        }
    }
}

impl DeepSeekPriceProfile {
    pub fn validate_for(&self, model: &str, require_current: bool) -> Result<(), String> {
        if self.profile_id.is_empty()
            || self.source_url != "https://api-docs.deepseek.com/quick_start/pricing/"
            || self.verified_at.trim().is_empty()
            || (require_current && !self.current)
        {
            return Err("DeepSeek dollar price evidence is missing or stale".to_string());
        }
        if !DEEPSEEK_MODELS.contains(&model) {
            return Err("DeepSeek price profile model is not admitted".to_string());
        }
        let rates = if model == "deepseek-v4-pro" {
            [
                self.pro_cache_hit_per_million_usd,
                self.pro_cache_miss_per_million_usd,
                self.pro_output_per_million_usd,
            ]
        } else {
            [
                self.flash_cache_hit_per_million_usd,
                self.flash_cache_miss_per_million_usd,
                self.flash_output_per_million_usd,
            ]
        };
        if rates.iter().any(|v| !v.is_finite() || *v < 0.0) {
            return Err("DeepSeek price profile contains invalid rates".to_string());
        }
        Ok(())
    }

    pub fn estimate_usd(&self, model: &str, usage: &ManagedUsage) -> Result<f64, String> {
        self.validate_for(model, true)?;
        let (hit, miss, output) = if model == "deepseek-v4-pro" {
            (
                self.pro_cache_hit_per_million_usd,
                self.pro_cache_miss_per_million_usd,
                self.pro_output_per_million_usd,
            )
        } else {
            (
                self.flash_cache_hit_per_million_usd,
                self.flash_cache_miss_per_million_usd,
                self.flash_output_per_million_usd,
            )
        };
        Ok((usage.cache_read_tokens as f64 * hit
            + usage.cache_creation_tokens as f64 * miss
            + usage.fresh_input_tokens as f64 * miss
            + usage.output_tokens as f64 * output)
            / 1_000_000.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagedProviderCallRequest {
    pub schema_version: String,
    pub provider_kind: String,
    pub protocol: DeepSeekProtocol,
    pub host: String,
    pub base_url: String,
    pub endpoint_path: String,
    pub credential_reference: String,
    pub role: ManagedModelRole,
    pub requested_model: String,
    pub system: Option<String>,
    pub messages: Vec<ManagedMessage>,
    #[serde(default)]
    pub tools: Vec<ManagedTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    pub thinking: ThinkingConfiguration,
    pub max_output_tokens: u64,
    pub stream: bool,
    pub limits: ManagedCallLimits,
    pub price_profile: DeepSeekPriceProfile,
    pub binding: ManagedCallBinding,
}

impl ManagedProviderCallRequest {
    pub fn for_role(
        role: ManagedModelRole,
        protocol: DeepSeekProtocol,
        binding: ManagedCallBinding,
    ) -> Self {
        let profile = DeepSeekVersionedProfile::default();
        Self {
            schema_version: MANAGED_PROVIDER_CALL_SCHEMA.to_string(),
            provider_kind: DEEPSEEK_PROVIDER_KIND.to_string(),
            protocol,
            host: "api.deepseek.com".to_string(),
            base_url: protocol.base_url().to_string(),
            endpoint_path: protocol.endpoint_path().to_string(),
            credential_reference: profile.credential_reference,
            role,
            requested_model: role.default_model().to_string(),
            system: None,
            messages: vec![ManagedMessage::text("user", "")],
            tools: Vec::new(),
            tool_choice: None,
            thinking: ThinkingConfiguration::default(),
            max_output_tokens: 1024,
            stream: false,
            limits: ManagedCallLimits::default(),
            price_profile: DeepSeekPriceProfile::default(),
            binding,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != MANAGED_PROVIDER_CALL_SCHEMA
            || self.provider_kind != DEEPSEEK_PROVIDER_KIND
            || self.host != "api.deepseek.com"
            || self.credential_reference != DEEPSEEK_CREDENTIAL_REFERENCE
            || self.base_url != self.protocol.base_url()
            || self.endpoint_path != self.protocol.endpoint_path()
        {
            return Err("managed DeepSeek provider identity or route is not canonical".to_string());
        }
        if !DEEPSEEK_MODELS.contains(&self.requested_model.as_str()) {
            return Err("requested model is not an admitted DeepSeek identity".to_string());
        }
        if self.requested_model != self.role.default_model() {
            return Err("requested model does not match the bounded role route".to_string());
        }
        self.binding.validate()?;
        if self.messages.is_empty() || self.max_output_tokens == 0 {
            return Err(
                "managed provider call requires messages and positive output limit".to_string(),
            );
        }
        if self.max_output_tokens > self.limits.max_output_tokens
            || self.max_output_tokens > 384_000
            || self.limits.max_input_tokens == 0
            || self.limits.max_input_tokens > self.limits.max_cumulative_tokens
            || self
                .limits
                .max_input_tokens
                .saturating_add(self.max_output_tokens)
                > self.limits.max_cumulative_tokens
            || self.limits.max_requests == 0
            || self.limits.max_cumulative_tokens < self.max_output_tokens
            || self.limits.timeout_ms == 0
        {
            return Err("managed provider call exceeds its bounded envelope".to_string());
        }
        validate_messages(&self.messages)?;
        validate_tools(&self.tools, self.tool_choice.as_ref())?;
        validate_thinking(&self.thinking)?;
        if self.estimated_input_tokens() > self.limits.max_input_tokens {
            return Err("managed input ceiling exceeded before send".to_string());
        }
        if let Some(max_cost_usd) = self.limits.max_cost_usd {
            self.price_profile
                .validate_for(&self.requested_model, true)?;
            let worst_case_rate = if self.requested_model == "deepseek-v4-pro" {
                self.price_profile
                    .pro_cache_miss_per_million_usd
                    .max(self.price_profile.pro_output_per_million_usd)
            } else {
                self.price_profile
                    .flash_cache_miss_per_million_usd
                    .max(self.price_profile.flash_output_per_million_usd)
            };
            let conservative_cost =
                self.limits.max_cumulative_tokens as f64 * worst_case_rate / 1_000_000.0;
            if conservative_cost > max_cost_usd {
                return Err(
                    "managed dollar ceiling is below the conservative pre-send reservation"
                        .to_string(),
                );
            }
        }
        Ok(())
    }

    pub fn url(&self) -> String {
        format!(
            "{}{}",
            self.base_url.trim_end_matches('/'),
            self.endpoint_path
        )
    }

    pub(crate) fn estimated_input_tokens(&self) -> u64 {
        // Conservative pre-send upper bound: count UTF-8 bytes of the request
        // body surfaces. Bytes are never smaller than token counts for admitted
        // text, so this cannot under-reserve max_input_tokens.
        let mut bytes = self
            .system
            .as_deref()
            .map_or(0, str::len)
            .saturating_add(serde_json::to_vec(&self.messages).map_or(0, |v| v.len()));
        bytes = bytes.saturating_add(serde_json::to_vec(&self.tools).map_or(0, |v| v.len()));
        bytes = bytes.saturating_add(
            self.tool_choice
                .as_ref()
                .and_then(|v| serde_json::to_vec(v).ok())
                .map_or(0, |v| v.len()),
        );
        bytes as u64
    }

    pub(crate) fn conservative_reserved_cost_usd(&self) -> Result<f64, String> {
        if self.limits.max_cost_usd.is_none() {
            return Ok(0.0);
        }
        self.price_profile
            .validate_for(&self.requested_model, true)?;
        let worst_case_rate = if self.requested_model == "deepseek-v4-pro" {
            self.price_profile
                .pro_cache_miss_per_million_usd
                .max(self.price_profile.pro_output_per_million_usd)
        } else {
            self.price_profile
                .flash_cache_miss_per_million_usd
                .max(self.price_profile.flash_output_per_million_usd)
        };
        Ok((self
            .estimated_input_tokens()
            .saturating_add(self.max_output_tokens)) as f64
            * worst_case_rate
            / 1_000_000.0)
    }
}

fn validate_messages(messages: &[ManagedMessage]) -> Result<(), String> {
    for message in messages {
        if !matches!(
            message.role.as_str(),
            "system" | "user" | "assistant" | "tool"
        ) {
            return Err("message role is not admitted".to_string());
        }
        if message.content.is_null() && message.tool_calls.is_none() {
            return Err("message content is missing".to_string());
        }
        if message.role == "tool" && message.tool_call_id.as_deref().unwrap_or("").is_empty() {
            return Err("tool result requires tool_call_id".to_string());
        }
        if let Some(calls) = &message.tool_calls {
            if message.role != "assistant" || calls.is_empty() || calls.len() > 8 {
                return Err("assistant tool calls are malformed or unbounded".to_string());
            }
            for call in calls {
                if call.call_type != "function"
                    || call.id.is_empty()
                    || call.function.name.is_empty()
                {
                    return Err("tool call identity is malformed".to_string());
                }
                let arguments = serde_json::from_str::<Value>(&call.function.arguments)
                    .map_err(|_| "tool call arguments are not valid JSON".to_string())?;
                if !arguments.is_object() {
                    return Err("tool call arguments must be a JSON object".to_string());
                }
            }
        }
    }
    Ok(())
}

fn validate_tools(tools: &[ManagedTool], choice: Option<&Value>) -> Result<(), String> {
    if tools.len() > 8 {
        return Err("tool count exceeds managed bound".to_string());
    }
    for tool in tools {
        if tool.tool_type != "function"
            || tool.function.name.is_empty()
            || tool.function.name.len() > 64
            || tool.function.description.is_empty()
            || tool.function.description.len() > 256
            || tool.function.parameters.get("type").and_then(Value::as_str) != Some("object")
        {
            return Err("bounded tool definition is malformed".to_string());
        }
    }
    if choice.is_some() && tools.is_empty() {
        return Err("tool_choice requires tools".to_string());
    }
    if let Some(choice) = choice {
        let valid = choice
            .as_str()
            .is_some_and(|value| matches!(value, "auto" | "none" | "required"))
            || choice
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|value| value == "function")
                && choice
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| tools.iter().any(|tool| tool.function.name == name));
        if !valid {
            return Err("tool_choice is outside the bounded allowlist".to_string());
        }
    }
    Ok(())
}

fn validate_thinking(thinking: &ThinkingConfiguration) -> Result<(), String> {
    if !matches!(thinking.mode.as_str(), "enabled" | "disabled") {
        return Err("thinking mode is not admitted".to_string());
    }
    if let Some(effort) = &thinking.reasoning_effort {
        if !matches!(effort.as_str(), "high" | "max") {
            return Err("reasoning effort must be high or max".to_string());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub fresh_input_tokens: u64,
    pub cumulative_tokens: u64,
    pub model: String,
    pub request_id: String,
}

impl ManagedUsage {
    fn from_protocol(
        usage: ProtocolTokenUsage,
        requested_model: &str,
    ) -> Result<Self, ManagedProviderCallError> {
        let model = usage
            .model
            .as_ref()
            .filter(|v| !v.is_empty())
            .cloned()
            .ok_or_else(|| {
                ManagedProviderCallError::invalid_response("usage model identity missing")
            })?;
        let request_id = usage
            .message_or_response_id
            .as_ref()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                ManagedProviderCallError::invalid_response("usage request identity missing")
            })?
            .clone();
        if model != requested_model {
            return Err(ManagedProviderCallError::identity(
                "usage model conflicts with request",
            ));
        }
        let buckets = usage.try_to_canonical_buckets().map_err(|anomaly| {
            ManagedProviderCallError::invalid_response(format!(
                "usage normalization rejected: {:?}",
                anomaly.reasons()
            ))
        })?;
        Ok(Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
            fresh_input_tokens: buckets.fresh_input,
            cumulative_tokens: buckets.billable_token_total(),
            model,
            request_id,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManagedProviderResponse {
    pub schema_version: String,
    pub provider_kind: String,
    pub protocol: DeepSeekProtocol,
    pub requested_model: String,
    pub resolved_model: String,
    pub request_id: String,
    pub output_text: String,
    pub tool_calls: Vec<ManagedToolCall>,
    pub stop_reason: String,
    pub usage: ManagedUsage,
    pub estimated_cost_usd: Option<f64>,
    pub stream: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedFailureEffect {
    PreSend,
    OutcomeUnknown,
    NoExternalEffect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedProviderCallError {
    pub domain: String,
    pub message: String,
    pub retryable: bool,
    pub effect: ManagedFailureEffect,
}

impl ManagedProviderCallError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            domain: "provider_request".to_string(),
            message: message.into(),
            retryable: false,
            effect: ManagedFailureEffect::NoExternalEffect,
        }
    }
    fn invalid_response(message: impl Into<String>) -> Self {
        Self {
            domain: "provider_response".to_string(),
            message: message.into(),
            retryable: false,
            effect: ManagedFailureEffect::OutcomeUnknown,
        }
    }
    fn identity(message: impl Into<String>) -> Self {
        Self {
            domain: "provider_identity".to_string(),
            message: message.into(),
            retryable: false,
            effect: ManagedFailureEffect::OutcomeUnknown,
        }
    }
    fn from_http(error: HttpError) -> Self {
        match error {
            HttpError::PreSend(message) => Self {
                domain: "provider_pre_send".to_string(),
                message: sanitize_error(&message),
                retryable: true,
                effect: ManagedFailureEffect::PreSend,
            },
            HttpError::Http { status, .. } if status == 401 || status == 403 => Self {
                domain: "provider_auth".to_string(),
                message: "DeepSeek authentication failed".to_string(),
                retryable: false,
                effect: ManagedFailureEffect::OutcomeUnknown,
            },
            HttpError::Http { status, .. } if status == 429 || status >= 500 => Self {
                domain: "provider_capacity".to_string(),
                message: format!("DeepSeek HTTP {status}"),
                retryable: true,
                effect: ManagedFailureEffect::OutcomeUnknown,
            },
            HttpError::Http { status, .. } => Self {
                domain: "provider_error".to_string(),
                message: format!("DeepSeek HTTP {status}"),
                retryable: false,
                effect: ManagedFailureEffect::OutcomeUnknown,
            },
            HttpError::Timeout(_) => Self {
                domain: "provider_timeout".to_string(),
                message: "DeepSeek request timed out; outcome is unknown".to_string(),
                retryable: false,
                effect: ManagedFailureEffect::OutcomeUnknown,
            },
            HttpError::Connection(_) => Self {
                domain: "provider_connection".to_string(),
                message: "DeepSeek connection outcome is unknown".to_string(),
                retryable: false,
                effect: ManagedFailureEffect::OutcomeUnknown,
            },
            HttpError::Parse(_) => Self {
                domain: "provider_response".to_string(),
                message: "DeepSeek response transport was malformed".to_string(),
                retryable: false,
                effect: ManagedFailureEffect::OutcomeUnknown,
            },
        }
    }
}

impl std::fmt::Display for ManagedProviderCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.domain, self.message)
    }
}

impl std::error::Error for ManagedProviderCallError {}

fn sanitize_error(message: &str) -> String {
    if message.contains(DEEPSEEK_CREDENTIAL_REFERENCE) {
        "DeepSeek credential resolution failed".to_string()
    } else {
        message.chars().take(256).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedReservationState {
    pub reserved_requests: u64,
    pub reserved_input_tokens: u64,
    pub observed_requests: u64,
    pub reserved_output_tokens: u64,
    pub cumulative_tokens: u64,
    pub cumulative_cost_usd: f64,
}

#[derive(Debug, Clone)]
pub struct ManagedBudgetLedger {
    limits: ManagedCallLimits,
    state: Arc<Mutex<ManagedReservationState>>,
}

impl ManagedBudgetLedger {
    pub fn new(limits: ManagedCallLimits) -> Result<Self, String> {
        if limits.max_requests == 0
            || limits.max_output_tokens == 0
            || limits.max_cumulative_tokens == 0
            || limits.max_input_tokens == 0
            || limits
                .max_input_tokens
                .saturating_add(limits.max_output_tokens)
                > limits.max_cumulative_tokens
        {
            return Err("managed budget limits must be positive".to_string());
        }
        if let Some(cost) = limits.max_cost_usd {
            if !cost.is_finite() || cost < 0.0 {
                return Err("managed dollar ceiling must be finite and non-negative".to_string());
            }
        }
        Ok(Self {
            limits,
            state: Arc::new(Mutex::new(ManagedReservationState {
                reserved_requests: 0,
                reserved_input_tokens: 0,
                observed_requests: 0,
                reserved_output_tokens: 0,
                cumulative_tokens: 0,
                cumulative_cost_usd: 0.0,
            })),
        })
    }

    pub fn reserve_before_send(
        &self,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<(), ManagedProviderCallError> {
        let mut state = self.state.lock().map_err(|_| {
            ManagedProviderCallError::invalid_request("managed budget lock poisoned")
        })?;
        if state.reserved_requests >= self.limits.max_requests {
            return Err(ManagedProviderCallError::invalid_request(
                "managed request ceiling exhausted",
            ));
        }
        if output_tokens == 0 || output_tokens > self.limits.max_output_tokens {
            return Err(ManagedProviderCallError::invalid_request(
                "managed output ceiling exceeded before send",
            ));
        }
        if input_tokens == 0 || input_tokens > self.limits.max_input_tokens {
            return Err(ManagedProviderCallError::invalid_request(
                "managed input ceiling exceeded before send",
            ));
        }
        if state
            .cumulative_tokens
            .saturating_add(state.reserved_input_tokens)
            .saturating_add(state.reserved_output_tokens)
            .saturating_add(input_tokens)
            .saturating_add(output_tokens)
            > self.limits.max_cumulative_tokens
        {
            return Err(ManagedProviderCallError::invalid_request(
                "managed cumulative token ceiling exceeded before send",
            ));
        }
        state.reserved_requests += 1;
        state.reserved_input_tokens = state.reserved_input_tokens.saturating_add(input_tokens);
        state.reserved_output_tokens = state.reserved_output_tokens.saturating_add(output_tokens);
        Ok(())
    }

    pub fn reconcile(
        &self,
        response: Option<&ManagedProviderResponse>,
    ) -> Result<ManagedReservationState, ManagedProviderCallError> {
        let mut state = self.state.lock().map_err(|_| {
            ManagedProviderCallError::invalid_request("managed budget lock poisoned")
        })?;
        state.observed_requests = state.observed_requests.saturating_add(1);
        // Sequential managed stages share one ledger with a single in-flight
        // reservation. Release the full reservation after each attempt so
        // unused max_output headroom cannot starve later stages.
        state.reserved_input_tokens = 0;
        state.reserved_output_tokens = 0;
        if let Some(response) = response {
            state.cumulative_tokens = state
                .cumulative_tokens
                .saturating_add(response.usage.cumulative_tokens);
            if let Some(cost) = response.estimated_cost_usd {
                state.cumulative_cost_usd += cost;
                if self
                    .limits
                    .max_cost_usd
                    .is_some_and(|max| state.cumulative_cost_usd > max)
                {
                    return Err(ManagedProviderCallError::invalid_response(
                        "managed dollar ceiling exceeded",
                    ));
                }
            }
            if state.cumulative_tokens > self.limits.max_cumulative_tokens {
                return Err(ManagedProviderCallError::invalid_response(
                    "managed cumulative token ceiling exceeded",
                ));
            }
        }
        Ok(state.clone())
    }

    pub fn snapshot(&self) -> Result<ManagedReservationState, String> {
        self.state
            .lock()
            .map(|v| v.clone())
            .map_err(|_| "managed budget lock poisoned".to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersistedAuthoritySnapshot {
    pub product_task_id: String,
    pub workflow_id: String,
    pub node_id: String,
    pub attempt_id: String,
    pub spend_authorization_id: String,
    pub attempt_lease_id: String,
    pub spend_status: String,
    pub consumed_by_attempt_id: Option<String>,
    pub lease_status: String,
    /// Present only when an immutable final execution manifest binds the
    /// complete provider request contract. A status-only legacy authority
    /// remains inspectable but cannot authorize a managed DeepSeek send.
    pub execution_contract: Option<PersistedManagedExecutionContract>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PersistedManagedExecutionContract {
    pub provider_kind: String,
    pub protocol: DeepSeekProtocol,
    pub host: String,
    pub base_url: String,
    pub endpoint_path: String,
    pub request_schema_version: String,
    pub response_schema_version: String,
    pub usage_parser_version: String,
    pub requested_model: String,
    pub limits: ManagedCallLimits,
    pub price_profile: DeepSeekPriceProfile,
}

pub trait ManagedAuthoritySource: Send + Sync {
    fn current_authority(
        &self,
        binding: &ManagedCallBinding,
    ) -> Result<PersistedAuthoritySnapshot, String>;

    /// Atomically persist a redacted pre-send network-effect claim through the
    /// existing authority owner. A production source must reject duplicate,
    /// recovered, or outcome-unknown claims before transport.
    fn claim_provider_request(&self, _request: &ManagedProviderCallRequest) -> Result<(), String> {
        Err("durable managed provider request journal is unavailable".into())
    }

    /// Reconcile the exact durable claim without persisting prompt or output
    /// content. Failure to reconcile leaves the claim non-retryable.
    fn reconcile_provider_request(
        &self,
        _request: &ManagedProviderCallRequest,
        _response: Option<&ManagedProviderResponse>,
        _effect: ManagedFailureEffect,
    ) -> Result<(), String> {
        Err("durable managed provider request reconciliation is unavailable".into())
    }

    /// Build bounded, request-time-only stage context from persisted typed
    /// receipts and the currently bound workspace. Implementations must not
    /// persist raw repository content or provider request bodies.
    fn stage_context(
        &self,
        _binding: &ManagedCallBinding,
        _node_metadata: &Value,
    ) -> Result<Option<Value>, String> {
        Ok(None)
    }

    /// Apply a typed implementer action through the existing ProductTask
    /// workspace owner. Provider adapters never mutate a workspace directly.
    fn apply_workspace_action(
        &self,
        _binding: &ManagedCallBinding,
        _node_metadata: &Value,
        _model_output: &str,
    ) -> Result<Value, String> {
        Err("managed workspace action sink is unavailable".to_string())
    }
}

#[derive(Clone)]
pub struct ManagedProviderCallAuthority {
    source: Arc<dyn ManagedAuthoritySource>,
    budget: ManagedBudgetLedger,
}

impl ManagedProviderCallAuthority {
    pub fn new(
        source: Arc<dyn ManagedAuthoritySource>,
        limits: ManagedCallLimits,
    ) -> Result<Self, String> {
        Ok(Self {
            source,
            budget: ManagedBudgetLedger::new(limits)?,
        })
    }

    pub fn budget(&self) -> &ManagedBudgetLedger {
        &self.budget
    }

    pub fn validate_current_authority(
        &self,
        request: &ManagedProviderCallRequest,
    ) -> Result<(), ManagedProviderCallError> {
        request
            .validate()
            .map_err(ManagedProviderCallError::invalid_request)?;
        let current = self
            .source
            .current_authority(&request.binding)
            .map_err(ManagedProviderCallError::invalid_request)?;
        let contract = current.execution_contract.as_ref().ok_or_else(|| {
            ManagedProviderCallError::invalid_request(
                "persisted managed authority lacks an immutable execution contract",
            )
        })?;
        if current.product_task_id != request.binding.product_task_id
            || current.workflow_id != request.binding.workflow_id
            || current.node_id != request.binding.node_id
            || current.attempt_id != request.binding.attempt_id
            || current.spend_authorization_id != request.binding.spend_authorization_id
            || current.attempt_lease_id != request.binding.attempt_lease_id
            || current.spend_status != "consumed"
            || current.consumed_by_attempt_id.as_deref()
                != Some(request.binding.attempt_id.as_str())
            || current.lease_status != "current"
            || contract.provider_kind != request.provider_kind
            || contract.protocol != request.protocol
            || contract.host != request.host
            || contract.base_url != request.base_url
            || contract.endpoint_path != request.endpoint_path
            || contract.request_schema_version != request.schema_version
            || contract.response_schema_version != MANAGED_PROVIDER_RESPONSE_SCHEMA
            || contract.usage_parser_version != DEEPSEEK_USAGE_PARSER_VERSION
            || contract.requested_model != request.requested_model
            || contract.limits != request.limits
            || contract.price_profile != request.price_profile
        {
            return Err(ManagedProviderCallError::invalid_request(
                "persisted managed authority is stale or mismatched",
            ));
        }
        Ok(())
    }

    /// Execute one bounded provider call under current persisted authority.
    /// The closure is an existing provider client; this coordinator does not
    /// become a scheduler, journal, or durable budget owner.
    pub async fn invoke_with_retry<F, Fut>(
        &self,
        request: &ManagedProviderCallRequest,
        mut send: F,
    ) -> Result<ManagedProviderResponse, ManagedProviderCallError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<ManagedProviderResponse, ManagedProviderCallError>>,
    {
        request
            .validate()
            .map_err(ManagedProviderCallError::invalid_request)?;
        let mut attempts = 0;
        loop {
            self.validate_current_authority(request)?;
            self.budget
                .reserve_before_send(request.estimated_input_tokens(), request.max_output_tokens)?;
            self.source
                .claim_provider_request(request)
                .map_err(ManagedProviderCallError::invalid_request)?;
            attempts += 1;
            match send().await {
                Ok(response) => {
                    self.source
                        .reconcile_provider_request(
                            request,
                            Some(&response),
                            ManagedFailureEffect::NoExternalEffect,
                        )
                        .map_err(|error| ManagedProviderCallError {
                            domain: "provider_reconciliation".into(),
                            message: sanitize_error(&error),
                            retryable: false,
                            effect: ManagedFailureEffect::OutcomeUnknown,
                        })?;
                    self.budget.reconcile(Some(&response))?;
                    return Ok(response);
                }
                Err(error) => {
                    // Release the in-flight reservation on every attempt outcome.
                    // Pre-send failures may retry under max_retries; timeout,
                    // connection, or malformed-response results are outcome-
                    // unknown and are never retried because the provider effect
                    // may already have landed.
                    let _ = self.budget.reconcile(None);
                    self.source
                        .reconcile_provider_request(request, None, error.effect)
                        .map_err(|reconcile_error| ManagedProviderCallError {
                            domain: "provider_reconciliation".into(),
                            message: sanitize_error(&reconcile_error),
                            retryable: false,
                            effect: ManagedFailureEffect::OutcomeUnknown,
                        })?;
                    let retryable_pre_send = error.effect == ManagedFailureEffect::PreSend
                        && error.retryable
                        && attempts <= request.limits.max_retries
                        && attempts < request.limits.max_requests;
                    if retryable_pre_send {
                        continue;
                    }
                    return Err(error);
                }
            }
        }
    }
}

/// Convert provider response evidence into the existing normalized usage event.
/// Prompts, output, credentials, raw headers, and raw response bodies never enter
/// this projection.
pub fn response_to_usage_event(
    request: &ManagedProviderCallRequest,
    response: &ManagedProviderResponse,
    timestamp: &str,
) -> crate::execution_usage::ExecutionUsageEventV1 {
    use crate::execution_usage::{
        stable_usage_event_id, CostSource, EventCompleteness, EvidenceSourceKind,
        ExecutionUsageEventV1, ExecutorKind, EXECUTION_USAGE_EVENT_SCHEMA,
    };
    let token_signature = format!(
        "i{}:c{}:w{}:o{}:r{}",
        response.usage.fresh_input_tokens,
        response.usage.cache_read_tokens,
        response.usage.cache_creation_tokens,
        response
            .usage
            .output_tokens
            .saturating_sub(response.usage.reasoning_output_tokens),
        response.usage.reasoning_output_tokens
    );
    let event_id = stable_usage_event_id(
        EvidenceSourceKind::ProviderResponse,
        &request.binding.attempt_id,
        &response.request_id,
        &token_signature,
        timestamp,
    );
    ExecutionUsageEventV1 {
        schema_version: EXECUTION_USAGE_EVENT_SCHEMA.to_string(),
        event_id: event_id.clone(),
        product_task_id: Some(request.binding.product_task_id.clone()),
        workflow_node_id: Some(request.binding.node_id.clone()),
        managed_execution_id: Some(request.binding.attempt_id.clone()),
        executor_kind: ExecutorKind::ProviderProxy,
        evidence_source_kind: EvidenceSourceKind::ProviderResponse,
        provider_id: Some(DEEPSEEK_PROVIDER_KIND.to_string()),
        requested_model: Some(response.requested_model.clone()),
        resolved_model: Some(response.resolved_model.clone()),
        executable_path_fingerprint: None,
        executable_version: None,
        executable_sha256: None,
        root_session_id: Some(request.binding.attempt_id.clone()),
        parent_session_id: None,
        request_or_message_id: Some(response.request_id.clone()),
        input_tokens: response.usage.fresh_input_tokens,
        cached_input_tokens: response.usage.cache_read_tokens,
        cache_creation_tokens: response.usage.cache_creation_tokens,
        output_tokens: response
            .usage
            .output_tokens
            .saturating_sub(response.usage.reasoning_output_tokens),
        reasoning_output_tokens: response.usage.reasoning_output_tokens,
        cumulative_task_tokens: Some(response.usage.cumulative_tokens),
        provider_reported_cost: None,
        locally_estimated_cost: response.estimated_cost_usd,
        cost_source: if response.estimated_cost_usd.is_some() {
            CostSource::Estimated
        } else {
            CostSource::Unavailable
        },
        pricing_table_version: response
            .estimated_cost_usd
            .map(|_| request.price_profile.profile_id.clone()),
        timestamp: timestamp.to_string(),
        event_completeness: EventCompleteness::Complete,
        source_schema_version: MANAGED_PROVIDER_RESPONSE_SCHEMA.to_string(),
        stable_dedupe_identity: format!(
            "deepseek:{}:{}",
            request.binding.attempt_id, response.request_id
        ),
        provenance_refs: vec![
            format!("protocol:{:?}", request.protocol),
            format!("role:{:?}", request.role),
            format!(
                "spend_authorization:{}",
                request.binding.spend_authorization_id
            ),
        ],
    }
}

pub struct ManagedDeepSeekProvider {
    inner: ManagedDeepSeekInner,
}

enum ManagedDeepSeekInner {
    OpenAi(Arc<OpenAiProvider>),
    Anthropic(Arc<AnthropicProvider>),
}

impl ManagedDeepSeekProvider {
    pub fn new_openai(
        config: ProviderConfig,
        boundary: CredentialBoundary,
        credential: CredentialRef,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        Self {
            inner: ManagedDeepSeekInner::OpenAi(Arc::new(OpenAiProvider::new(
                config, boundary, credential, transport, None,
            ))),
        }
    }

    pub fn new_anthropic(
        config: ProviderConfig,
        boundary: CredentialBoundary,
        credential: CredentialRef,
        transport: Arc<dyn HttpTransport>,
    ) -> Self {
        Self {
            inner: ManagedDeepSeekInner::Anthropic(Arc::new(AnthropicProvider::new(
                config, boundary, credential, transport, None,
            ))),
        }
    }

    async fn invoke(
        &self,
        request: &ManagedProviderCallRequest,
    ) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
        request
            .validate()
            .map_err(ManagedProviderCallError::invalid_request)?;
        match &self.inner {
            ManagedDeepSeekInner::OpenAi(provider) => {
                provider.invoke_managed_deepseek(request).await
            }
            ManagedDeepSeekInner::Anthropic(provider) => {
                provider.invoke_managed_deepseek(request).await
            }
        }
    }

    /// The sole production entry point binds the protocol client to the
    /// existing persisted managed authority and ProductTask-owned budget.
    /// Direct wire invocation remains private to this module and tests.
    pub async fn invoke_with_authority(
        &self,
        authority: &ManagedProviderCallAuthority,
        request: &ManagedProviderCallRequest,
    ) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
        authority
            .invoke_with_retry(request, || self.invoke(request))
            .await
    }
}

pub(crate) async fn invoke_openai_wire(
    config: &ProviderConfig,
    boundary: &CredentialBoundary,
    credential: &CredentialRef,
    transport: &Arc<dyn HttpTransport>,
    request: &ManagedProviderCallRequest,
) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
    invoke_wire(
        DeepSeekProtocol::OpenAiCompatible,
        config,
        boundary,
        credential,
        transport,
        request,
    )
    .await
}

pub(crate) async fn invoke_anthropic_wire(
    config: &ProviderConfig,
    boundary: &CredentialBoundary,
    credential: &CredentialRef,
    transport: &Arc<dyn HttpTransport>,
    request: &ManagedProviderCallRequest,
) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
    invoke_wire(
        DeepSeekProtocol::AnthropicCompatible,
        config,
        boundary,
        credential,
        transport,
        request,
    )
    .await
}

async fn invoke_wire(
    protocol: DeepSeekProtocol,
    config: &ProviderConfig,
    boundary: &CredentialBoundary,
    credential: &CredentialRef,
    transport: &Arc<dyn HttpTransport>,
    request: &ManagedProviderCallRequest,
) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
    request
        .validate()
        .map_err(ManagedProviderCallError::invalid_request)?;
    if !config.enabled
        || credential.credential_ref_id != request.credential_reference
        || config.model_id != request.requested_model
        || request.protocol != protocol
        || config.base_url.trim_end_matches('/') != request.base_url
    {
        return Err(ManagedProviderCallError::invalid_request(
            "provider config or credential identity conflicts with managed request",
        ));
    }
    let api_key = boundary
        .resolve(&credential.credential_ref_id)
        .map_err(|_| ManagedProviderCallError {
            domain: "provider_auth".to_string(),
            message: "DeepSeek credential is absent".to_string(),
            retryable: false,
            effect: ManagedFailureEffect::PreSend,
        })?;
    let body = if protocol == DeepSeekProtocol::OpenAiCompatible {
        openai_body(request)
    } else {
        anthropic_body(request)
    };
    let headers = if protocol == DeepSeekProtocol::OpenAiCompatible {
        vec![
            ("Authorization".to_string(), format!("Bearer {api_key}")),
            ("content-type".to_string(), "application/json".to_string()),
        ]
    } else {
        vec![
            ("x-api-key".to_string(), api_key),
            ("anthropic-version".to_string(), "2023-06-01".to_string()),
            ("content-type".to_string(), "application/json".to_string()),
        ]
    };
    let response = transport
        .send(&HttpRequest {
            url: request.url(),
            method: "POST".to_string(),
            headers,
            body: Some(body.to_string().into_bytes()),
            timeout_secs: Some(request.limits.timeout_ms as f64 / 1000.0),
        })
        .await
        .map_err(ManagedProviderCallError::from_http)?;
    parse_wire_response(protocol, request, response)
}

fn openai_body(request: &ManagedProviderCallRequest) -> Value {
    let mut messages =
        Vec::with_capacity(request.messages.len() + usize::from(request.system.is_some()));
    if let Some(system) = &request.system {
        messages.push(ManagedMessage::text("system", system));
    }
    messages.extend(request.messages.iter().cloned());
    let mut body = json!({
        "model": request.requested_model,
        "messages": messages,
        "thinking": {"type": request.thinking.mode},
        "stream": request.stream,
        "max_tokens": request.max_output_tokens,
    });
    if let Some(effort) = &request.thinking.reasoning_effort {
        body["reasoning_effort"] = Value::String(effort.clone());
    }
    if !request.tools.is_empty() {
        body["tools"] = serde_json::to_value(&request.tools).unwrap_or(Value::Null);
    }
    if let Some(choice) = &request.tool_choice {
        body["tool_choice"] = choice.clone();
    }
    body
}

fn anthropic_body(request: &ManagedProviderCallRequest) -> Value {
    let mut messages = Vec::new();
    let mut system = request.system.clone();
    for message in &request.messages {
        if message.role == "system" {
            if system.is_none() {
                system = message.content.as_str().map(str::to_string);
            }
        } else {
            let mut value = if message.role == "tool" {
                json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": message.tool_call_id.clone(),
                        "content": message.content.clone()
                    }]
                })
            } else {
                json!({"role": message.role, "content": message.content.clone()})
            };
            if let Some(calls) = &message.tool_calls {
                value["content"] = json!(calls.iter().map(|call| json!({"type":"tool_use","id":call.id,"name":call.function.name,"input":serde_json::from_str::<Value>(&call.function.arguments).unwrap_or(Value::Null)})).collect::<Vec<_>>());
            }
            messages.push(value);
        }
    }
    let mut body = json!({"model": request.requested_model, "max_tokens": request.max_output_tokens, "messages": messages, "stream": request.stream});
    if let Some(system) = &system {
        body["system"] = Value::String(system.clone());
    }
    if !request.tools.is_empty() {
        body["tools"] = Value::Array(request.tools.iter().map(|tool| json!({"name":tool.function.name,"description":tool.function.description,"input_schema":tool.function.parameters})).collect());
    }
    if let Some(choice) = &request.tool_choice {
        body["tool_choice"] = choice.clone();
    }
    body
}

fn parse_wire_response(
    protocol: DeepSeekProtocol,
    request: &ManagedProviderCallRequest,
    response: HttpResponse,
) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
    if response.status >= 400 {
        return Err(ManagedProviderCallError::from_http(HttpError::Http {
            status: response.status,
            reason: "provider returned an error".to_string(),
        }));
    }
    let body = String::from_utf8(response.body).map_err(|_| {
        ManagedProviderCallError::invalid_response("provider response was not UTF-8")
    })?;
    if request.stream
        || body
            .lines()
            .any(|line| line.trim_start().starts_with("data:"))
    {
        parse_stream_response(protocol, request, &body)
    } else {
        let value: Value = serde_json::from_str(&body).map_err(|_| {
            ManagedProviderCallError::invalid_response("provider response JSON was malformed")
        })?;
        parse_non_stream_response(protocol, request, &value)
    }
}

fn parse_non_stream_response(
    protocol: DeepSeekProtocol,
    request: &ManagedProviderCallRequest,
    body: &Value,
) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
    let resolved_model = body.get("model").and_then(Value::as_str).ok_or_else(|| {
        ManagedProviderCallError::identity("provider response model identity missing")
    })?;
    if resolved_model != request.requested_model {
        return Err(ManagedProviderCallError::identity(
            "provider response model conflicts with requested model",
        ));
    }
    let request_id = body
        .get("id")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            ManagedProviderCallError::invalid_response("provider response request id missing")
        })?;
    let (output_text, tool_calls, stop_reason) = if protocol == DeepSeekProtocol::OpenAiCompatible {
        let choice = body.pointer("/choices/0").ok_or_else(|| {
            ManagedProviderCallError::invalid_response("OpenAI response choices missing")
        })?;
        let text = choice
            .pointer("/message/content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let calls = parse_openai_tool_calls(choice.pointer("/message/tool_calls"))?;
        let stop = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                ManagedProviderCallError::invalid_response("OpenAI finish status missing")
            })?;
        (text, calls, stop.to_string())
    } else {
        let blocks = body
            .get("content")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ManagedProviderCallError::invalid_response("Anthropic content blocks missing")
            })?;
        let mut text = String::new();
        let mut calls = Vec::new();
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    text.push_str(block.get("text").and_then(Value::as_str).unwrap_or(""))
                }
                Some("tool_use") => calls.push(parse_anthropic_tool_call(block)?),
                _ => {}
            }
        }
        let stop = body
            .get("stop_reason")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                ManagedProviderCallError::invalid_response("Anthropic stop status missing")
            })?;
        (text, calls, stop.to_string())
    };
    let usage = usage_from_body(protocol.endpoint_kind(), body).ok_or_else(|| {
        ManagedProviderCallError::invalid_response("provider usage is missing or insufficient")
    })?;
    let usage = ManagedUsage::from_protocol(usage, request.requested_model.as_str())?;
    validate_usage_bounds(request, &usage)?;
    let cost = if request.limits.max_cost_usd.is_some() {
        Some(
            request
                .price_profile
                .estimate_usd(&request.requested_model, &usage)
                .map_err(ManagedProviderCallError::invalid_request)?,
        )
    } else {
        None
    };
    if cost.is_some_and(|v| request.limits.max_cost_usd.is_some_and(|max| v > max)) {
        return Err(ManagedProviderCallError::invalid_response(
            "provider response exceeded dollar ceiling",
        ));
    }
    Ok(ManagedProviderResponse {
        schema_version: MANAGED_PROVIDER_RESPONSE_SCHEMA.to_string(),
        provider_kind: DEEPSEEK_PROVIDER_KIND.to_string(),
        protocol,
        requested_model: request.requested_model.clone(),
        resolved_model: resolved_model.to_string(),
        request_id: request_id.to_string(),
        output_text,
        tool_calls,
        stop_reason,
        usage,
        estimated_cost_usd: cost,
        stream: false,
    })
}

fn parse_stream_response(
    protocol: DeepSeekProtocol,
    request: &ManagedProviderCallRequest,
    body: &str,
) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
    let mut events = Vec::new();
    let mut saw_done = false;
    for line in body.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if data == "[DONE]" {
                saw_done = true;
            } else if !data.is_empty() {
                events.push(serde_json::from_str::<Value>(data).map_err(|_| {
                    ManagedProviderCallError::invalid_response(
                        "provider stream event was malformed",
                    )
                })?);
            }
        }
    }
    let saw_anthropic_stop = events
        .iter()
        .any(|event| event.get("type").and_then(Value::as_str) == Some("message_stop"));
    if (protocol == DeepSeekProtocol::OpenAiCompatible && !saw_done)
        || (protocol == DeepSeekProtocol::AnthropicCompatible && !saw_anthropic_stop && !saw_done)
    {
        return Err(ManagedProviderCallError::invalid_response(
            "provider stream was truncated",
        ));
    }
    if events.is_empty() {
        return Err(ManagedProviderCallError::invalid_response(
            "provider stream was empty",
        ));
    }
    let resolved_model = events
        .iter()
        .find_map(|v| {
            v.get("model")
                .and_then(Value::as_str)
                .or_else(|| v.pointer("/message/model").and_then(Value::as_str))
        })
        .ok_or_else(|| {
            ManagedProviderCallError::identity("provider stream model identity missing")
        })?;
    if resolved_model != request.requested_model {
        return Err(ManagedProviderCallError::identity(
            "provider stream model conflicts with requested model",
        ));
    }
    let request_id = events
        .iter()
        .find_map(|v| {
            v.get("id")
                .and_then(Value::as_str)
                .or_else(|| v.pointer("/message/id").and_then(Value::as_str))
        })
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            ManagedProviderCallError::invalid_response("provider stream request id missing")
        })?;
    let mut output_text = String::new();
    let mut stop_reason = None;
    if protocol == DeepSeekProtocol::OpenAiCompatible {
        for event in &events {
            if let Some(choice) = event.pointer("/choices/0") {
                if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str) {
                    output_text.push_str(text);
                }
                if let Some(stop) = choice.get("finish_reason").and_then(Value::as_str) {
                    stop_reason = Some(stop.to_string());
                }
            }
        }
        let usage = aggregate_stream_usage(protocol.endpoint_kind(), &events).ok_or_else(|| {
            ManagedProviderCallError::invalid_response(
                "OpenAI stream usage is missing or insufficient",
            )
        })?;
        let tool_calls = collect_openai_stream_tool_calls(&events)?;
        let usage = ManagedUsage::from_protocol(usage, request.requested_model.as_str())?;
        return finish_stream(
            request,
            protocol,
            resolved_model,
            request_id,
            output_text,
            tool_calls,
            stop_reason,
            usage,
        );
    }
    for event in &events {
        match event.get("type").and_then(Value::as_str) {
            Some("content_block_delta") => {
                if let Some(text) = event.pointer("/delta/text").and_then(Value::as_str) {
                    output_text.push_str(text);
                }
            }
            Some("message_delta") => {
                stop_reason = event
                    .pointer("/delta/stop_reason")
                    .and_then(Value::as_str)
                    .map(str::to_string);
            }
            _ => {}
        }
    }
    let usage = aggregate_stream_usage(protocol.endpoint_kind(), &events).ok_or_else(|| {
        ManagedProviderCallError::invalid_response(
            "Anthropic stream usage is missing or insufficient",
        )
    })?;
    let tool_calls = collect_anthropic_stream_tool_calls(&events)?;
    let usage = ManagedUsage::from_protocol(usage, request.requested_model.as_str())?;
    validate_usage_bounds(request, &usage)?;
    finish_stream(
        request,
        protocol,
        resolved_model,
        request_id,
        output_text,
        tool_calls,
        stop_reason,
        usage,
    )
}

fn validate_usage_bounds(
    request: &ManagedProviderCallRequest,
    usage: &ManagedUsage,
) -> Result<(), ManagedProviderCallError> {
    if usage.output_tokens > request.max_output_tokens {
        return Err(ManagedProviderCallError::invalid_response(
            "provider output exceeded the pre-send output ceiling",
        ));
    }
    if usage.cumulative_tokens > request.limits.max_cumulative_tokens {
        return Err(ManagedProviderCallError::invalid_response(
            "provider cumulative usage exceeded the task ceiling",
        ));
    }
    Ok(())
}

fn collect_anthropic_stream_tool_calls(
    events: &[Value],
) -> Result<Vec<ManagedToolCall>, ManagedProviderCallError> {
    let mut calls = Vec::new();
    for event in events {
        match event.get("type").and_then(Value::as_str) {
            Some("content_block_start") => {
                if let Some(block) = event.get("content_block") {
                    if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                        calls.push(parse_anthropic_tool_call(block)?);
                    }
                }
            }
            Some("content_block_delta")
                if event.pointer("/delta/type").and_then(Value::as_str)
                    == Some("input_json_delta") =>
            {
                if let Some(partial) = event.pointer("/delta/partial_json").and_then(Value::as_str)
                {
                    let call = calls.last_mut().ok_or_else(|| {
                        ManagedProviderCallError::invalid_response(
                            "Anthropic tool input delta has no tool_use block",
                        )
                    })?;
                    if call.function.arguments == "{}" {
                        call.function.arguments.clear();
                    }
                    call.function.arguments.push_str(partial);
                }
            }
            _ => {}
        }
    }
    for call in &calls {
        if serde_json::from_str::<Value>(&call.function.arguments).is_err() {
            return Err(ManagedProviderCallError::invalid_response(
                "Anthropic stream tool input was malformed",
            ));
        }
    }
    Ok(calls)
}

fn finish_stream(
    request: &ManagedProviderCallRequest,
    protocol: DeepSeekProtocol,
    resolved_model: &str,
    request_id: &str,
    output_text: String,
    tool_calls: Vec<ManagedToolCall>,
    stop_reason: Option<String>,
    usage: ManagedUsage,
) -> Result<ManagedProviderResponse, ManagedProviderCallError> {
    validate_usage_bounds(request, &usage)?;
    let stop_reason = stop_reason.filter(|v| !v.is_empty()).ok_or_else(|| {
        ManagedProviderCallError::invalid_response("provider stream stop status missing")
    })?;
    let cost = if request.limits.max_cost_usd.is_some() {
        Some(
            request
                .price_profile
                .estimate_usd(&request.requested_model, &usage)
                .map_err(ManagedProviderCallError::invalid_request)?,
        )
    } else {
        None
    };
    if cost.is_some_and(|v| request.limits.max_cost_usd.is_some_and(|max| v > max)) {
        return Err(ManagedProviderCallError::invalid_response(
            "provider stream exceeded dollar ceiling",
        ));
    }
    Ok(ManagedProviderResponse {
        schema_version: MANAGED_PROVIDER_RESPONSE_SCHEMA.to_string(),
        provider_kind: DEEPSEEK_PROVIDER_KIND.to_string(),
        protocol,
        requested_model: request.requested_model.clone(),
        resolved_model: resolved_model.to_string(),
        request_id: request_id.to_string(),
        output_text,
        tool_calls,
        stop_reason,
        usage,
        estimated_cost_usd: cost,
        stream: true,
    })
}

fn parse_openai_tool_calls(
    value: Option<&Value>,
) -> Result<Vec<ManagedToolCall>, ManagedProviderCallError> {
    let calls: Vec<ManagedToolCall> = value
        .map(|v| {
            serde_json::from_value(v.clone()).map_err(|_| {
                ManagedProviderCallError::invalid_response("OpenAI tool call was malformed")
            })
        })
        .transpose()
        .map(|v| v.unwrap_or_default())?;
    for call in &calls {
        if call.call_type != "function"
            || call.id.is_empty()
            || call.function.name.is_empty()
            || serde_json::from_str::<Value>(&call.function.arguments)
                .map(|v| !v.is_object())
                .unwrap_or(true)
        {
            return Err(ManagedProviderCallError::invalid_response(
                "OpenAI tool call arguments were malformed",
            ));
        }
    }
    Ok(calls)
}

fn parse_anthropic_tool_call(value: &Value) -> Result<ManagedToolCall, ManagedProviderCallError> {
    let input = value
        .get("input")
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| {
            ManagedProviderCallError::invalid_response(
                "Anthropic tool_use input was missing or not an object",
            )
        })?;
    Ok(ManagedToolCall {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                ManagedProviderCallError::invalid_response("Anthropic tool_use id missing")
            })?
            .to_string(),
        call_type: "function".to_string(),
        function: ManagedFunctionCall {
            name: value
                .get("name")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    ManagedProviderCallError::invalid_response("Anthropic tool_use name missing")
                })?
                .to_string(),
            arguments: serde_json::to_string(&input).map_err(|_| {
                ManagedProviderCallError::invalid_response("Anthropic tool_use input was malformed")
            })?,
        },
    })
}

fn collect_openai_stream_tool_calls(
    events: &[Value],
) -> Result<Vec<ManagedToolCall>, ManagedProviderCallError> {
    let mut calls: Vec<ManagedToolCall> = Vec::new();
    for event in events {
        if let Some(delta_calls) = event
            .pointer("/choices/0/delta/tool_calls")
            .and_then(Value::as_array)
        {
            for delta in delta_calls {
                let index = delta.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                while calls.len() <= index {
                    calls.push(ManagedToolCall {
                        id: String::new(),
                        call_type: "function".to_string(),
                        function: ManagedFunctionCall {
                            name: String::new(),
                            arguments: String::new(),
                        },
                    });
                }
                if let Some(id) = delta.get("id").and_then(Value::as_str) {
                    calls[index].id.push_str(id);
                }
                if let Some(name) = delta.pointer("/function/name").and_then(Value::as_str) {
                    calls[index].function.name.push_str(name);
                }
                if let Some(args) = delta.pointer("/function/arguments").and_then(Value::as_str) {
                    calls[index].function.arguments.push_str(args);
                }
            }
        }
    }
    for call in &calls {
        if call.id.is_empty()
            || call.function.name.is_empty()
            || serde_json::from_str::<Value>(&call.function.arguments).is_err()
        {
            return Err(ManagedProviderCallError::invalid_response(
                "OpenAI stream tool call was malformed",
            ));
        }
    }
    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::transport::{HttpError, HttpRequest, MockTransport};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, OnceLock};

    struct CapturingTransport {
        request: Mutex<Option<HttpRequest>>,
        response: HttpResponse,
    }

    #[async_trait::async_trait]
    impl HttpTransport for CapturingTransport {
        async fn send(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            *self.request.lock().unwrap() = Some(request.clone());
            Ok(self.response.clone())
        }
    }

    fn binding() -> ManagedCallBinding {
        ManagedCallBinding {
            product_task_id: "pt".into(),
            workflow_id: "wf".into(),
            node_id: "node".into(),
            attempt_id: "attempt".into(),
            spend_authorization_id: "spend".into(),
            attempt_lease_id: "lease".into(),
        }
    }
    fn request(protocol: DeepSeekProtocol) -> ManagedProviderCallRequest {
        ManagedProviderCallRequest::for_role(ManagedModelRole::Planner, protocol, binding())
    }
    fn config_for(protocol: DeepSeekProtocol, model: &str) -> ProviderConfig {
        ProviderConfig::new(
            "deepseek-test",
            "deepseek",
            protocol.base_url(),
            model,
            DEEPSEEK_CREDENTIAL_REFERENCE,
            "2026-07-30T00:00:00Z",
        )
    }
    fn config(protocol: DeepSeekProtocol) -> ProviderConfig {
        config_for(protocol, "deepseek-v4-pro")
    }
    fn credential() -> CredentialRef {
        CredentialRef::new(
            DEEPSEEK_CREDENTIAL_REFERENCE,
            "env",
            "DEE***KEY",
            "provider:deepseek",
            "2026-07-30T00:00:00Z",
        )
    }
    fn key_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }
    fn fixture_secret() -> String {
        ["opaque", "provider", "key"].join("-")
    }
    fn response_for(protocol: DeepSeekProtocol, model: &str) -> Value {
        if protocol == DeepSeekProtocol::OpenAiCompatible {
            json!({"id":"req-1","model":model,"choices":[{"message":{"role":"assistant","content":"planned"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":4,"prompt_cache_hit_tokens":2,"prompt_cache_miss_tokens":8,"completion_tokens_details":{"reasoning_tokens":1}}})
        } else {
            json!({"id":"msg-1","type":"message","model":model,"content":[{"type":"text","text":"planned"}],"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":4,"cache_read_input_tokens":2,"cache_creation_input_tokens":0}})
        }
    }
    #[test]
    fn profile_has_only_official_models_and_routes() {
        let profile = DeepSeekVersionedProfile::default();
        profile.validate().unwrap();
        assert_eq!(profile.models, vec!["deepseek-v4-flash", "deepseek-v4-pro"]);
        assert_eq!(
            request(DeepSeekProtocol::AnthropicCompatible).url(),
            "https://api.deepseek.com/anthropic/v1/messages"
        );
        let versioned: Value = serde_json::from_str(include_str!(
            "../../../docs/provider_profiles/deepseek-v4-managed.v1.json"
        ))
        .unwrap();
        assert_eq!(versioned["schema_version"], MANAGED_DEEPSEEK_PROFILE_SCHEMA);
        assert_eq!(versioned["provider_kind"], DEEPSEEK_PROVIDER_KIND);
        assert_eq!(versioned["verified_at"], DEEPSEEK_PROFILE_VERIFIED_AT);
        assert_eq!(versioned["models"], serde_json::json!(DEEPSEEK_MODELS));
    }

    #[test]
    fn dollar_gate_requires_current_price_evidence_and_conservative_reservation() {
        let mut stale = request(DeepSeekProtocol::OpenAiCompatible);
        stale.limits.max_cost_usd = Some(1.0);
        stale.price_profile.current = false;
        assert!(stale.validate().unwrap_err().contains("missing or stale"));

        let mut too_low = request(DeepSeekProtocol::OpenAiCompatible);
        too_low.limits.max_cost_usd = Some(0.0001);
        assert!(too_low
            .validate()
            .unwrap_err()
            .contains("conservative pre-send reservation"));
    }

    #[test]
    fn input_and_tool_argument_bounds_fail_closed_before_send() {
        let mut input_bound = request(DeepSeekProtocol::OpenAiCompatible);
        input_bound.limits.max_input_tokens = 1;
        assert!(input_bound
            .validate()
            .unwrap_err()
            .contains("input ceiling"));

        let mut malformed_tool = request(DeepSeekProtocol::OpenAiCompatible);
        malformed_tool.messages = vec![ManagedMessage {
            role: "assistant".into(),
            content: Value::Null,
            name: None,
            tool_call_id: None,
            tool_calls: Some(vec![ManagedToolCall {
                id: "call-1".into(),
                call_type: "function".into(),
                function: ManagedFunctionCall {
                    name: "read".into(),
                    arguments: "[]".into(),
                },
            }]),
        }];
        assert!(malformed_tool
            .validate()
            .unwrap_err()
            .contains("JSON object"));
    }

    #[test]
    fn attempt_lease_binding_is_non_secret_and_exact() {
        let first = managed_attempt_lease_id("lease-a");
        assert_eq!(first.len(), 64);
        assert_ne!(first, managed_attempt_lease_id("lease-b"));
        assert!(!first.contains("lease-a"));
    }

    #[test]
    fn aliases_and_fallback_models_are_rejected_before_send() {
        let mut req = request(DeepSeekProtocol::AnthropicCompatible);
        req.requested_model = "claude-haiku".into();
        assert!(req
            .validate()
            .unwrap_err()
            .contains("admitted DeepSeek identity"));
    }

    #[tokio::test]
    async fn both_protocols_send_exact_route_and_redact_credential_boundary() {
        let _lock = key_lock().lock().await;
        let secret = fixture_secret();
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, &secret);
        for (protocol, role, model) in [
            (
                DeepSeekProtocol::OpenAiCompatible,
                ManagedModelRole::Planner,
                "deepseek-v4-pro",
            ),
            (
                DeepSeekProtocol::OpenAiCompatible,
                ManagedModelRole::Implementer,
                "deepseek-v4-flash",
            ),
            (
                DeepSeekProtocol::AnthropicCompatible,
                ManagedModelRole::Planner,
                "deepseek-v4-pro",
            ),
            (
                DeepSeekProtocol::AnthropicCompatible,
                ManagedModelRole::Implementer,
                "deepseek-v4-flash",
            ),
        ] {
            let transport = MockTransport::new(vec![Ok(HttpResponse {
                status: 200,
                body: response_for(protocol, model).to_string().into_bytes(),
            })]);
            let provider = if protocol == DeepSeekProtocol::OpenAiCompatible {
                ManagedDeepSeekProvider::new_openai(
                    config_for(protocol, model),
                    CredentialBoundary::new("env").unwrap(),
                    credential(),
                    Arc::new(transport),
                )
            } else {
                ManagedDeepSeekProvider::new_anthropic(
                    config_for(protocol, model),
                    CredentialBoundary::new("env").unwrap(),
                    credential(),
                    Arc::new(transport),
                )
            };
            let mut managed_request =
                ManagedProviderCallRequest::for_role(role, protocol, binding());
            managed_request.requested_model = model.to_string();
            let result = provider.invoke(&managed_request).await.unwrap();
            assert_eq!(result.resolved_model, model);
            assert_eq!(
                result.usage.request_id,
                if protocol == DeepSeekProtocol::OpenAiCompatible {
                    "req-1"
                } else {
                    "msg-1"
                }
            );
            assert!(!result.output_text.contains(&secret));
        }
        std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
    }

    #[tokio::test]
    async fn missing_credential_fails_closed_before_transport() {
        let _lock = key_lock().lock().await;
        std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
        let transport = Arc::new(CapturingTransport {
            request: Mutex::new(None),
            response: HttpResponse {
                status: 200,
                body: response_for(DeepSeekProtocol::OpenAiCompatible, "deepseek-v4-pro")
                    .to_string()
                    .into_bytes(),
            },
        });
        let provider = ManagedDeepSeekProvider::new_openai(
            config(DeepSeekProtocol::OpenAiCompatible),
            CredentialBoundary::new("env").unwrap(),
            credential(),
            transport.clone(),
        );
        let error = provider
            .invoke(&request(DeepSeekProtocol::OpenAiCompatible))
            .await
            .unwrap_err();
        assert_eq!(error.domain, "provider_auth");
        assert_eq!(error.effect, ManagedFailureEffect::PreSend);
        assert!(transport.request.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn missing_identity_usage_and_truncated_stream_fail_closed() {
        let _lock = key_lock().lock().await;
        let secret = fixture_secret();
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, &secret);
        let mut req = request(DeepSeekProtocol::OpenAiCompatible);
        req.stream = true;
        let transport = MockTransport::new(vec![Ok(HttpResponse {
            status: 200,
            body: b"data: {\"id\":\"r\",\"model\":\"deepseek-v4-pro\"}\n".to_vec(),
        })]);
        let provider = ManagedDeepSeekProvider::new_openai(
            config(DeepSeekProtocol::OpenAiCompatible),
            CredentialBoundary::new("env").unwrap(),
            credential(),
            Arc::new(transport),
        );
        let error = provider.invoke(&req).await.unwrap_err();
        assert_eq!(error.domain, "provider_response");
        std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
    }

    #[test]
    fn conservative_ledger_reserves_before_send_and_never_expands() {
        let ledger = ManagedBudgetLedger::new(ManagedCallLimits {
            max_requests: 1,
            max_retries: 1,
            max_input_tokens: 10,
            max_output_tokens: 10,
            max_cumulative_tokens: 20,
            ..ManagedCallLimits::default()
        })
        .unwrap();
        ledger.reserve_before_send(1, 10).unwrap();
        assert!(ledger.reserve_before_send(1, 1).is_err());
    }

    #[test]
    fn three_sequential_stages_release_reservations_and_charge_actual_usage() {
        // Accepted live-seal envelope: three Pro/Flash/Pro stages under 24k
        // cumulative when actual usage is low; full release after each stage.
        let ledger = ManagedBudgetLedger::new(ManagedCallLimits {
            max_requests: 3,
            max_retries: 0,
            max_input_tokens: 8_000,
            max_output_tokens: 4_000,
            max_cumulative_tokens: 24_000,
            timeout_ms: 30_000,
            max_cost_usd: Some(0.50),
        })
        .unwrap();
        let mut charged = 0u64;
        for (stage_input, stage_output, stage_cumulative) in [
            (1_900u64, 4_000u64, 2_100u64),
            (2_000, 4_000, 2_900),
            (2_000, 4_000, 2_300),
        ] {
            ledger
                .reserve_before_send(stage_input, stage_output)
                .unwrap_or_else(|error| panic!("stage reserve failed: {error:?}"));
            let snap = ledger.snapshot().unwrap();
            assert_eq!(snap.reserved_input_tokens, stage_input);
            assert_eq!(snap.reserved_output_tokens, stage_output);
            let mut usage = manual_response().usage;
            usage.input_tokens = stage_input.min(2_000);
            usage.output_tokens = 200;
            usage.cumulative_tokens = stage_cumulative;
            charged = charged.saturating_add(stage_cumulative);
            let mut response = manual_response();
            response.usage = usage;
            ledger.reconcile(Some(&response)).unwrap();
            let after = ledger.snapshot().unwrap();
            assert_eq!(
                after.reserved_input_tokens, 0,
                "reconcile must release transient input reservation"
            );
            assert_eq!(
                after.reserved_output_tokens, 0,
                "reconcile must release transient output reservation"
            );
            assert_eq!(after.cumulative_tokens, charged);
        }
        assert_eq!(ledger.snapshot().unwrap().observed_requests, 3);
        // Actual cumulative remains charged and constrains a later oversized reserve.
        let err = ledger.reserve_before_send(8_000, 4_000).unwrap_err();
        assert!(
            err.message.contains("cumulative token ceiling")
                || err.message.contains("request ceiling"),
            "expected cumulative/request constraint, got {:?}",
            err.message
        );
    }

    #[test]
    fn failed_reconcile_releases_reservation_without_charging_usage() {
        let ledger = ManagedBudgetLedger::new(ManagedCallLimits {
            max_requests: 3,
            max_retries: 1,
            max_input_tokens: 8_000,
            max_output_tokens: 4_000,
            max_cumulative_tokens: 24_000,
            ..ManagedCallLimits::default()
        })
        .unwrap();
        ledger.reserve_before_send(1_000, 4_000).unwrap();
        ledger.reconcile(None).unwrap();
        let snap = ledger.snapshot().unwrap();
        assert_eq!(snap.reserved_input_tokens, 0);
        assert_eq!(snap.reserved_output_tokens, 0);
        assert_eq!(snap.cumulative_tokens, 0);
        assert_eq!(snap.observed_requests, 1);
        // A later stage can still reserve after a failed attempt released headroom.
        ledger.reserve_before_send(1_000, 4_000).unwrap();
        ledger.reconcile(Some(&manual_response())).unwrap();
        assert_eq!(ledger.snapshot().unwrap().cumulative_tokens, 13);
    }

    #[test]
    fn estimated_input_uses_byte_upper_bound_for_non_ascii_json_and_code() {
        let binding = ManagedCallBinding {
            product_task_id: "pt".into(),
            workflow_id: "wf".into(),
            node_id: "n".into(),
            attempt_id: "a".into(),
            spend_authorization_id: "s".into(),
            attempt_lease_id: "l".into(),
        };
        let mut req = ManagedProviderCallRequest::for_role(
            ManagedModelRole::Planner,
            DeepSeekProtocol::OpenAiCompatible,
            binding,
        );
        req.limits = ManagedCallLimits {
            max_requests: 3,
            max_retries: 0,
            max_input_tokens: 8_000,
            max_output_tokens: 4_000,
            max_cumulative_tokens: 24_000,
            timeout_ms: 30_000,
            max_cost_usd: None,
        };
        // Non-ASCII prose, JSON structure, code, and identifier-heavy tokens.
        let payload = format!(
            "{}\n{}\n{}\n{}",
            "日本語の識別子と絵文字🚀".repeat(20),
            r#"{"schema_version":"managed_deepseek_plan.v1","path":"docs/USER_GUIDE.md"}"#,
            "fn compute_sha256(input: &[u8]) -> String { hex::encode(Sha256::digest(input)) }",
            "Igzela_token_efficient_agent_harness_lab_product_task_id_abcdefghijklmnopqrstuvwxyz",
        );
        req.messages = vec![ManagedMessage::text("user", &payload)];
        req.system = Some("system preamble for bounded docs planning".into());
        let estimated = req.estimated_input_tokens();
        let expected_bytes = req.system.as_ref().map(|s| s.len() as u64).unwrap_or(0)
            + serde_json::to_vec(&req.messages).unwrap().len() as u64
            + serde_json::to_vec(&req.tools).unwrap().len() as u64
            + req
                .tool_choice
                .as_ref()
                .and_then(|v| serde_json::to_vec(v).ok())
                .map(|v| v.len() as u64)
                .unwrap_or(0);
        assert_eq!(
            estimated, expected_bytes,
            "estimator must retain the prior safe byte upper bound"
        );
        assert!(estimated >= payload.len() as u64);
        // Over-size the bound so max_input_tokens rejects before send.
        req.limits.max_input_tokens = estimated.saturating_sub(1).max(1);
        let err = req.validate().unwrap_err();
        assert!(
            err.contains("input ceiling"),
            "byte-bound inputs must not escape max_input_tokens: {err}"
        );
    }

    #[tokio::test]
    async fn openai_messages_tools_and_tool_result_round_trip_are_bounded() {
        let _lock = key_lock().lock().await;
        let secret = fixture_secret();
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, &secret);
        let transport = Arc::new(CapturingTransport {
            request: Mutex::new(None),
            response: HttpResponse {
                status: 200,
                body: json!({
                    "id":"tool-1",
                    "model":"deepseek-v4-pro",
                    "choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"read","arguments":"{\"path\":\"README.md\"}"}}]},"finish_reason":"tool_calls"}],
                    "usage":{"prompt_tokens":12,"completion_tokens":5}
                }).to_string().into_bytes(),
            },
        });
        let mut req = request(DeepSeekProtocol::OpenAiCompatible);
        req.messages = vec![
            ManagedMessage::text("user", "inspect"),
            ManagedMessage {
                role: "assistant".into(),
                content: Value::Null,
                name: None,
                tool_call_id: None,
                tool_calls: Some(vec![ManagedToolCall {
                    id: "call-1".into(),
                    call_type: "function".into(),
                    function: ManagedFunctionCall {
                        name: "read".into(),
                        arguments: r#"{"path":"README.md"}"#.into(),
                    },
                }]),
            },
            ManagedMessage {
                role: "tool".into(),
                content: json!({"text":"ok"}),
                name: None,
                tool_call_id: Some("call-1".into()),
                tool_calls: None,
            },
        ];
        req.tools = vec![ManagedTool {
            tool_type: "function".into(),
            function: ManagedToolFunction {
                name: "read".into(),
                description: "Read an approved file".into(),
                parameters: json!({"type":"object","properties":{"path":{"type":"string"}}}),
            },
        }];
        req.tool_choice = Some(Value::String("required".into()));
        let provider = ManagedDeepSeekProvider::new_openai(
            config(DeepSeekProtocol::OpenAiCompatible),
            CredentialBoundary::new("env").unwrap(),
            credential(),
            transport.clone(),
        );
        let result = provider.invoke(&req).await.unwrap();
        assert_eq!(result.tool_calls[0].function.name, "read");
        let sent = transport.request.lock().unwrap().clone().unwrap();
        let sent_body: Value = serde_json::from_slice(&sent.body.unwrap()).unwrap();
        assert_eq!(sent.url, "https://api.deepseek.com/chat/completions");
        assert_eq!(sent_body["messages"][0]["role"], "user");
        assert_eq!(sent_body["tools"][0]["function"]["name"], "read");
        assert_eq!(sent_body["tool_choice"], "required");
        assert!(sent
            .headers
            .iter()
            .any(|(name, value)| name == "Authorization" && value == &format!("Bearer {secret}")));
        std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
    }

    #[tokio::test]
    async fn openai_and_anthropic_streams_require_terminal_usage_and_preserve_identity() {
        let _lock = key_lock().lock().await;
        let secret = fixture_secret();
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, &secret);
        let openai_stream = concat!(
            "data: {\"id\":\"s-1\",\"model\":\"deepseek-v4-pro\",\"choices\":[{\"delta\":{\"content\":\"plan\"},\"finish_reason\":null}]}\n",
            "data: {\"id\":\"s-1\",\"model\":\"deepseek-v4-pro\",\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":4}}\n",
            "data: [DONE]\n"
        );
        let openai = ManagedDeepSeekProvider::new_openai(
            config(DeepSeekProtocol::OpenAiCompatible),
            CredentialBoundary::new("env").unwrap(),
            credential(),
            Arc::new(MockTransport::new(vec![Ok(HttpResponse {
                status: 200,
                body: openai_stream.as_bytes().to_vec(),
            })])),
        );
        let mut openai_request = request(DeepSeekProtocol::OpenAiCompatible);
        openai_request.stream = true;
        let result = openai.invoke(&openai_request).await.unwrap();
        assert_eq!(result.output_text, "plan");
        assert!(result.stream);

        let anthropic_stream = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"s-2\",\"model\":\"deepseek-v4-pro\",\"usage\":{\"input_tokens\":10}}}\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"review\"}}\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n"
        );
        let anthropic = ManagedDeepSeekProvider::new_anthropic(
            config(DeepSeekProtocol::AnthropicCompatible),
            CredentialBoundary::new("env").unwrap(),
            credential(),
            Arc::new(MockTransport::new(vec![Ok(HttpResponse {
                status: 200,
                body: anthropic_stream.as_bytes().to_vec(),
            })])),
        );
        let mut anthropic_request = request(DeepSeekProtocol::AnthropicCompatible);
        anthropic_request.stream = true;
        let result = anthropic.invoke(&anthropic_request).await.unwrap();
        assert_eq!(result.output_text, "review");
        assert!(result.stream);
        std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
    }

    struct StaticAuthority {
        limits: ManagedCallLimits,
    }

    impl ManagedAuthoritySource for StaticAuthority {
        fn current_authority(
            &self,
            binding: &ManagedCallBinding,
        ) -> Result<PersistedAuthoritySnapshot, String> {
            Ok(PersistedAuthoritySnapshot {
                product_task_id: binding.product_task_id.clone(),
                workflow_id: binding.workflow_id.clone(),
                node_id: binding.node_id.clone(),
                attempt_id: binding.attempt_id.clone(),
                spend_authorization_id: binding.spend_authorization_id.clone(),
                attempt_lease_id: binding.attempt_lease_id.clone(),
                spend_status: "consumed".into(),
                consumed_by_attempt_id: Some(binding.attempt_id.clone()),
                lease_status: "current".into(),
                execution_contract: Some(PersistedManagedExecutionContract {
                    provider_kind: DEEPSEEK_PROVIDER_KIND.into(),
                    protocol: DeepSeekProtocol::OpenAiCompatible,
                    host: "api.deepseek.com".into(),
                    base_url: DEEPSEEK_OPENAI_BASE_URL.into(),
                    endpoint_path: DEEPSEEK_OPENAI_PATH.into(),
                    request_schema_version: MANAGED_PROVIDER_CALL_SCHEMA.into(),
                    response_schema_version: MANAGED_PROVIDER_RESPONSE_SCHEMA.into(),
                    usage_parser_version: DEEPSEEK_USAGE_PARSER_VERSION.into(),
                    requested_model: "deepseek-v4-pro".into(),
                    limits: self.limits.clone(),
                    price_profile: DeepSeekPriceProfile::default(),
                }),
            })
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
            _effect: ManagedFailureEffect,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    fn manual_response() -> ManagedProviderResponse {
        ManagedProviderResponse {
            schema_version: MANAGED_PROVIDER_RESPONSE_SCHEMA.into(),
            provider_kind: DEEPSEEK_PROVIDER_KIND.into(),
            protocol: DeepSeekProtocol::OpenAiCompatible,
            requested_model: "deepseek-v4-pro".into(),
            resolved_model: "deepseek-v4-pro".into(),
            request_id: "r-1".into(),
            output_text: "ok".into(),
            tool_calls: vec![],
            stop_reason: "stop".into(),
            usage: ManagedUsage {
                input_tokens: 10,
                output_tokens: 4,
                cache_read_tokens: 2,
                cache_creation_tokens: 0,
                reasoning_output_tokens: 1,
                fresh_input_tokens: 8,
                cumulative_tokens: 13,
                model: "deepseek-v4-pro".into(),
                request_id: "r-1".into(),
            },
            estimated_cost_usd: None,
            stream: false,
        }
    }

    #[tokio::test]
    async fn pre_send_retry_is_bounded_but_outcome_unknown_is_not_retried() {
        let limits = ManagedCallLimits {
            max_requests: 2,
            max_retries: 1,
            max_input_tokens: 1024,
            max_output_tokens: 1024,
            max_cumulative_tokens: 4096,
            ..ManagedCallLimits::default()
        };
        let authority = ManagedProviderCallAuthority::new(
            Arc::new(StaticAuthority {
                limits: limits.clone(),
            }),
            limits.clone(),
        )
        .unwrap();
        let mut req = request(DeepSeekProtocol::OpenAiCompatible);
        req.limits = limits;
        req.max_output_tokens = req.limits.max_output_tokens;
        let attempts = Arc::new(AtomicUsize::new(0));
        let retry_attempts = attempts.clone();
        let result = authority
            .invoke_with_retry(&req, || {
                let count = retry_attempts.fetch_add(1, Ordering::SeqCst);
                async move {
                    if count == 0 {
                        Err(ManagedProviderCallError {
                            domain: "provider_pre_send".into(),
                            message: "mock pre-send".into(),
                            retryable: true,
                            effect: ManagedFailureEffect::PreSend,
                        })
                    } else {
                        Ok(manual_response())
                    }
                }
            })
            .await
            .unwrap();
        assert_eq!(result.request_id, "r-1");
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        // Pre-send retry reuses request slots only after each attempt releases
        // its transient reservation; actual success still charges usage once.
        let snap = authority.budget().snapshot().unwrap();
        assert_eq!(snap.reserved_input_tokens, 0);
        assert_eq!(snap.reserved_output_tokens, 0);
        assert_eq!(snap.cumulative_tokens, 13);
        assert_eq!(snap.observed_requests, 2);

        let unknown_limits = ManagedCallLimits {
            max_requests: 1,
            max_retries: 1,
            max_input_tokens: 1024,
            max_output_tokens: 1024,
            max_cumulative_tokens: 4096,
            ..ManagedCallLimits::default()
        };
        let unknown_authority = ManagedProviderCallAuthority::new(
            Arc::new(StaticAuthority {
                limits: unknown_limits.clone(),
            }),
            unknown_limits.clone(),
        )
        .unwrap();
        let mut unknown_req = request(DeepSeekProtocol::OpenAiCompatible);
        unknown_req.limits = unknown_limits;
        unknown_req.max_output_tokens = unknown_req.limits.max_output_tokens;
        let unknown_attempts = Arc::new(AtomicUsize::new(0));
        let count = unknown_attempts.clone();
        let error = unknown_authority
            .invoke_with_retry(&unknown_req, || {
                count.fetch_add(1, Ordering::SeqCst);
                async {
                    Err(ManagedProviderCallError {
                        domain: "provider_timeout".into(),
                        message: "mock unknown".into(),
                        retryable: false,
                        effect: ManagedFailureEffect::OutcomeUnknown,
                    })
                }
            })
            .await
            .unwrap_err();
        assert_eq!(error.effect, ManagedFailureEffect::OutcomeUnknown);
        assert_eq!(unknown_attempts.load(Ordering::SeqCst), 1);
        let unknown_snap = unknown_authority.budget().snapshot().unwrap();
        assert_eq!(unknown_snap.reserved_input_tokens, 0);
        assert_eq!(unknown_snap.reserved_output_tokens, 0);
        assert_eq!(
            unknown_snap.cumulative_tokens, 0,
            "outcome-unknown must not charge usage or retain reservation"
        );
        assert_eq!(unknown_snap.observed_requests, 1);

        // Failed-known (non-retryable pre-send) also releases reservation and
        // does not duplicate provider attempts.
        let failed_limits = ManagedCallLimits {
            max_requests: 2,
            max_retries: 1,
            max_input_tokens: 1024,
            max_output_tokens: 1024,
            max_cumulative_tokens: 4096,
            ..ManagedCallLimits::default()
        };
        let failed_authority = ManagedProviderCallAuthority::new(
            Arc::new(StaticAuthority {
                limits: failed_limits.clone(),
            }),
            failed_limits.clone(),
        )
        .unwrap();
        let mut failed_req = request(DeepSeekProtocol::OpenAiCompatible);
        failed_req.limits = failed_limits;
        failed_req.max_output_tokens = failed_req.limits.max_output_tokens;
        let failed_attempts = Arc::new(AtomicUsize::new(0));
        let failed_count = failed_attempts.clone();
        let failed = failed_authority
            .invoke_with_retry(&failed_req, || {
                failed_count.fetch_add(1, Ordering::SeqCst);
                async {
                    Err(ManagedProviderCallError {
                        domain: "provider_auth".into(),
                        message: "mock failed known".into(),
                        retryable: false,
                        effect: ManagedFailureEffect::PreSend,
                    })
                }
            })
            .await
            .unwrap_err();
        assert_eq!(failed.effect, ManagedFailureEffect::PreSend);
        assert_eq!(failed_attempts.load(Ordering::SeqCst), 1);
        let failed_snap = failed_authority.budget().snapshot().unwrap();
        assert_eq!(failed_snap.reserved_input_tokens, 0);
        assert_eq!(failed_snap.reserved_output_tokens, 0);
        assert_eq!(failed_snap.cumulative_tokens, 0);
    }

    #[tokio::test]
    async fn production_entry_requires_and_uses_persisted_authority() {
        let _lock = key_lock().lock().await;
        let secret = fixture_secret();
        std::env::set_var(DEEPSEEK_CREDENTIAL_REFERENCE, &secret);
        let provider = ManagedDeepSeekProvider::new_openai(
            config(DeepSeekProtocol::OpenAiCompatible),
            CredentialBoundary::new("env").unwrap(),
            credential(),
            Arc::new(MockTransport::new(vec![Ok(HttpResponse {
                status: 200,
                body: response_for(DeepSeekProtocol::OpenAiCompatible, "deepseek-v4-pro")
                    .to_string()
                    .into_bytes(),
            })])),
        );
        let limits = ManagedCallLimits {
            max_input_tokens: 1024,
            max_output_tokens: 1024,
            max_cumulative_tokens: 4096,
            ..ManagedCallLimits::default()
        };
        let authority = ManagedProviderCallAuthority::new(
            Arc::new(StaticAuthority {
                limits: limits.clone(),
            }),
            limits.clone(),
        )
        .unwrap();
        let mut req = request(DeepSeekProtocol::OpenAiCompatible);
        req.limits = limits;
        req.max_output_tokens = req.limits.max_output_tokens;
        let result = provider
            .invoke_with_authority(&authority, &req)
            .await
            .unwrap();
        assert_eq!(result.request_id, "req-1");
        assert_eq!(authority.budget().snapshot().unwrap().observed_requests, 1);
        std::env::remove_var(DEEPSEEK_CREDENTIAL_REFERENCE);
    }

    #[test]
    fn provider_response_projects_only_redacted_usage_evidence() {
        let req = request(DeepSeekProtocol::OpenAiCompatible);
        let event = response_to_usage_event(&req, &manual_response(), "2026-07-30T00:00:00Z");
        assert_eq!(event.provider_id.as_deref(), Some("deepseek"));
        assert_eq!(event.request_or_message_id.as_deref(), Some("r-1"));
        assert_eq!(event.input_tokens, 8);
        assert_eq!(event.reasoning_output_tokens, 1);
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(!encoded.contains("planned"));
        assert!(!encoded.contains(DEEPSEEK_CREDENTIAL_REFERENCE));
    }
}
