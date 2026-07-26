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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::config::{validate_binary_file_identity, ADMITTED_CODEX_VERSION};

pub const CODEX_BUDGET_AUTHORITY_SCHEMA: &str = "codex_budget_authority.v2";
pub const CODEX_BUDGET_GATEWAY_SCHEMA: &str = "codex_budget_gateway.v2";
pub const CODEX_SESSION_TOKEN_PREFIX: &str = "acp-codex-budget-";
pub const CODEX_PROVIDER_KIND_OPENAI_COMPATIBLE: &str = "openai_compatible";

/// Admitted product-managed Codex CLI identity (exact version pin).
pub const ADMITTED_CODEX_CLI_VERSION: &str = ADMITTED_CODEX_VERSION;

/// Default per-request provider output ceiling when the product budget does not
/// pin a lower value. Kept deliberately below common model maxima so product
/// tasks fail closed rather than open-ended generation.
pub const DEFAULT_CODEX_MAX_OUTPUT_TOKENS_PER_REQUEST: u64 = 8_192;

/// Default max provider HTTP requests (including internal retries Codex may issue).
pub const DEFAULT_CODEX_MAX_PROVIDER_REQUESTS: u64 = 8;

/// Default max *declared* retry axis. Codex 0.145.0 does not label internal
/// retries on the HTTP wire, so this axis is recorded and capped only as an
/// additional total-POST ceiling fragment — not as independently identified retries.
pub const DEFAULT_CODEX_MAX_RETRIES: u64 = 0;

/// Exact provider binding for one product-managed attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexProviderIdentity {
    pub provider_kind: String,
    /// Normalized base URL (scheme://host[:port][/v1]).
    pub base_url: String,
    pub host: String,
    /// Absolute paths admitted on the loopback gateway (including /v1 prefix forms).
    pub admitted_endpoint_paths: Vec<String>,
}

impl CodexProviderIdentity {
    pub fn openai_compatible(base_url: &str) -> Result<Self, String> {
        let base_url = normalize_base_url(base_url)?;
        let host = host_from_base_url(&base_url)?;
        Ok(Self {
            provider_kind: CODEX_PROVIDER_KIND_OPENAI_COMPATIBLE.to_string(),
            base_url,
            host,
            admitted_endpoint_paths: vec![
                "/v1/responses".into(),
                "/responses".into(),
                "/v1/chat/completions".into(),
                "/chat/completions".into(),
                "/v1/models".into(),
                "/models".into(),
            ],
        })
    }

    pub fn admits_path(&self, path: &str) -> bool {
        self.admitted_endpoint_paths.iter().any(|p| p == path)
    }
}

fn host_from_base_url(base_url: &str) -> Result<String, String> {
    let without = base_url
        .strip_prefix("https://")
        .or_else(|| base_url.strip_prefix("http://"))
        .ok_or_else(|| "upstream base URL must be http or https".to_string())?;
    let host = without
        .split('/')
        .next()
        .unwrap_or("")
        .split('@')
        .next_back()
        .unwrap_or("")
        .trim();
    if host.is_empty() {
        return Err("upstream base URL is missing host".to_string());
    }
    Ok(host.to_string())
}

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
    pub(crate) schema_version: String,
    pub(crate) task_id: String,
    pub(crate) workflow_node_id: String,
    /// Collision-resistant attempt identity (UUID-based). Only this attempt may
    /// resume its parent-owned journal.
    pub(crate) execution_id: String,
    pub(crate) executable: CodexExecutableIdentity,
    pub(crate) provider: CodexProviderIdentity,
    pub(crate) model: String,
    /// Hard total provider HTTP POSTs (including any Codex-internal retries).
    pub(crate) max_provider_requests: u64,
    /// Declared separate retry axis. Codex does not label retries on the wire;
    /// enforcement is therefore partial (see mediation admission report).
    pub(crate) max_retries: u64,
    pub(crate) max_input_tokens_per_request: u64,
    pub(crate) max_output_tokens_per_request: u64,
    pub(crate) max_cumulative_tokens: u64,
    pub(crate) max_cost_usd: Option<f64>,
    pub(crate) timeout_ms: u64,
    pub(crate) worktree: PathBuf,
    pub(crate) expires_unix_ms: u64,
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
            ("provider_kind", self.provider.provider_kind.as_str()),
            ("provider_host", self.provider.host.as_str()),
            ("provider_base_url", self.provider.base_url.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("codex budget authority {name} is required"));
            }
        }
        if !self.execution_id.starts_with("codex-attempt-") {
            return Err(
                "execution_id must be a codex-attempt-* collision-resistant attempt identity"
                    .to_string(),
            );
        }
        if self.provider.admitted_endpoint_paths.is_empty() {
            return Err("provider admitted_endpoint_paths must be non-empty".to_string());
        }
        if self.max_provider_requests == 0 {
            return Err("max_provider_requests must be > 0".to_string());
        }
        // Separate axes: retries never expand the hard total POST ceiling.
        if self.max_retries >= self.max_provider_requests && self.max_provider_requests > 0 {
            // Allowed: max_retries can be 0; if set high it still cannot expand total.
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
            "provider_kind": self.provider.provider_kind,
            "provider_host": self.provider.host,
            "provider_base_url": self.provider.base_url,
            "admitted_endpoint_paths": self.provider.admitted_endpoint_paths,
            "budget_authority_schema": CODEX_BUDGET_AUTHORITY_SCHEMA,
            "execution_id": self.execution_id,
            "attempt_id": self.execution_id,
            "max_provider_requests": self.max_provider_requests,
            "max_retries": self.max_retries,
            "retry_axis_note": "codex_internal_retries_not_wire_labeled",
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

/// Collision-resistant attempt identity for one product-managed Codex execution.
pub fn new_codex_attempt_id() -> String {
    format!("codex-attempt-{}", Uuid::new_v4())
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
    pub journal_halted: bool,
    pub observed_retry_posts: u64,
}

#[derive(Debug)]
struct GatewayState {
    authority: CodexBudgetAuthority,
    session_token: String,
    /// Real upstream credential; never exposed to the child process.
    upstream_api_key: String,
    provider_requests: AtomicU64,
    cumulative_input_tokens: AtomicU64,
    cumulative_output_tokens: AtomicU64,
    stop: AtomicBool,
    last_reject: Mutex<Option<(BudgetRejectClass, String)>>,
    /// Serializes pre-dispatch residual checks so concurrent requests cannot overspend.
    budget_lock: Mutex<()>,
    /// Parent-owned durable journal; required for product mediation (not optional).
    journal: Mutex<crate::cli::codex_usage_journal::CodexUsageJournal>,
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

    fn journal_halted(&self) -> bool {
        self.journal.lock().map(|j| j.is_halted()).unwrap_or(true)
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
        let observed_retry_posts = self
            .journal
            .lock()
            .map(|j| j.entry().observed_retry_posts)
            .unwrap_or(0);
        BudgetGatewayUsage {
            provider_requests: self.provider_requests.load(Ordering::SeqCst),
            cumulative_input_tokens: self.cumulative_input_tokens.load(Ordering::SeqCst),
            cumulative_output_tokens: self.cumulative_output_tokens.load(Ordering::SeqCst),
            cumulative_tokens: self.cumulative_tokens(),
            last_reject,
            last_reject_class,
            journal_halted: self.journal_halted(),
            observed_retry_posts,
        }
    }
}

/// Loopback HTTP gateway that mediates Codex → provider traffic for one task.
pub struct CodexBudgetGateway {
    state: Arc<GatewayState>,
    local_addr: SocketAddr,
    join: Option<JoinHandle<()>>,
}

/// Non-secret ownership facts read from the gateway's actual parent-owned journal.
/// These are intentionally derived by the gateway, not reconstructed from an
/// environment declaration or a path-name convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GatewayJournalOwnershipFacts {
    pub parent_owned_for_attempt: bool,
    pub durable_and_current: bool,
}

/// Opaque capability required to start a budget gateway.  Production permits
/// originate only from a consumed store attempt lease; the provider-free
/// variant is intentionally explicit so dry-runs/tests cannot be mistaken for
/// live ProductTask execution authority.
#[derive(Debug, Clone)]
pub(crate) struct CodexGatewayStartPermit {
    execution_id: String,
    kind: CodexGatewayStartPermitKind,
}

#[derive(Debug, Clone)]
enum CodexGatewayStartPermitKind {
    ManagedStoreLease { lease_token_sha256: String },
    ProviderFreeFixture,
}

impl CodexGatewayStartPermit {
    pub(crate) fn managed_store_lease(execution_id: &str, lease_token: &str) -> Self {
        Self {
            execution_id: execution_id.to_string(),
            kind: CodexGatewayStartPermitKind::ManagedStoreLease {
                lease_token_sha256: hex::encode(Sha256::digest(lease_token.as_bytes())),
            },
        }
    }

    /// This is an intentionally named provider-free seam for the deterministic
    /// managed-acceptance dry-run and unit tests.  It is not exported as a
    /// production authorization constructor.
    pub(crate) fn provider_free_fixture(execution_id: &str) -> Self {
        Self {
            execution_id: execution_id.to_string(),
            kind: CodexGatewayStartPermitKind::ProviderFreeFixture,
        }
    }

    fn validate_for(&self, authority: &CodexBudgetAuthority) -> Result<(), String> {
        if self.execution_id != authority.execution_id {
            return Err("codex gateway start permit execution identity mismatch".to_string());
        }
        match &self.kind {
            CodexGatewayStartPermitKind::ManagedStoreLease { lease_token_sha256 }
                if lease_token_sha256.len() == 64
                    && lease_token_sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit()) =>
            {
                Ok(())
            }
            CodexGatewayStartPermitKind::ManagedStoreLease { .. } => {
                Err("codex gateway start permit lease binding is invalid".to_string())
            }
            CodexGatewayStartPermitKind::ProviderFreeFixture => Ok(()),
        }
    }
}

impl CodexBudgetGateway {
    /// Start the gateway with a **required** parent-owned journal path.
    ///
    /// `upstream_base_url` must exactly match `authority.provider.base_url` after
    /// normalization — environment substitution of provider identity is rejected.
    pub(crate) fn start(
        permit: CodexGatewayStartPermit,
        authority: CodexBudgetAuthority,
        upstream_base_url: &str,
        upstream_api_key: &str,
        parent_journal_path: PathBuf,
    ) -> Result<Self, String> {
        permit.validate_for(&authority)?;
        Self::start_with_journal(
            authority,
            upstream_base_url,
            upstream_api_key,
            parent_journal_path,
        )
    }

    fn start_with_journal(
        authority: CodexBudgetAuthority,
        upstream_base_url: &str,
        upstream_api_key: &str,
        parent_journal_path: PathBuf,
    ) -> Result<Self, String> {
        let authority = authority.validate_new()?;
        let upstream_base_url = normalize_base_url(upstream_base_url)?;
        if upstream_base_url != authority.provider.base_url {
            return Err(format!(
                "provider base URL substitution rejected: requested={upstream_base_url} admitted={}",
                authority.provider.base_url
            ));
        }
        if upstream_api_key.trim().is_empty() {
            return Err("upstream API key is required for Codex budget mediation".to_string());
        }
        // One-use unforgeable session token: random UUID material + attempt binding.
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

        let journal = if parent_journal_path.is_file() {
            crate::cli::codex_usage_journal::CodexUsageJournal::resume_exact_attempt(
                parent_journal_path,
                &authority.execution_id,
                &authority.task_id,
                &authority.provider.provider_kind,
                &authority.provider.host,
                &authority.model,
                &authority.executable.binary_sha256,
            )?
        } else {
            crate::cli::codex_usage_journal::CodexUsageJournal::create_new(
                parent_journal_path,
                &authority.execution_id,
                &authority.task_id,
                &authority.provider.provider_kind,
                &authority.provider.host,
                &authority.model,
                &authority.executable.binary_sha256,
            )?
        };
        let restored_requests = journal.entry().provider_requests;
        let restored_input = journal.entry().cumulative_input_tokens;
        let restored_output = journal.entry().cumulative_output_tokens;

        let state = Arc::new(GatewayState {
            authority,
            session_token,
            upstream_api_key: upstream_api_key.to_string(),
            provider_requests: AtomicU64::new(restored_requests),
            cumulative_input_tokens: AtomicU64::new(restored_input),
            cumulative_output_tokens: AtomicU64::new(restored_output),
            stop: AtomicBool::new(false),
            last_reject: Mutex::new(None),
            budget_lock: Mutex::new(()),
            journal: Mutex::new(journal),
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

    /// The gateway can only exist after `start` validated a non-empty upstream
    /// credential. Expose presence only; never expose its value.
    pub(crate) fn parent_credential_owner_present(&self) -> bool {
        !self.state.upstream_api_key.trim().is_empty()
    }

    /// Re-read the exact journal owned by this gateway and verify it remains
    /// bound to this attempt. Any lock/read/integrity error is propagated so a
    /// caller cannot treat an unavailable owner as evidence.
    pub(crate) fn journal_ownership_facts(&self) -> Result<GatewayJournalOwnershipFacts, String> {
        let journal = self
            .state
            .journal
            .lock()
            .map_err(|_| "codex budget gateway journal lock poisoned".to_string())?;
        let persisted = crate::cli::codex_usage_journal::load_journal(journal.path())?;
        let current = journal.entry();
        // The journal lives in the gateway's private parent-owned state, not
        // in a child environment or a path-name convention.  Bind that actual
        // owner to the authority before treating its durable record as proof.
        let parent_owned_for_attempt = current.attempt_id == self.state.authority.execution_id
            && current.task_id == self.state.authority.task_id
            && current.provider_kind == self.state.authority.provider.provider_kind
            && current.provider_host == self.state.authority.provider.host
            && current.model == self.state.authority.model
            && current.binary_sha256 == self.state.authority.executable.binary_sha256;
        let durable_and_current = parent_owned_for_attempt
            && persisted.attempt_id == current.attempt_id
            && persisted.task_id == current.task_id
            && persisted.provider_kind == current.provider_kind
            && persisted.provider_host == current.provider_host
            && persisted.model == current.model
            && persisted.binary_sha256 == current.binary_sha256
            && persisted.integrity_sha256 == current.integrity_sha256;
        Ok(GatewayJournalOwnershipFacts {
            parent_owned_for_attempt,
            durable_and_current,
        })
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
    if state.journal_halted() {
        state.record_reject(
            BudgetRejectClass::OutcomeUnknown,
            "usage journal halted; no further provider requests admitted".to_string(),
        );
        return json_error(
            503,
            "journal_halted",
            "usage journal halted; no further provider requests admitted",
        );
    }
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

    let path = request.path.as_str();
    if !state.authority.provider.admits_path(path) {
        state.record_reject(
            BudgetRejectClass::PreCall,
            format!("path {path} is not in admitted_endpoint_paths"),
        );
        return json_error(
            403,
            "path_not_admitted",
            "only authority-bound model endpoints may be mediated",
        );
    }
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

    // Hold the budget lock across residual check + durable reservation so concurrent
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

        if let Ok(journal) = state.journal.lock() {
            if let Err(error) = journal.admits_new_request() {
                state.record_reject(BudgetRejectClass::OutcomeUnknown, error.clone());
                return json_error(503, "journal_blocks_admit", &error);
            }
        }

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
        // Separate retry axis: without wire-labeled retries, every POST after the
        // first increments observed_retry_posts. Enforce max_retries as a cap on
        // those subsequent POSTs (does not expand max_provider_requests).
        if prior_requests >= 1 && state.authority.max_retries == 0 {
            state.record_reject(
                BudgetRejectClass::PreCall,
                "max_retries exhausted (zero retries admitted; subsequent POSTs blocked)"
                    .to_string(),
            );
            return json_error(
                429,
                "retry_budget_exhausted",
                "max_retries exhausted; subsequent provider POSTs are not admitted",
            );
        }
        if prior_requests >= 1 {
            let retries_used = prior_requests.saturating_sub(1);
            if retries_used >= state.authority.max_retries {
                state.record_reject(
                    BudgetRejectClass::PreCall,
                    "max_retries exhausted".to_string(),
                );
                return json_error(429, "retry_budget_exhausted", "max_retries exhausted");
            }
        }

        let used = state.cumulative_tokens();
        let remaining = state.authority.max_cumulative_tokens.saturating_sub(used);
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

        // Durable pre-forward reservation MUST succeed before upstream call.
        {
            let mut journal = match state.journal.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    state.record_reject(
                        BudgetRejectClass::OutcomeUnknown,
                        "journal lock poisoned".to_string(),
                    );
                    return json_error(500, "journal_lock_poisoned", "journal lock poisoned");
                }
            };
            if let Err(error) = journal.reserve_before_forward(reserved_input, max_out) {
                state.record_reject(
                    BudgetRejectClass::OutcomeUnknown,
                    format!("journal reserve failed: {error}"),
                );
                state.stop.store(true, Ordering::SeqCst);
                return json_error(
                    503,
                    "journal_reserve_failed",
                    "usage journal reserve failed; gateway halted",
                );
            }
        }
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
                // Persist committed usage BEFORE returning the body to the child.
                {
                    let mut journal = match state.journal.lock() {
                        Ok(guard) => guard,
                        Err(_) => {
                            state.record_reject(
                                BudgetRejectClass::OutcomeUnknown,
                                "journal lock poisoned on commit".to_string(),
                            );
                            state.stop.store(true, Ordering::SeqCst);
                            return json_error(
                                503,
                                "journal_commit_failed",
                                "usage journal commit lock failed; gateway halted",
                            );
                        }
                    };
                    if let Err(error) = journal.commit_after_forward(
                        input_tokens,
                        output_tokens,
                        extract_response_id(&response_body),
                    ) {
                        state.record_reject(
                            BudgetRejectClass::OutcomeUnknown,
                            format!("journal commit failed: {error}"),
                        );
                        state.stop.store(true, Ordering::SeqCst);
                        return json_error(
                            503,
                            "journal_commit_failed",
                            "usage journal commit failed; gateway halted",
                        );
                    }
                }
                HttpResponseParts {
                    status,
                    reason: reason_phrase(status),
                    headers: vec![("Content-Type".into(), "application/json".into())],
                    body: response_body,
                }
            }
            Err(error) => {
                // Charge reserved worst-case into both journal and gateway counters so
                // ProductTask residual never under-accounts a possibly billed forward.
                if let Ok(mut journal) = state.journal.lock() {
                    let reserved_in = journal.entry().reserved_input_tokens;
                    let reserved_out = journal.entry().reserved_output_tokens;
                    if journal.mark_outcome_unknown(&error).is_ok() {
                        state
                            .cumulative_input_tokens
                            .fetch_add(reserved_in, Ordering::SeqCst);
                        state
                            .cumulative_output_tokens
                            .fetch_add(reserved_out, Ordering::SeqCst);
                    }
                }
                state.record_reject(BudgetRejectClass::OutcomeUnknown, error.clone());
                json_error(502, "usage_unavailable", &error)
            }
        },
        Err(error) => {
            if let Ok(mut journal) = state.journal.lock() {
                let reserved_in = journal.entry().reserved_input_tokens;
                let reserved_out = journal.entry().reserved_output_tokens;
                if journal.mark_outcome_unknown(&error).is_ok() {
                    state
                        .cumulative_input_tokens
                        .fetch_add(reserved_in, Ordering::SeqCst);
                    state
                        .cumulative_output_tokens
                        .fetch_add(reserved_out, Ordering::SeqCst);
                }
            }
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
    let url = format!("{}{}", state.authority.provider.base_url, path);
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
    use std::time::Instant;

    fn sample_authority_for_upstream(
        max_requests: u64,
        max_cumulative: u64,
        max_retries: u64,
        upstream: &str,
    ) -> CodexBudgetAuthority {
        let binary = std::env::temp_dir().join(format!(
            "codex-budget-test-bin-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
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
        let provider = CodexProviderIdentity::openai_compatible(upstream).unwrap();
        CodexBudgetAuthority {
            schema_version: CODEX_BUDGET_AUTHORITY_SCHEMA.to_string(),
            task_id: "ptask-test".into(),
            workflow_node_id: "node-1".into(),
            execution_id: new_codex_attempt_id(),
            executable,
            provider,
            model: "gpt-test-model".into(),
            max_provider_requests: max_requests,
            max_retries,
            max_input_tokens_per_request: 50_000,
            max_output_tokens_per_request: 128,
            max_cumulative_tokens: max_cumulative,
            max_cost_usd: None,
            timeout_ms: 30_000,
            worktree: std::env::temp_dir(),
            expires_unix_ms: now_unix_ms() + 60_000,
        }
    }

    fn start_gateway(
        authority: CodexBudgetAuthority,
        upstream: &str,
        key: &str,
    ) -> CodexBudgetGateway {
        let journal =
            crate::cli::codex_usage_journal::parent_owned_journal_path(&authority.execution_id);
        let _ = std::fs::remove_file(&journal);
        let permit = CodexGatewayStartPermit::provider_free_fixture(&authority.execution_id);
        CodexBudgetGateway::start(permit, authority, upstream, key, journal).unwrap()
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
        let authority = sample_authority_for_upstream(2, 1_000, 1, &upstream);
        let gateway = start_gateway(authority, &upstream, "upstream-secret");

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
    fn gateway_forwards_once_injects_output_cap_and_blocks_retry_when_max_retries_zero() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), true);
        let mut authority = sample_authority_for_upstream(4, 50_000, 0, &upstream);
        authority.max_output_tokens_per_request = 64;
        let gateway = start_gateway(authority, &upstream, "upstream-secret");

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
        assert!(resp.contains("\"input_tokens\":10"), "{resp}");
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 1);

        // max_retries=0 blocks the second POST on the retry axis.
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
            resp.contains("retry_budget_exhausted") || resp.contains("request_budget_exhausted"),
            "{resp}"
        );
        assert_eq!(hits.load(AtomicOrdering::SeqCst), 1);
        let usage = gateway.shutdown();
        assert_eq!(usage.provider_requests, 1);
        assert_eq!(usage.cumulative_input_tokens, 10);
        assert_eq!(usage.cumulative_output_tokens, 5);
    }

    #[test]
    fn provider_base_url_substitution_is_rejected() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), true);
        let authority = sample_authority_for_upstream(2, 1_000, 1, "https://api.openai.com/v1");
        let journal =
            crate::cli::codex_usage_journal::parent_owned_journal_path(&authority.execution_id);
        let _ = std::fs::remove_file(&journal);
        let permit = CodexGatewayStartPermit::provider_free_fixture(&authority.execution_id);
        match CodexBudgetGateway::start(permit, authority, &upstream, "upstream-secret", journal) {
            Ok(_) => panic!("expected provider substitution rejection"),
            Err(err) => assert!(err.contains("provider base URL substitution"), "{err}"),
        }
    }

    #[test]
    fn ephemeral_codex_home_writes_gateway_provider() {
        let dir = std::env::temp_dir().join(format!("codex-home-{}", Uuid::new_v4()));
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
    fn session_tokens_are_unique_across_gateways() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), true);
        let g1 = start_gateway(
            sample_authority_for_upstream(2, 1_000, 1, &upstream),
            &upstream,
            "upstream-secret",
        );
        let g2 = start_gateway(
            sample_authority_for_upstream(2, 1_000, 1, &upstream),
            &upstream,
            "upstream-secret",
        );
        assert_ne!(g1.session_token(), g2.session_token());
        assert!(g1.session_token().starts_with(CODEX_SESSION_TOKEN_PREFIX));

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
        let mut authority = sample_authority_for_upstream(2, 50_000, 1, &upstream);
        authority.max_output_tokens_per_request = 64;
        let gateway = start_gateway(authority, &upstream, "upstream-secret");

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
    fn gateway_restart_restores_committed_usage_from_parent_journal() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), true);
        let mut authority = sample_authority_for_upstream(3, 50_000, 2, &upstream);
        authority.max_output_tokens_per_request = 64;
        let journal =
            crate::cli::codex_usage_journal::parent_owned_journal_path(&authority.execution_id);
        let _ = std::fs::remove_file(&journal);
        let gateway = CodexBudgetGateway::start(
            CodexGatewayStartPermit::provider_free_fixture(&authority.execution_id),
            authority.clone(),
            &upstream,
            "upstream-secret",
            journal.clone(),
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

        let permit = CodexGatewayStartPermit::provider_free_fixture(&authority.execution_id);
        let gateway2 = CodexBudgetGateway::start(
            permit,
            authority,
            &upstream,
            "upstream-secret",
            journal.clone(),
        )
        .unwrap();
        let restored = gateway2.usage();
        assert_eq!(restored.provider_requests, 1);
        assert_eq!(restored.cumulative_input_tokens, 10);
        assert_eq!(restored.cumulative_output_tokens, 5);
        let _ = gateway2.shutdown();
        let raw = std::fs::read_to_string(&journal).unwrap();
        assert!(!raw.contains("upstream-secret"));
        assert!(!raw.contains("OPENAI_API_KEY"));
        assert!(raw.contains("attempt_id") || raw.contains("codex_usage_journal.v2"));
        let _ = std::fs::remove_file(&journal);
    }

    #[test]
    fn outcome_unknown_restart_never_returns_budget() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (upstream, _join) = spawn_fake_upstream(Arc::clone(&hits), false); // missing usage
        let authority = sample_authority_for_upstream(3, 50_000, 2, &upstream);
        let journal =
            crate::cli::codex_usage_journal::parent_owned_journal_path(&authority.execution_id);
        let _ = std::fs::remove_file(&journal);
        let gateway = CodexBudgetGateway::start(
            CodexGatewayStartPermit::provider_free_fixture(&authority.execution_id),
            authority.clone(),
            &upstream,
            "upstream-secret",
            journal.clone(),
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
        assert!(resp.contains("usage_unavailable"), "{resp}");
        let _ = gateway.shutdown();

        let permit = CodexGatewayStartPermit::provider_free_fixture(&authority.execution_id);
        let gateway2 = CodexBudgetGateway::start(
            permit,
            authority,
            &upstream,
            "upstream-secret",
            journal.clone(),
        )
        .unwrap();
        // Restart must block new admits after outcome-unknown charge.
        let mut stream = TcpStream::connect(gateway2.local_addr()).unwrap();
        let body = br#"{"model":"gpt-test-model","input":"again"}"#;
        let req = format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            body.len(),
            gateway2.session_token()
        );
        stream.write_all(req.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut resp = String::new();
        stream.read_to_string(&mut resp).unwrap();
        assert!(
            resp.contains("journal_blocks_admit")
                || resp.contains("outcome_unknown")
                || resp.contains("journal"),
            "{resp}"
        );
        let usage = gateway2.usage();
        assert!(usage.provider_requests >= 1);
        assert!(usage.cumulative_input_tokens > 0 || usage.cumulative_output_tokens > 0);
        let _ = gateway2.shutdown();
        let _ = std::fs::remove_file(&journal);
    }

    #[test]
    fn cumulative_exhaustion_blocks_next_request_without_upstream_hit() {
        let hits = Arc::new(AtomicUsize::new(0));
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

        let upstream = format!("http://{addr}");
        let mut authority = sample_authority_for_upstream(4, 100, 3, &upstream);
        authority.max_output_tokens_per_request = 20;
        authority.max_input_tokens_per_request = 50_000;
        let gateway = start_gateway(authority, &upstream, "upstream-secret");

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

    #[test]
    fn attempt_ids_are_unique() {
        let a = new_codex_attempt_id();
        let b = new_codex_attempt_id();
        assert_ne!(a, b);
        assert!(a.starts_with("codex-attempt-"));
    }
}
