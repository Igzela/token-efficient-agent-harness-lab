//! Task-scoped Codex budget authority and loopback provider mediation.
//!
//! The installed Codex CLI 0.145.0 exposes `features.rollout_budget.limit_tokens`,
//! but that knob does not enforce a pre-dispatch or during-call provider token
//! ceiling (a turn can complete with measured usage far above the configured
//! limit). Product-managed Codex therefore requires this app-owned loopback
//! gateway:
//!
//! - real provider credentials never enter the child environment;
//! - every forwarded provider request is bound to the active task execution;
//! - model identity is pinned;
//! - request count and cumulative tokens are tracked before further dispatch;
//! - `max_output_tokens` is injected so the provider request itself is bounded;
//! - input reservation uses a documented byte-upper-bound (UTF-8 bytes) that is
//!   always ≥ token count for any tokenizer that emits at most one token per
//!   input byte.
//!
//! Process kill after unbounded generation is not treated as a during-call cap.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::config::{validate_binary_file_identity, ADMITTED_CODEX_VERSION};

pub const CODEX_BUDGET_AUTHORITY_SCHEMA: &str = "codex_budget_authority.v1";
pub const CODEX_BUDGET_GATEWAY_SCHEMA: &str = "codex_budget_gateway.v1";
pub const CODEX_SESSION_TOKEN_PREFIX: &str = "acp-codex-budget-";

/// Admitted product-managed Codex CLI identity (exact version pin).
pub const ADMITTED_CODEX_CLI_VERSION: &str = ADMITTED_CODEX_VERSION;

/// Default per-request provider output ceiling when the product budget does not
/// pin a lower value. Kept deliberately below common model maxima so product
/// tasks fail closed rather than open-ended generation.
pub const DEFAULT_CODEX_MAX_OUTPUT_TOKENS_PER_REQUEST: u64 = 8_192;

/// Default max provider HTTP requests (including internal retries Codex may issue).
pub const DEFAULT_CODEX_MAX_PROVIDER_REQUESTS: u64 = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct CodexExecutableIdentity {
    pub binary_path: PathBuf,
    pub binary_version: String,
    pub binary_sha256: String,
}

impl CodexExecutableIdentity {
    pub fn validate(
        binary_path: &Path,
        expected_version: &str,
        expected_sha256: &str,
    ) -> Result<Self, String> {
        if !binary_path.is_absolute() {
            return Err("Codex binary path must be absolute".to_string());
        }
        let canonical = std::fs::canonicalize(binary_path)
            .map_err(|error| format!("Codex binary is unavailable: {error}"))?;
        if canonical != binary_path {
            return Err(
                "Codex binary path must already be canonical with no symlink components"
                    .to_string(),
            );
        }
        let expected_sha256 = expected_sha256.trim().to_ascii_lowercase();
        let binary_sha256 = validate_binary_file_identity(binary_path, &expected_sha256)?;
        let expected_version = expected_version.trim();
        if expected_version != ADMITTED_CODEX_CLI_VERSION {
            return Err(format!(
                "Codex binary version is not the admitted version {ADMITTED_CODEX_CLI_VERSION}"
            ));
        }
        Ok(Self {
            binary_path: binary_path.to_path_buf(),
            binary_version: expected_version.to_string(),
            binary_sha256,
        })
    }
}

/// Pre-launch authority binding for one product-managed Codex execution.
#[derive(Debug, Clone, PartialEq)]
pub struct CodexBudgetAuthority {
    pub schema_version: String,
    pub task_id: String,
    pub workflow_node_id: String,
    pub execution_id: String,
    pub executable: CodexExecutableIdentity,
    pub model: String,
    pub max_provider_requests: u64,
    pub max_retries: u64,
    pub max_input_tokens_per_request: u64,
    pub max_output_tokens_per_request: u64,
    pub max_cumulative_tokens: u64,
    pub max_cost_usd: Option<f64>,
    pub timeout_ms: u64,
    pub worktree: PathBuf,
    pub expires_unix_ms: u64,
}

impl CodexBudgetAuthority {
    pub fn validate_new(self) -> Result<Self, String> {
        if self.schema_version != CODEX_BUDGET_AUTHORITY_SCHEMA {
            return Err("codex budget authority schema version is unsupported".to_string());
        }
        for (name, value) in [
            ("task_id", self.task_id.as_str()),
            ("workflow_node_id", self.workflow_node_id.as_str()),
            ("execution_id", self.execution_id.as_str()),
            ("model", self.model.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("codex budget authority {name} is required"));
            }
        }
        if self.max_provider_requests == 0 {
            return Err("max_provider_requests must be > 0".to_string());
        }
        if self.max_input_tokens_per_request == 0 || self.max_output_tokens_per_request == 0 {
            return Err("per-request token ceilings must be > 0".to_string());
        }
        if self.max_cumulative_tokens == 0 {
            return Err("max_cumulative_tokens must be > 0".to_string());
        }
        if self.max_output_tokens_per_request > self.max_cumulative_tokens {
            return Err(
                "max_output_tokens_per_request cannot exceed max_cumulative_tokens".to_string(),
            );
        }
        if self.timeout_ms == 0 {
            return Err("timeout_ms must be > 0".to_string());
        }
        if !self.worktree.is_absolute() {
            return Err("worktree must be an absolute path".to_string());
        }
        if now_unix_ms() >= self.expires_unix_ms {
            return Err("codex budget authority has expired".to_string());
        }
        if let Some(cost) = self.max_cost_usd {
            if !cost.is_finite() || cost < 0.0 {
                return Err("max_cost_usd must be finite and non-negative".to_string());
            }
        }
        Ok(self)
    }

    pub fn to_identity_json(&self) -> Value {
        json!({
            "schema_version": "managed_executor_identity.v1",
            "executor_type": "codex_cli",
            "executor_class": "managed_coding",
            "binary_path": self.executable.binary_path,
            "binary_version": self.executable.binary_version,
            "binary_sha256": self.executable.binary_sha256,
            "model": self.model,
            "model_resolution": "exact_admitted_pin",
            "budget_authority_schema": CODEX_BUDGET_AUTHORITY_SCHEMA,
            "execution_id": self.execution_id,
            "max_provider_requests": self.max_provider_requests,
            "max_retries": self.max_retries,
            "max_input_tokens_per_request": self.max_input_tokens_per_request,
            "max_output_tokens_per_request": self.max_output_tokens_per_request,
            "max_cumulative_tokens": self.max_cumulative_tokens,
            "max_cost_usd": self.max_cost_usd,
            "timeout_ms": self.timeout_ms,
            "mediation": "loopback_budget_gateway",
            "input_reservation": "utf8_byte_upper_bound",
        })
    }
}

/// Conservative input token upper bound: each UTF-8 byte counts as one token.
/// Any practical tokenizer emits ≤ 1 token per input byte, so this is always an
/// upper bound and may over-reserve. It is not an exact tokenizer.
pub fn conservative_input_token_upper_bound(body: &[u8]) -> u64 {
    body.len() as u64
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetRejectClass {
    /// Rejected before any upstream provider request was started.
    PreCall,
    /// Upstream returned a known failure with no billed effect assumed only when
    /// the gateway never forwarded a body (not used for post-forward paths).
    KnownNoEffect,
    /// A request was forwarded and the outcome cannot be classified cleanly.
    OutcomeUnknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetGatewayUsage {
    pub provider_requests: u64,
    pub cumulative_input_tokens: u64,
    pub cumulative_output_tokens: u64,
    pub cumulative_tokens: u64,
    pub last_reject: Option<String>,
    pub last_reject_class: Option<String>,
}

#[derive(Debug)]
struct GatewayState {
    authority: CodexBudgetAuthority,
    session_token: String,
    upstream_base_url: String,
    /// Real upstream credential; never exposed to the child process.
    upstream_api_key: String,
    provider_requests: AtomicU64,
    cumulative_input_tokens: AtomicU64,
    cumulative_output_tokens: AtomicU64,
    stop: AtomicBool,
    last_reject: Mutex<Option<(BudgetRejectClass, String)>>,
    /// Serializes pre-dispatch residual checks so concurrent requests cannot overspend.
    budget_lock: Mutex<()>,
    /// Optional path for restart-safe committed usage journal (no secrets/prompts).
    usage_journal_path: Option<PathBuf>,
}

impl GatewayState {
    fn cumulative_tokens(&self) -> u64 {
        self.cumulative_input_tokens
            .load(Ordering::SeqCst)
            .saturating_add(self.cumulative_output_tokens.load(Ordering::SeqCst))
    }

    fn record_reject(&self, class: BudgetRejectClass, message: String) {
        if let Ok(mut guard) = self.last_reject.lock() {
            *guard = Some((class, message));
        }
    }

    fn usage_snapshot(&self) -> BudgetGatewayUsage {
        let (last_reject_class, last_reject) = self
            .last_reject
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .map(|(class, message)| {
                let class = match class {
                    BudgetRejectClass::PreCall => "pre_call",
                    BudgetRejectClass::KnownNoEffect => "known_no_effect",
                    BudgetRejectClass::OutcomeUnknown => "outcome_unknown",
                };
                (Some(class.to_string()), Some(message))
            })
            .unwrap_or((None, None));
        BudgetGatewayUsage {
            provider_requests: self.provider_requests.load(Ordering::SeqCst),
            cumulative_input_tokens: self.cumulative_input_tokens.load(Ordering::SeqCst),
            cumulative_output_tokens: self.cumulative_output_tokens.load(Ordering::SeqCst),
            cumulative_tokens: self.cumulative_tokens(),
            last_reject,
            last_reject_class,
        }
    }
}

/// Loopback HTTP gateway that mediates Codex → provider traffic for one task.
pub struct CodexBudgetGateway {
    state: Arc<GatewayState>,
    local_addr: SocketAddr,
    join: Option<JoinHandle<()>>,
}

impl CodexBudgetGateway {
    pub fn start(
        authority: CodexBudgetAuthority,
        upstream_base_url: &str,
        upstream_api_key: &str,
    ) -> Result<Self, String> {
        Self::start_with_journal(authority, upstream_base_url, upstream_api_key, None)
    }

    /// Start the gateway, optionally restoring committed counters from a journal
    /// written by a prior gateway for the same execution identity.
    pub fn start_with_journal(
        authority: CodexBudgetAuthority,
        upstream_base_url: &str,
        upstream_api_key: &str,
        usage_journal_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        let authority = authority.validate_new()?;
        let upstream_base_url = normalize_base_url(upstream_base_url)?;
        if upstream_api_key.trim().is_empty() {
            return Err("upstream API key is required for Codex budget mediation".to_string());
        }
        // One-use unforgeable session token: random UUID material + execution binding.
        // Not derivable from task metadata alone, so another task cannot replay it.
        let session_token = format!(
            "{CODEX_SESSION_TOKEN_PREFIX}{}",
            hex::encode(Sha256::digest(
                format!(
                    "{}:{}:{}:{}",
                    Uuid::new_v4(),
                    authority.execution_id,
                    authority.task_id,
                    Uuid::new_v4()
                )
                .as_bytes()
            ))
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .map_err(|error| format!("failed to bind codex budget gateway: {error}"))?;
        listener
            .set_nonblocking(false)
            .map_err(|error| format!("failed to configure codex budget gateway: {error}"))?;
        let local_addr = listener
            .local_addr()
            .map_err(|error| format!("failed to read gateway address: {error}"))?;

        let mut restored_requests = 0u64;
        let mut restored_input = 0u64;
        let mut restored_output = 0u64;
        if let Some(path) = usage_journal_path.as_ref() {
            if path.is_file() {
                let entry =
                    super::codex_mediation_admission::CodexUsageJournalEntry::load_from(path)?;
                if entry.execution_id != authority.execution_id
                    || entry.task_id != authority.task_id
                {
                    return Err(
                        "usage journal execution/task identity does not match authority"
                            .to_string(),
                    );
                }
                restored_requests = entry.provider_requests;
                restored_input = entry.cumulative_input_tokens;
                restored_output = entry.cumulative_output_tokens;
            }
        }

        let state = Arc::new(GatewayState {
            authority,
            session_token,
            upstream_base_url,
            upstream_api_key: upstream_api_key.to_string(),
            provider_requests: AtomicU64::new(restored_requests),
            cumulative_input_tokens: AtomicU64::new(restored_input),
            cumulative_output_tokens: AtomicU64::new(restored_output),
            stop: AtomicBool::new(false),
            last_reject: Mutex::new(None),
            budget_lock: Mutex::new(()),
            usage_journal_path,
        });

        let thread_state = Arc::clone(&state);
        let join = thread::Builder::new()
            .name("codex-budget-gateway".into())
            .spawn(move || gateway_accept_loop(listener, thread_state))
            .map_err(|error| format!("failed to start codex budget gateway thread: {error}"))?;

        Ok(Self {
            state,
            local_addr,
            join: Some(join),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.local_addr)
    }

    pub fn session_token(&self) -> &str {
        &self.state.session_token
    }

    pub fn authority(&self) -> &CodexBudgetAuthority {
        &self.state.authority
    }

    pub fn usage(&self) -> BudgetGatewayUsage {
        self.state.usage_snapshot()
    }

    pub fn shutdown(mut self) -> BudgetGatewayUsage {
        self.state.stop.store(true, Ordering::SeqCst);
        // Unblock accept with a local connection.
        if let Ok(stream) = TcpStream::connect_timeout(&self.local_addr, Duration::from_millis(100))
        {
            drop(stream);
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        self.state.usage_snapshot()
    }
}

impl Drop for CodexBudgetGateway {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
        // Best-effort wakeup; ignore failures if the listener is already gone.
        if let Ok(stream) = TcpStream::connect_timeout(&self.local_addr, Duration::from_millis(100))
        {
            drop(stream);
        }
        if let Some(join) = self.join.take() {
            // Bounded wait: never hang the test harness on a stuck accept loop.
            let _ = join.join();
        }
    }
}

fn gateway_accept_loop(listener: TcpListener, state: Arc<GatewayState>) {
    // Non-blocking accept so the stop flag can be observed without a client.
    let _ = listener.set_nonblocking(true);
    while !state.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if state.stop.load(Ordering::SeqCst) {
                    // Wakeup/shutdown connection — do not block on HTTP parse.
                    break;
                }
                let _ = stream.set_nonblocking(false);
                // Handle serially: one product task, one-use execution identity.
                if let Err(error) = handle_client(stream, &state) {
                    state.record_reject(BudgetRejectClass::OutcomeUnknown, error);
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(_) => break,
        }
    }
}

fn handle_client(mut stream: TcpStream, state: &GatewayState) -> Result<(), String> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let request = read_http_request(&mut stream)?;
    let response = dispatch_request(state, &request);
    write_http_response(&mut stream, &response)
}

#[derive(Debug)]
struct HttpRequestParts {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Debug)]
struct HttpResponseParts {
    status: u16,
    reason: &'static str,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequestParts, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("gateway read failed: {error}"))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if find_header_end(&buffer).is_some() {
            break;
        }
        if buffer.len() > 1024 * 1024 {
            return Err("gateway request headers exceed 1 MiB".to_string());
        }
    }
    let header_end =
        find_header_end(&buffer).ok_or_else(|| "incomplete HTTP headers".to_string())?;
    let header_text = std::str::from_utf8(&buffer[..header_end])
        .map_err(|_| "HTTP headers are not valid UTF-8".to_string())?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "missing method".to_string())?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| "missing path".to_string())?
        .to_string();
    let mut headers = Vec::new();
    let mut content_length = 0usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "malformed header".to_string())?;
        let name = name.trim().to_string();
        let value = value.trim().to_string();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = value
                .parse()
                .map_err(|_| "invalid content-length".to_string())?;
        }
        headers.push((name, value));
    }
    if content_length > 8 * 1024 * 1024 {
        return Err("gateway request body exceeds 8 MiB".to_string());
    }
    let mut body = buffer[header_end + 4..].to_vec();
    while body.len() < content_length {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("gateway body read failed: {error}"))?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
    }
    body.truncate(content_length);
    Ok(HttpRequestParts {
        method,
        path,
        headers,
        body,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_http_response(stream: &mut TcpStream, response: &HttpResponseParts) -> Result<(), String> {
    let mut out = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.reason,
        response.body.len()
    );
    for (name, value) in &response.headers {
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    stream
        .write_all(out.as_bytes())
        .and_then(|_| stream.write_all(&response.body))
        .map_err(|error| format!("gateway write failed: {error}"))
}

fn dispatch_request(state: &GatewayState, request: &HttpRequestParts) -> HttpResponseParts {
    if now_unix_ms() >= state.authority.expires_unix_ms {
        state.record_reject(
            BudgetRejectClass::PreCall,
            "codex budget authority expired".to_string(),
        );
        return json_error(401, "authority_expired", "codex budget authority expired");
    }
    if !authorized(request, &state.session_token) {
        state.record_reject(
            BudgetRejectClass::PreCall,
            "request is not bound to the active codex budget session".to_string(),
        );
        return json_error(
            401,
            "session_unbound",
            "request is not bound to the active codex budget session",
        );
    }

    // Allow only OpenAI-compatible paths Codex needs under API-key mediation.
    let path = request.path.as_str();
    if request.method.eq_ignore_ascii_case("GET") && (path == "/v1/models" || path == "/models") {
        return json_ok(json!({
            "object": "list",
            "data": [{
                "id": state.authority.model,
                "object": "model",
                "owned_by": "acp-budget-gateway"
            }]
        }));
    }

    let is_responses =
        path == "/v1/responses" || path == "/responses" || path.ends_with("/responses");
    let is_chat = path == "/v1/chat/completions"
        || path == "/chat/completions"
        || path.ends_with("/chat/completions");
    if !(request.method.eq_ignore_ascii_case("POST") && (is_responses || is_chat)) {
        state.record_reject(
            BudgetRejectClass::PreCall,
            format!(
                "path {} is not admitted for codex budget mediation",
                request.path
            ),
        );
        return json_error(
            403,
            "path_not_admitted",
            "only admitted model-generation endpoints may be mediated",
        );
    }

    let mut body: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(_) => {
            state.record_reject(
                BudgetRejectClass::PreCall,
                "provider request body is not valid JSON".to_string(),
            );
            return json_error(
                400,
                "malformed_body",
                "provider request body is not valid JSON",
            );
        }
    };

    let requested_model = body
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if requested_model.is_empty() {
        body.as_object_mut()
            .expect("object body")
            .insert("model".into(), json!(state.authority.model));
    } else if requested_model != state.authority.model {
        state.record_reject(
            BudgetRejectClass::PreCall,
            format!(
                "model substitution rejected: requested={requested_model} admitted={}",
                state.authority.model
            ),
        );
        return json_error(
            403,
            "model_substitution",
            "model substitution is not admitted",
        );
    }

    // Force non-stream mediation so usage is always extractable before completion.
    // Streaming would allow unbounded generation without a mid-stream hard stop that
    // is still a pre-dispatch output ceiling; the provider-side max_output_tokens
    // field is the during-call bound.
    if let Some(object) = body.as_object_mut() {
        object.insert("stream".into(), json!(false));
    }

    let max_out = state.authority.max_output_tokens_per_request;
    // Reject explicit removal, zero, or increase of the provider-side output ceiling
    // before injection. The gateway always re-injects the admitted cap for requests
    // that omit the field.
    if let Some(object) = body.as_object() {
        let key = if is_responses {
            "max_output_tokens"
        } else {
            "max_tokens"
        };
        match object.get(key) {
            None => {}
            Some(Value::Null) => {
                state.record_reject(
                    BudgetRejectClass::PreCall,
                    "output token limit removal is not admitted".to_string(),
                );
                return json_error(
                    403,
                    "output_limit_required",
                    "provider-side output token limit removal is not admitted",
                );
            }
            Some(value) => match value.as_u64() {
                Some(0) => {
                    state.record_reject(
                        BudgetRejectClass::PreCall,
                        "output token limit must be > 0".to_string(),
                    );
                    return json_error(
                        403,
                        "output_limit_invalid",
                        "provider-side output token limit must be positive",
                    );
                }
                Some(requested) if requested > max_out => {
                    state.record_reject(
                        BudgetRejectClass::PreCall,
                        format!(
                            "output token limit increase rejected: requested={requested} admitted={max_out}"
                        ),
                    );
                    return json_error(
                        403,
                        "output_limit_increase",
                        "raising the provider-side output token limit is not admitted",
                    );
                }
                Some(_) => {}
                None => {
                    state.record_reject(
                        BudgetRejectClass::PreCall,
                        "output token limit must be a positive integer".to_string(),
                    );
                    return json_error(
                        403,
                        "output_limit_invalid",
                        "provider-side output token limit must be a positive integer",
                    );
                }
            },
        }
    }
    // Hard single-request bound: inject the admitted provider-side output ceiling.
    inject_max_output_tokens(&mut body, max_out, is_responses);

    let reserved_input = conservative_input_token_upper_bound(
        &serde_json::to_vec(&body).unwrap_or_else(|_| request.body.clone()),
    );

    // Hold the budget lock across residual check + request count so concurrent
    // connections cannot overspend the same task authority.
    {
        let _guard = match state.budget_lock.lock() {
            Ok(guard) => guard,
            Err(_) => {
                state.record_reject(
                    BudgetRejectClass::PreCall,
                    "budget lock poisoned".to_string(),
                );
                return json_error(500, "budget_lock_poisoned", "budget lock poisoned");
            }
        };

        if reserved_input > state.authority.max_input_tokens_per_request {
            state.record_reject(
                BudgetRejectClass::PreCall,
                format!(
                    "input reservation {reserved_input} exceeds max_input_tokens_per_request {}",
                    state.authority.max_input_tokens_per_request
                ),
            );
            return json_error(
                429,
                "input_budget_exhausted",
                "conservative input reservation exceeds per-request input ceiling",
            );
        }

        let prior_requests = state.provider_requests.load(Ordering::SeqCst);
        if prior_requests >= state.authority.max_provider_requests {
            state.record_reject(
                BudgetRejectClass::PreCall,
                "max_provider_requests exhausted".to_string(),
            );
            return json_error(
                429,
                "request_budget_exhausted",
                "max_provider_requests exhausted",
            );
        }
        // Retries are ordinary provider POSTs and count toward max_provider_requests.
        // max_retries is recorded on the authority for product evidence; it never
        // expands the hard request ceiling.

        let used = state.cumulative_tokens();
        let remaining = state.authority.max_cumulative_tokens.saturating_sub(used);
        // Pre-dispatch residual check: reserved input + max possible output must fit.
        let worst_case = reserved_input.saturating_add(max_out);
        if worst_case > remaining {
            state.record_reject(
                BudgetRejectClass::PreCall,
                format!(
                    "remaining cumulative budget {remaining} insufficient for reserved input {reserved_input} + max output {max_out}"
                ),
            );
            return json_error(
                429,
                "cumulative_budget_insufficient",
                "remaining cumulative budget is insufficient for the next bounded request",
            );
        }

        // Count the request only once pre-checks pass and immediately before forward.
        state.provider_requests.fetch_add(1, Ordering::SeqCst);
    }

    match forward_upstream(state, path, &body) {
        Ok((status, response_body)) => match extract_usage(&response_body) {
            Ok((input_tokens, output_tokens)) => {
                state
                    .cumulative_input_tokens
                    .fetch_add(input_tokens, Ordering::SeqCst);
                state
                    .cumulative_output_tokens
                    .fetch_add(output_tokens, Ordering::SeqCst);
                let total = state.cumulative_tokens();
                if total > state.authority.max_cumulative_tokens {
                    state.record_reject(
                        BudgetRejectClass::OutcomeUnknown,
                        "cumulative tokens exceeded after measured usage".to_string(),
                    );
                }
                // Persist committed usage for restart recovery (no secrets/prompts).
                if let Some(path) = state.usage_journal_path.as_ref() {
                    let usage = state.usage_snapshot();
                    let entry =
                        super::codex_mediation_admission::CodexUsageJournalEntry::from_usage(
                            &state.authority,
                            &usage,
                            extract_response_id(&response_body),
                        );
                    let _ = entry.write_to(path);
                }
                HttpResponseParts {
                    status,
                    reason: reason_phrase(status),
                    headers: vec![("Content-Type".into(), "application/json".into())],
                    body: response_body,
                }
            }
            Err(error) => {
                state.record_reject(BudgetRejectClass::OutcomeUnknown, error.clone());
                // Fail closed: ambiguous usage after a forwarded call is not success.
                // Outcome-unknown does not auto-retry; the child must observe the error.
                json_error(502, "usage_unavailable", &error)
            }
        },
        Err(error) => {
            state.record_reject(BudgetRejectClass::OutcomeUnknown, error.clone());
            json_error(502, "upstream_forward_failed", &error)
        }
    }
}

fn extract_response_id(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    value.get("id").and_then(Value::as_str).map(str::to_string)
}

fn authorized(request: &HttpRequestParts, session_token: &str) -> bool {
    for (name, value) in &request.headers {
        if name.eq_ignore_ascii_case("authorization") {
            let value = value.trim();
            if let Some(token) = value
                .strip_prefix("Bearer ")
                .or_else(|| value.strip_prefix("bearer "))
            {
                return token.trim() == session_token;
            }
        }
        if name.eq_ignore_ascii_case("x-api-key") && value.trim() == session_token {
            return true;
        }
    }
    false
}

fn inject_max_output_tokens(body: &mut Value, max_out: u64, is_responses: bool) {
    let Some(object) = body.as_object_mut() else {
        return;
    };
    let key = if is_responses {
        "max_output_tokens"
    } else {
        "max_tokens"
    };
    let existing = object.get(key).and_then(Value::as_u64);
    let capped = existing.map(|value| value.min(max_out)).unwrap_or(max_out);
    object.insert(key.to_string(), json!(capped));
    // Also cap Responses-compatible aliases if present.
    if let Some(value) = object.get("max_tokens").and_then(Value::as_u64) {
        object.insert("max_tokens".into(), json!(value.min(max_out)));
    }
}

fn extract_usage(body: &[u8]) -> Result<(u64, u64), String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| "upstream response is not valid JSON".to_string())?;
    let usage = value
        .get("usage")
        .or_else(|| value.pointer("/response/usage"))
        .ok_or_else(|| "upstream response is missing usage".to_string())?;
    let input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(Value::as_u64)
        .ok_or_else(|| "upstream usage is missing input_tokens".to_string())?;
    let output = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(Value::as_u64)
        .ok_or_else(|| "upstream usage is missing output_tokens".to_string())?;
    Ok((input, output))
}

fn forward_upstream(
    state: &GatewayState,
    path: &str,
    body: &Value,
) -> Result<(u16, Vec<u8>), String> {
    let path = if path.starts_with("/v1/") {
        path.trim_start_matches("/v1").to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let url = format!("{}{}", state.upstream_base_url, path);
    let body_bytes =
        serde_json::to_vec(body).map_err(|error| format!("failed to encode body: {error}"))?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to build forward runtime: {error}"))?;
    runtime.block_on(async move {
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(state.authority.timeout_ms.max(1_000)))
            .build()
            .map_err(|error| format!("failed to build upstream client: {error}"))?;
        let response = client
            .post(&url)
            .header(
                "Authorization",
                format!("Bearer {}", state.upstream_api_key),
            )
            .header("Content-Type", "application/json")
            .body(body_bytes)
            .send()
            .await
            .map_err(|error| format!("upstream request failed: {error}"))?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| format!("upstream body read failed: {error}"))?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("upstream response exceeds 8 MiB".to_string());
        }
        Ok((status, bytes.to_vec()))
    })
}

fn json_ok(body: Value) -> HttpResponseParts {
    let body = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    HttpResponseParts {
        status: 200,
        reason: "OK",
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    }
}

fn json_error(status: u16, code: &str, message: &str) -> HttpResponseParts {
    let body = serde_json::to_vec(&json!({
        "error": {
            "message": message,
            "type": code,
            "code": code,
            "param": null
        }
    }))
    .unwrap_or_else(|_| b"{}".to_vec());
    HttpResponseParts {
        status,
        reason: reason_phrase(status),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body,
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        429 => "Too Many Requests",
        502 => "Bad Gateway",
        _ => "Error",
    }
}

fn normalize_base_url(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        return Err("upstream base URL is required".to_string());
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("upstream base URL must be http or https".to_string());
    }
    // Accept either .../v1 or host root; forward_upstream joins paths carefully.
    Ok(url.to_string())
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Build a one-use authority from product node metadata and exact executable identity.
pub fn authority_from_product_metadata(
    executable: CodexExecutableIdentity,
    metadata: &Value,
    model: &str,
    timeout_ms: u64,
) -> Result<CodexBudgetAuthority, String> {
    let task_id = metadata
        .get("product_task_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "product codex budget requires product_task_id".to_string())?
        .to_string();
    let workflow_node_id = metadata
        .get("node_id")
        .or_else(|| metadata.get("workflow_node_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("product-apply")
        .to_string();
    let worktree = metadata
        .get("workspace_path")
        .or_else(|| metadata.get("workspace_root"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "product codex budget requires workspace_path".to_string())?;
    let budget = metadata
        .get("product_budget")
        .ok_or_else(|| "product codex budget requires product_budget".to_string())?;
    let max_cumulative_tokens = budget
        .get("total_tokens")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| "product_budget.total_tokens must be > 0".to_string())?;
    let max_provider_requests = budget
        .get("total_calls")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_CODEX_MAX_PROVIDER_REQUESTS)
        .max(1);
    let max_retries = budget
        .get("max_retries")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let max_output_tokens_per_request = budget
        .get("max_output_tokens_per_request")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_CODEX_MAX_OUTPUT_TOKENS_PER_REQUEST)
        .min(max_cumulative_tokens)
        .max(1);
    let max_input_tokens_per_request = budget
        .get("max_input_tokens_per_request")
        .and_then(Value::as_u64)
        .unwrap_or(max_cumulative_tokens)
        .min(max_cumulative_tokens)
        .max(1);
    let execution_id = format!(
        "codex-exec-{}",
        hex::encode(
            &Sha256::digest(
                format!(
                    "{task_id}:{workflow_node_id}:{}:{}",
                    executable.binary_sha256,
                    Instant::now().elapsed().as_nanos()
                )
                .as_bytes()
            )[..16]
        )
    );
    let expires_unix_ms = now_unix_ms().saturating_add(timeout_ms.saturating_add(60_000));
    CodexBudgetAuthority {
        schema_version: CODEX_BUDGET_AUTHORITY_SCHEMA.to_string(),
        task_id,
        workflow_node_id,
        execution_id,
        executable,
        model: model.trim().to_string(),
        max_provider_requests,
        max_retries,
        max_input_tokens_per_request,
        max_output_tokens_per_request,
        max_cumulative_tokens,
        max_cost_usd: None,
        timeout_ms,
        worktree: PathBuf::from(worktree),
        expires_unix_ms,
    }
    .validate_new()
}

/// Create an ephemeral CODEX_HOME that forces API-key auth through the gateway.
pub fn write_ephemeral_codex_home(
    root: &Path,
    model: &str,
    gateway_base_url: &str,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(root)
        .map_err(|error| format!("failed to create ephemeral CODEX_HOME: {error}"))?;
    let config = format!(
        r#"# Generated by ACP Codex budget gateway — do not reuse outside one task execution.
model = "{model}"
model_provider = "acp_budget_gateway"
approval_policy = "never"
sandbox_mode = "workspace-write"
suppress_unstable_features_warning = true

[model_providers.acp_budget_gateway]
name = "ACP Budget Gateway"
base_url = "{gateway_base_url}"
env_key = "OPENAI_API_KEY"
wire_api = "responses"
requires_openai_auth = true
"#
    );
    std::fs::write(root.join("config.toml"), config)
        .map_err(|error| format!("failed to write ephemeral Codex config: {error}"))?;
    // Empty auth.json forces env-key auth rather than ChatGPT login reuse.
    std::fs::write(root.join("auth.json"), "{}\n")
        .map_err(|error| format!("failed to write ephemeral Codex auth: {error}"))?;
    Ok(root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Arc;

    fn sample_authority(max_requests: u64, max_cumulative: u64) -> CodexBudgetAuthority {
        let binary =
            std::env::temp_dir().join(format!("codex-budget-test-bin-{}", std::process::id()));
        std::fs::write(&binary, b"#!/bin/sh\necho codex-cli 0.145.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let sha = {
            use sha2::{Digest, Sha256};
            hex::encode(Sha256::digest(std::fs::read(&binary).unwrap()))
        };
        let executable = CodexExecutableIdentity {
            binary_path: binary,
            binary_version: ADMITTED_CODEX_CLI_VERSION.to_string(),
            binary_sha256: sha,
        };
        CodexBudgetAuthority {
            schema_version: CODEX_BUDGET_AUTHORITY_SCHEMA.to_string(),
            task_id: "ptask-test".into(),
            workflow_node_id: "node-1".into(),
            execution_id: "exec-1".into(),
            executable,
            model: "gpt-test-model".into(),
            max_provider_requests: max_requests,
            max_retries: 0,
            max_input_tokens_per_request: 50_000,
            max_output_tokens_per_request: 128,
            max_cumulative_tokens: max_cumulative,
            max_cost_usd: None,
            timeout_ms: 30_000,
            worktree: std::env::temp_dir(),
            expires_unix_ms: now_unix_ms() + 60_000,
        }
    }

    fn spawn_fake_upstream(hits: Arc<AtomicUsize>, force_usage: bool) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let _ = listener.set_nonblocking(true);
        let join = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = [0u8; 65536];
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let _ = stream.read(&mut buf);
                        hits.fetch_add(1, AtomicOrdering::SeqCst);
                        let body = if force_usage {
                            br#"{"id":"resp_1","usage":{"input_tokens":10,"output_tokens":5},"output":[]}"#
                                .to_vec()
                        } else {
                            br#"{"id":"resp_1","output":[]}"#.to_vec()
                        };
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.write_all(&body);
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            || error.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });
        (format!("http://{addr}"), join)
    }

    #[test]
    fn conservative_input_bound_uses_bytes() {
        assert_eq!(conservative_input_token_upper_bound(b"abcd"), 4);
        assert_eq!(conservative_input_token_upper_bound("é".as_bytes()), 2);
    }

    #[test]
    fn gateway_rejects_unbound_session_and_model_substitution() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), true);
        let gateway =
            CodexBudgetGateway::start(sample_authority(2, 1_000), &upstream, "upstream-secret")
                .unwrap();

        // Unbound — use std TCP for assertions (no reqwest blocking feature).
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let body = br#"{"model":"gpt-test-model","input":"hi"}"#;
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer wrong\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("session_unbound"), "{resp}");
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 0);

        // Model substitution
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let body = br#"{"model":"other-model","input":"hi"}"#;
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("model_substitution"), "{resp}");
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 0);

        let _ = gateway.shutdown();
    }

    #[test]
    fn gateway_forwards_once_injects_output_cap_and_blocks_second_request() {
        let hits = Arc::new(AtomicUsize::new(0));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen_body = Arc::new(Mutex::new(String::new()));
        let seen_body_thread = Arc::clone(&seen_body);
        let hits_thread = Arc::clone(&hits);
        let _ = listener.set_nonblocking(true);
        let join = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = Vec::new();
                        let mut chunk = [0u8; 8192];
                        loop {
                            let n = stream.read(&mut chunk).unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            buf.extend_from_slice(&chunk[..n]);
                            if let Some(end) = find_header_end(&buf) {
                                let headers = std::str::from_utf8(&buf[..end]).unwrap_or("");
                                let mut content_length = 0usize;
                                for line in headers.lines() {
                                    if let Some(value) =
                                        line.to_ascii_lowercase().strip_prefix("content-length:")
                                    {
                                        content_length = value.trim().parse().unwrap_or(0);
                                    }
                                }
                                while buf.len() < end + 4 + content_length {
                                    let n = stream.read(&mut chunk).unwrap_or(0);
                                    if n == 0 {
                                        break;
                                    }
                                    buf.extend_from_slice(&chunk[..n]);
                                }
                                if let Ok(text) =
                                    std::str::from_utf8(&buf[end + 4..end + 4 + content_length])
                                {
                                    *seen_body_thread.lock().unwrap() = text.to_string();
                                }
                                break;
                            }
                        }
                        hits_thread.fetch_add(1, AtomicOrdering::SeqCst);
                        let body = br#"{"id":"resp_1","usage":{"input_tokens":20,"output_tokens":10},"output":[]}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.write_all(body);
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            || error.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });

        // Cumulative must cover conservative UTF-8 input reservation + max output.
        let mut authority = sample_authority(1, 50_000);
        authority.max_output_tokens_per_request = 64;
        let gateway =
            CodexBudgetGateway::start(authority, &format!("http://{addr}"), "upstream-secret")
                .unwrap();

        // Omit max_output_tokens so the gateway injects the admitted ceiling.
        let body = br#"{"model":"gpt-test-model","input":"implement a tiny change"}"#;
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("\"input_tokens\":20"), "{resp}");
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 1);
        let forwarded = seen_body.lock().unwrap().clone();
        assert!(
            forwarded.contains("\"max_output_tokens\":64"),
            "expected injected cap, got {forwarded}"
        );
        assert!(
            forwarded.contains("\"stream\":false"),
            "expected stream forced off, got {forwarded}"
        );

        // Second request must fail pre-call without another upstream hit.
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let body = br#"{"model":"gpt-test-model","input":"again"}"#;
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(
            resp.contains("request_budget_exhausted")
                || resp.contains("cumulative_budget_insufficient"),
            "{resp}"
        );
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 1);

        let usage = gateway.shutdown();
        assert_eq!(usage.provider_requests, 1);
        assert_eq!(usage.cumulative_input_tokens, 20);
        assert_eq!(usage.cumulative_output_tokens, 10);
        let _ = join.join();
    }

    #[test]
    fn ephemeral_codex_home_writes_gateway_provider() {
        let dir = std::env::temp_dir().join(format!("codex-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        write_ephemeral_codex_home(&dir, "gpt-test", "http://127.0.0.1:9/v1").unwrap();
        let config = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(config.contains("acp_budget_gateway"));
        assert!(config.contains("http://127.0.0.1:9/v1"));
        assert!(!config.contains("sk-"));
        let auth = std::fs::read_to_string(dir.join("auth.json")).unwrap();
        assert_eq!(auth.trim(), "{}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_tokens_are_unique_across_gateways_and_not_task_metadata_only() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), true);
        let g1 =
            CodexBudgetGateway::start(sample_authority(2, 1_000), &upstream, "upstream-secret")
                .unwrap();
        let g2 =
            CodexBudgetGateway::start(sample_authority(2, 1_000), &upstream, "upstream-secret")
                .unwrap();
        assert_ne!(g1.session_token(), g2.session_token());
        assert!(g1.session_token().starts_with(CODEX_SESSION_TOKEN_PREFIX));

        // Token from g1 must not authorize g2.
        let mut stream = TcpStream::connect(g2.local_addr()).unwrap();
        let body = br#"{"model":"gpt-test-model","input":"hi"}"#;
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            g1.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("session_unbound"), "{resp}");
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 0);
        let _ = g1.shutdown();
        let _ = g2.shutdown();
    }

    #[test]
    fn gateway_rejects_output_limit_increase_and_null_removal() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), true);
        let mut authority = sample_authority(2, 50_000);
        authority.max_output_tokens_per_request = 64;
        let gateway = CodexBudgetGateway::start(authority, &upstream, "upstream-secret").unwrap();

        let body = br#"{"model":"gpt-test-model","input":"hi","max_output_tokens":9999}"#;
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("output_limit_increase"), "{resp}");
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 0);

        let body = br#"{"model":"gpt-test-model","input":"hi","max_output_tokens":null}"#;
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("output_limit_required"), "{resp}");
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 0);
        let _ = gateway.shutdown();
    }

    #[test]
    fn gateway_restart_restores_committed_usage_from_journal() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), true);
        let journal = std::env::temp_dir().join(format!(
            "codex-gw-journal-{}-{}.json",
            std::process::id(),
            now_unix_ms()
        ));
        let _ = std::fs::remove_file(&journal);
        let mut authority = sample_authority(3, 50_000);
        authority.execution_id = "exec-journal-1".into();
        authority.max_output_tokens_per_request = 64;
        let gateway = CodexBudgetGateway::start_with_journal(
            authority.clone(),
            &upstream,
            "upstream-secret",
            Some(journal.clone()),
        )
        .unwrap();

        let body = br#"{"model":"gpt-test-model","input":"hi"}"#;
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("\"input_tokens\":10"), "{resp}");
        let usage = gateway.shutdown();
        assert_eq!(usage.provider_requests, 1);
        assert_eq!(usage.cumulative_input_tokens, 10);

        // Restart with the same journal: counters restored, next request counted.
        let gateway2 = CodexBudgetGateway::start_with_journal(
            authority,
            &upstream,
            "upstream-secret",
            Some(journal.clone()),
        )
        .unwrap();
        let restored = gateway2.usage();
        assert_eq!(restored.provider_requests, 1);
        assert_eq!(restored.cumulative_input_tokens, 10);
        assert_eq!(restored.cumulative_output_tokens, 5);
        let _ = gateway2.shutdown();
        let raw = std::fs::read_to_string(&journal).unwrap();
        assert!(!raw.contains("upstream-secret"));
        assert!(!raw.contains("sk-real"));
        assert!(!raw.contains("OPENAI_API_KEY"));
        let _ = std::fs::remove_file(&journal);
    }

    #[test]
    fn cumulative_exhaustion_blocks_next_request_without_upstream_hit() {
        let hits = Arc::new(AtomicUsize::new(0));
        // Upstream reports nearly the full cumulative budget so residual next
        // worst-case (UTF-8 reservation + max_out) cannot fit.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hits_thread = Arc::clone(&hits);
        let _ = listener.set_nonblocking(true);
        let join = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = [0u8; 65536];
                        let _ = stream.read(&mut buf);
                        hits_thread.fetch_add(1, AtomicOrdering::SeqCst);
                        let body = br#"{"id":"resp_1","usage":{"input_tokens":80,"output_tokens":10},"output":[]}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.write_all(body);
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            || error.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });

        let mut authority = sample_authority(4, 100);
        authority.max_output_tokens_per_request = 20;
        authority.max_input_tokens_per_request = 50_000;
        let gateway =
            CodexBudgetGateway::start(authority, &format!("http://{addr}"), "upstream-secret")
                .unwrap();

        let body = br#"{"model":"gpt-test-model","input":"x"}"#;
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(
            resp.contains("\"input_tokens\":80"),
            "first request should forward; got {resp}"
        );
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 1);
        // Remaining cumulative = 100 - 90 = 10; next worst-case needs reserved+20 >> 10.
        let mut stream = TcpStream::connect(gateway.local_addr()).unwrap();
        let body = br#"{"model":"gpt-test-model","input":"y"}"#;
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(resp.contains("cumulative_budget_insufficient"), "{resp}");
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 1);
        let _ = gateway.shutdown();
        let _ = join.join();
    }
}
