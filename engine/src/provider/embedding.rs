#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
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
pub const OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID: &str = OPENROUTER_EMBEDDING_MODEL_ID;
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
        request_cost_per_request_usd: 0.0,
        image_cost_per_image_usd: 0.0,
        web_search_cost_per_request_usd: 0.0,
        internal_reasoning_cost_per_token_usd: 0.0,
        input_cache_read_cost_per_token_usd: 0.0,
        input_cache_write_cost_per_token_usd: 0.0,
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
    request_cost_per_request_usd: f64,
    image_cost_per_image_usd: f64,
    web_search_cost_per_request_usd: f64,
    internal_reasoning_cost_per_token_usd: f64,
    input_cache_read_cost_per_token_usd: f64,
    input_cache_write_cost_per_token_usd: f64,
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
const TRANSPORT_WORKER_COUNT: usize = 4;
const TRANSPORT_QUEUE_CAPACITY: usize = 32;

#[cfg(test)]
pub(crate) static EMBEDDING_TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingPricingEvidence {
    pub prompt_cost_per_token_usd: f64,
    pub completion_cost_per_token_usd: f64,
    pub request_cost_per_request_usd: f64,
    pub image_cost_per_image_usd: f64,
    pub web_search_cost_per_request_usd: f64,
    pub internal_reasoning_cost_per_token_usd: f64,
    pub input_cache_read_cost_per_token_usd: f64,
    pub input_cache_write_cost_per_token_usd: f64,
    pub request_max_price: EmbeddingPricingOverrides,
    pub currency: String,
    pub effective_date: String,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmbeddingPricingOverrides {
    pub prompt_usd_per_million_tokens: f64,
    pub completion_usd_per_million_tokens: f64,
    pub request_usd: f64,
    pub image_usd: f64,
}

impl EmbeddingPricingOverrides {
    pub(crate) fn zero() -> Self {
        Self {
            prompt_usd_per_million_tokens: 0.0,
            completion_usd_per_million_tokens: 0.0,
            request_usd: 0.0,
            image_usd: 0.0,
        }
    }
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

pub(crate) fn pinned_free_embedding_contract_evidence() -> EmbeddingContractEvidence {
    EmbeddingContractEvidence::current(EmbeddingPricingEvidence {
        prompt_cost_per_token_usd: 0.0,
        completion_cost_per_token_usd: 0.0,
        request_cost_per_request_usd: 0.0,
        image_cost_per_image_usd: 0.0,
        web_search_cost_per_request_usd: 0.0,
        internal_reasoning_cost_per_token_usd: 0.0,
        input_cache_read_cost_per_token_usd: 0.0,
        input_cache_write_cost_per_token_usd: 0.0,
        request_max_price: EmbeddingPricingOverrides::zero(),
        currency: "USD".to_string(),
        effective_date: OPENROUTER_EMBEDDING_PRICING_EFFECTIVE_DATE.to_string(),
        source: OPENROUTER_EMBEDDING_PRICING_SOURCE.to_string(),
    })
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
        && pricing.request_cost_per_request_usd == identity.request_cost_per_request_usd
        && pricing.image_cost_per_image_usd == identity.image_cost_per_image_usd
        && pricing.web_search_cost_per_request_usd == identity.web_search_cost_per_request_usd
        && pricing.internal_reasoning_cost_per_token_usd
            == identity.internal_reasoning_cost_per_token_usd
        && pricing.input_cache_read_cost_per_token_usd
            == identity.input_cache_read_cost_per_token_usd
        && pricing.input_cache_write_cost_per_token_usd
            == identity.input_cache_write_cost_per_token_usd
        && pricing.request_max_price == EmbeddingPricingOverrides::zero()
        && pricing.currency == identity.currency
        && pricing.effective_date == identity.pricing_effective_date
        && pricing.source == identity.pricing_source
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProviderEmbeddingAttemptError {
    FailedBeforeSend(String),
    Definitive(String),
    OutcomeUnknown(String),
}

impl ProviderEmbeddingAttemptError {
    pub(crate) fn outcome_unknown(&self) -> bool {
        matches!(self, Self::OutcomeUnknown(_))
    }

    pub(crate) fn failed_before_send(&self) -> bool {
        matches!(self, Self::FailedBeforeSend(_))
    }
}

impl std::fmt::Display for ProviderEmbeddingAttemptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedBeforeSend(message)
            | Self::Definitive(message)
            | Self::OutcomeUnknown(message) => formatter.write_str(message),
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
    transport_executor: Option<Arc<BoundedTransportExecutor>>,
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
            transport_executor: BoundedTransportExecutor::new(
                TRANSPORT_WORKER_COUNT,
                TRANSPORT_QUEUE_CAPACITY,
            )
            .ok()
            .map(Arc::new),
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
        validate_inputs(inputs).map_err(ProviderEmbeddingAttemptError::FailedBeforeSend)?;
        let boundary = CredentialBoundary::new("env")
            .map_err(ProviderEmbeddingAttemptError::FailedBeforeSend)?;
        let api_key = boundary
            .resolve(OPENROUTER_EMBEDDING_CREDENTIAL_ENV)
            .map_err(ProviderEmbeddingAttemptError::FailedBeforeSend)?;
        let body = json!({
            "model": OPENROUTER_EMBEDDING_MODEL_ID,
            "input": inputs,
            "dimensions": OPENROUTER_EMBEDDING_DIMENSIONS,
            "encoding_format": "float",
            "provider": {
                "allow_fallbacks": false,
                "require_parameters": true,
                "max_price": {
                    "prompt": 0,
                    "completion": 0,
                    "request": 0,
                    "image": 0,
                }
            },
        });
        let result = self.embedding_circuit_breaker.call(|| {
            let response = self.send_embedding_once(HttpRequest {
                url: format!("{OPENROUTER_EMBEDDING_BASE_URL}/embeddings"),
                method: "POST".to_string(),
                headers: authorization_headers(&api_key),
                body: Some(
                    serde_json::to_vec(&body).map_err(|error| {
                        ProviderEmbeddingAttemptError::FailedBeforeSend(error.to_string())
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
                Err(ProviderEmbeddingAttemptError::FailedBeforeSend(
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
            match send_blocking(
                self.transport_executor.as_ref(),
                Arc::clone(&self.transport),
                request.clone(),
            ) {
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
            return Err(ProviderEmbeddingAttemptError::FailedBeforeSend(
                "durable memory embedding kill switch became active".to_string(),
            ));
        }
        match send_blocking(
            self.transport_executor.as_ref(),
            Arc::clone(&self.transport),
            request,
        ) {
            Ok(response) if (200..=299).contains(&response.status) => Ok(response),
            Ok(response) => {
                let error = HttpError::Http {
                    status: response.status,
                    reason: String::from_utf8_lossy(&response.body).into_owned(),
                };
                if proved_pre_effect_refusal(&error) {
                    return Err(ProviderEmbeddingAttemptError::Definitive(
                        redacted_http_error(&error),
                    ));
                }
                Err(ProviderEmbeddingAttemptError::OutcomeUnknown(format!(
                    "embedding provider outcome unknown; automatic replay is forbidden (unexpected HTTP status {})",
                    response.status
                )))
            }
            Err(error) if proved_pre_effect_refusal(&error) => Err(
                ProviderEmbeddingAttemptError::Definitive(redacted_http_error(&error)),
            ),
            Err(HttpError::PreSend(_)) => Err(ProviderEmbeddingAttemptError::FailedBeforeSend(
                "embedding provider request was not sent".to_string(),
            )),
            Err(error) => Err(ProviderEmbeddingAttemptError::OutcomeUnknown(format!(
                "embedding provider outcome unknown; automatic replay is forbidden ({})",
                outcome_unknown_transport_class(&error)
            ))),
        }
    }
}

fn outcome_unknown_transport_class(error: &HttpError) -> &'static str {
    match error {
        HttpError::PreSend(_) => "request not sent",
        HttpError::Timeout(_) => "timeout after send",
        HttpError::Connection(_) => "connection lost after send",
        HttpError::Http {
            status: 300..=399, ..
        } => "redirect refused",
        HttpError::Http { .. } => "unexpected HTTP outcome",
        HttpError::Parse(detail) if detail.contains("body limit exceeded") => "oversized response",
        HttpError::Parse(detail) if detail.contains("failed to read body") => "truncated response",
        HttpError::Parse(_) => "malformed response",
    }
}

fn proved_pre_effect_refusal(error: &HttpError) -> bool {
    let HttpError::Http { status, reason } = error else {
        return false;
    };
    let Ok(body) = serde_json::from_str::<Value>(reason) else {
        return false;
    };
    if body.pointer("/error/code").and_then(Value::as_u64) != Some(u64::from(*status)) {
        return false;
    }
    if !matches!(
        status,
        400 | 401 | 402 | 403 | 404 | 409 | 412 | 413 | 415 | 422 | 429
    ) {
        return false;
    }
    let error_type = body
        .pointer("/error/metadata/error_type")
        .and_then(Value::as_str);
    let edge_refusal = matches!(
        error_type,
        Some(
            "authentication"
                | "payment_required"
                | "invalid_request"
                | "invalid_prompt"
                | "not_found"
                | "precondition_failed"
                | "payload_too_large"
                | "unprocessable"
        )
    );
    let attempt = body
        .pointer("/openrouter_metadata/attempt")
        .and_then(Value::as_u64);
    let requested = body
        .pointer("/openrouter_metadata/requested")
        .and_then(Value::as_str);
    edge_refusal && attempt == Some(0) && requested == Some(OPENROUTER_EMBEDDING_MODEL_ID)
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

enum TransportTask {
    Send {
        transport: Arc<dyn HttpTransport>,
        request: HttpRequest,
        deadline: std::time::Instant,
        response: mpsc::Sender<Result<super::transport::HttpResponse, HttpError>>,
    },
    Shutdown,
}

struct BoundedTransportExecutor {
    sender: Option<mpsc::SyncSender<TransportTask>>,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(test)]
    liveness: Arc<AtomicUsize>,
}

impl BoundedTransportExecutor {
    fn new(worker_count: usize, queue_capacity: usize) -> Result<Self, String> {
        if worker_count == 0 || queue_capacity == 0 {
            return Err("embedding transport executor bounds must be positive".to_string());
        }
        let (sender, receiver) = mpsc::sync_channel(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(test)]
        let liveness = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::with_capacity(worker_count);
        let (startup_sender, startup_receiver) = mpsc::channel();
        for index in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            let shutdown = Arc::clone(&shutdown);
            #[cfg(test)]
            let liveness = Arc::clone(&liveness);
            let startup_sender = startup_sender.clone();
            let worker = std::thread::Builder::new()
                .name(format!("provider-embedding-transport-{index}"))
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build();
                    let Ok(runtime) = runtime else {
                        let _ = startup_sender.send(false);
                        return;
                    };
                    #[cfg(test)]
                    liveness.fetch_add(1, Ordering::SeqCst);
                    let _ = startup_sender.send(true);
                    loop {
                        let task = receiver
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .recv();
                        match task {
                            Ok(TransportTask::Send {
                                transport,
                                mut request,
                                deadline,
                                response,
                            }) => {
                                if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
                                    let _ = response.send(Err(HttpError::PreSend(
                                        "embedding transport executor is shutting down".to_string(),
                                    )));
                                } else if std::env::var("ACP_DURABLE_MEMORY_EMBEDDING_KILL_SWITCH")
                                    .as_deref()
                                    == Ok("1")
                                {
                                    let _ = response.send(Err(HttpError::PreSend(
                                        "embedding kill switch became active before network send"
                                            .to_string(),
                                    )));
                                } else if let Some(remaining) =
                                    deadline.checked_duration_since(std::time::Instant::now())
                                {
                                    request.timeout_secs = Some(remaining.as_secs_f64());
                                    let _ =
                                        response.send(runtime.block_on(transport.send(&request)));
                                } else {
                                    let _ = response.send(Err(HttpError::PreSend(
                                        "embedding transport deadline expired before network send"
                                            .to_string(),
                                    )));
                                }
                            }
                            Ok(TransportTask::Shutdown) | Err(_) => break,
                        }
                    }
                    #[cfg(test)]
                    liveness.fetch_sub(1, Ordering::SeqCst);
                })
                .map_err(|error| error.to_string())?;
            workers.push(worker);
        }
        drop(startup_sender);
        for _ in 0..worker_count {
            if startup_receiver.recv().ok() != Some(true) {
                for _ in 0..worker_count {
                    let _ = sender.send(TransportTask::Shutdown);
                }
                for worker in workers {
                    let _ = worker.join();
                }
                return Err("embedding transport worker initialization failed".to_string());
            }
        }
        Ok(Self {
            sender: Some(sender),
            workers: Mutex::new(workers),
            shutdown,
            #[cfg(test)]
            liveness,
        })
    }

    fn send(
        &self,
        transport: Arc<dyn HttpTransport>,
        request: HttpRequest,
    ) -> Result<super::transport::HttpResponse, HttpError> {
        let timeout = std::time::Duration::try_from_secs_f64(
            request
                .timeout_secs
                .unwrap_or(DEFAULT_TIMEOUT_MS as f64 / 1000.0),
        )
        .map_err(|_| HttpError::Timeout("invalid embedding overall timeout".to_string()))?;
        let deadline = std::time::Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| HttpError::Timeout("invalid embedding overall timeout".to_string()))?;
        let (response_sender, response_receiver) = mpsc::channel();
        self.sender
            .as_ref()
            .ok_or_else(|| {
                HttpError::PreSend(
                    "embedding transport executor stopped before admission".to_string(),
                )
            })?
            .try_send(TransportTask::Send {
                transport,
                request,
                deadline,
                response: response_sender,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => HttpError::PreSend(
                    "embedding transport queue is full before network send".to_string(),
                ),
                mpsc::TrySendError::Disconnected(_) => HttpError::PreSend(
                    "embedding transport executor stopped before admission".to_string(),
                ),
            })?;
        response_receiver
            .recv_timeout(timeout)
            .map_err(|error| match error {
                mpsc::RecvTimeoutError::Timeout => {
                    HttpError::Timeout("embedding transport overall deadline exceeded".to_string())
                }
                mpsc::RecvTimeoutError::Disconnected => HttpError::Connection(
                    "embedding transport worker stopped before response".to_string(),
                ),
            })?
    }

    #[cfg(test)]
    fn liveness_probe(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.liveness)
    }
}

impl Drop for BoundedTransportExecutor {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.sender.take();
        let workers = self
            .workers
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for worker in workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn send_blocking(
    executor: Option<&Arc<BoundedTransportExecutor>>,
    transport: Arc<dyn HttpTransport>,
    request: HttpRequest,
) -> Result<super::transport::HttpResponse, HttpError> {
    executor
        .ok_or_else(|| {
            HttpError::Connection("embedding transport executor unavailable".to_string())
        })?
        .send(transport, request)
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
    let pricing = record
        .get("pricing")
        .and_then(Value::as_object)
        .ok_or_else(|| "embedding provider pricing is unknown or incomplete".to_string())?;
    const KNOWN_FIELDS: &[&str] = &[
        "prompt",
        "completion",
        "request",
        "image",
        "web_search",
        "internal_reasoning",
        "input_cache_read",
        "input_cache_write",
        "discount",
    ];
    if pricing
        .keys()
        .any(|key| !KNOWN_FIELDS.contains(&key.as_str()))
    {
        return Err(
            "embedding provider pricing contains an unknown or unmodelled charge field".to_string(),
        );
    }
    let prompt = parse_price(pricing.get("prompt"))?;
    let completion = parse_price(pricing.get("completion"))?;
    let request = parse_price(pricing.get("request"))?;
    let image = parse_price(pricing.get("image"))?;
    let web_search = parse_price(pricing.get("web_search"))?;
    let internal_reasoning = parse_price(pricing.get("internal_reasoning"))?;
    let input_cache_read = parse_price(pricing.get("input_cache_read"))?;
    let input_cache_write = parse_price(pricing.get("input_cache_write"))?;
    let discount = pricing
        .get("discount")
        .map(|value| parse_price(Some(value)))
        .transpose()?;
    if [
        prompt,
        completion,
        request,
        image,
        web_search,
        internal_reasoning,
        input_cache_read,
        input_cache_write,
    ]
    .into_iter()
    .any(|price| price != 0.0)
        || discount.is_some_and(|price| price != 0.0)
    {
        return Err("pinned free embedding model pricing changed".to_string());
    }
    Ok(EmbeddingPricingEvidence {
        prompt_cost_per_token_usd: prompt,
        completion_cost_per_token_usd: completion,
        request_cost_per_request_usd: request,
        image_cost_per_image_usd: image,
        web_search_cost_per_request_usd: web_search,
        internal_reasoning_cost_per_token_usd: internal_reasoning,
        input_cache_read_cost_per_token_usd: input_cache_read,
        input_cache_write_cost_per_token_usd: input_cache_write,
        request_max_price: EmbeddingPricingOverrides::zero(),
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
        ("X-OpenRouter-Metadata".to_string(), "enabled".to_string()),
    ]
}

fn retryable_http_error(error: &HttpError) -> bool {
    matches!(
        error,
        HttpError::PreSend(_)
            | HttpError::Timeout(_)
            | HttpError::Connection(_)
            | HttpError::Http {
                status: 429 | 500..=599,
                ..
            }
    )
}

fn redacted_http_error(error: &HttpError) -> String {
    match error {
        HttpError::PreSend(_) => "embedding provider request was not sent".to_string(),
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
    let raw = value
        .and_then(Value::as_str)
        .ok_or_else(|| "embedding provider pricing is unknown or incomplete".to_string())?;
    let parsed = raw
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| "embedding provider pricing is unknown or incomplete".to_string())?;
    let significand = raw
        .split_once(['e', 'E'])
        .map_or(raw, |(significand, _)| significand);
    if parsed == 0.0 && significand.bytes().any(|byte| matches!(byte, b'1'..=b'9')) {
        return Err("embedding provider pricing underflowed exact zero".to_string());
    }
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn catalog() -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"data":[{
                "id": OPENROUTER_EMBEDDING_MODEL_ID,
                "canonical_slug": OPENROUTER_EMBEDDING_CANONICAL_SLUG,
                "context_length": OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
                "pricing":{"prompt":"0","completion":"0","request":"0","image":"0","web_search":"0","internal_reasoning":"0","input_cache_read":"0","input_cache_write":"0"},
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
            let lock = EMBEDDING_TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                "model":OPENROUTER_EMBEDDING_RESOLVED_MODEL_ID,
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
            request_cost_per_request_usd: 0.0,
            image_cost_per_image_usd: 0.0,
            web_search_cost_per_request_usd: 0.0,
            internal_reasoning_cost_per_token_usd: 0.0,
            input_cache_read_cost_per_token_usd: 0.0,
            input_cache_write_cost_per_token_usd: 0.0,
            request_max_price: EmbeddingPricingOverrides::zero(),
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
                request_cost_per_request_usd: 0.0,
                image_cost_per_image_usd: 0.0,
                web_search_cost_per_request_usd: 0.0,
                internal_reasoning_cost_per_token_usd: 0.0,
                input_cache_read_cost_per_token_usd: 0.0,
                input_cache_write_cost_per_token_usd: 0.0,
                request_max_price: EmbeddingPricingOverrides::zero(),
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
                reason: json!({
                    "error": {
                        "code": 401,
                        "message": "redacted",
                        "metadata": {"error_type": "authentication"}
                    }
                })
                .to_string(),
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
            "embedding provider outcome unknown; automatic replay is forbidden (timeout after send)"
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
            "pricing":{"prompt":"0.0001","completion":"0","request":"0","image":"0","web_search":"0","internal_reasoning":"0","input_cache_read":"0","input_cache_write":"0"},
            "architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]}
        }]});
        assert!(validate_catalog(&serde_json::to_vec(&changed).unwrap())
            .unwrap_err()
            .contains("pricing changed"));
    }

    #[test]
    fn request_pricing_and_all_catalog_charge_fields_fail_closed() {
        let mut incomplete_persisted =
            serde_json::to_value(pinned_free_embedding_contract_evidence()).unwrap();
        incomplete_persisted["pricing"]
            .as_object_mut()
            .unwrap()
            .remove("web_search_cost_per_request_usd");
        assert!(
            serde_json::from_value::<EmbeddingContractEvidence>(incomplete_persisted).is_err(),
            "persisted pricing evidence must not default a missing modeled dimension to zero"
        );
        let base = json!({"data":[{
            "id":OPENROUTER_EMBEDDING_MODEL_ID,
            "canonical_slug":OPENROUTER_EMBEDDING_CANONICAL_SLUG,
            "context_length":OPENROUTER_EMBEDDING_CONTEXT_LENGTH,
            "pricing":{"prompt":"0","completion":"0","request":"0","image":"0"},
            "architecture":{"input_modalities":["text"],"output_modalities":["embeddings"]}
        }]});
        assert!(
            validate_catalog(&serde_json::to_vec(&base).unwrap()).is_err(),
            "missing modeled pricing dimensions must fail closed"
        );

        let mut documented_zero_fields = base.clone();
        documented_zero_fields["data"][0]["pricing"] = json!({
            "prompt":"0","completion":"0","request":"0","image":"0",
            "web_search":"0","internal_reasoning":"0",
            "input_cache_read":"0","input_cache_write":"0"
        });
        assert!(
            validate_catalog(&serde_json::to_vec(&documented_zero_fields).unwrap()).is_ok(),
            "documented zero-price dimensions must not break a free catalog contract"
        );
        let mut zero_discount = documented_zero_fields.clone();
        zero_discount["data"][0]["pricing"]["discount"] = json!("0");
        assert!(validate_catalog(&serde_json::to_vec(&zero_discount).unwrap()).is_ok());
        for discount in [json!("0.01"), json!("free"), Value::Null] {
            let mut invalid_discount = documented_zero_fields.clone();
            invalid_discount["data"][0]["pricing"]["discount"] = discount;
            assert!(validate_catalog(&serde_json::to_vec(&invalid_discount).unwrap()).is_err());
        }
        let mut underflow_price = documented_zero_fields.clone();
        underflow_price["data"][0]["pricing"]["request"] = json!("1e-9999");
        assert!(
            validate_catalog(&serde_json::to_vec(&underflow_price).unwrap()).is_err(),
            "positive decimal prices must not underflow into free evidence"
        );

        for (name, pricing) in [
            ("missing-request", json!({"prompt":"0","completion":"0"})),
            (
                "paid-request",
                json!({"prompt":"0","completion":"0","request":"0.01"}),
            ),
            (
                "unknown-paid-field",
                json!({"prompt":"0","completion":"0","request":"0","future_charge":"0.01"}),
            ),
            (
                "documented-paid-field",
                json!({"prompt":"0","completion":"0","request":"0","image":"0","web_search":"0.01"}),
            ),
            (
                "unknown-zero-field",
                json!({"prompt":"0","completion":"0","request":"0","image":"0","future_charge":"0"}),
            ),
            (
                "unparseable-field",
                json!({"prompt":"0","completion":"0","request":"0","image":"free"}),
            ),
        ] {
            let mut changed = base.clone();
            changed["data"][0]["pricing"] = pricing;
            assert!(
                validate_catalog(&serde_json::to_vec(&changed).unwrap()).is_err(),
                "{name} pricing must fail closed"
            );
        }
    }

    #[test]
    fn undocumented_private_response_model_identity_is_refused() {
        let body = json!({
            "model":"private/openrouter/nvidia/llama-nemotron-embed-vl-1b-v2",
            "data":[{"index":0,"embedding":vec![0.25;OPENROUTER_EMBEDDING_DIMENSIONS]}],
            "usage":{"prompt_tokens":7}
        });
        assert!(parse_embedding_response(
            &serde_json::to_vec(&body).unwrap(),
            &["bounded memory".to_string()],
            EmbeddingPricingEvidence {
                prompt_cost_per_token_usd: 0.0,
                completion_cost_per_token_usd: 0.0,
                request_cost_per_request_usd: 0.0,
                image_cost_per_image_usd: 0.0,
                web_search_cost_per_request_usd: 0.0,
                internal_reasoning_cost_per_token_usd: 0.0,
                input_cache_read_cost_per_token_usd: 0.0,
                input_cache_write_cost_per_token_usd: 0.0,
                request_max_price: EmbeddingPricingOverrides::zero(),
                currency: "USD".to_string(),
                effective_date: OPENROUTER_EMBEDDING_PRICING_EFFECTIVE_DATE.to_string(),
                source: OPENROUTER_EMBEDDING_PRICING_SOURCE.to_string(),
            },
        )
        .is_err());
    }

    #[test]
    fn post_send_transport_failures_preserve_bounded_audit_classes() {
        let _guard = EnvGuard::enabled();
        for (error, expected) in [
            (
                HttpError::Http {
                    status: 302,
                    reason: "redirect refused".to_string(),
                },
                "redirect refused",
            ),
            (
                HttpError::Parse("response body limit exceeded".to_string()),
                "oversized response",
            ),
            (
                HttpError::Parse("failed to read body: incomplete message".to_string()),
                "truncated response",
            ),
            (
                HttpError::Parse("invalid response framing".to_string()),
                "malformed response",
            ),
        ] {
            let client =
                ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![Err(error)])));
            let failure = client
                .send_embedding_once(HttpRequest {
                    url: "https://example.invalid/embeddings".to_string(),
                    method: "POST".to_string(),
                    headers: Vec::new(),
                    body: Some(Vec::new()),
                    timeout_secs: Some(1.0),
                })
                .unwrap_err();
            assert!(failure.outcome_unknown());
            assert!(failure.to_string().contains(expected));
        }
    }

    #[test]
    fn post_send_status_is_retryable_only_with_proved_pre_effect_evidence() {
        let _guard = EnvGuard::enabled();
        for status in [408, 425, 429, 500, 502, 503, 504, 529] {
            let client = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![Err(
                HttpError::Http {
                    status,
                    reason: json!({
                        "error": {"code": status, "message": "redacted"},
                        "openrouter_metadata": {"attempt": 1}
                    })
                    .to_string(),
                },
            )])));
            assert!(client
                .send_embedding_once(HttpRequest {
                    url: "https://example.invalid/embeddings".to_string(),
                    method: "POST".to_string(),
                    headers: Vec::new(),
                    body: Some(Vec::new()),
                    timeout_secs: Some(1.0),
                })
                .unwrap_err()
                .outcome_unknown());
        }

        let client = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![Err(
            HttpError::Http {
                status: 404,
                reason: json!({
                    "error": {
                        "code": 404,
                        "message": "redacted",
                        "metadata": {"error_type": "not_found"}
                    },
                    "openrouter_metadata": {
                        "requested": OPENROUTER_EMBEDDING_MODEL_ID,
                        "attempt": 0
                    }
                })
                .to_string(),
            },
        )])));
        assert!(!client
            .send_embedding_once(HttpRequest {
                url: "https://example.invalid/embeddings".to_string(),
                method: "POST".to_string(),
                headers: Vec::new(),
                body: Some(Vec::new()),
                timeout_secs: Some(1.0),
            })
            .unwrap_err()
            .outcome_unknown());

        for (status, body) in [
            (
                401,
                json!({
                    "error": {
                        "code": 401,
                        "message": "redacted",
                        "metadata": {"error_type": "authentication"}
                    }
                }),
            ),
            (
                500,
                json!({
                    "error": {
                        "code": 500,
                        "message": "redacted",
                        "metadata": {"error_type": "invalid_request"}
                    }
                }),
            ),
            (
                503,
                json!({
                    "error": {"code": 503, "message": "redacted"},
                    "openrouter_metadata": {
                        "requested": OPENROUTER_EMBEDDING_MODEL_ID,
                        "attempt": 0
                    }
                }),
            ),
        ] {
            let client = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![Err(
                HttpError::Http {
                    status,
                    reason: body.to_string(),
                },
            )])));
            assert!(client
                .send_embedding_once(HttpRequest {
                    url: "https://example.invalid/embeddings".to_string(),
                    method: "POST".to_string(),
                    headers: Vec::new(),
                    body: Some(Vec::new()),
                    timeout_secs: Some(1.0),
                })
                .unwrap_err()
                .outcome_unknown());
        }
    }

    #[test]
    fn post_send_plain_four_xx_without_typed_evidence_is_unknown() {
        let _guard = EnvGuard::enabled();
        for status in [400, 401, 403, 404, 409, 413, 415, 422] {
            let client = ProviderEmbeddingClient::new(Arc::new(MockTransport::new(vec![Err(
                HttpError::Http {
                    status,
                    reason: "untrusted provider detail".to_string(),
                },
            )])));
            assert!(client
                .send_embedding_once(HttpRequest {
                    url: "https://example.invalid/embeddings".to_string(),
                    method: "POST".to_string(),
                    headers: Vec::new(),
                    body: Some(Vec::new()),
                    timeout_secs: Some(1.0),
                })
                .unwrap_err()
                .outcome_unknown());
        }
    }

    struct ConcurrencyProbeTransport {
        calls: AtomicUsize,
        active: AtomicUsize,
        maximum: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl HttpTransport for ConcurrencyProbeTransport {
        async fn send(&self, _request: &HttpRequest) -> Result<HttpResponse, HttpError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.maximum.fetch_max(active, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status: 200,
                body: Vec::new(),
            })
        }
    }

    #[test]
    fn transport_executor_bounds_concurrency_and_shuts_down_workers() {
        let _guard = EnvGuard::enabled();
        let executor = Arc::new(BoundedTransportExecutor::new(2, 4).unwrap());
        let liveness = executor.liveness_probe();
        let admission_failures = Arc::new(AtomicUsize::new(0));
        let unexpected_failures = Arc::new(AtomicUsize::new(0));
        let transport = Arc::new(ConcurrencyProbeTransport {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        });
        std::thread::scope(|scope| {
            for _ in 0..24 {
                let executor = Arc::clone(&executor);
                let transport = Arc::clone(&transport);
                let admission_failures = Arc::clone(&admission_failures);
                let unexpected_failures = Arc::clone(&unexpected_failures);
                scope.spawn(move || {
                    if let Err(error) = executor.send(
                        transport,
                        HttpRequest {
                            url: "https://example.invalid/embeddings".to_string(),
                            method: "POST".to_string(),
                            headers: Vec::new(),
                            body: Some(Vec::new()),
                            timeout_secs: Some(1.0),
                        },
                    ) {
                        if matches!(error, HttpError::PreSend(_)) {
                            admission_failures.fetch_add(1, Ordering::SeqCst);
                        } else {
                            unexpected_failures.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                });
            }
        });
        let observed_maximum = transport.maximum.load(Ordering::SeqCst);
        assert!((1..=2).contains(&observed_maximum));
        assert!(admission_failures.load(Ordering::SeqCst) > 0);
        assert_eq!(unexpected_failures.load(Ordering::SeqCst), 0);
        assert_eq!(liveness.load(Ordering::SeqCst), 2);
        drop(executor);
        assert_eq!(liveness.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn transport_executor_deadline_includes_worker_execution() {
        let _guard = EnvGuard::enabled();
        let executor = BoundedTransportExecutor::new(1, 1).unwrap();
        let transport = Arc::new(ConcurrencyProbeTransport {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        });
        let started = std::time::Instant::now();
        let error = executor
            .send(
                transport,
                HttpRequest {
                    url: "https://example.invalid/embeddings".to_string(),
                    method: "POST".to_string(),
                    headers: Vec::new(),
                    body: Some(Vec::new()),
                    timeout_secs: Some(0.005),
                },
            )
            .unwrap_err();
        assert!(matches!(error, HttpError::Timeout(_)));
        assert!(started.elapsed() < Duration::from_millis(20));
    }

    #[test]
    fn stopped_executor_before_admission_is_typed_pre_send() {
        let mut executor = BoundedTransportExecutor::new(1, 1).unwrap();
        executor.sender.take();
        let error = executor
            .send(
                Arc::new(MockTransport::empty()),
                HttpRequest {
                    url: "https://example.invalid/embeddings".to_string(),
                    method: "POST".to_string(),
                    headers: Vec::new(),
                    body: Some(Vec::new()),
                    timeout_secs: Some(1.0),
                },
            )
            .unwrap_err();
        assert!(matches!(error, HttpError::PreSend(_)));
    }

    #[test]
    fn expired_queued_transport_task_never_sends_after_caller_timeout() {
        let _guard = EnvGuard::enabled();
        let executor = Arc::new(BoundedTransportExecutor::new(1, 2).unwrap());
        let transport = Arc::new(ConcurrencyProbeTransport {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
        });
        std::thread::scope(|scope| {
            let thread_executor = Arc::clone(&executor);
            let thread_transport = Arc::clone(&transport);
            scope.spawn(move || {
                thread_executor
                    .send(
                        thread_transport,
                        HttpRequest {
                            url: "https://example.invalid/embeddings".to_string(),
                            method: "POST".to_string(),
                            headers: Vec::new(),
                            body: Some(Vec::new()),
                            timeout_secs: Some(1.0),
                        },
                    )
                    .unwrap();
            });
            let start_deadline = std::time::Instant::now() + Duration::from_secs(1);
            while transport.active.load(Ordering::SeqCst) == 0
                && std::time::Instant::now() < start_deadline
            {
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(transport.active.load(Ordering::SeqCst), 1);
            let error = executor
                .send(
                    Arc::clone(&transport) as Arc<dyn HttpTransport>,
                    HttpRequest {
                        url: "https://example.invalid/embeddings".to_string(),
                        method: "POST".to_string(),
                        headers: Vec::new(),
                        body: Some(Vec::new()),
                        timeout_secs: Some(0.005),
                    },
                )
                .unwrap_err();
            assert!(matches!(error, HttpError::Timeout(_)));
        });
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(transport.calls.load(Ordering::SeqCst), 1);
        assert_eq!(transport.maximum.load(Ordering::SeqCst), 1);
        assert_eq!(transport.active.load(Ordering::SeqCst), 0);
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
