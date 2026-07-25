//! PE7-PRODUCT-GOLDEN-PATH-MANAGED-ACCEPTANCE-PREFLIGHT-1
//!
//! Provider-free deterministic preflight for a managed Codex Golden Path trial.
//! Does **not** perform a live model request. Never embeds secret values in
//! results or manifests (presence/classification only).

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::codex_mediation_admission::{
    unprivileged_user_ns_available, CodexAdmissionClass, BUBBLEWRAP_BIN,
};
use super::codex_partial_mediation_authority_decision::{
    AuthorityDecisionStatus, PartialMediationAuthorityDecision,
    PARTIAL_MEDIATION_AUTHORITY_DECISION_SCHEMA,
};
use super::codex_residual_admission::{
    evaluate_residual_admission, probe_unshare_net_available, CapabilityEvidenceClass,
    ResidualAdmissionVerdict,
};
use super::config::{ADMITTED_CODEX_MODEL, ADMITTED_CODEX_VERSION};

pub const MANAGED_ACCEPTANCE_PREFLIGHT_SCHEMA: &str = "codex_managed_acceptance_preflight.v1";
pub const MANAGED_ACCEPTANCE_MANIFEST_SCHEMA: &str = "codex_managed_acceptance_manifest.v1";

/// Typed preflight result (fail closed on missing/contradictory state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedAcceptancePreflightResult {
    ReadyUnderFullAdmission,
    ReadyPendingOperatorRiskAcceptance,
    BlockedMissingCredential,
    BlockedMissingAuthorization,
    BlockedIsolation,
    BlockedIdentity,
    BlockedBudget,
    OutcomeUnknown,
}

impl ManagedAcceptancePreflightResult {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadyUnderFullAdmission => "ready_under_full_admission",
            Self::ReadyPendingOperatorRiskAcceptance => "ready_pending_operator_risk_acceptance",
            Self::BlockedMissingCredential => "blocked_missing_credential",
            Self::BlockedMissingAuthorization => "blocked_missing_authorization",
            Self::BlockedIsolation => "blocked_isolation",
            Self::BlockedIdentity => "blocked_identity",
            Self::BlockedBudget => "blocked_budget",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(
            self,
            Self::ReadyUnderFullAdmission | Self::ReadyPendingOperatorRiskAcceptance
        )
    }

    /// Only full admission is live-ready without residual risk acceptance.
    /// `ReadyPendingOperatorRiskAcceptance` still requires multi-field operator ack.
    pub fn allows_live_trial_without_further_operator_ack(&self) -> bool {
        matches!(self, Self::ReadyUnderFullAdmission)
    }
}

/// Redacted inputs for preflight (never include secret material).
#[derive(Debug, Clone, PartialEq)]
pub struct ManagedAcceptancePreflightInput {
    pub execution_gate_enabled: bool,
    pub codex_binary_path: Option<PathBuf>,
    pub codex_version: Option<String>,
    pub codex_sha256: Option<String>,
    pub admitted_model: Option<String>,
    pub provider_kind: Option<String>,
    pub provider_host: Option<String>,
    pub provider_base_url: Option<String>,
    pub admitted_endpoint_paths: Vec<String>,
    /// Parent credential presence only — never the secret value.
    pub parent_credential_present: bool,
    /// Child env audit: true when no reusable upstream credential is present.
    pub child_env_has_no_reusable_credential: bool,
    pub mediation_gateway_configured: bool,
    pub gateway_is_loopback: bool,
    pub journal_path_parent_owned: bool,
    pub journal_durable: bool,
    pub max_provider_requests: Option<u64>,
    pub max_retries: Option<u64>,
    pub max_input_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub max_total_tokens: Option<u64>,
    /// Typed cost authority: provider_reported | local_estimate | cost_unavailable.
    pub cost_authority_kind: String,
    pub max_cost_usd: Option<f64>,
    pub pricing_table_version: Option<String>,
    pub cost_currency: Option<String>,
    pub max_wall_time_ms: Option<u64>,
    pub product_launch_enforces_loopback_only: bool,
    pub disposable_target_repo: Option<String>,
    pub target_main_sha: Option<String>,
    pub allowed_output_branch_prefix: String,
    pub draft_pr_only: bool,
    pub auto_merge_disabled: bool,
    pub verification_commands_declared: bool,
    pub approval_requirements_declared: bool,
    pub output_confirmation_requirements_declared: bool,
    pub evidence_sink_available: bool,
    pub cancellation_cleanup_rollback_ready: bool,
    /// Authority decision status string (draft_pending_operator / operator_accepted / …).
    pub authority_decision_status: Option<String>,
    pub authority_decision_body_sha256: Option<String>,
    pub residual_finding_sha256: Option<String>,
}

impl Default for ManagedAcceptancePreflightInput {
    fn default() -> Self {
        Self {
            execution_gate_enabled: false,
            codex_binary_path: None,
            codex_version: None,
            codex_sha256: None,
            admitted_model: None,
            provider_kind: None,
            provider_host: None,
            provider_base_url: None,
            admitted_endpoint_paths: Vec::new(),
            parent_credential_present: false,
            child_env_has_no_reusable_credential: true,
            mediation_gateway_configured: false,
            gateway_is_loopback: false,
            journal_path_parent_owned: false,
            journal_durable: false,
            max_provider_requests: None,
            max_retries: None,
            max_input_tokens: None,
            max_output_tokens: None,
            max_total_tokens: None,
            cost_authority_kind: "cost_unavailable".into(),
            max_cost_usd: None,
            pricing_table_version: None,
            cost_currency: None,
            max_wall_time_ms: None,
            product_launch_enforces_loopback_only: false,
            disposable_target_repo: None,
            target_main_sha: None,
            allowed_output_branch_prefix: "acp/".into(),
            draft_pr_only: true,
            auto_merge_disabled: true,
            verification_commands_declared: false,
            approval_requirements_declared: false,
            output_confirmation_requirements_declared: false,
            evidence_sink_available: false,
            cancellation_cleanup_rollback_ready: false,
            authority_decision_status: None,
            authority_decision_body_sha256: None,
            residual_finding_sha256: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityClassificationReport {
    pub bwrap: CapabilityEvidenceClass,
    pub userns_pid: CapabilityEvidenceClass,
    pub unshare_net: CapabilityEvidenceClass,
    pub network_confinement: CapabilityEvidenceClass,
    pub residual_verdict: String,
    pub product_admission_class: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManagedAcceptancePreflightReport {
    pub schema_version: String,
    pub result: ManagedAcceptancePreflightResult,
    pub blockers: Vec<String>,
    pub checks: Vec<(String, bool, String)>,
    pub capabilities: CapabilityClassificationReport,
    pub manifest: Value,
    pub notes: Vec<String>,
}

impl ManagedAcceptancePreflightReport {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "result": self.result.as_str(),
            "is_ready": self.result.is_ready(),
            "blockers": self.blockers,
            "checks": self.checks.iter().map(|(name, ok, detail)| json!({
                "name": name,
                "ok": ok,
                "detail": detail,
            })).collect::<Vec<_>>(),
            "capabilities": {
                "bwrap": self.capabilities.bwrap.as_str(),
                "userns_pid": self.capabilities.userns_pid.as_str(),
                "unshare_net": self.capabilities.unshare_net.as_str(),
                "network_confinement": self.capabilities.network_confinement.as_str(),
                "residual_verdict": self.capabilities.residual_verdict,
                "product_admission_class": self.capabilities.product_admission_class,
            },
            "manifest": self.manifest,
            "notes": self.notes,
        })
    }
}

fn check(
    checks: &mut Vec<(String, bool, String)>,
    blockers: &mut Vec<String>,
    name: &str,
    ok: bool,
    detail: impl Into<String>,
    blocker_tag: Option<&str>,
) {
    let detail = detail.into();
    checks.push((name.to_string(), ok, detail.clone()));
    if !ok {
        if let Some(tag) = blocker_tag {
            blockers.push(format!("{tag}: {name}: {detail}"));
        } else {
            blockers.push(format!("{name}: {detail}"));
        }
    }
}

fn classify_bwrap() -> CapabilityEvidenceClass {
    if Path::new(BUBBLEWRAP_BIN).is_file() {
        CapabilityEvidenceClass::Proved
    } else {
        CapabilityEvidenceClass::Unsupported
    }
}

fn classify_userns_pid() -> CapabilityEvidenceClass {
    if !Path::new(BUBBLEWRAP_BIN).is_file() {
        return CapabilityEvidenceClass::Unsupported;
    }
    if unprivileged_user_ns_available() {
        CapabilityEvidenceClass::Proved
    } else {
        CapabilityEvidenceClass::UnavailableFailClosed
    }
}

/// Run deterministic managed-acceptance preflight (no live provider call).
/// Fixture/caller-asserted preflight. Production must use
/// [`run_owner_derived_managed_acceptance_preflight`].
#[doc(hidden)]
pub fn run_managed_acceptance_preflight_fixture_only(
    input: &ManagedAcceptancePreflightInput,
) -> ManagedAcceptancePreflightReport {
    let residual = evaluate_residual_admission(input.product_launch_enforces_loopback_only);
    let capabilities = CapabilityClassificationReport {
        bwrap: classify_bwrap(),
        userns_pid: classify_userns_pid(),
        unshare_net: probe_unshare_net_available(),
        network_confinement: residual.network.classification.clone(),
        residual_verdict: residual.verdict.as_str().to_string(),
        product_admission_class: residual.product_admission_class.clone(),
    };

    let mut checks = Vec::new();
    let mut blockers = Vec::new();

    // Execution gate
    check(
        &mut checks,
        &mut blockers,
        "execution_gate_enabled",
        input.execution_gate_enabled,
        if input.execution_gate_enabled {
            "product/execution gate enabled"
        } else {
            "execution gate is disabled (default-off)"
        },
        Some("blocked_budget_or_gate"),
    );

    // Identity
    let version_ok = input.codex_version.as_deref() == Some(ADMITTED_CODEX_VERSION);
    check(
        &mut checks,
        &mut blockers,
        "exact_codex_version",
        version_ok,
        format!(
            "expected={ADMITTED_CODEX_VERSION} observed={:?}",
            input.codex_version
        ),
        Some("blocked_identity"),
    );
    let sha_ok = input
        .codex_sha256
        .as_ref()
        .is_some_and(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()));
    check(
        &mut checks,
        &mut blockers,
        "exact_codex_sha256",
        sha_ok,
        if sha_ok {
            "sha256 present (value redacted from blocker text length-only)".to_string()
        } else {
            "codex sha256 missing or malformed".to_string()
        },
        Some("blocked_identity"),
    );
    let path_ok = input
        .codex_binary_path
        .as_ref()
        .is_some_and(|p| p.is_absolute());
    check(
        &mut checks,
        &mut blockers,
        "exact_codex_path",
        path_ok,
        if path_ok {
            "absolute path declared".to_string()
        } else {
            "binary path missing or not absolute".to_string()
        },
        Some("blocked_identity"),
    );
    let model_ok = input.admitted_model.as_deref() == Some(ADMITTED_CODEX_MODEL)
        || input
            .admitted_model
            .as_ref()
            .is_some_and(|m| !m.trim().is_empty());
    // Prefer exact admitted default model when set; allow explicit non-empty pin.
    let model_exact = input.admitted_model.as_deref() == Some(ADMITTED_CODEX_MODEL);
    check(
        &mut checks,
        &mut blockers,
        "admitted_model",
        model_ok && model_exact,
        format!(
            "expected_default={ADMITTED_CODEX_MODEL} observed={:?}",
            input.admitted_model
        ),
        Some("blocked_identity"),
    );

    let provider_ok = input.provider_kind.as_deref() == Some("openai_compatible")
        && input.provider_host.as_ref().is_some_and(|h| !h.is_empty())
        && input
            .provider_base_url
            .as_ref()
            .is_some_and(|u| u.starts_with("https://") || u.starts_with("http://"))
        && !input.admitted_endpoint_paths.is_empty();
    check(
        &mut checks,
        &mut blockers,
        "provider_identity",
        provider_ok,
        format!(
            "kind={:?} host_set={} base_set={} paths={}",
            input.provider_kind,
            input.provider_host.is_some(),
            input.provider_base_url.is_some(),
            input.admitted_endpoint_paths.len()
        ),
        Some("blocked_identity"),
    );

    // Credentials (presence only)
    check(
        &mut checks,
        &mut blockers,
        "parent_only_credential_present",
        input.parent_credential_present,
        if input.parent_credential_present {
            "parent credential present (value not recorded)"
        } else {
            "parent credential missing"
        },
        Some("blocked_missing_credential"),
    );
    check(
        &mut checks,
        &mut blockers,
        "child_env_no_reusable_credential",
        input.child_env_has_no_reusable_credential,
        if input.child_env_has_no_reusable_credential {
            "child env audit clean"
        } else {
            "child env may contain reusable credential"
        },
        Some("blocked_missing_credential"),
    );

    // Mediation / journal
    check(
        &mut checks,
        &mut blockers,
        "mediation_gateway_configured",
        input.mediation_gateway_configured && input.gateway_is_loopback,
        format!(
            "configured={} loopback={}",
            input.mediation_gateway_configured, input.gateway_is_loopback
        ),
        Some("blocked_isolation"),
    );
    check(
        &mut checks,
        &mut blockers,
        "journal_parent_owned_durable",
        input.journal_path_parent_owned && input.journal_durable,
        format!(
            "parent_owned={} durable={}",
            input.journal_path_parent_owned, input.journal_durable
        ),
        Some("blocked_budget"),
    );

    // Bounds
    let retries_ok = input.max_retries == Some(0);
    check(
        &mut checks,
        &mut blockers,
        "max_retries_zero_while_retry_identity_unproved",
        retries_ok,
        format!("max_retries={:?}", input.max_retries),
        Some("blocked_budget"),
    );
    let req_ok = input
        .max_provider_requests
        .is_some_and(|n| (1..=8).contains(&n));
    check(
        &mut checks,
        &mut blockers,
        "max_provider_requests",
        req_ok,
        format!("{:?}", input.max_provider_requests),
        Some("blocked_budget"),
    );
    let tokens_ok = input.max_input_tokens.is_some_and(|n| n > 0)
        && input.max_output_tokens.is_some_and(|n| n > 0)
        && input.max_total_tokens.is_some_and(|n| n > 0);
    check(
        &mut checks,
        &mut blockers,
        "token_bounds",
        tokens_ok,
        format!(
            "in={:?} out={:?} total={:?}",
            input.max_input_tokens, input.max_output_tokens, input.max_total_tokens
        ),
        Some("blocked_budget"),
    );
    let cost_ok = match input.cost_authority_kind.as_str() {
        "provider_reported" => input.max_cost_usd.is_some_and(|c| c > 0.0),
        "local_estimate" => {
            input.max_cost_usd.is_some_and(|c| c > 0.0)
                && input
                    .pricing_table_version
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())
        }
        "cost_unavailable" => true, // monetary ceiling not pretended
        _ => false,
    };
    check(
        &mut checks,
        &mut blockers,
        "cost_authority",
        cost_ok,
        format!(
            "kind={} max_cost={:?} pricing={:?}",
            input.cost_authority_kind, input.max_cost_usd, input.pricing_table_version
        ),
        Some("blocked_budget"),
    );
    check(
        &mut checks,
        &mut blockers,
        "wall_time_bound",
        input.max_wall_time_ms.is_some_and(|n| n > 0),
        format!("wall_ms={:?}", input.max_wall_time_ms),
        Some("blocked_budget"),
    );

    // Isolation classification (typed; bwrap required for mediated path)
    let bwrap_ok = capabilities.bwrap.is_proved();
    check(
        &mut checks,
        &mut blockers,
        "bwrap_available",
        bwrap_ok,
        capabilities.bwrap.as_str(),
        Some("blocked_isolation"),
    );
    // Userns is host-typed: record but only hard-block full admission path.
    checks.push((
        "userns_pid_classification".into(),
        true,
        capabilities.userns_pid.as_str().into(),
    ));
    checks.push((
        "unshare_net_classification".into(),
        true,
        capabilities.unshare_net.as_str().into(),
    ));
    checks.push((
        "network_confinement_classification".into(),
        true,
        format!(
            "{} product_enforced={}",
            capabilities.network_confinement.as_str(),
            input.product_launch_enforces_loopback_only
        ),
    ));

    // Target / output
    let repo_ok = input
        .disposable_target_repo
        .as_ref()
        .is_some_and(|r| r.contains('/') && !r.contains(' '));
    check(
        &mut checks,
        &mut blockers,
        "disposable_target_repo",
        repo_ok,
        format!("{:?}", input.disposable_target_repo),
        Some("blocked_identity"),
    );
    let main_sha_ok = input
        .target_main_sha
        .as_ref()
        .is_some_and(|s| s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()));
    check(
        &mut checks,
        &mut blockers,
        "target_main_sha",
        main_sha_ok,
        if main_sha_ok {
            "40-char hex sha present".to_string()
        } else {
            "target main sha missing or malformed".to_string()
        },
        Some("blocked_identity"),
    );
    check(
        &mut checks,
        &mut blockers,
        "allowed_output_branch_prefix",
        input.allowed_output_branch_prefix == "acp/",
        format!("prefix={}", input.allowed_output_branch_prefix),
        Some("blocked_identity"),
    );
    check(
        &mut checks,
        &mut blockers,
        "draft_pr_only",
        input.draft_pr_only,
        format!("draft_pr_only={}", input.draft_pr_only),
        Some("blocked_identity"),
    );
    check(
        &mut checks,
        &mut blockers,
        "auto_merge_disabled",
        input.auto_merge_disabled,
        format!("auto_merge_disabled={}", input.auto_merge_disabled),
        Some("blocked_identity"),
    );

    // Process / evidence readiness
    check(
        &mut checks,
        &mut blockers,
        "verification_commands",
        input.verification_commands_declared,
        "declared",
        Some("blocked_identity"),
    );
    check(
        &mut checks,
        &mut blockers,
        "approval_and_output_confirmation",
        input.approval_requirements_declared && input.output_confirmation_requirements_declared,
        format!(
            "approval={} output_confirm={}",
            input.approval_requirements_declared, input.output_confirmation_requirements_declared
        ),
        Some("blocked_missing_authorization"),
    );
    check(
        &mut checks,
        &mut blockers,
        "evidence_sink",
        input.evidence_sink_available,
        "available",
        Some("blocked_identity"),
    );
    check(
        &mut checks,
        &mut blockers,
        "cancellation_cleanup_rollback",
        input.cancellation_cleanup_rollback_ready,
        "ready",
        Some("blocked_isolation"),
    );

    // Authority decision / residual
    let residual_no_go = matches!(
        residual.verdict,
        ResidualAdmissionVerdict::ResidualAdmissionNoGo
    );
    let full_admission = matches!(
        residual.verdict,
        ResidualAdmissionVerdict::FullProviderFreeMediationAdmission
    );
    let decision_status = input.authority_decision_status.as_deref().unwrap_or("");
    let operator_accepted = decision_status == AuthorityDecisionStatus::OperatorAccepted.as_str();
    let draft_pending = decision_status == AuthorityDecisionStatus::DraftPendingOperator.as_str();

    if residual_no_go {
        check(
            &mut checks,
            &mut blockers,
            "authority_decision_binding",
            draft_pending || operator_accepted,
            format!(
                "residual=no_go decision_status={decision_status:?} (operator ack required before live trial)"
            ),
            Some("blocked_missing_authorization"),
        );
    }

    // Build redacted hash-bound manifest (no secrets).
    let manifest_body = json!({
        "schema_version": MANAGED_ACCEPTANCE_MANIFEST_SCHEMA,
        "admitted_codex_version": ADMITTED_CODEX_VERSION,
        "codex_sha256": input.codex_sha256,
        "codex_path_set": path_ok,
        "model": input.admitted_model,
        "provider_kind": input.provider_kind,
        "provider_host": input.provider_host,
        "provider_base_url": input.provider_base_url,
        "admitted_endpoint_paths": input.admitted_endpoint_paths,
        "parent_credential_present": input.parent_credential_present,
        "child_env_has_no_reusable_credential": input.child_env_has_no_reusable_credential,
        "gateway_loopback": input.gateway_is_loopback,
        "journal_parent_owned": input.journal_path_parent_owned,
        "limits": {
            "max_provider_requests": input.max_provider_requests,
            "max_retries": input.max_retries,
            "max_input_tokens": input.max_input_tokens,
            "max_output_tokens": input.max_output_tokens,
            "max_total_tokens": input.max_total_tokens,
            "cost_authority_kind": input.cost_authority_kind,
            "max_cost_usd": input.max_cost_usd,
            "pricing_table_version": input.pricing_table_version,
            "cost_currency": input.cost_currency,
            "max_wall_time_ms": input.max_wall_time_ms,
        },
        "capabilities": {
            "bwrap": capabilities.bwrap.as_str(),
            "userns_pid": capabilities.userns_pid.as_str(),
            "unshare_net": capabilities.unshare_net.as_str(),
            "network_confinement": capabilities.network_confinement.as_str(),
            "product_launch_enforces_loopback_only": input.product_launch_enforces_loopback_only,
        },
        "target": {
            "disposable_repo": input.disposable_target_repo,
            "main_sha": input.target_main_sha,
            "output_branch_prefix": input.allowed_output_branch_prefix,
            "draft_pr_only": input.draft_pr_only,
            "auto_merge_disabled": input.auto_merge_disabled,
        },
        "authority": {
            "decision_schema": PARTIAL_MEDIATION_AUTHORITY_DECISION_SCHEMA,
            "decision_status": input.authority_decision_status,
            "decision_body_sha256": input.authority_decision_body_sha256,
            "residual_finding_sha256": input.residual_finding_sha256,
            "residual_verdict": residual.verdict.as_str(),
            "product_admission_class": residual.product_admission_class,
        },
        "expected_evidence_references": [
            "gateway_usage_snapshot",
            "codex_usage_journal.v2",
            "execution_usage_event.v1_reconcile",
            "session_jsonl_or_missing_log_record",
            "terminal_evidence",
            "draft_pr_identity_if_any",
            "target_main_unchanged_proof",
        ],
        // Explicit non-secret markers
        "secrets_embedded": false,
        "live_provider_request": false,
    });
    let manifest_hash = hex::encode(Sha256::digest(manifest_body.to_string().as_bytes()));
    let mut manifest = manifest_body;
    manifest["manifest_sha256"] = json!(manifest_hash);

    // Classify result (fail closed; most severe blocker wins by priority).
    let result = classify_preflight_result(
        &blockers,
        full_admission,
        residual_no_go,
        operator_accepted,
        draft_pending,
        input.parent_credential_present,
        bwrap_ok,
    );

    ManagedAcceptancePreflightReport {
        schema_version: MANAGED_ACCEPTANCE_PREFLIGHT_SCHEMA.to_string(),
        result,
        blockers,
        checks,
        capabilities,
        manifest,
        notes: vec![
            "Provider-free preflight only; no live model request was made.".into(),
            "Secret values are never included in checks or manifest.".into(),
            "ready_pending_operator_risk_acceptance still requires multi-field operator acknowledgement before live trial.".into(),
            "Does not start RWE, Level-2, Meta, OpenCode, or PR #225.".into(),
            format!(
                "product_admission_class={}",
                CodexAdmissionClass::MediationHardenedPartial.as_str()
            ),
        ],
    }
}

fn classify_preflight_result(
    blockers: &[String],
    full_admission: bool,
    residual_no_go: bool,
    operator_accepted: bool,
    draft_pending: bool,
    parent_credential_present: bool,
    bwrap_ok: bool,
) -> ManagedAcceptancePreflightResult {
    if blockers.iter().any(|b| b.starts_with("blocked_identity")) {
        return ManagedAcceptancePreflightResult::BlockedIdentity;
    }
    if blockers
        .iter()
        .any(|b| b.starts_with("blocked_missing_credential"))
    {
        return ManagedAcceptancePreflightResult::BlockedMissingCredential;
    }
    if !parent_credential_present {
        return ManagedAcceptancePreflightResult::BlockedMissingCredential;
    }
    if blockers.iter().any(|b| b.starts_with("blocked_isolation")) || !bwrap_ok {
        return ManagedAcceptancePreflightResult::BlockedIsolation;
    }
    if blockers.iter().any(|b| b.starts_with("blocked_budget"))
        || blockers
            .iter()
            .any(|b| b.starts_with("blocked_budget_or_gate"))
    {
        return ManagedAcceptancePreflightResult::BlockedBudget;
    }
    if blockers
        .iter()
        .any(|b| b.starts_with("blocked_missing_authorization"))
    {
        return ManagedAcceptancePreflightResult::BlockedMissingAuthorization;
    }
    if !blockers.is_empty() {
        return ManagedAcceptancePreflightResult::OutcomeUnknown;
    }
    if full_admission {
        return ManagedAcceptancePreflightResult::ReadyUnderFullAdmission;
    }
    if residual_no_go {
        // All structural checks green. Live trial still requires operator risk
        // acceptance (draft_pending or already operator_accepted both map to
        // ready_pending — never silent live authorization).
        if operator_accepted || draft_pending {
            return ManagedAcceptancePreflightResult::ReadyPendingOperatorRiskAcceptance;
        }
        return ManagedAcceptancePreflightResult::BlockedMissingAuthorization;
    }
    ManagedAcceptancePreflightResult::OutcomeUnknown
}

/// Build a fixture input that is fully green except operator acceptance (draft pending).
pub fn fixture_ready_pending_operator_input(
    decision: &PartialMediationAuthorityDecision,
) -> ManagedAcceptancePreflightInput {
    ManagedAcceptancePreflightInput {
        execution_gate_enabled: true,
        codex_binary_path: Some(PathBuf::from("/opt/acp/managed-codex")),
        codex_version: Some(ADMITTED_CODEX_VERSION.into()),
        codex_sha256: Some("a".repeat(64)),
        admitted_model: Some(ADMITTED_CODEX_MODEL.into()),
        provider_kind: Some("openai_compatible".into()),
        provider_host: Some("api.openai.com".into()),
        provider_base_url: Some("https://api.openai.com/v1".into()),
        admitted_endpoint_paths: vec!["/v1/responses".into()],
        parent_credential_present: true,
        child_env_has_no_reusable_credential: true,
        mediation_gateway_configured: true,
        gateway_is_loopback: true,
        journal_path_parent_owned: true,
        journal_durable: true,
        max_provider_requests: Some(1),
        max_retries: Some(0),
        max_input_tokens: Some(32_000),
        max_output_tokens: Some(4_096),
        max_total_tokens: Some(36_096),
        cost_authority_kind: "cost_unavailable".into(),
        max_cost_usd: None,
        pricing_table_version: None,
        cost_currency: Some("USD".into()),
        max_wall_time_ms: Some(600_000),
        product_launch_enforces_loopback_only: false,
        disposable_target_repo: Some("Igzela/pe7-golden-path-acceptance-fixture".into()),
        target_main_sha: Some("b".repeat(40)),
        allowed_output_branch_prefix: "acp/".into(),
        draft_pr_only: true,
        auto_merge_disabled: true,
        verification_commands_declared: true,
        approval_requirements_declared: true,
        output_confirmation_requirements_declared: true,
        evidence_sink_available: true,
        cancellation_cleanup_rollback_ready: true,
        authority_decision_status: Some(decision.status.as_str().into()),
        authority_decision_body_sha256: Some(decision.decision_body_sha256.clone()),
        residual_finding_sha256: Some(decision.residual_finding_sha256.clone()),
    }
}



/// Production preflight entry: loads decision/risk/spend from store and verifies hash binding.
/// Runtime binary SHA/version and host probes remain host-derived evidence.
pub fn run_owner_derived_managed_acceptance_preflight(
    store: &crate::storage::local_product_store::LocalProductStore,
    tenant_id: &str,
    decision_id: &str,
    risk_authorization_id: &str,
    spend_authorization_id: Option<&str>,
    expected_identities: &ManagedAcceptancePreflightInput,
) -> Result<ManagedAcceptancePreflightReport, String> {
    let decision = store
        .get_managed_acceptance_decision(decision_id)?
        .ok_or_else(|| format!("decision {decision_id} not found"))?;
    if decision.get("tenant_id").and_then(serde_json::Value::as_str) != Some(tenant_id) {
        return Err("decision tenant mismatch".into());
    }
    let risk = store
        .get_active_managed_acceptance_authorization(risk_authorization_id)?
        .ok_or_else(|| "active risk authorization required".to_string())?;
    if risk.get("decision_id").and_then(serde_json::Value::as_str) != Some(decision_id) {
        return Err("risk authorization decision mismatch".into());
    }
    if risk
        .get("execution_granted")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Err("risk acknowledgement must not grant execution".into());
    }
    if let Some(spend_id) = spend_authorization_id {
        let spend = store
            .get_managed_acceptance_spend_authorization(spend_id)?
            .ok_or_else(|| "spend authorization not found".to_string())?;
        if spend.get("status").and_then(serde_json::Value::as_str) != Some("active")
            && spend.get("status").and_then(serde_json::Value::as_str) != Some("consumed")
        {
            return Err(format!(
                "spend authorization status {}",
                spend.get("status").and_then(serde_json::Value::as_str).unwrap_or("?")
            ));
        }
        if spend.get("risk_authorization_id").and_then(serde_json::Value::as_str)
            != Some(risk_authorization_id)
        {
            return Err("spend/risk authorization mismatch".into());
        }
        // Expected identities may only match, never expand store-owned spend envelope.
        if let Some(exp_sha) = expected_identities
            .authority_decision_body_sha256
            .as_deref()
        {
            if spend.get("decision_body_sha256").and_then(serde_json::Value::as_str) != Some(exp_sha)
            {
                return Err("spend decision hash mismatch vs expected identity".into());
            }
        }
    }
    // Bind store hashes into a fixture-shaped input only as expected identities, then run
    // the shared check matrix (host probes remain runtime-derived).
    let mut input = expected_identities.clone();
    input.authority_decision_status = Some(
        decision
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_string(),
    );
    input.authority_decision_body_sha256 = decision
        .get("decision_body_sha256")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    input.residual_finding_sha256 = decision
        .get("residual_finding_sha256")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    Ok(run_managed_acceptance_preflight_fixture_only(&input))
}

/// Back-compat fixture name used by dry-run harness only.
pub fn run_managed_acceptance_preflight(
    input: &ManagedAcceptancePreflightInput,
) -> ManagedAcceptancePreflightReport {
    run_managed_acceptance_preflight_fixture_only(input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::codex_partial_mediation_authority_decision::draft_partial_mediation_authority_decision;

    #[test]
    fn default_input_is_blocked_identity_or_budget() {
        let report = run_managed_acceptance_preflight_fixture_only(&ManagedAcceptancePreflightInput::default());
        assert!(!report.result.is_ready());
        assert!(matches!(
            report.result,
            ManagedAcceptancePreflightResult::BlockedIdentity
                | ManagedAcceptancePreflightResult::BlockedBudget
                | ManagedAcceptancePreflightResult::BlockedMissingCredential
                | ManagedAcceptancePreflightResult::BlockedIsolation
                | ManagedAcceptancePreflightResult::BlockedMissingAuthorization
        ));
        let rendered = report.to_json().to_string();
        assert!(!rendered.contains("sk-"));
        assert_eq!(report.manifest["secrets_embedded"], false);
        assert_eq!(report.manifest["live_provider_request"], false);
        assert!(report.manifest["manifest_sha256"].as_str().unwrap().len() == 64);
    }

    #[test]
    fn fixture_pending_operator_is_ready_pending_when_decision_draft_bound() {
        let decision = draft_partial_mediation_authority_decision();
        assert_eq!(
            decision.status,
            AuthorityDecisionStatus::DraftPendingOperator
        );
        let input = fixture_ready_pending_operator_input(&decision);
        let report = run_managed_acceptance_preflight_fixture_only(&input);
        assert_eq!(
            report.result,
            ManagedAcceptancePreflightResult::ReadyPendingOperatorRiskAcceptance,
            "blockers={:?}",
            report.blockers
        );
        assert!(report.result.is_ready());
        // Ready-pending does not mean live trial is authorized without operator ack.
        assert_eq!(
            report.manifest["authority"]["decision_status"],
            "draft_pending_operator"
        );
    }

    #[test]
    fn missing_credential_classifies_correctly() {
        let decision = draft_partial_mediation_authority_decision();
        let mut input = fixture_ready_pending_operator_input(&decision);
        input.parent_credential_present = false;
        let report = run_managed_acceptance_preflight_fixture_only(&input);
        assert_eq!(
            report.result,
            ManagedAcceptancePreflightResult::BlockedMissingCredential
        );
    }

    #[test]
    fn max_retries_nonzero_blocks_budget() {
        let decision = draft_partial_mediation_authority_decision();
        let mut input = fixture_ready_pending_operator_input(&decision);
        input.max_retries = Some(3);
        let report = run_managed_acceptance_preflight_fixture_only(&input);
        assert_eq!(
            report.result,
            ManagedAcceptancePreflightResult::BlockedBudget
        );
    }

    #[test]
    fn auto_merge_enabled_blocks_identity() {
        let decision = draft_partial_mediation_authority_decision();
        let mut input = fixture_ready_pending_operator_input(&decision);
        input.auto_merge_disabled = false;
        let report = run_managed_acceptance_preflight_fixture_only(&input);
        assert_eq!(
            report.result,
            ManagedAcceptancePreflightResult::BlockedIdentity
        );
    }

    #[test]
    fn manifest_never_contains_secret_shapes() {
        let decision = draft_partial_mediation_authority_decision();
        let input = fixture_ready_pending_operator_input(&decision);
        let report = run_managed_acceptance_preflight_fixture_only(&input);
        let text = report.manifest.to_string();
        assert!(!text.contains("sk-"));
        assert!(!text.contains("Bearer "));
        assert!(!text.contains("OPENAI_API_KEY="));
        assert_eq!(report.manifest["parent_credential_present"], true);
    }
}
