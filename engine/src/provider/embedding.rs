use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::credential::CredentialBoundary;
use super::redaction::contains_sensitive_patterns;
use super::transport::{HttpError, HttpRequest, HttpTransport, ReqwestTransport};
use super::RetryPolicy;
use crate::infrastructure::circuit_breaker::{CircuitBreaker, CircuitBreakerError};

pub const OPENROUTER_EMBEDDING_PROVIDER_ID: &str = "openrouter";
pub const OPENROUTER_EMBEDDING_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const OPENROUTER_EMBEDDING_MODEL_ID: &str = "nvidia/llama-nemotron-embed-vl-1b-v2:free";
pub const OPENROUTER_EMBEDDING_CANONICAL_SLUG: &str =
    "nvidia/llama-nemotron-embed-vl-1b-v2-20260224";
pub const OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID: &str =
    "private/openrouter/nvidia/llama-nemotron-embed-vl-1b-v2";
pub const OPENROUTER_EMBEDDING_DIMENSIONS: usize = 1_536;
pub const OPENROUTER_EMBEDDING_CONTEXT_LENGTH: u64 = 131_072;
pub const OPENROUTER_EMBEDDING_PRICING_EFFECTIVE_DATE: &str = "2026-07-15";
pub const OPENROUTER_EMBEDDING_PRICING_SOURCE: &str =
    "provider_catalog_reported_prices+harness_pinned_effective_date";
pub const OPENROUTER_EMBEDDING_CREDENTIAL_ENV: &str = "OPENROUTER_API_KEY";

/// Append-only registry for durable provider-vector identities. When the
/// current contract rotates, retain the prior tuple here so historical rows
/// remain inspectable and can be re-embedded through the bounded owner.
const SUPPORTED_DURABLE_EMBEDDING_IDENTITIES: &[DurableEmbeddingIdentity] =
    &[DurableEmbeddingIdentity {
        provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID,
        requested_model_id: OPENROUTER_EMBEDDING_MODEL_ID,
        canonical_model_slug: OPENROUTER_EMBEDDING_CANONICAL_SLUG,
        resolved_model_id: OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
        dimensions: OPENROUTER_EMBEDDING_DIMENSIONS,
        context_length: OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
        prompt_cost_per_token_usd: 0.0,
        completion_cost_per_token_usd: 0.0,
        currency: "USD",
        pricing_effective_date: OPENROUTER_EMBEDDING_PRICING_EFFECTIVE_DATE,
        pricing_source: OPENROUTER_EMBEDDING_PRICING_SOURCE,
    }];

struct DurableEmbeddingIdentity {
    provider_id: &'static str,
    requested_model_id: &'static str,
    canonical_model_slug: &'static str,
    resolved_model_id: &'static str,
    dimensions: usize,
    context_length: u64,
    prompt_cost_per_token_usd: f64,
    completion_cost_per_token_usd: f64,
    currency: &'static str,
    pricing_effective_date: &'static str,
    pricing_source: &'static str,
}

const MAX_BATCH_INPUTS: usize = 16;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 20_000;
const DEFAULT_MAX_RETRIES: usize = 2;
const DEFAULT_PER_CALL_CAP_USD: f64 = 0.01;
const DEFAULT_DAILY_CAP_USD: f64 = 0.10;

#[cfg(test)]
pub(crate) static EMBEDDING_TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingPricingEvidence {
    pub prompt_cost_per_token_usd: f64,
    pub completion_cost_per_token_usd: f64,
    pub currency: String,
    pub effective_date: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingContractEvidence {
    pub provider_id: String,
    pub requested_model_id: String,
    pub canonical_model_slug: String,
    pub resolved_model_id: String,
    pub dimensions: usize,
    pub context_length: u64,
    pub pricing: EmbeddingPricingEvidence,
}

impl EmbeddingContractEvidence {
    pub(crate) fn current(pricing: EmbeddingPricingEvidence) -> Self {
        Self {
            provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
            requested_model_id: OPENROUTER_EMBEDDING_MODEL_ID.to_string(),
            canonical_model_slug: OPENROUTER_EMBEDDING_CANONICAL_SLUG.to_string(),
            resolved_model_id: OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID.to_string(),
            dimensions: OPENROUTER_EMBEDDING_DIMENSIONS,
            context_length: OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
            pricing,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderEmbeddingMetadata {
    pub provider_id: String,
    pub requested_model_id: String,
    pub canonical_model_slug: String,
    pub resolved_model_id: String,
    pub dimensions: usize,
    pub input_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub pricing: EmbeddingPricingEvidence,
    pub measurement_provenance: String,
    pub normalized_content_sha256: String,
    pub vector_sha256: String,
}

pub(crate) fn is_supported_durable_embedding_identity(
    metadata: &ProviderEmbeddingMetadata,
) -> bool {
    SUPPORTED_DURABLE_EMBEDDING_IDENTITIES
        .iter()
        .any(|identity| {
            metadata.provider_id == identity.provider_id
                && metadata.requested_model_id == identity.requested_model_id
                && metadata.canonical_model_slug == identity.canonical_model_slug
                && metadata.resolved_model_id == identity.resolved_model_id
                && metadata.dimensions == identity.dimensions
                && pricing_matches_identity(&metadata.pricing, identity)
        })
}

pub(crate) fn is_supported_durable_embedding_contract(
    contract: &EmbeddingContractEvidence,
) -> bool {
    SUPPORTED_DURABLE_EMBEDDING_IDENTITIES
        .iter()
        .any(|identity| {
            contract.provider_id == identity.provider_id
                && contract.requested_model_id == identity.requested_model_id
                && contract.canonical_model_slug == identity.canonical_model_slug
                && contract.resolved_model_id == identity.resolved_model_id
                && contract.dimensions == identity.dimensions
                && contract.context_length == identity.context_length
                && pricing_matches_identity(&contract.pricing, identity)
        })
}

fn pricing_matches_identity(
    pricing: &EmbeddingPricingEvidence,
    identity: &DurableEmbeddingIdentity,
) -> bool {
    pricing.prompt_cost_per_token_usd == identity.prompt_cost_per_token_usd
        && pricing.completion_cost_per_token_usd == identity.completion_cost_per_token_usd
        && pricing.currency == identity.currency
        && pricing.effective_date == identity.pricing_effective_date
        && pricing.source == identity.pricing_source
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProviderEmbeddingAttemptError {
    Definitive(String),
    OutcomeUnknown(String),
}

impl ProviderEmbeddingAttemptError {
    pub(crate) fn outcome_unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_))
    }
}

impl std::fmt::Display for ProviderEmbeddingAttemptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Definitive(message) | Self::OutcomeUnknown(message) => {
                formatter.write_str(message)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProviderEmbeddingOutput {
    pub vectors: Vec<Vec<f64>>,
    pub metadata: Vec<ProviderEmbeddingMetadata>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VerifiedEmbeddingContract {
    pub pricing: EmbeddingPricingEvidence,
}

impl VerifiedEmbeddingContract {
    pub(crate) fn evidence(&self) -> EmbeddingContractEvidence {
        EmbeddingContractEvidence::current(self.pricing.clone())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ProviderEmbeddingConfig {
    pub timeout_ms: u64,
    pub max_retries: usize,
    pub per_call_cap_usd: f64,
    pub daily_cap_usd: f64,
}

impl ProviderEmbeddingConfig {
    pub(crate) fn from_env() -> Result<Self, String> {
        if std::env::var("CI").is_ok() {
            return Err("provider embedding generation is prohibited in CI".to_string());
        }
        if std::env::var("ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS").as_deref() != Ok("1") {
            return Err("durable memory embedding gate is disabled".to_string());
        }
        if !crate::trusted_local::EffectiveExecutionGates::from_env().provider_execution {
            return Err("provider execution gate is disabled".to_string());
        }
        if std::env::var("ACP_REQUIRE_AUTH").as_deref() != Ok("1") {
            return Err("provider embeddings require authenticated runtime mode".to_string());
        }
        if std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH").as_deref() == Ok("1") {
            return Err("durable memory embedding kill switch is active".to_string());
        }
        let credential_env = std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_CREDENTIAL_ENV")
            .unwrap_or_else(|_| OPENROUTER_EMBEDDING_CREDENTIAL_ENV.to_string());
        if credential_env != OPENROUTER_EMBEDDING_CREDENTIAL_ENV {
            return Err(format!(
                "durable memory embedding credential reference must be {OPENROUTER_EMBEDDING_CREDENTIAL_ENV}"
            ));
        }
        let timeout_ms = bounded_u64_env(
            "ACP_DURABLE_MEMORY_EMBEDDING_TIMEOUT_MS",
            DEFAULT_TIMEOUT_MS,
            100,
            60_000,
        )?;
        let max_retries = bounded_u64_env(
            "ACP_DURABLE_MEMORY_EMBEDDING_MAX_RETRIES",
            DEFAULT_MAX_RETRIES as u64,
            0,
            3,
        )? as usize;
        let per_call_cap_usd = positive_f64_env(
            "ACP_DURABLE_MEMORY_EMBEDDING_PER_CALL_CAP_USD",
            DEFAULT_PER_CALL_CAP_USD,
        )?;
        let daily_cap_usd = positive_f64_env(
            "ACP_DURABLE_MEMORY_EMBEDDING_DAILY_CAP_USD",
            DEFAULT_DAILY_CAP_USD,
        )?;
        Ok(Self {
            timeout_ms,
            max_retries,
            per_call_cap_usd,
            daily_cap_usd,
        })
    }
}

pub(crate) struct ProviderEmbeddingClient {
    transport: Arc<dyn HttpTransport>,
    catalog_circuit_breaker: Arc<CircuitBreaker>,
    embedding_circuit_breaker: Arc<CircuitBreaker>,
}

impl Default for ProviderEmbeddingClient {
    fn default() -> Self {
        Self::new(Arc::new(ReqwestTransport::new()))
    }
}

impl ProviderEmbeddingClient {
    pub(crate) fn new(transport: Arc<dyn HttpTransport>) -> Self {
        Self {
            transport,
            catalog_circuit_breaker: Arc::new(CircuitBreaker::new(
                "openrouter-durable-memory-embedding-catalog",
                3,
                30_000,
            )),
            embedding_circuit_breaker: Arc::new(CircuitBreaker::new(
                "openrouter-durable-memory-embedding-post",
                3,
                30_000,
            )),
        }
    }

    #[cfg(test)]
    pub(crate) fn embed(
        &self,
        inputs: &[String],
        config: &ProviderEmbeddingConfig,
    ) -> Result<ProviderEmbeddingOutput, String> {
        validate_inputs(inputs)?;
        let contract = self.verify_contract(config)?;
        self.embed_verified(inputs, config, &contract)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn verify_contract(
        &self,
        config: &ProviderEmbeddingConfig,
    ) -> Result<VerifiedEmbeddingContract, String> {
        let boundary = CredentialBoundary::new("env")?;
        let api_key = boundary.resolve(OPENROUTER_EMBEDDING_CREDENTIAL_ENV)?;
        let result = self.catalog_circuit_breaker.call(|| {
            let catalog = self.send_catalog_with_retry(
                HttpRequest {
                    url: format!("{OPENROUTER_EMBEDDING_BASE_URL}/embeddings/models"),
                    method: "GET".to_string(),
                    headers: authorization_headers(&api_key),
                    body: None,
                    timeout_secs: Some(config.timeout_ms as f64 / 1000.0),
                },
                config,
            )?;
            validate_catalog(&catalog.body).map(|pricing| VerifiedEmbeddingContract { pricing })
        });
        map_circuit_result(result)
    }

    pub(crate) fn embed_verified(
        &self,
        inputs: &[String],
        config: &ProviderEmbeddingConfig,
        contract: &VerifiedEmbeddingContract,
    ) -> Result<ProviderEmbeddingOutput, ProviderEmbeddingAttemptError> {
        validate_inputs(inputs).map_err(ProviderEmbeddingAttemptError::Definitive)?;
        let boundary =
            CredentialBoundary::new("env").map_err(ProviderEmbeddingAttemptError::Definitive)?;
        let api_key = boundary
            .resolve(OPENROUTER_EMBEDDING_CREDENTIAL_ENV)
            .map_err(ProviderEmbeddingAttemptError::Definitive)?;
        let body = json!({
            "model": OPENROUTER_EMBEDDING_MODEL_ID,
            "input": inputs,
            "dimensions": OPENROUTER_EMBEDDING_DIMENSIONS,
            "encoding_format": "float",
        });
        let result = self.embedding_circuit_breaker.call(|| {
            let response = self.send_embedding_once(HttpRequest {
                url: format!("{OPENROUTER_EMBEDDING_BASE_URL}/embeddings"),
                method: "POST".to_string(),
                headers: authorization_headers(&api_key),
                body: Some(
                    serde_json::to_vec(&body).map_err(|error| {
                        ProviderEmbeddingAttemptError::Definitive(error.to_string())
                    })?,
                ),
                timeout_secs: Some(config.timeout_ms as f64 / 1000.0),
            })?;
            parse_embedding_response(&response.body, inputs, contract.pricing.clone()).map_err(
                |detail| {
                    ProviderEmbeddingAttemptError::OutcomeUnknown(format!(
                        "embedding provider outcome unknown; automatic replay is forbidden ({detail})"
                    ))
                },
            )
        });
        match result {
            Ok(output) => Ok(output),
            Err(CircuitBreakerError::CircuitOpen) => {
                Err(ProviderEmbeddingAttemptError::Definitive(
                    "durable memory embedding provider circuit is open".to_string(),
                ))
            }
            Err(CircuitBreakerError::Inner(error)) => Err(error),
        }
    }

    fn send_catalog_with_retry(
        &self,
        request: HttpRequest,
        config: &ProviderEmbeddingConfig,
    ) -> Result<super::transport::HttpResponse, String> {
        let mut policy = RetryPolicy::new("openrouter-embedding-catalog");
        policy.max_retries = config.max_retries as i64;
        policy.base_delay_ms = 100;
        policy.max_delay_ms = 400;
        let mut attempt = 0usize;
        loop {
            if std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH").as_deref() == Ok("1") {
                return Err("durable memory embedding kill switch became active".to_string());
            }
            match send_blocking(Arc::clone(&self.transport), request.clone()) {
                Ok(response) => return Ok(response),
                Err(error) if retryable_http_error(&error) && attempt < config.max_retries => {
                    let delay = Duration::from_millis(
                        super::retry::compute_delay_ms(&policy, attempt as i64).max(0) as u64,
                    );
                    std::thread::sleep(delay);
                    attempt += 1;
                }
                Err(error) => return Err(redacted_http_error(&error)),
            }
        }
    }

    fn send_embedding_once(
        &self,
        request: HttpRequest,
    ) -> Result<super::transport::HttpResponse, ProviderEmbeddingAttemptError> {
        if std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH").as_deref() == Ok("1") {
            return Err(ProviderEmbeddingAttemptError::Definitive(
                "durable memory embedding kill switch became active".to_string(),
            ));
        }
        match send_blocking(Arc::clone(&self.transport), request) {
            Ok(response) if (200..=299).contains(&response.status) => Ok(response),
            Ok(response) if definitive_refusal_status(response.status) => Err(
                ProviderEmbeddingAttemptError::Definitive(redacted_http_error(&HttpError::Http {
                    status: response.status,
                    reason: String::new(),
                })),
            ),
            Ok(_) => Err(ProviderEmbeddingAttemptError::OutcomeUnknown(
                "embedding provider outcome unknown; automatic replay is forbidden".to_string(),
            )),
            Err(error @ HttpError::Http { status, .. }) if definitive_refusal_status(status) => {
                Err(ProviderEmbeddingAttemptError::Definitive(
                    redacted_http_error(&error),
                ))
            }
            Err(_) => Err(ProviderEmbeddingAttemptError::OutcomeUnknown(
                "embedding provider outcome unknown; automatic replay is forbidden".to_string(),
            )),
        }
    }
}

fn definitive_refusal_status(status: u16) -> bool {
    matches!(
        status,
        400..=407 | 409..=451
    )
}

fn map_circuit_result<T>(result: Result<T, CircuitBreakerError<String>>) -> Result<T, String> {
    match result {
        Ok(output) => Ok(output),
        Err(CircuitBreakerError::CircuitOpen) => {
            Err("durable memory embedding provider circuit is open".to_string())
        }
        Err(CircuitBreakerError::Inner(error)) => Err(error),
    }
}

fn send_blocking(
    transport: Arc<dyn HttpTransport>,
    request: HttpRequest,
) -> Result<super::transport::HttpResponse, HttpError> {
    std::thread::scope(|scope| {
        scope
            .spawn(move || {
                tokio::runtime::Runtime::new()
                    .map_err(|error| HttpError::Connection(error.to_string()))?
                    .block_on(transport.send(&request))
            })
            .join()
            .map_err(|_| HttpError::Connection("embedding transport worker panicked".to_string()))?
    })
}

fn validate_catalog(body: &[u8]) -> Result<EmbeddingPricingEvidence, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| "embedding model catalog response is malformed".to_string())?;
    let record = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|records| {
            records.iter().find(|record| {
                record.get("id").and_then(Value::as_str) == Some(OPENROUTER_EMBEDDING_MODEL_ID)
            })
        })
        .ok_or_else(|| "pinned embedding model is unavailable".to_string())?;
    if record.get("canonical_slug").and_then(Value::as_str)
        != Some(OPENROUTER_EMBEDDING_CANONICAL_SLUG)
    {
        return Err("embedding model canonical identity changed".to_string());
    }
    let output_modalities = record
        .pointer("/architecture/output_modalities")
        .and_then(Value::as_array)
        .ok_or_else(|| "embedding model capability record is incomplete".to_string())?;
    if !output_modalities
        .iter()
        .any(|value| value.as_str() == Some("embeddings"))
    {
        return Err("pinned model is not embedding-capable".to_string());
    }
    let input_modalities = record
        .pointer("/architecture/input_modalities")
        .and_then(Value::as_array)
        .ok_or_else(|| "embedding model input capability record is incomplete".to_string())?;
    if !input_modalities
        .iter()
        .any(|value| value.as_str() == Some("text"))
    {
        return Err("pinned embedding model does not accept text".to_string());
    }
    if record.get("context_length").and_then(Value::as_u64)
        != Some(OPENROUTER_EMBEDDING_CONTEXT_LENGTH)
    {
        return Err("embedding model context contract changed".to_string());
    }
    let prompt = parse_price(record.pointer("/pricing/prompt"))?;
    let completion = parse_price(record.pointer("/pricing/completion"))?;
    if prompt != 0.0 || completion != 0.0 {
        return Err("pinned free embedding model pricing changed".to_string());
    }
    Ok(EmbeddingPricingEvidence {
        prompt_cost_per_token_usd: prompt,
        completion_cost_per_token_usd: completion,
        currency: "USD".to_string(),
        effective_date: OPENROUTER_EMBEDDING_PRICING_EFFECTIVE_DATE.to_string(),
        source: OPENROUTER_EMBEDDING_PRICING_SOURCE.to_string(),
    })
}

fn parse_embedding_response(
    body: &[u8],
    inputs: &[String],
    pricing: EmbeddingPricingEvidence,
) -> Result<ProviderEmbeddingOutput, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| "embedding provider response is malformed".to_string())?;
    let resolved_model_id = value
        .get("model")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .ok_or_else(|| "embedding provider outcome has no resolved model identity".to_string())?;
    if resolved_model_id != OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID {
        return Err("embedding provider resolved model identity mismatch".to_string());
    }
    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "embedding provider outcome has no vectors".to_string())?;
    if data.len() != inputs.len() {
        return Err("embedding provider vector count mismatch".to_string());
    }
    let input_tokens = value
        .pointer("/usage/prompt_tokens")
        .and_then(Value::as_i64);
    if input_tokens.is_some_and(|tokens| tokens < 0) {
        return Err("embedding provider usage is invalid".to_string());
    }
    let mut indexed = vec![None; inputs.len()];
    for item in data {
        let index = item
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|index| *index < inputs.len())
            .ok_or_else(|| "embedding provider vector index is invalid".to_string())?;
        if indexed[index].is_some() {
            return Err("embedding provider returned duplicate vector index".to_string());
        }
        let values = item
            .get("embedding")
            .and_then(Value::as_array)
            .ok_or_else(|| "embedding provider returned an empty vector".to_string())?
            .iter()
            .map(|value| {
                value.as_f64().ok_or_else(|| {
                    "embedding provider returned a non-numeric vector value".to_string()
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        validate_embedding_vector(&values)?;
        indexed[index] = Some(values);
    }
    let vectors = indexed
        .into_iter()
        .map(|value| value.ok_or_else(|| "embedding provider vector index is missing".to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let per_input_tokens = (inputs.len() == 1).then_some(input_tokens).flatten();
    let metadata = inputs
        .iter()
        .zip(&vectors)
        .map(|(input, vector)| ProviderEmbeddingMetadata {
            provider_id: OPENROUTER_EMBEDDING_PROVIDER_ID.to_string(),
            requested_model_id: OPENROUTER_EMBEDDING_MODEL_ID.to_string(),
            canonical_model_slug: OPENROUTER_EMBEDDING_CANONICAL_SLUG.to_string(),
            resolved_model_id: resolved_model_id.to_string(),
            dimensions: vector.len(),
            input_tokens: per_input_tokens,
            // The catalog price makes the request reservable, but it is not a
            // provider-reported billed amount. Keep cost unavailable unless the
            // response contract gains an explicit provider cost field.
            cost_usd: None,
            pricing: pricing.clone(),
            measurement_provenance: "provider_reported".to_string(),
            normalized_content_sha256: normalized_content_sha256(input),
            vector_sha256: sha256_json(&serde_json::to_value(vector).unwrap_or(Value::Null)),
        })
        .collect();
    Ok(ProviderEmbeddingOutput { vectors, metadata })
}

pub(crate) fn validate_inputs(inputs: &[String]) -> Result<(), String> {
    if inputs.is_empty() || inputs.len() > MAX_BATCH_INPUTS {
        return Err(format!(
            "embedding batch size must be within 1..={MAX_BATCH_INPUTS}"
        ));
    }
    let total = inputs.iter().try_fold(0usize, |total, input| {
        if input.trim().is_empty() {
            return Err("embedding input is empty".to_string());
        }
        if contains_sensitive_patterns(input) {
            return Err("embedding input contains secret-shaped content".to_string());
        }
        Ok(total.saturating_add(input.len()))
    })?;
    if total > MAX_INPUT_BYTES {
        return Err(format!("embedding input exceeds {MAX_INPUT_BYTES} bytes"));
    }
    Ok(())
}

fn validate_embedding_vector(values: &[f64]) -> Result<(), String> {
    if values.is_empty() {
        return Err("embedding provider returned an empty vector".to_string());
    }
    if values.iter().any(|value| !value.is_finite()) {
        return Err("embedding provider returned a non-finite vector value".to_string());
    }
    if values.len() != OPENROUTER_EMBEDDING_DIMENSIONS {
        return Err(format!(
            "embedding dimension mismatch: expected {OPENROUTER_EMBEDDING_DIMENSIONS}, got {}",
            values.len()
        ));
    }
    Ok(())
}

fn authorization_headers(api_key: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".to_string(), format!("Bearer {api_key}")),
        ("content-type".to_string(), "application/json".to_string()),
    ]
}

fn retryable_http_error(error: &HttpError) -> bool {
    matches!(
        error,
        HttpError::Timeout(_)
            | HttpError::Connection(_)
            | HttpError::Http {
                status: 429 | 500..=599,
                ..
            }
    )
}

fn redacted_http_error(error: &HttpError) -> String {
    match error {
        HttpError::Http {
            status: 401 | 403, ..
        } => "embedding provider authentication refused".to_string(),
        HttpError::Http { status: 429, .. } => "embedding provider rate limit exceeded".to_string(),
        HttpError::Http { status, .. } => format!("embedding provider HTTP {status}"),
        HttpError::Timeout(_) => "embedding provider request timed out".to_string(),
        HttpError::Connection(_) => "embedding provider connection failed".to_string(),
        HttpError::Parse(_) => "embedding provider transport response was malformed".to_string(),
    }
}

fn parse_price(value: Option<&Value>) -> Result<f64, String> {
    let parsed = value
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| "embedding provider pricing is unknown or incomplete".to_string())?;
    Ok(parsed)
}

fn bounded_u64_env(key: &str, default: u64, min: u64, max: u64) -> Result<u64, String> {
    let value = std::env::var(key)
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("{key} is invalid"))
        })
        .transpose()?
        .unwrap_or(default);
    if !(min..=max).contains(&value) {
        return Err(format!("{key} is outside {min}..={max}"));
    }
    Ok(value)
}

fn positive_f64_env(key: &str, default: f64) -> Result<f64, String> {
    let value = std::env::var(key)
        .ok()
        .map(|value| {
            value
                .parse::<f64>()
                .map_err(|_| format!("{key} is invalid"))
        })
        .transpose()?
        .unwrap_or(default);
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("{key} must be finite and positive"));
    }
    Ok(value)
}

fn normalize_input(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn normalized_content_sha256(input: &str) -> String {
    sha256_bytes(normalize_input(input).as_bytes())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn sha256_json(value: &Value) -> String {
    serde_json::to_vec(value)
        .map(|bytes| sha256_bytes(&bytes))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::transport::{HttpResponse, MockTransport};

    fn catalog() -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"data":[{
                "id": OPENROUTER_EMBEDDING_MODEL_ID,
                "canonical_slug": OPENROUTER_EMBEDDING_CANONICAL_SLUG,
                "context_length": OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
                "pricing":{"prompt":"0","completion":"0"},
                "architecture":{"input_modalities":["text","image"],"output_modalities":["embeddings"]}
            }]}))
            .unwrap(),
        }
    }

    fn response(values: Vec<f64>) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({
                "model":OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
                "data":[{"index":0,"embedding":values}],
                "usage":{"prompt_tokens":7}
            }))
            .unwrap(),
        }
    }

    fn config() -> ProviderEmbeddingConfig {
        ProviderEmbeddingConfig {
            timeout_ms: 1000,
            max_retries: 0,
            per_call_cap_usd: 0.01,
            daily_cap_usd: 0.10,
        }
    }

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        prior_key: Option<String>,
        prior_kill_switch: Option<String>,
    }
    impl EnvGuard {
        fn enabled() -> Self {
            let lock = EMBEDDING_TEST_ENV_LOCK.lock().unwrap();
            let prior_key = std::env::var(OPENROUTER_EMBEDDING_CREDENTIAL_ENV).ok();
            let prior_kill_switch = std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH").ok();
            std::env::set_var(OPENROUTER_EMBEDDING_CREDENTIAL_ENV, "test-secret");
            std::env::remove_var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH");
            Self {
                _lock: lock,
                prior_key,
                prior_kill_switch,
            }
        }

        fn missing() -> Self {
            let guard = Self::enabled();
            std::env::remove_var(OPENROUTER_EMBEDDING_CREDENTIAL_ENV);
            guard
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.prior_key {
                std::env::set_var(OPENROUTER_EMBEDDING_CREDENTIAL_ENV, value);
            } else {
                std::env::remove_var(OPENROUTER_EMBEDDING_CREDENTIAL_ENV);
            }
            if let Some(value) = &self.prior_kill_switch {
                std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH", value);
            } else {
                std::env::remove_var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH");
            }
        }
    }

    #[test]
    fn provider_embedding_validates_catalog_and_response() {
        let _guard = EnvGuard::enabled();
        let client = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![
            Ok(catalog()),
            Ok(response(vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS])),
        ])));
        let output = client
            .embed(&["bounded memory".to_string()], &config())
            .unwrap();
        assert_eq!(output.vectors[0].len(), OPENROUTER_EMBEDDING_DIMENSIONS);
        assert_eq!(output.metadata[0].provider_id, "openrouter");
        assert_eq!(output.metadata[0].input_tokens, Some(7));
        assert_eq!(output.metadata[0].cost_usd, None);
        assert_eq!(
            output.metadata[0].pricing.effective_date,
            OPENROUTER_EMBEDDING_PRICING_EFFECTIVE_DATE
        );
        assert_eq!(
            output.metadata[0].pricing.source,
            OPENROUTER_EMBEDDING_PRICING_SOURCE
        );
        assert_eq!(
            output.metadata[0].measurement_provenance,
            "provider_reported"
        );
    }

    #[test]
    fn provider_embedding_supports_bounded_batches_without_fabricating_per_item_usage() {
        let _guard = EnvGuard::enabled();
        let batch_response = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({
                "model":"private/openrouter/nvidia/llama-nemotron-embed-vl-1b-v2",
                "data":[
                    {"index":0,"embedding":vec![0.25;OPENROUTER_EMBEDDING_DIMENSIONS]},
                    {"index":1,"embedding":vec![0.5;OPENROUTER_EMBEDDING_DIMENSIONS]}
                ],
                "usage":{"prompt_tokens":11}
            }))
            .unwrap(),
        };
        let client = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![
            Ok(catalog()),
            Ok(batch_response),
        ])));
        let output = client
            .embed(&["first".to_string(), "second".to_string()], &config())
            .unwrap();
        assert_eq!(output.vectors.len(), 2);
        assert_eq!(output.metadata[0].input_tokens, None);
        assert_eq!(output.metadata[1].cost_usd, None);
    }

    #[test]
    fn malformed_non_finite_empty_and_dimension_mismatch_fail_closed() {
        let pricing = EmbeddingPricingEvidence {
            prompt_cost_per_token_usd: 0.0,
            completion_cost_per_token_usd: 0.0,
            currency: "USD".to_string(),
            effective_date: OPENROUTER_EMBEDDING_PRICING_EFFECTIVE_DATE.to_string(),
            source: OPENROUTER_EMBEDDING_PRICING_SOURCE.to_string(),
        };
        let input = ["memory".to_string()];
        assert!(parse_embedding_response(b"{}", &input, pricing.clone()).is_err());
        assert!(validate_embedding_vector(&[f64::NAN])
            .unwrap_err()
            .contains("non-finite"));
        let empty = json!({"model":OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,"data":[{"index":0,"embedding":[]}],"usage":{}});
        assert!(parse_embedding_response(
            &serde_json::to_vec(&empty).unwrap(),
            &input,
            pricing.clone()
        )
        .unwrap_err()
        .contains("empty vector"));
        let short = json!({"model":OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,"data":[{"index":0,"embedding":[0.1]}],"usage":{}});
        assert!(
            parse_embedding_response(&serde_json::to_vec(&short).unwrap(), &input, pricing)
                .unwrap_err()
                .contains("dimension")
        );
        let wrong_model = json!({
            "model":"private/openrouter/unexpected-model",
            "data":[{"index":0,"embedding":vec![0.1;OPENROUTER_EMBEDDING_DIMENSIONS]}],
            "usage":{}
        });
        assert!(parse_embedding_response(
            &serde_json::to_vec(&wrong_model).unwrap(),
            &input,
            EmbeddingPricingEvidence {
                prompt_cost_per_token_usd: 0.0,
                completion_cost_per_token_usd: 0.0,
                currency: "USD".to_string(),
                effective_date: OPENROUTER_EMBEDDING_PRICING_EFFECTIVE_DATE.to_string(),
                source: OPENROUTER_EMBEDDING_PRICING_SOURCE.to_string(),
            },
        )
        .unwrap_err()
        .contains("resolved model identity mismatch"));
    }

    #[test]
    fn auth_timeout_kill_switch_and_pricing_change_are_refused() {
        let _guard = EnvGuard::enabled();
        let auth = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![Err(
            HttpError::Http {
                status: 401,
                reason: "secret-shaped provider body".to_string(),
            },
        )])));
        assert_eq!(
            auth.embed(&["x".to_string()], &config()).unwrap_err(),
            "embedding provider authentication refused"
        );

        let timeout = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![
            Ok(catalog()),
            Err(HttpError::Timeout("internal detail".to_string())),
            Ok(response(vec![0.25; OPENROUTER_EMBEDDING_DIMENSIONS])),
        ])));
        let mut retrying_config = config();
        retrying_config.max_retries = 3;
        assert_eq!(
            timeout
                .embed(&["x".to_string()], &retrying_config)
                .unwrap_err(),
            "embedding provider outcome unknown; automatic replay is forbidden"
        );

        std::env::set_var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH", "1");
        let blocked = ProviderEmbeddingClient::new(Arc::new(MockTransport::empty()));
        assert!(blocked
            .embed(&["x".to_string()], &config())
            .unwrap_err()
            .contains("kill switch"));
        std::env::remove_var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH");

        let changed = json!({"data":[{
            "id":OPENROUTER_EMBEDDING_MODEL_ID,
            "canonical_slug":OPENROUTER_EMBEDDING_CANONICAL_SLUG,
            "context_length":OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
            "pricing":{"prompt":"0.0001","completion":"0"},
            "architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]}
        }]});
        assert!(validate_catalog(&serde_json::to_vec(&changed).unwrap())
            .unwrap_err()
            .contains("pricing changed"));
    }

    #[test]
    fn missing_credentials_and_oversized_batches_are_refused() {
        let _guard = EnvGuard::missing();
        let client = ProviderEmbeddingClient::new(Arc::new(MockTransport::empty()));
        assert!(client
            .embed(&["x".to_string()], &config())
            .unwrap_err()
            .contains("is not set"));
        assert!(validate_inputs(&vec!["x".to_string(); MAX_BATCH_INPUTS + 1]).is_err());
        assert!(
            validate_inputs(&["OPENROUTER_API_KEY=sk-test-fixture".to_string()])
                .unwrap_err()
                .contains("secret-shaped")
        );
    }

    #[test]
    fn repeated_provider_failures_open_the_circuit() {
        let _guard = EnvGuard::enabled();
        let mut responses = Vec::new();
        for _ in 0..3 {
            responses.push(Ok(catalog()));
            responses.push(Err(HttpError::Http {
                status: 503,
                reason: "unavailable".to_string(),
            }));
        }
        // Contract verification remains healthy. Its success must not reset the
        // independent embedding POST failure count.
        responses.push(Ok(catalog()));
        let client = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(responses)));
        for _ in 0..3 {
            assert!(client.embed(&["x".to_string()], &config()).is_err());
        }
        assert_eq!(
            client.embed(&["x".to_string()], &config()).unwrap_err(),
            "durable memory embedding provider circuit is open"
        );
    }

    #[test]
    fn ready_trusted_local_profile_enables_provider_without_legacy_gate() {
        const KEYS: &[&str] = &[
            "CI",
            "ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS",
            "ACP_ENABLE_PROVIDER_EXECUTION",
            "ACP_REQUIRE_AUTH",
            "ACP_TRUSTED_LOCAL_PROFILE",
            "ACP_ADMIN_API_KEY",
            "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON",
            "ACP_COST_PER_DISPATCH_USD",
            "ACP_COST_DAILY_USD",
            "ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH",
            "ACP_DURABLE_MEMORY_EMBEDDING_CREDENTIAL_ENV",
        ];

        struct RestoreEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);
        impl RestoreEnv {
            fn capture(keys: &'static [&'static str]) -> Self {
                Self(
                    keys.iter()
                        .map(|key| (*key, std::env::var_os(key)))
                        .collect(),
                )
            }
        }
        impl Drop for RestoreEnv {
            fn drop(&mut self) {
                for (key, value) in &self.0 {
                    if let Some(value) = value {
                        std::env::set_var(key, value);
                    } else {
                        std::env::remove_var(key);
                    }
                }
            }
        }

        let _lock = EMBEDDING_TEST_ENV_LOCK.lock().unwrap();
        let _restore = RestoreEnv::capture(KEYS);
        for key in KEYS {
            std::env::remove_var(key);
        }
        std::env::set_var("ACP_ENABLE_DURABLE_MEMORY_EMBEDDINGS", "1");
        std::env::set_var("ACP_REQUIRE_AUTH", "1");
        std::env::set_var("ACP_TRUSTED_LOCAL_PROFILE", "1");
        std::env::set_var("ACP_ADMIN_API_KEY", format!("harness_{}", "a".repeat(64)));
        std::env::set_var(
            "ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON",
            r#"[{"endpoint_id":"embedding-gate","provider_type":"stub","model":"fixture","timeout_ms":1000,"input_cost_per_1k_usd":0.001,"output_cost_per_1k_usd":0.001}]"#,
        );
        std::env::set_var("ACP_COST_PER_DISPATCH_USD", "0.01");
        std::env::set_var("ACP_COST_DAILY_USD", "0.10");

        assert!(
            crate::trusted_local::EffectiveExecutionGates::from_env()
                .profile
                .ready
        );
        ProviderEmbeddingConfig::from_env()
            .expect("ready trusted-local profile should authorize provider embeddings");
    }
}
