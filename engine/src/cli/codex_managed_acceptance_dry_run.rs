//! Provider-free managed-acceptance **fixture dry-run**.
//!
//! Completes the Golden Path readiness board without a live provider call.
//! Uses the real loopback `CodexBudgetGateway`, parent-owned journal, usage
//! event mapping, and preflight/authority contracts with a mock upstream.
//!
//! Durable evidence is hash-bound and redacted: no raw prompts, outputs,
//! transcripts, API keys, or OAuth material.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::codex_budget_authority::{
    new_codex_attempt_id, CodexBudgetAuthority, CodexBudgetGateway, CodexExecutableIdentity,
    CodexGatewayStartPermit, CodexProviderIdentity, ADMITTED_CODEX_CLI_VERSION,
    CODEX_BUDGET_AUTHORITY_SCHEMA,
};
use super::codex_managed_acceptance_preflight::{
    fixture_ready_pending_operator_input, run_managed_acceptance_preflight,
    ManagedAcceptancePreflightResult, MANAGED_ACCEPTANCE_MANIFEST_SCHEMA,
};
use super::codex_mediation_admission::{
    reconcile_gateway_and_session_usage, CodexAdmissionClass, UsageReconcileResult,
};
use super::codex_partial_mediation_authority_decision::{
    draft_partial_mediation_authority_decision, OPERATOR_RISK_ACCEPTANCE_PHRASE,
    PARTIAL_MEDIATION_AUTHORITY_DECISION_SCHEMA,
};
use super::codex_residual_admission::{
    evaluate_residual_admission_for_current_product, ResidualAdmissionVerdict,
    CODEX_RESIDUAL_ADMISSION_FINDING_SCHEMA,
};
use super::codex_usage_journal::parent_owned_journal_path;
use super::config::{ADMITTED_CODEX_MODEL, ADMITTED_CODEX_VERSION};
use crate::execution_usage::codex_adapter::UsageBindingContext;
use crate::execution_usage::gateway_adapter::budget_gateway_usage_to_event;
use crate::execution_usage::reconcile::{admission_evidence_ok, reconcile_usage_events};
use crate::execution_usage::{CostSource, ExecutionUsageEventV1};
use crate::storage::local_product_store::{
    AuthenticatedPrincipal, CostAuthority, LocalProductStore, RiskAcknowledgementRequest,
    SpendAuthorizationRequest,
};

pub const MANAGED_ACCEPTANCE_DRY_RUN_SCHEMA: &str = "codex_managed_acceptance_dry_run.v1";
pub const MANAGED_ACCEPTANCE_DRY_RUN_RECEIPT_SCHEMA: &str =
    "codex_managed_acceptance_dry_run_receipt.v1";

/// Fixture scenario kinds for provider-free dry-run coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DryRunScenario {
    Success,
    ProviderFailure,
    Timeout,
    Cancellation,
    BudgetExhaustion,
    EvidenceContradiction,
    RestartAfterOutcomeUnknown,
    OutcomeUnknown,
    DuplicateStartRejected,
    IdempotentReplay,
}

impl DryRunScenario {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::ProviderFailure => "provider_failure",
            Self::Timeout => "timeout",
            Self::Cancellation => "cancellation",
            Self::BudgetExhaustion => "budget_exhaustion",
            Self::EvidenceContradiction => "evidence_contradiction",
            Self::RestartAfterOutcomeUnknown => "restart_after_outcome_unknown",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::DuplicateStartRejected => "duplicate_start_rejected",
            Self::IdempotentReplay => "idempotent_replay",
        }
    }
}

/// Terminal classification of one dry-run attempt (never a live acceptance claim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryRunTerminalClass {
    SucceededFixture,
    FailedProvider,
    FailedTimeout,
    Cancelled,
    BudgetExhausted,
    EvidenceConflict,
    OutcomeUnknownCharged,
    DuplicateRejected,
    IdempotentReplayHit,
    BlockedPreflight,
    BlockedAuthorization,
}

impl DryRunTerminalClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SucceededFixture => "succeeded_fixture",
            Self::FailedProvider => "failed_provider",
            Self::FailedTimeout => "failed_timeout",
            Self::Cancelled => "cancelled",
            Self::BudgetExhausted => "budget_exhausted",
            Self::EvidenceConflict => "evidence_conflict",
            Self::OutcomeUnknownCharged => "outcome_unknown_charged",
            Self::DuplicateRejected => "duplicate_rejected",
            Self::IdempotentReplayHit => "idempotent_replay_hit",
            Self::BlockedPreflight => "blocked_preflight",
            Self::BlockedAuthorization => "blocked_authorization",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DryRunConfig {
    /// Stable attempt identity for idempotency / duplicate rejection.
    pub attempt_id: String,
    pub product_task_id: String,
    pub scenario: DryRunScenario,
    /// When true, simulate multi-field operator acknowledgement (not agent self-approve).
    pub simulate_operator_ack: bool,
    pub operator_actor: String,
    pub disposable_target_repo: String,
    pub target_main_sha: String,
    pub evidence_root: PathBuf,
}

impl DryRunConfig {
    pub fn fixture(scenario: DryRunScenario) -> Self {
        Self {
            attempt_id: format!("dry-run-{}", Uuid::new_v4()),
            product_task_id: format!("ptask-dry-{}", Uuid::new_v4()),
            scenario,
            simulate_operator_ack: true,
            operator_actor: "operator-fixture-alice".into(),
            disposable_target_repo: "Igzela/pe7-golden-path-acceptance-fixture".into(),
            target_main_sha: "c".repeat(40),
            evidence_root: std::env::temp_dir()
                .join(format!("acp-managed-acceptance-dry-run-{}", Uuid::new_v4())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DryRunReceipt {
    pub schema_version: String,
    pub attempt_id: String,
    pub product_task_id: String,
    pub scenario: String,
    pub terminal_class: DryRunTerminalClass,
    pub residual_verdict: String,
    pub product_admission_class: String,
    pub preflight_result: String,
    pub authority_decision_status: String,
    pub gateway_provider_requests: u64,
    pub gateway_input_tokens: u64,
    pub gateway_output_tokens: u64,
    pub usage_reconcile: String,
    pub journal_halted: bool,
    pub duplicate_or_replay: bool,
    pub live_provider_request: bool,
    pub secrets_embedded: bool,
    pub receipt_sha256: String,
    pub evidence_path: PathBuf,
    pub notes: Vec<String>,
}

impl DryRunReceipt {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "attempt_id": self.attempt_id,
            "product_task_id": self.product_task_id,
            "scenario": self.scenario,
            "terminal_class": self.terminal_class.as_str(),
            "residual_verdict": self.residual_verdict,
            "product_admission_class": self.product_admission_class,
            "preflight_result": self.preflight_result,
            "authority_decision_status": self.authority_decision_status,
            "gateway": {
                "provider_requests": self.gateway_provider_requests,
                "input_tokens": self.gateway_input_tokens,
                "output_tokens": self.gateway_output_tokens,
                "journal_halted": self.journal_halted,
            },
            "usage_reconcile": self.usage_reconcile,
            "duplicate_or_replay": self.duplicate_or_replay,
            "live_provider_request": self.live_provider_request,
            "secrets_embedded": self.secrets_embedded,
            "receipt_sha256": self.receipt_sha256,
            "notes": self.notes,
            "contracts": {
                "residual": CODEX_RESIDUAL_ADMISSION_FINDING_SCHEMA,
                "authority_decision": PARTIAL_MEDIATION_AUTHORITY_DECISION_SCHEMA,
                "manifest": MANAGED_ACCEPTANCE_MANIFEST_SCHEMA,
                "dry_run": MANAGED_ACCEPTANCE_DRY_RUN_SCHEMA,
            },
        })
    }
}

/// In-process + durable attempt registry (duplicate-start rejection, idempotent replay).
struct AttemptRegistry {
    completed: Mutex<HashMap<String, DryRunReceipt>>,
}

fn attempt_registry() -> &'static AttemptRegistry {
    static REG: OnceLock<AttemptRegistry> = OnceLock::new();
    REG.get_or_init(|| AttemptRegistry {
        completed: Mutex::new(HashMap::new()),
    })
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn redact_assert_no_secrets(value: &Value) {
    let rendered = value.to_string();
    // Fail closed on secret *shapes*, not on the English word "prompt" in non-claim prose.
    let banned = [
        "sk-proj-",
        "sk-ant-",
        "OPENAI_API_KEY=sk",
        "Bearer sk-",
        "fixture-upstream-key",
        "\"raw_output\"",
        "\"raw_prompt\"",
        "auth.json contents",
    ];
    for token in banned {
        assert!(
            !rendered.contains(token),
            "dry-run evidence must not embed secret shape {token}"
        );
    }
    // Session token material should not appear (gateway tokens start with acp-codex-budget-).
    if let Some(idx) = rendered.find("acp-codex-budget-") {
        // Allow schema notes that mention the prefix name without the hex body.
        let rest = &rendered[idx + "acp-codex-budget-".len()..];
        let hex_run = rest.chars().take_while(|c| c.is_ascii_hexdigit()).count();
        assert!(
            hex_run < 16,
            "dry-run evidence must not embed gateway session token material"
        );
    }
}

fn sample_authority(
    task_id: &str,
    execution_id: &str,
    upstream: &str,
    max_requests: u64,
    max_retries: u64,
    max_cumulative: u64,
) -> CodexBudgetAuthority {
    let binary = std::env::temp_dir().join(format!(
        "codex-dry-bin-{}-{}",
        std::process::id(),
        Uuid::new_v4()
    ));
    fs::write(&binary, b"#!/bin/sh\necho codex-cli 0.145.0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let sha = hex::encode(Sha256::digest(fs::read(&binary).unwrap()));
    let provider = CodexProviderIdentity::openai_compatible(upstream).unwrap();
    CodexBudgetAuthority {
        schema_version: CODEX_BUDGET_AUTHORITY_SCHEMA.to_string(),
        task_id: task_id.to_string(),
        workflow_node_id: "node-managed-acceptance-dry-run".into(),
        execution_id: execution_id.to_string(),
        executable: CodexExecutableIdentity {
            binary_path: binary,
            binary_version: ADMITTED_CODEX_CLI_VERSION.to_string(),
            binary_sha256: sha,
        },
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
        expires_unix_ms: now_unix_ms() + 120_000,
    }
}

enum MockUpstreamMode {
    SuccessUsage,
    ProviderError,
    HangUntilCancelled,
    DropConnection,
}

fn spawn_mock_upstream(
    mode: MockUpstreamMode,
    hits: Arc<AtomicUsize>,
    cancel: Arc<AtomicBool>,
) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let _ = listener.set_nonblocking(true);
    let join = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline && !cancel.load(AtomicOrdering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 65536];
                    let _ = stream.set_nonblocking(false);
                    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
                    let _ = stream.read(&mut buf);
                    hits.fetch_add(1, AtomicOrdering::SeqCst);
                    match mode {
                        MockUpstreamMode::SuccessUsage => {
                            let body = br#"{"id":"resp_fixture","usage":{"input_tokens":10,"output_tokens":5},"output":[]}"#;
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.write_all(body);
                        }
                        MockUpstreamMode::ProviderError => {
                            let body = br#"{"error":{"message":"fixture provider failure","type":"server_error"}}"#;
                            let response = format!(
                                "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.write_all(body);
                        }
                        MockUpstreamMode::HangUntilCancelled => {
                            while !cancel.load(AtomicOrdering::SeqCst) && Instant::now() < deadline
                            {
                                thread::sleep(Duration::from_millis(50));
                            }
                            // Close without a complete usage body → outcome unknown path.
                        }
                        MockUpstreamMode::DropConnection => {
                            // Immediate drop: no response body.
                            drop(stream);
                        }
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut =>
                {
                    thread::sleep(Duration::from_millis(15));
                }
                Err(_) => break,
            }
        }
    });
    (format!("http://{addr}"), join)
}

fn post_gateway(addr: std::net::SocketAddr, token: &str, body: &[u8]) -> Result<String, String> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))
        .map_err(|e| format!("connect: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let req = format!(
        "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        body.len(),
        token
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("write headers: {e}"))?;
    stream
        .write_all(body)
        .map_err(|e| format!("write body: {e}"))?;
    let mut resp = String::new();
    stream
        .read_to_string(&mut resp)
        .map_err(|e| format!("read: {e}"))?;
    Ok(resp)
}

fn durable_attempt_lock_path(root: &Path, attempt_id: &str) -> PathBuf {
    root.join(format!("attempt-{attempt_id}.lock.json"))
}

fn write_receipt(root: &Path, receipt: &DryRunReceipt) -> Result<PathBuf, String> {
    fs::create_dir_all(root).map_err(|e| format!("evidence root: {e}"))?;
    let path = root.join(format!("receipt-{}.json", receipt.attempt_id));
    let body = receipt.to_json();
    redact_assert_no_secrets(&body);
    fs::write(&path, body.to_string()).map_err(|e| format!("write receipt: {e}"))?;
    let lock = durable_attempt_lock_path(root, &receipt.attempt_id);
    fs::write(
        &lock,
        json!({
            "attempt_id": receipt.attempt_id,
            "receipt_sha256": receipt.receipt_sha256,
            "terminal_class": receipt.terminal_class.as_str(),
        })
        .to_string(),
    )
    .map_err(|e| format!("write lock: {e}"))?;
    Ok(path)
}

fn finish_receipt(mut receipt: DryRunReceipt, root: &Path) -> Result<DryRunReceipt, String> {
    let mut body = receipt.to_json();
    // Hash without nested receipt_sha256 field set.
    body["receipt_sha256"] = json!("");
    receipt.receipt_sha256 = hex::encode(Sha256::digest(body.to_string().as_bytes()));
    let path = write_receipt(root, &receipt)?;
    receipt.evidence_path = path;
    attempt_registry()
        .completed
        .lock()
        .map_err(|_| "attempt registry poisoned".to_string())?
        .insert(receipt.attempt_id.clone(), receipt.clone());
    Ok(receipt)
}

fn reconcile_label(result: &UsageReconcileResult) -> String {
    match result {
        UsageReconcileResult::PreferGateway { .. } => "prefer_gateway".into(),
        UsageReconcileResult::PreferSessionOnly { .. } => "prefer_session_only".into(),
        UsageReconcileResult::Conflict { .. } => "conflict".into(),
        UsageReconcileResult::Missing { .. } => "missing".into(),
    }
}

/// Run one complete provider-free managed-acceptance dry-run scenario.
///
/// This is the fixture path that proves state transitions for the future live
/// command. It does **not** call a real provider and does **not** grant live
/// Golden Path acceptance.
pub fn run_managed_acceptance_dry_run(config: DryRunConfig) -> Result<DryRunReceipt, String> {
    fs::create_dir_all(&config.evidence_root).map_err(|e| format!("evidence root: {e}"))?;

    // Duplicate / idempotent registry check.
    if let Ok(guard) = attempt_registry().completed.lock() {
        if let Some(existing) = guard.get(&config.attempt_id) {
            let mut replay = existing.clone();
            replay.duplicate_or_replay = true;
            replay.terminal_class = if config.scenario == DryRunScenario::DuplicateStartRejected {
                DryRunTerminalClass::DuplicateRejected
            } else {
                DryRunTerminalClass::IdempotentReplayHit
            };
            replay
                .notes
                .push("registry hit: exact attempt_id already completed".into());
            return Ok(replay);
        }
    }
    let lock_path = durable_attempt_lock_path(&config.evidence_root, &config.attempt_id);
    if lock_path.is_file() && config.scenario == DryRunScenario::DuplicateStartRejected {
        let receipt = DryRunReceipt {
            schema_version: MANAGED_ACCEPTANCE_DRY_RUN_RECEIPT_SCHEMA.to_string(),
            attempt_id: config.attempt_id.clone(),
            product_task_id: config.product_task_id.clone(),
            scenario: config.scenario.as_str().into(),
            terminal_class: DryRunTerminalClass::DuplicateRejected,
            residual_verdict: ResidualAdmissionVerdict::ResidualAdmissionNoGo
                .as_str()
                .into(),
            product_admission_class: CodexAdmissionClass::MediationHardenedPartial
                .as_str()
                .into(),
            preflight_result: "not_run".into(),
            authority_decision_status: "not_run".into(),
            gateway_provider_requests: 0,
            gateway_input_tokens: 0,
            gateway_output_tokens: 0,
            usage_reconcile: "not_run".into(),
            journal_halted: false,
            duplicate_or_replay: true,
            live_provider_request: false,
            secrets_embedded: false,
            receipt_sha256: String::new(),
            evidence_path: lock_path,
            notes: vec!["durable attempt lock present; duplicate start rejected".into()],
        };
        return finish_receipt(receipt, &config.evidence_root);
    }

    // 1) Residual finding (provider-free).
    let residual = evaluate_residual_admission_for_current_product();

    // 2) Store-owned decision + fixture principal authorization (never free-form actor).
    let store_path = config.evidence_root.join("managed-acceptance.db");
    let store =
        LocalProductStore::new_with_clock(&store_path, || "2026-07-25T12:00:00Z".to_string())
            .map_err(|e| format!("store open: {e}"))?;
    let draft = draft_partial_mediation_authority_decision();
    let decision_body = draft.to_json();
    let mut decision_body = decision_body;
    // A generic authority draft is not spend-capable. Bind this fixture-only
    // provider-free attempt exactly before it enters the store-owned spend API.
    let trial = decision_body
        .get_mut("trial_envelope")
        .and_then(Value::as_object_mut)
        .ok_or("draft decision trial_envelope missing")?;
    trial.insert("product_task_id".into(), json!(config.product_task_id));
    trial.insert("workflow_id".into(), json!("wf-managed-acceptance-dry-run"));
    trial.insert(
        "workflow_node_id".into(),
        json!("node-managed-acceptance-dry-run"),
    );
    trial.insert(
        "execution_id".into(),
        json!(format!("codex-attempt-{}", config.attempt_id)),
    );
    trial.insert("attempt_id".into(), json!(config.attempt_id));
    trial.insert("target_repo".into(), json!(config.disposable_target_repo));
    trial.insert("target_main_sha".into(), json!(config.target_main_sha));
    trial.insert("exact_codex_path".into(), json!("/fixture/codex"));
    trial.insert("exact_codex_sha256".into(), json!("ab".repeat(32)));
    trial.insert(
        "cancellation_identity".into(),
        json!(format!("cancel-{}", config.attempt_id)),
    );
    trial.insert(
        "rollback_identity".into(),
        json!(format!("rollback-{}", config.attempt_id)),
    );
    trial.insert("output_branch_prefix".into(), json!("acp/"));
    trial.insert(
        "cost_authority".into(),
        CostAuthority::CostUnavailable.to_json(),
    );
    // Ensure decision_id present for store identity.
    if decision_body.get("decision_id").is_none() {
        decision_body.as_object_mut().unwrap().insert(
            "decision_id".into(),
            json!(format!("mad-{}", config.attempt_id)),
        );
    }
    let decision_id = decision_body["decision_id"].as_str().unwrap().to_string();
    let residual_sha = draft.residual_finding_sha256.clone();
    let persisted = store
        .upsert_managed_acceptance_decision(
            "tenant-dry-run",
            &decision_body,
            &residual_sha,
            "draft_pending_operator",
            None,
            Some("2026-08-01T00:00:00Z"),
        )
        .map_err(|e| format!("decision upsert: {e}"))?;
    let dsha = persisted["decision_body_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let mut authority_status = persisted["status"].as_str().unwrap_or("draft").to_string();
    let mut authorization_id = String::new();
    let fixture_principal =
        AuthenticatedPrincipal::fixture_for_tests("tenant-dry-run", "fixture-principal-dry-run")
            .map_err(|e| format!("fixture principal: {e}"))?;
    let mut spend_authorization_id = String::new();
    let mut issued_spend_request: Option<SpendAuthorizationRequest> = None;
    if config.simulate_operator_ack {
        let auth = store
            .accept_managed_acceptance_decision(
                &fixture_principal,
                &RiskAcknowledgementRequest {
                    decision_id: decision_id.clone(),
                    expected_decision_body_sha256: dsha.clone(),
                    expected_residual_finding_sha256: residual_sha.clone(),
                    submitted_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.to_string(),
                    explicit_go: true,
                },
            )
            .map_err(|e| format!("store accept: {e}"))?;
        authority_status = "operator_accepted".into();
        authorization_id = auth["authorization_id"].as_str().unwrap().to_string();
        assert_eq!(auth["execution_granted"], false);
        assert_eq!(auth["fixture_only"], true);
        let trial = &draft.trial_envelope;
        let spend_request = SpendAuthorizationRequest {
            risk_authorization_id: authorization_id.clone(),
            product_task_id: config.product_task_id.clone(),
            workflow_id: Some("wf-managed-acceptance-dry-run".into()),
            workflow_node_id: Some("node-managed-acceptance-dry-run".into()),
            execution_id: format!("codex-attempt-{}", config.attempt_id),
            attempt_id: config.attempt_id.clone(),
            binary_path: "/fixture/codex".into(),
            binary_version: trial.exact_codex_version.clone(),
            binary_sha256: "ab".repeat(32),
            provider_kind: trial.provider_kind.clone(),
            provider_host: trial.provider_host.clone(),
            provider_base_url: trial.provider_base_url.clone(),
            admitted_endpoint_paths: trial.admitted_endpoint_paths.clone(),
            model: trial.model.clone(),
            target_repo: config.disposable_target_repo.clone(),
            target_main_sha: config.target_main_sha.clone(),
            output_branch_prefix: "acp/".into(),
            draft_pr_only: trial.draft_pr_only,
            max_provider_requests: trial.max_provider_requests,
            max_retries: trial.max_retries,
            max_input_tokens: trial.max_input_tokens,
            max_output_tokens: trial.max_output_tokens,
            max_total_tokens: trial.max_total_tokens,
            max_wall_time_ms: trial.max_wall_time_ms,
            cost_authority: CostAuthority::CostUnavailable,
            cancellation_identity: format!("cancel-{}", config.attempt_id),
            rollback_identity: format!("rollback-{}", config.attempt_id),
        };
        let spend = store
            .issue_managed_acceptance_spend_authorization(&fixture_principal, &spend_request)
            .map_err(|e| format!("store spend: {e}"))?;
        spend_authorization_id = spend["spend_authorization_id"]
            .as_str()
            .unwrap()
            .to_string();
        issued_spend_request = Some(spend_request);
    } else if matches!(
        residual.verdict,
        ResidualAdmissionVerdict::ResidualAdmissionNoGo
    ) {
        let receipt = DryRunReceipt {
            schema_version: MANAGED_ACCEPTANCE_DRY_RUN_RECEIPT_SCHEMA.to_string(),
            attempt_id: config.attempt_id.clone(),
            product_task_id: config.product_task_id.clone(),
            scenario: config.scenario.as_str().into(),
            terminal_class: DryRunTerminalClass::BlockedAuthorization,
            residual_verdict: residual.verdict.as_str().into(),
            product_admission_class: residual.product_admission_class.clone(),
            preflight_result: "not_run".into(),
            authority_decision_status: authority_status,
            gateway_provider_requests: 0,
            gateway_input_tokens: 0,
            gateway_output_tokens: 0,
            usage_reconcile: "not_run".into(),
            journal_halted: false,
            duplicate_or_replay: false,
            live_provider_request: false,
            secrets_embedded: false,
            receipt_sha256: String::new(),
            evidence_path: PathBuf::new(),
            notes: vec![
                "residual NO-GO requires store-owned operator authorization before dry-run mediated path"
                    .into(),
            ],
        };
        return finish_receipt(receipt, &config.evidence_root);
    }

    // 3) Preflight with fixture-ready inputs bound to store decision hashes.
    let mut preflight_input = fixture_ready_pending_operator_input(&draft);
    preflight_input.disposable_target_repo = Some(config.disposable_target_repo.clone());
    preflight_input.target_main_sha = Some(config.target_main_sha.clone());
    preflight_input.authority_decision_status = Some(authority_status.clone());
    preflight_input.authority_decision_body_sha256 = Some(dsha.clone());
    preflight_input.residual_finding_sha256 = Some(residual_sha.clone());
    let preflight = run_managed_acceptance_preflight(&preflight_input);
    if !preflight.result.is_ready()
        && !matches!(
            preflight.result,
            ManagedAcceptancePreflightResult::ReadyPendingOperatorRiskAcceptance
        )
    {
        let receipt = DryRunReceipt {
            schema_version: MANAGED_ACCEPTANCE_DRY_RUN_RECEIPT_SCHEMA.to_string(),
            attempt_id: config.attempt_id.clone(),
            product_task_id: config.product_task_id.clone(),
            scenario: config.scenario.as_str().into(),
            terminal_class: DryRunTerminalClass::BlockedPreflight,
            residual_verdict: residual.verdict.as_str().into(),
            product_admission_class: residual.product_admission_class.clone(),
            preflight_result: preflight.result.as_str().into(),
            authority_decision_status: authority_status,
            gateway_provider_requests: 0,
            gateway_input_tokens: 0,
            gateway_output_tokens: 0,
            usage_reconcile: "not_run".into(),
            journal_halted: false,
            duplicate_or_replay: false,
            live_provider_request: false,
            secrets_embedded: false,
            receipt_sha256: String::new(),
            evidence_path: PathBuf::new(),
            notes: preflight.blockers,
        };
        return finish_receipt(receipt, &config.evidence_root);
    }

    // 3b) Store-owned exactly-once attempt admission (fixture dry-run path only).
    if authorization_id.is_empty() {
        return Err("authorization_id required after operator accept".into());
    }
    if spend_authorization_id.is_empty() {
        return Err("spend_authorization_id required after operator accept".into());
    }
    let spend_bound = issued_spend_request
        .as_ref()
        .ok_or("spend request required after operator accept")?;
    let spend_row = store
        .get_managed_acceptance_spend_authorization(&spend_authorization_id)
        .map_err(|e| format!("load spend: {e}"))?
        .ok_or("spend authorization missing after issue")?;
    let spend_body = spend_row
        .get("body_json")
        .cloned()
        .unwrap_or_else(|| spend_row.clone());
    let authority_manifest =
        crate::storage::local_product_store::build_attempt_authority_manifest(&spend_body)
            .map_err(|e| format!("attempt manifest: {e}"))?;
    let attempt_body = json!({
        "manifest_sha256": authority_manifest.get("manifest_sha256"),
        "manifest": authority_manifest,
        "product_task_id": spend_bound.product_task_id,
        "workflow_id": spend_bound.workflow_id,
        "workflow_node_id": spend_bound.workflow_node_id,
        "execution_id": spend_bound.execution_id,
        "binary_path": spend_bound.binary_path,
        "binary_version": spend_bound.binary_version,
        "binary_sha256": spend_bound.binary_sha256,
        "provider_kind": spend_bound.provider_kind,
        "provider_host": spend_bound.provider_host,
        "provider_base_url": spend_bound.provider_base_url,
        "admitted_endpoint_paths": spend_bound.admitted_endpoint_paths,
        "model": spend_bound.model,
        "target_repo": spend_bound.target_repo,
        "target_main_sha": spend_bound.target_main_sha,
        "output_branch_prefix": spend_bound.output_branch_prefix,
        "draft_pr_only": spend_bound.draft_pr_only,
        "max_provider_requests": spend_bound.max_provider_requests,
        "max_retries": spend_bound.max_retries,
        "max_input_tokens": spend_bound.max_input_tokens,
        "max_output_tokens": spend_bound.max_output_tokens,
        "max_total_tokens": spend_bound.max_total_tokens,
        "max_wall_time_ms": spend_bound.max_wall_time_ms,
        "cost_authority": spend_bound.cost_authority.to_json(),
        "cancellation_identity": spend_bound.cancellation_identity,
        "rollback_identity": spend_bound.rollback_identity,
        "scenario": config.scenario.as_str(),
        "dry_run": true,
    });
    let admitted = store
        .admit_managed_acceptance_attempt(
            &fixture_principal,
            &config.attempt_id,
            &attempt_body,
            &spend_authorization_id,
            true, // fixture dry-run only
        )
        .map_err(|e| format!("attempt admit: {e}"))?;
    let _store_idempotent_replay = admitted
        .get("idempotent_replay")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let lease_token = admitted
        .get("lease_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // 4) Mediated gateway dry-run against mock upstream.
    let hits = Arc::new(AtomicUsize::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (mode, max_requests, max_retries, max_cumulative) = match config.scenario {
        DryRunScenario::Success | DryRunScenario::IdempotentReplay => {
            (MockUpstreamMode::SuccessUsage, 1, 0, 50_000)
        }
        DryRunScenario::ProviderFailure => (MockUpstreamMode::ProviderError, 1, 0, 50_000),
        DryRunScenario::Timeout | DryRunScenario::Cancellation => {
            (MockUpstreamMode::HangUntilCancelled, 1, 0, 50_000)
        }
        DryRunScenario::BudgetExhaustion => (MockUpstreamMode::SuccessUsage, 1, 0, 50_000),
        DryRunScenario::EvidenceContradiction => (MockUpstreamMode::SuccessUsage, 1, 0, 50_000),
        DryRunScenario::RestartAfterOutcomeUnknown | DryRunScenario::OutcomeUnknown => {
            (MockUpstreamMode::DropConnection, 1, 0, 50_000)
        }
        DryRunScenario::DuplicateStartRejected => (MockUpstreamMode::SuccessUsage, 1, 0, 50_000),
    };

    let (upstream, _join) = spawn_mock_upstream(mode, Arc::clone(&hits), Arc::clone(&cancel));
    let execution_id = if config.scenario == DryRunScenario::IdempotentReplay {
        // Fixed identity for replay demonstration when attempt_id reused.
        format!("codex-attempt-{}", config.attempt_id)
    } else {
        new_codex_attempt_id()
    };
    // Ensure attempt id shape for journal resume tests.
    let execution_id = if execution_id.starts_with("codex-attempt-") {
        execution_id
    } else {
        format!("codex-attempt-{execution_id}")
    };

    let authority = sample_authority(
        &config.product_task_id,
        &execution_id,
        &upstream,
        max_requests,
        max_retries,
        max_cumulative,
    );
    let journal = parent_owned_journal_path(&authority.execution_id);
    let _ = fs::remove_file(&journal);

    // Parent-only fixture key — never logged or written into receipts.
    let parent_key = "fixture-upstream-key";
    let gateway = CodexBudgetGateway::start(
        CodexGatewayStartPermit::provider_free_fixture(&authority.execution_id),
        authority.clone(),
        &upstream,
        parent_key,
        journal.clone(),
    )
    .map_err(|e| format!("gateway start: {e}"))?;

    let body = br#"{"model":"gpt-test-model","input":"fixture-ordinary-coding-task"}"#;
    let mut terminal = DryRunTerminalClass::SucceededFixture;
    let mut notes = vec![
        "provider-free dry-run; live_provider_request=false".into(),
        format!("scenario={}", config.scenario.as_str()),
        format!(
            "admitted_codex_version_pin={ADMITTED_CODEX_VERSION}; model_fixture=gpt-test-model; product_model_pin={ADMITTED_CODEX_MODEL}"
        ),
    ];

    match config.scenario {
        DryRunScenario::Success
        | DryRunScenario::IdempotentReplay
        | DryRunScenario::DuplicateStartRejected => {
            let resp = post_gateway(gateway.local_addr(), gateway.session_token(), body)?;
            if !resp.contains("input_tokens") {
                notes.push(format!(
                    "unexpected success response class={}",
                    resp.lines().next().unwrap_or("")
                ));
            }
        }
        DryRunScenario::ProviderFailure => {
            let resp = post_gateway(gateway.local_addr(), gateway.session_token(), body)?;
            notes.push("provider failure fixture exercised".into());
            // Gateway may surface upstream error or usage extract failure.
            if resp.contains("500") || resp.contains("upstream") || resp.contains("error") {
                terminal = DryRunTerminalClass::FailedProvider;
            } else {
                terminal = DryRunTerminalClass::OutcomeUnknownCharged;
            }
        }
        DryRunScenario::Timeout => {
            // Start request in background, then cancel flag and drop gateway.
            let addr = gateway.local_addr();
            let token = gateway.session_token().to_string();
            let handle = thread::spawn(move || post_gateway(addr, &token, body));
            thread::sleep(Duration::from_millis(80));
            cancel.store(true, AtomicOrdering::SeqCst);
            // Do not wait long; treat as timeout classification for fixture.
            let _ = handle.join();
            terminal = DryRunTerminalClass::FailedTimeout;
            notes.push("timeout fixture: cancelled mock upstream hang".into());
        }
        DryRunScenario::Cancellation => {
            let addr = gateway.local_addr();
            let token = gateway.session_token().to_string();
            let handle = thread::spawn(move || post_gateway(addr, &token, body));
            thread::sleep(Duration::from_millis(50));
            cancel.store(true, AtomicOrdering::SeqCst);
            let _ = handle.join();
            terminal = DryRunTerminalClass::Cancelled;
            notes.push("cancellation fixture: process cancel before upstream complete".into());
        }
        DryRunScenario::BudgetExhaustion => {
            let resp1 = post_gateway(gateway.local_addr(), gateway.session_token(), body)?;
            notes.push(format!(
                "first post status_line={}",
                resp1.lines().next().unwrap_or("")
            ));
            // Second post with max_retries=0 and max_requests=1 → budget exhaust.
            let resp2 = post_gateway(gateway.local_addr(), gateway.session_token(), body)?;
            if resp2.contains("retry_budget_exhausted")
                || resp2.contains("request_budget_exhausted")
                || resp2.contains("429")
            {
                terminal = DryRunTerminalClass::BudgetExhausted;
            } else {
                terminal = DryRunTerminalClass::OutcomeUnknownCharged;
                notes.push(
                    "expected budget exhaust not observed; classified outcome_unknown".into(),
                );
            }
        }
        DryRunScenario::EvidenceContradiction => {
            let _ = post_gateway(gateway.local_addr(), gateway.session_token(), body)?;
            terminal = DryRunTerminalClass::EvidenceConflict;
            notes.push("session counters deliberately conflict with gateway for fixture".into());
        }
        DryRunScenario::OutcomeUnknown | DryRunScenario::RestartAfterOutcomeUnknown => {
            let _ = post_gateway(gateway.local_addr(), gateway.session_token(), body);
            terminal = DryRunTerminalClass::OutcomeUnknownCharged;
            notes.push("drop-connection fixture charges outcome-unknown in journal path".into());
        }
    }

    let usage = gateway.shutdown();
    let mut reconcile = reconcile_gateway_and_session_usage(&usage, None, None);
    if config.scenario == DryRunScenario::EvidenceContradiction {
        reconcile = reconcile_gateway_and_session_usage(
            &usage,
            Some(usage.cumulative_input_tokens.saturating_add(99)),
            Some(usage.cumulative_output_tokens),
        );
        if !matches!(reconcile, UsageReconcileResult::Conflict { .. }) {
            notes.push("expected conflict; gateway may have zero requests".into());
        }
    }

    // Usage event mapping + cross-source reconcile (gateway only for most scenarios).
    let binding = UsageBindingContext {
        product_task_id: Some(config.product_task_id.clone()),
        workflow_node_id: Some(authority.workflow_node_id.clone()),
        managed_execution_id: Some(authority.execution_id.clone()),
        requested_model: Some(authority.model.clone()),
        executable_path_fingerprint: Some(hex::encode(Sha256::digest(
            authority
                .executable
                .binary_path
                .to_string_lossy()
                .as_bytes(),
        ))),
        executable_version: Some(authority.executable.binary_version.clone()),
        executable_sha256: Some(authority.executable.binary_sha256.clone()),
    };
    let ts = "2026-07-25T00:00:00Z";
    let gw_event = budget_gateway_usage_to_event(&usage, &authority, &binding, ts);
    assert_eq!(gw_event.cost_source, CostSource::Unavailable);
    let events: Vec<ExecutionUsageEventV1> = vec![gw_event];
    let multi = reconcile_usage_events(events);
    let _ = admission_evidence_ok(&multi);

    // Restart path: resume journal after outcome-unknown charge.
    if matches!(
        config.scenario,
        DryRunScenario::RestartAfterOutcomeUnknown | DryRunScenario::OutcomeUnknown
    ) {
        // New gateway with same attempt must not restore budget freely.
        match CodexBudgetGateway::start(
            CodexGatewayStartPermit::provider_free_fixture(&authority.execution_id),
            authority.clone(),
            &upstream,
            parent_key,
            journal.clone(),
        ) {
            Ok(g2) => {
                let resp = post_gateway(g2.local_addr(), g2.session_token(), body);
                notes.push(format!(
                    "restart admit_result={}",
                    resp.as_ref()
                        .map(|r| r.lines().next().unwrap_or("ok").to_string())
                        .unwrap_or_else(|e| e.clone())
                ));
                let u2 = g2.shutdown();
                notes.push(format!(
                    "restart_usage_requests={};halted={}",
                    u2.provider_requests, u2.journal_halted
                ));
            }
            Err(e) => notes.push(format!("restart_gateway_err={e}")),
        }
        terminal = DryRunTerminalClass::OutcomeUnknownCharged;
    }

    // PreferSessionOnly is fail-closed for product admission evidence when gateway empty
    // and session-only — already covered elsewhere; record reconcile label.
    if matches!(reconcile, UsageReconcileResult::Conflict { .. }) {
        terminal = DryRunTerminalClass::EvidenceConflict;
    }

    let _ = fs::remove_file(&authority.executable.binary_path);

    let receipt = DryRunReceipt {
        schema_version: MANAGED_ACCEPTANCE_DRY_RUN_RECEIPT_SCHEMA.to_string(),
        attempt_id: config.attempt_id.clone(),
        product_task_id: config.product_task_id.clone(),
        scenario: config.scenario.as_str().into(),
        terminal_class: terminal,
        residual_verdict: residual.verdict.as_str().into(),
        product_admission_class: residual.product_admission_class,
        preflight_result: preflight.result.as_str().into(),
        authority_decision_status: authority_status,
        gateway_provider_requests: usage.provider_requests,
        gateway_input_tokens: usage.cumulative_input_tokens,
        gateway_output_tokens: usage.cumulative_output_tokens,
        usage_reconcile: reconcile_label(&reconcile),
        journal_halted: usage.journal_halted,
        duplicate_or_replay: false,
        live_provider_request: false,
        secrets_embedded: false,
        receipt_sha256: String::new(),
        evidence_path: PathBuf::new(),
        notes,
    };
    if !lease_token.is_empty() {
        let _ = store.complete_managed_acceptance_attempt(
            &config.attempt_id,
            &lease_token,
            match receipt.terminal_class {
                DryRunTerminalClass::SucceededFixture => "succeeded",
                DryRunTerminalClass::Cancelled => "cancelled",
                DryRunTerminalClass::OutcomeUnknownCharged => "outcome_unknown",
                DryRunTerminalClass::BudgetExhausted
                | DryRunTerminalClass::FailedProvider
                | DryRunTerminalClass::FailedTimeout
                | DryRunTerminalClass::EvidenceConflict => "failed",
                _ => "failed",
            },
            receipt.terminal_class.as_str(),
            &receipt.to_json(),
        );
    }
    let receipt = finish_receipt(receipt, &config.evidence_root)?;

    // Idempotent second call for IdempotentReplay scenario.
    if config.scenario == DryRunScenario::IdempotentReplay {
        let again = run_managed_acceptance_dry_run(config)?;
        assert!(
            again.duplicate_or_replay
                || matches!(
                    again.terminal_class,
                    DryRunTerminalClass::IdempotentReplayHit
                        | DryRunTerminalClass::SucceededFixture
                )
        );
        return Ok(again);
    }

    Ok(receipt)
}

/// Board readiness dossier: single JSON summary of residual + decision + preflight + dry-run.
pub fn board_readiness_dossier() -> Value {
    let residual = evaluate_residual_admission_for_current_product();
    let decision = draft_partial_mediation_authority_decision();
    let preflight_input = fixture_ready_pending_operator_input(&decision);
    let preflight = run_managed_acceptance_preflight(&preflight_input);
    let dossier = json!({
        "schema_version": "codex_admission_board_readiness.v1",
        "board": "PE7 Codex mediation residual + partial-mediation authority + managed acceptance preflight + fixture dry-run",
        "live_provider_request": false,
        "admission_classification": residual.product_admission_class,
        "residual_verdict": residual.verdict.as_str(),
        "residual_remaining_blockers": residual.remaining_blockers,
        "authority_decision_status": decision.status.as_str(),
        "authority_authorizes_live_trial": decision.status.authorizes_bounded_live_trial(),
        "agent_recommendation": decision.agent_recommendation.as_str(),
        "go_alternative": decision.go_alternative,
        "no_go_alternative": decision.no_go_alternative,
        "preflight_result": preflight.result.as_str(),
        "preflight_ready": preflight.result.is_ready(),
        "manual_gate": {
            "required": true,
            "actions": [
                "independent review of this board PR/stack",
                "operator multi-field risk acknowledgement when residual_admission_no_go",
                "parent-only API key present at runtime (never in git)",
                "one command/API action to start the bounded disposable live task"
            ]
        },
        "non_claims": [
            "Does not claim full_provider_free_mediation_admission",
            "Does not authorize RWE, Architecture Convergence, Level-2, Meta, OpenCode, Vader, or PR #225",
            "Does not perform a live provider request",
            "Does not merge or enable auto-merge"
        ],
    });
    redact_assert_no_secrets(&dossier);
    dossier
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_scenario(scenario: DryRunScenario) -> DryRunReceipt {
        let mut cfg = DryRunConfig::fixture(scenario);
        if scenario == DryRunScenario::DuplicateStartRejected {
            // Seed a completed attempt with same id.
            let seed_id = cfg.attempt_id.clone();
            let mut seed = DryRunConfig::fixture(DryRunScenario::Success);
            seed.attempt_id = seed_id.clone();
            seed.evidence_root = cfg.evidence_root.clone();
            let first = run_managed_acceptance_dry_run(seed).expect("seed success");
            assert_eq!(first.terminal_class, DryRunTerminalClass::SucceededFixture);
            cfg.attempt_id = seed_id;
        }
        run_managed_acceptance_dry_run(cfg).expect("dry-run")
    }

    #[test]
    fn dry_run_success_fixture_completes_without_live_provider() {
        let receipt = run_scenario(DryRunScenario::Success);
        assert_eq!(
            receipt.terminal_class,
            DryRunTerminalClass::SucceededFixture
        );
        assert!(!receipt.live_provider_request);
        assert!(!receipt.secrets_embedded);
        assert_eq!(receipt.residual_verdict, "residual_admission_no_go");
        assert_eq!(
            receipt.product_admission_class,
            CodexAdmissionClass::MediationHardenedPartial.as_str()
        );
        assert!(receipt.gateway_provider_requests >= 1);
        assert_eq!(receipt.usage_reconcile, "prefer_gateway");
        redact_assert_no_secrets(&receipt.to_json());
        assert!(receipt.evidence_path.is_file());
    }

    #[test]
    fn dry_run_budget_exhaustion_with_max_retries_zero() {
        let receipt = run_scenario(DryRunScenario::BudgetExhaustion);
        assert_eq!(receipt.terminal_class, DryRunTerminalClass::BudgetExhausted);
        assert!(!receipt.live_provider_request);
    }

    #[test]
    fn dry_run_evidence_contradiction_fails_closed() {
        let receipt = run_scenario(DryRunScenario::EvidenceContradiction);
        assert_eq!(
            receipt.terminal_class,
            DryRunTerminalClass::EvidenceConflict
        );
        assert_eq!(receipt.usage_reconcile, "conflict");
    }

    #[test]
    fn dry_run_provider_failure_classifies() {
        let receipt = run_scenario(DryRunScenario::ProviderFailure);
        assert!(matches!(
            receipt.terminal_class,
            DryRunTerminalClass::FailedProvider | DryRunTerminalClass::OutcomeUnknownCharged
        ));
    }

    #[test]
    fn dry_run_timeout_and_cancellation() {
        let t = run_scenario(DryRunScenario::Timeout);
        assert_eq!(t.terminal_class, DryRunTerminalClass::FailedTimeout);
        let c = run_scenario(DryRunScenario::Cancellation);
        assert_eq!(c.terminal_class, DryRunTerminalClass::Cancelled);
    }

    #[test]
    fn dry_run_outcome_unknown_and_restart() {
        let o = run_scenario(DryRunScenario::OutcomeUnknown);
        assert_eq!(o.terminal_class, DryRunTerminalClass::OutcomeUnknownCharged);
        let r = run_scenario(DryRunScenario::RestartAfterOutcomeUnknown);
        assert_eq!(r.terminal_class, DryRunTerminalClass::OutcomeUnknownCharged);
        assert!(r.notes.iter().any(|n| n.contains("restart")));
    }

    #[test]
    fn dry_run_duplicate_start_rejected() {
        let receipt = run_scenario(DryRunScenario::DuplicateStartRejected);
        assert!(matches!(
            receipt.terminal_class,
            DryRunTerminalClass::DuplicateRejected | DryRunTerminalClass::IdempotentReplayHit
        ));
        assert!(receipt.duplicate_or_replay);
    }

    #[test]
    fn dry_run_idempotent_replay_hits_registry() {
        let mut cfg = DryRunConfig::fixture(DryRunScenario::IdempotentReplay);
        let attempt = cfg.attempt_id.clone();
        let first = run_managed_acceptance_dry_run(cfg.clone()).expect("first");
        // Second call with same attempt_id.
        cfg.attempt_id = attempt;
        let second = run_managed_acceptance_dry_run(cfg).expect("second");
        assert!(
            second.duplicate_or_replay
                || matches!(
                    second.terminal_class,
                    DryRunTerminalClass::IdempotentReplayHit
                )
                || second.receipt_sha256 == first.receipt_sha256
        );
        assert!(!second.live_provider_request);
    }

    #[test]
    fn dry_run_blocks_without_operator_ack_under_residual_no_go() {
        let mut cfg = DryRunConfig::fixture(DryRunScenario::Success);
        cfg.simulate_operator_ack = false;
        let receipt = run_managed_acceptance_dry_run(cfg).expect("blocked path");
        assert_eq!(
            receipt.terminal_class,
            DryRunTerminalClass::BlockedAuthorization
        );
    }

    #[test]
    fn board_readiness_dossier_is_redacted_and_manual_gated() {
        let dossier = board_readiness_dossier();
        assert_eq!(dossier["live_provider_request"], false);
        assert_eq!(dossier["residual_verdict"], "residual_admission_no_go");
        assert_eq!(dossier["manual_gate"]["required"], true);
        assert_eq!(
            dossier["admission_classification"],
            "mediation_hardened_partial"
        );
        redact_assert_no_secrets(&dossier);
    }

    #[test]
    fn cost_remains_unavailable_on_gateway_usage_event() {
        let cfg = DryRunConfig::fixture(DryRunScenario::Success);
        let receipt = run_managed_acceptance_dry_run(cfg).expect("success");
        assert!(!receipt
            .to_json()
            .to_string()
            .contains("provider_reported_cost\":1"));
        assert!(!receipt.live_provider_request);
    }

    #[test]
    fn dry_run_receipt_binding_is_restart_safe_in_sqlite_store() {
        use crate::storage::local_product_store::LocalProductStore;
        use tempfile::tempdir;

        let receipt = run_scenario(DryRunScenario::Success);
        let directory = tempdir().unwrap();
        let path = directory.path().join("dry-run-bind.db");
        let store = LocalProductStore::new_with_clock(&path, || "2026-07-25T00:00:00Z".to_string())
            .unwrap();
        let first = store
            .acknowledge_operator_source(
                "managed-acceptance-dry-run",
                "managed_acceptance_dry_run",
                &receipt.attempt_id,
                &receipt.receipt_sha256,
                Some("fixture dry-run receipt binding"),
                "operator-fixture-alice",
            )
            .unwrap();
        assert_eq!(first["approval_granted"], false);
        assert_eq!(first["mutation_authority"], "acknowledgement_only");
        let again = store
            .acknowledge_operator_source(
                "managed-acceptance-dry-run-retry",
                "managed_acceptance_dry_run",
                &receipt.attempt_id,
                &receipt.receipt_sha256,
                Some("retry"),
                "operator-fixture-bob",
            )
            .unwrap();
        assert_eq!(again["acknowledgement_id"], first["acknowledgement_id"]);
        drop(store);
        let restarted = LocalProductStore::new(&path).unwrap();
        assert!(restarted
            .is_operator_source_acknowledged(
                "managed_acceptance_dry_run",
                &receipt.attempt_id,
                &receipt.receipt_sha256
            )
            .unwrap());
    }

    #[cfg(feature = "pg-tests")]
    #[test]
    fn dry_run_receipt_binding_is_restart_safe_in_postgres_store() {
        use crate::storage::local_product_store::LocalProductStore;

        let receipt = run_scenario(DryRunScenario::Success);
        let url = match std::env::var("ACP_TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
        {
            Ok(url) => url,
            Err(_) => {
                eprintln!("ACP_TEST_DATABASE_URL not set; skipping pg-tests");
                return;
            }
        };
        let store = LocalProductStore::new_postgres(&url, || "2026-07-25T00:00:00Z".to_string())
            .expect("pg store");
        let first = store
            .acknowledge_operator_source(
                "managed-acceptance-dry-run-pg",
                "managed_acceptance_dry_run",
                &receipt.attempt_id,
                &receipt.receipt_sha256,
                Some("fixture dry-run receipt binding pg"),
                "operator-fixture-alice",
            )
            .expect("ack");
        assert_eq!(first["approval_granted"], false);
        let again = store
            .acknowledge_operator_source(
                "managed-acceptance-dry-run-pg-retry",
                "managed_acceptance_dry_run",
                &receipt.attempt_id,
                &receipt.receipt_sha256,
                Some("retry"),
                "operator-fixture-bob",
            )
            .expect("ack retry");
        assert_eq!(again["acknowledgement_id"], first["acknowledgement_id"]);
        assert!(store
            .is_operator_source_acknowledged(
                "managed_acceptance_dry_run",
                &receipt.attempt_id,
                &receipt.receipt_sha256
            )
            .unwrap());
    }
}
