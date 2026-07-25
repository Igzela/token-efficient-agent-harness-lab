//! PE7-CODEX-PARTIAL-MEDIATION-AUTHORITY-DECISION-1
//!
//! Versioned **draft** authority decision for a tightly bounded live trial under
//! `mediation_hardened_partial` when residual full admission remains NO-GO.
//!
//! The Agent may recommend GO or NO-GO. The Agent **must not** issue operator
//! approval. A bare configuration flag cannot forge acknowledgement.
//!
//! This module does not start live Golden Path, call providers, or merge PRs.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::codex_mediation_admission::CodexAdmissionClass;
use super::codex_residual_admission::{
    evaluate_residual_admission_for_current_product, ResidualAdmissionFinding,
    ResidualAdmissionVerdict, CODEX_RESIDUAL_ADMISSION_FINDING_SCHEMA,
};
use super::config::{ADMITTED_CODEX_MODEL, ADMITTED_CODEX_VERSION};

/// Versioned partial-mediation live-trial authority decision contract.
pub const PARTIAL_MEDIATION_AUTHORITY_DECISION_SCHEMA: &str =
    "codex_partial_mediation_authority_decision.v1";

/// Exact phrase an operator must type (with decision hash binding) to accept risk.
/// Cannot be satisfied by a boolean env flag alone.
pub const OPERATOR_RISK_ACCEPTANCE_PHRASE: &str =
    "I ACCEPT residual Codex partial-mediation risk for one disposable bounded trial";

/// Decision status — never auto-approved by the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityDecisionStatus {
    /// Draft recorded; awaiting independent operator action.
    DraftPendingOperator,
    /// Operator explicitly accepted under the acknowledgement contract.
    OperatorAccepted,
    /// Operator explicitly rejected / NO-GO.
    OperatorRejected,
    /// Expired or invalidated by residual finding change.
    Invalidated,
}

impl AuthorityDecisionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DraftPendingOperator => "draft_pending_operator",
            Self::OperatorAccepted => "operator_accepted",
            Self::OperatorRejected => "operator_rejected",
            Self::Invalidated => "invalidated",
        }
    }

    pub fn authorizes_bounded_live_trial(&self) -> bool {
        matches!(self, Self::OperatorAccepted)
    }
}

/// Agent recommendation only — not approval authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRecommendation {
    /// Recommend a tightly bounded trial under partial mediation.
    RecommendGoBoundedTrial,
    /// Recommend holding until residual axes close.
    RecommendNoGoHold,
}

impl AgentRecommendation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RecommendGoBoundedTrial => "recommend_go_bounded_trial",
            Self::RecommendNoGoHold => "recommend_no_go_hold",
        }
    }
}

/// Risk domain tags for residual axes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskDomain {
    Confidentiality,
    BudgetEnforcement,
    RequestCount,
    ProcessIsolation,
    EvidenceTrust,
}

impl RiskDomain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Confidentiality => "confidentiality",
            Self::BudgetEnforcement => "budget_enforcement",
            Self::RequestCount => "request_count",
            Self::ProcessIsolation => "process_isolation",
            Self::EvidenceTrust => "evidence_trust",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidualRiskEntry {
    pub id: String,
    pub title: String,
    pub description: String,
    pub why_uneliminated: String,
    pub domains: Vec<RiskDomain>,
    pub potential_bypass_consequences: String,
    pub compensating_controls: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedTrialEnvelope {
    pub disposable_repository_only: bool,
    pub ordinary_coding_task_only: bool,
    pub exact_codex_version: String,
    pub exact_codex_sha_required: bool,
    pub provider_kind: String,
    pub provider_host: String,
    pub provider_base_url: String,
    pub admitted_endpoint_paths: Vec<String>,
    pub model: String,
    pub parent_only_api_key: bool,
    pub chatgpt_oauth_reuse_forbidden: bool,
    pub max_retries: u64,
    pub max_provider_requests: u64,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_total_tokens: u64,
    pub max_cost_usd_estimate: Option<String>,
    pub max_wall_time_ms: u64,
    pub draft_pr_only: bool,
    pub target_default_branch_unchanged: bool,
    pub auto_merge_disabled: bool,
    pub release_or_deploy_forbidden: bool,
    pub explicit_cancellation_and_cleanup: bool,
    pub gateway_and_session_usage_reconciliation_required: bool,
    pub exact_terminal_evidence_required: bool,
}

impl Default for BoundedTrialEnvelope {
    fn default() -> Self {
        Self {
            disposable_repository_only: true,
            ordinary_coding_task_only: true,
            exact_codex_version: ADMITTED_CODEX_VERSION.to_string(),
            exact_codex_sha_required: true,
            provider_kind: "openai_compatible".into(),
            provider_host: "api.openai.com".into(),
            provider_base_url: "https://api.openai.com/v1".into(),
            admitted_endpoint_paths: vec![
                "/v1/responses".into(),
                "/responses".into(),
                "/v1/chat/completions".into(),
                "/chat/completions".into(),
                "/v1/models".into(),
                "/models".into(),
            ],
            model: ADMITTED_CODEX_MODEL.to_string(),
            parent_only_api_key: true,
            chatgpt_oauth_reuse_forbidden: true,
            // Retry identity unproved → max_retries must stay 0.
            max_retries: 0,
            max_provider_requests: 1,
            max_input_tokens: 32_000,
            max_output_tokens: 4_096,
            max_total_tokens: 36_096,
            max_cost_usd_estimate: Some(
                "client_estimate_only_unresolved_unless_provider_receipt".into(),
            ),
            max_wall_time_ms: 600_000,
            draft_pr_only: true,
            target_default_branch_unchanged: true,
            auto_merge_disabled: true,
            release_or_deploy_forbidden: true,
            explicit_cancellation_and_cleanup: true,
            gateway_and_session_usage_reconciliation_required: true,
            exact_terminal_evidence_required: true,
        }
    }
}

/// Operator acknowledgement material that cannot be forged by a bare config flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorAcknowledgementRequirement {
    pub required_phrase: String,
    pub requires_decision_body_sha256: bool,
    pub requires_residual_finding_sha256: bool,
    pub requires_human_actor_identity: bool,
    pub forbids_env_boolean_alone: bool,
    pub forbids_agent_self_approval: bool,
    pub approval_granted_by_acknowledgement_row_alone: bool,
}

impl Default for OperatorAcknowledgementRequirement {
    fn default() -> Self {
        Self {
            required_phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.to_string(),
            requires_decision_body_sha256: true,
            requires_residual_finding_sha256: true,
            requires_human_actor_identity: true,
            forbids_env_boolean_alone: true,
            forbids_agent_self_approval: true,
            // Existing store acknowledgement rows are acknowledgement-only, not mutation authority.
            approval_granted_by_acknowledgement_row_alone: false,
        }
    }
}

/// Fixture-only in-memory acknowledgement submission. Production must use store-owned
/// `RiskAcknowledgementRequest` + authenticated principal — never free-form `actor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorAcknowledgementSubmission {
    /// Deprecated free-form actor; ignored by production store path. Fixture tests only.
    pub actor: String,
    pub phrase: String,
    pub decision_body_sha256: String,
    pub residual_finding_sha256: String,
    pub explicit_go: bool,
}

/// Draft authority decision for independent operator review.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialMediationAuthorityDecision {
    pub schema_version: String,
    pub status: AuthorityDecisionStatus,
    pub residual_verdict: String,
    pub residual_finding_schema: String,
    pub residual_finding_sha256: String,
    pub product_admission_class: String,
    pub agent_recommendation: AgentRecommendation,
    pub agent_recommendation_rationale: String,
    pub residual_risks: Vec<ResidualRiskEntry>,
    pub go_alternative: String,
    pub no_go_alternative: String,
    pub trial_envelope: BoundedTrialEnvelope,
    pub acknowledgement: OperatorAcknowledgementRequirement,
    pub expiry_unix_ms: Option<u64>,
    pub invalidation_conditions: Vec<String>,
    pub rollback_and_kill: Vec<String>,
    pub post_trial_evidence_required: Vec<String>,
    pub non_claims: Vec<String>,
    pub decision_body_sha256: String,
}

impl PartialMediationAuthorityDecision {
    pub fn to_json(&self) -> Value {
        json!({
            "schema_version": self.schema_version,
            "status": self.status.as_str(),
            "authorizes_bounded_live_trial": self.status.authorizes_bounded_live_trial(),
            "residual_verdict": self.residual_verdict,
            "residual_finding_schema": self.residual_finding_schema,
            "residual_finding_sha256": self.residual_finding_sha256,
            "product_admission_class": self.product_admission_class,
            "agent_recommendation": self.agent_recommendation.as_str(),
            "agent_recommendation_rationale": self.agent_recommendation_rationale,
            "residual_risks": self.residual_risks.iter().map(|r| json!({
                "id": r.id,
                "title": r.title,
                "description": r.description,
                "why_uneliminated": r.why_uneliminated,
                "domains": r.domains.iter().map(|d| d.as_str()).collect::<Vec<_>>(),
                "potential_bypass_consequences": r.potential_bypass_consequences,
                "compensating_controls": r.compensating_controls,
            })).collect::<Vec<_>>(),
            "go_alternative": self.go_alternative,
            "no_go_alternative": self.no_go_alternative,
            "trial_envelope": {
                "disposable_repository_only": self.trial_envelope.disposable_repository_only,
                "ordinary_coding_task_only": self.trial_envelope.ordinary_coding_task_only,
                "exact_codex_version": self.trial_envelope.exact_codex_version,
                "exact_codex_sha_required": self.trial_envelope.exact_codex_sha_required,
                "provider_kind": self.trial_envelope.provider_kind,
                "provider_host": self.trial_envelope.provider_host,
                "provider_base_url": self.trial_envelope.provider_base_url,
                "admitted_endpoint_paths": self.trial_envelope.admitted_endpoint_paths,
                "model": self.trial_envelope.model,
                "parent_only_api_key": self.trial_envelope.parent_only_api_key,
                "chatgpt_oauth_reuse_forbidden": self.trial_envelope.chatgpt_oauth_reuse_forbidden,
                "max_retries": self.trial_envelope.max_retries,
                "max_provider_requests": self.trial_envelope.max_provider_requests,
                "max_input_tokens": self.trial_envelope.max_input_tokens,
                "max_output_tokens": self.trial_envelope.max_output_tokens,
                "max_total_tokens": self.trial_envelope.max_total_tokens,
                "max_cost_usd_estimate": self.trial_envelope.max_cost_usd_estimate,
                "max_wall_time_ms": self.trial_envelope.max_wall_time_ms,
                "draft_pr_only": self.trial_envelope.draft_pr_only,
                "target_default_branch_unchanged": self.trial_envelope.target_default_branch_unchanged,
                "auto_merge_disabled": self.trial_envelope.auto_merge_disabled,
                "release_or_deploy_forbidden": self.trial_envelope.release_or_deploy_forbidden,
                "explicit_cancellation_and_cleanup": self.trial_envelope.explicit_cancellation_and_cleanup,
                "gateway_and_session_usage_reconciliation_required":
                    self.trial_envelope.gateway_and_session_usage_reconciliation_required,
                "exact_terminal_evidence_required":
                    self.trial_envelope.exact_terminal_evidence_required,
            },
            "acknowledgement": {
                "required_phrase": self.acknowledgement.required_phrase,
                "requires_decision_body_sha256": self.acknowledgement.requires_decision_body_sha256,
                "requires_residual_finding_sha256":
                    self.acknowledgement.requires_residual_finding_sha256,
                "requires_human_actor_identity":
                    self.acknowledgement.requires_human_actor_identity,
                "forbids_env_boolean_alone": self.acknowledgement.forbids_env_boolean_alone,
                "forbids_agent_self_approval": self.acknowledgement.forbids_agent_self_approval,
                "approval_granted_by_acknowledgement_row_alone":
                    self.acknowledgement.approval_granted_by_acknowledgement_row_alone,
            },
            "expiry_unix_ms": self.expiry_unix_ms,
            "invalidation_conditions": self.invalidation_conditions,
            "rollback_and_kill": self.rollback_and_kill,
            "post_trial_evidence_required": self.post_trial_evidence_required,
            "non_claims": self.non_claims,
            "decision_body_sha256": self.decision_body_sha256,
        })
    }
}

fn residual_finding_sha256(finding: &ResidualAdmissionFinding) -> String {
    let body = finding.to_json().to_string();
    hex::encode(Sha256::digest(body.as_bytes()))
}

/// Canonical hash material covers every authority-relevant field (A2).
/// Excludes only `decision_body_sha256` itself.
fn decision_body_hash_material(decision: &PartialMediationAuthorityDecision) -> String {
    let mut full = decision.to_json();
    if let Some(obj) = full.as_object_mut() {
        obj.remove("decision_body_sha256");
        // Sort keys for deterministic serialization.
        let mut keys: Vec<_> = obj.keys().cloned().collect();
        keys.sort();
        let mut sorted = serde_json::Map::new();
        for k in keys {
            if let Some(v) = obj.remove(&k) {
                sorted.insert(k, v);
            }
        }
        *obj = sorted;
    }
    // Expand trial envelope completely (already in to_json).
    full.to_string()
}

fn compute_decision_body_sha256(decision: &PartialMediationAuthorityDecision) -> String {
    hex::encode(Sha256::digest(
        decision_body_hash_material(decision).as_bytes(),
    ))
}

fn build_residual_risks(finding: &ResidualAdmissionFinding) -> Vec<ResidualRiskEntry> {
    let mut risks = Vec::new();

    if !finding.retry.enforceable_retry_identity {
        risks.push(ResidualRiskEntry {
            id: "retry_identity_unlabeled".into(),
            title: "Codex internal retry identity unavailable".into(),
            description: finding.retry.reason.clone(),
            why_uneliminated: "Admitted Codex CLI 0.145.0 does not expose a documented gateway-visible retry-of linkage on the HTTP wire; heuristics are forbidden.".into(),
            domains: vec![RiskDomain::RequestCount, RiskDomain::BudgetEnforcement, RiskDomain::EvidenceTrust],
            potential_bypass_consequences: "Internal Codex retries may appear as additional provider POSTs; without wire identity they cannot be classified as retries vs new logical rounds. With max_retries=0 every subsequent POST is rejected, which may fail closed more often than necessary but prevents unbounded retry spend.".into(),
            compensating_controls: vec![
                "max_retries=0 for any partial-mediation trial".into(),
                "max_provider_requests predeclared and gateway-enforced".into(),
                "parent-owned fail-closed usage journal charges in-flight/outcome-unknown".into(),
                "gateway primary measurement; session JSONL corroboration only".into(),
            ],
        });
    }

    if !finding.network.loopback_only_enforced {
        risks.push(ResidualRiskEntry {
            id: "network_not_loopback_only".into(),
            title: "Product launch does not enforce loopback-only network confinement".into(),
            description: finding.network.reason.clone(),
            why_uneliminated: "Product mediated launch still shares the host network. Unprivileged unshare-net + Unix gateway design is host-proved where available but not product-wired.".into(),
            domains: vec![
                RiskDomain::Confidentiality,
                RiskDomain::BudgetEnforcement,
                RiskDomain::ProcessIsolation,
            ],
            potential_bypass_consequences: "A compromised or misbehaving child could attempt direct external TCP to an alternate provider or proxy if it obtained or embedded credentials. Current product path prevents reusable upstream credential in the child (session token only), which blocks credential-based direct billing bypass but does not prove network non-egress.".into(),
            compensating_controls: vec![
                "parent-only API key; child holds only one-use gateway session token".into(),
                "empty ephemeral auth.json; real HOME hidden by bwrap tmpfs".into(),
                "OPENAI_BASE_URL forced to loopback gateway".into(),
                "proxy/env credential shapes rejected in launch plan".into(),
                "no ChatGPT OAuth reuse".into(),
            ],
        });
    }

    if !finding.user_pid_ns.classification.is_proved() {
        risks.push(ResidualRiskEntry {
            id: "user_pid_ns_host_dependent".into(),
            title: "Unprivileged user/PID namespace isolation is host-dependent".into(),
            description: finding.user_pid_ns.reason.clone(),
            why_uneliminated: "Some hosts (including common CI) deny unprivileged uid_map; host-wide sysctl changes are not authorized.".into(),
            domains: vec![RiskDomain::ProcessIsolation, RiskDomain::Confidentiality],
            potential_bypass_consequences: "Without PID ns, child may observe more host process metadata; FS isolation (tmpfs over /home, bind worktree) still applies where bwrap runs. Isolation must not be silently claimed when userns is denied.".into(),
            compensating_controls: vec![
                "typed capability evidence; fail closed for full admission".into(),
                "bwrap FS isolation still hides real HOME/auth when probes succeed".into(),
                "process group cleanup on timeout/cancel".into(),
            ],
        });
    }

    risks.push(ResidualRiskEntry {
        id: "live_credential_authorization".into(),
        title: "Live operator credential and authorization still required".into(),
        description: "Provider-free residual investigation does not supply credentials or operator approval for a live model request.".into(),
        why_uneliminated: "Credentials must be parent-supplied at runtime; operator risk acceptance is out-of-band of agent automation.".into(),
        domains: vec![
            RiskDomain::Confidentiality,
            RiskDomain::BudgetEnforcement,
            RiskDomain::EvidenceTrust,
        ],
        potential_bypass_consequences: "Without explicit operator acknowledgement, a live trial must not run. Credential scraping is forbidden.".into(),
        compensating_controls: vec![
            "parent-only API key path".into(),
            "multi-field operator acknowledgement (phrase + hashes + human actor)".into(),
            "agent cannot self-approve".into(),
        ],
    });

    risks
}

/// Draft a partial-mediation authority decision from the current residual finding.
///
/// Always returns `DraftPendingOperator`. Never returns `OperatorAccepted`.
pub fn draft_partial_mediation_authority_decision() -> PartialMediationAuthorityDecision {
    let finding = evaluate_residual_admission_for_current_product();
    draft_from_residual_finding(&finding)
}

pub fn draft_from_residual_finding(
    finding: &ResidualAdmissionFinding,
) -> PartialMediationAuthorityDecision {
    let residual_sha = residual_finding_sha256(finding);
    let risks = build_residual_risks(finding);

    // Agent recommendation: residual NO-GO means full admission is refused, but a
    // tightly bounded trial may be recommended for operator consideration only.
    let (recommendation, rationale) = match finding.verdict {
        ResidualAdmissionVerdict::FullProviderFreeMediationAdmission => (
            AgentRecommendation::RecommendGoBoundedTrial,
            "Residual axes are fully closed; full admission path is preferred over partial risk acceptance.".into(),
        ),
        ResidualAdmissionVerdict::ResidualAdmissionNoGo => (
            AgentRecommendation::RecommendGoBoundedTrial,
            "Residual full admission is NO-GO, but compensating controls (parent-only key, max_retries=0, max_provider_requests=1, disposable repo, Draft PR only, gateway journal) may support a single operator-accepted disposable trial. This is a recommendation only — not approval.".into(),
        ),
    };

    let mut decision = PartialMediationAuthorityDecision {
        schema_version: PARTIAL_MEDIATION_AUTHORITY_DECISION_SCHEMA.to_string(),
        status: AuthorityDecisionStatus::DraftPendingOperator,
        residual_verdict: finding.verdict.as_str().to_string(),
        residual_finding_schema: CODEX_RESIDUAL_ADMISSION_FINDING_SCHEMA.to_string(),
        residual_finding_sha256: residual_sha,
        product_admission_class: CodexAdmissionClass::MediationHardenedPartial
            .as_str()
            .to_string(),
        agent_recommendation: recommendation,
        agent_recommendation_rationale: rationale,
        residual_risks: risks,
        go_alternative: "GO (operator-only): accept residual risks for exactly one disposable bounded trial under the trial envelope, with multi-field acknowledgement bound to decision and residual hashes. Does not grant full admission, auto-merge, release, or recurring live runs.".into(),
        no_go_alternative: "NO-GO: refuse live trial until retry identity is wire-enforceable, product launch enforces loopback-only network confinement with executed bypass probes, and user/PID capability is proved on the execution host (or residual axes are otherwise closed by a later reviewed packet).".into(),
        trial_envelope: BoundedTrialEnvelope::default(),
        acknowledgement: OperatorAcknowledgementRequirement::default(),
        expiry_unix_ms: None, // operator sets when accepting; draft has no auto-expiry stamp
        invalidation_conditions: vec![
            "residual_finding_sha256 changes".into(),
            "admitted Codex version or SHA changes".into(),
            "provider kind/host/base URL/admitted paths change".into(),
            "product admission class upgraded or downgraded without re-decision".into(),
            "max_retries > 0 while retry identity remains unproved".into(),
            "target default branch mutation observed".into(),
            "auto-merge enabled".into(),
            "ChatGPT OAuth reuse detected".into(),
            "expiry reached".into(),
            "operator rejects or revokes".into(),
        ],
        rollback_and_kill: vec![
            "cancel active managed process group (existing CLI process owner)".into(),
            "halt CodexBudgetGateway (stop flag + no further admits)".into(),
            "retain outcome-unknown journal charges; do not restore budget from missing session logs".into(),
            "leave target default branch unchanged; close or abandon Draft PR only".into(),
            "delete ephemeral CODEX_HOME and parent journal path only after evidence capture".into(),
            "record terminal evidence with exact identities and usage reconcile".into(),
        ],
        post_trial_evidence_required: vec![
            "exact Codex binary path/version/SHA".into(),
            "gateway usage snapshot + journal final state".into(),
            "session JSONL corroboration or explicit missing-log record".into(),
            "execution_usage_event.v1 reconcile result".into(),
            "Draft PR identity (if any) and target main SHA unchanged proof".into(),
            "capability classifications (bwrap/userns/net) at trial time".into(),
            "operator acknowledgement id and decision hashes".into(),
            "cancellation/cleanup/rollback outcome".into(),
        ],
        non_claims: vec![
            "Does not claim full_provider_free_mediation_admission.".into(),
            "Does not authorize RWE, Architecture Convergence, Level-2, Meta, OpenCode, Vader, or PR #225.".into(),
            "Does not authorize agent self-approval or auto-merge.".into(),
            "Does not treat client token-times-price as a provider billing receipt.".into(),
            "Does not scrape credentials, OAuth files, raw prompts, or transcripts into evidence.".into(),
        ],
        decision_body_sha256: String::new(),
    };
    decision.decision_body_sha256 = compute_decision_body_sha256(&decision);
    decision
}

/// Validate an operator acknowledgement attempt against a draft decision.
///
/// Returns Ok(()) only when all multi-field requirements match. Never treats a
/// bare boolean / env flag as sufficient. Agent identities are rejected.
pub fn validate_operator_acknowledgement(
    decision: &PartialMediationAuthorityDecision,
    submission: &OperatorAcknowledgementSubmission,
) -> Result<(), String> {
    if decision.status != AuthorityDecisionStatus::DraftPendingOperator
        && decision.status != AuthorityDecisionStatus::OperatorAccepted
    {
        return Err(format!(
            "decision status {} does not accept acknowledgement",
            decision.status.as_str()
        ));
    }
    if !submission.explicit_go {
        return Err("operator did not set explicit_go".into());
    }
    if submission.phrase != decision.acknowledgement.required_phrase {
        return Err("operator risk-acceptance phrase mismatch".into());
    }
    if submission.decision_body_sha256 != decision.decision_body_sha256 {
        return Err("decision_body_sha256 mismatch".into());
    }
    if submission.residual_finding_sha256 != decision.residual_finding_sha256 {
        return Err("residual_finding_sha256 mismatch".into());
    }
    let actor = submission.actor.trim();
    if actor.is_empty() {
        return Err("human actor identity is required".into());
    }
    let actor_lower = actor.to_ascii_lowercase();
    for forbidden in [
        "agent",
        "bot",
        "automation",
        "ci",
        "github-actions",
        "self",
        "system",
    ] {
        if actor_lower == forbidden || actor_lower.starts_with(&format!("{forbidden}-")) {
            return Err(format!(
                "actor identity {actor:?} is not accepted as human operator acknowledgement"
            ));
        }
    }
    // Enforce trial envelope safety that residual NO-GO requires.
    if decision.trial_envelope.max_retries != 0
        && !decision.residual_verdict.contains("full_provider")
    {
        return Err("max_retries must be 0 while residual full admission is not closed".into());
    }
    if !decision.trial_envelope.parent_only_api_key {
        return Err("parent_only_api_key is required".into());
    }
    if !decision.trial_envelope.chatgpt_oauth_reuse_forbidden {
        return Err("ChatGPT OAuth reuse must remain forbidden".into());
    }
    if !decision.trial_envelope.draft_pr_only || !decision.trial_envelope.auto_merge_disabled {
        return Err("Draft PR only and auto-merge disabled are required".into());
    }
    Ok(())
}

/// Apply a validated operator acknowledgement. Agent must not call this with forged actors.
/// Fixture/in-memory only. Does **not** create store-owned authority or execution rights.
/// Production acceptance must go through `LocalProductStore::accept_managed_acceptance_decision`.
#[doc(hidden)]
pub fn apply_operator_acknowledgement_fixture_only(
    decision: &mut PartialMediationAuthorityDecision,
    submission: &OperatorAcknowledgementSubmission,
) -> Result<(), String> {
    validate_operator_acknowledgement(decision, submission)?;
    decision.status = AuthorityDecisionStatus::OperatorAccepted;
    Ok(())
}

/// Explicit operator reject.
pub fn apply_operator_reject(
    decision: &mut PartialMediationAuthorityDecision,
) -> Result<(), String> {
    if decision.status == AuthorityDecisionStatus::Invalidated {
        return Err("decision already invalidated".into());
    }
    decision.status = AuthorityDecisionStatus::OperatorRejected;
    Ok(())
}

/// Invalidate when residual finding hash changes or envelope is violated.
pub fn invalidate_if_residual_changed(
    decision: &mut PartialMediationAuthorityDecision,
    current_finding: &ResidualAdmissionFinding,
) -> bool {
    let current_sha = residual_finding_sha256(current_finding);
    if current_sha != decision.residual_finding_sha256 {
        decision.status = AuthorityDecisionStatus::Invalidated;
        return true;
    }
    false
}

/// Agent helper: never returns an accepted decision.
pub fn agent_must_not_self_approve(decision: &PartialMediationAuthorityDecision) -> bool {
    !decision.status.authorizes_bounded_live_trial()
        || decision.status == AuthorityDecisionStatus::DraftPendingOperator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_is_pending_and_does_not_authorize_trial() {
        let decision = draft_partial_mediation_authority_decision();
        assert_eq!(
            decision.schema_version,
            PARTIAL_MEDIATION_AUTHORITY_DECISION_SCHEMA
        );
        assert_eq!(
            decision.status,
            AuthorityDecisionStatus::DraftPendingOperator
        );
        assert!(!decision.status.authorizes_bounded_live_trial());
        assert_eq!(
            decision.product_admission_class,
            CodexAdmissionClass::MediationHardenedPartial.as_str()
        );
        assert_eq!(
            decision.residual_verdict,
            ResidualAdmissionVerdict::ResidualAdmissionNoGo.as_str()
        );
        assert_eq!(decision.trial_envelope.max_retries, 0);
        assert_eq!(decision.trial_envelope.max_provider_requests, 1);
        assert!(decision.trial_envelope.draft_pr_only);
        assert!(decision.trial_envelope.auto_merge_disabled);
        assert!(decision.acknowledgement.forbids_agent_self_approval);
        assert!(decision.acknowledgement.forbids_env_boolean_alone);
        assert!(!decision.decision_body_sha256.is_empty());
        assert_eq!(decision.decision_body_sha256.len(), 64);
        // Risks cover residual axes.
        let ids: Vec<_> = decision
            .residual_risks
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        assert!(ids.contains(&"retry_identity_unlabeled"));
        assert!(
            ids.contains(&"network_not_loopback_only")
                || ids.contains(&"user_pid_ns_host_dependent")
                || ids.contains(&"live_credential_authorization")
        );
        assert!(ids.contains(&"live_credential_authorization"));
        let json = decision.to_json();
        assert_eq!(json["status"], "draft_pending_operator");
        assert_eq!(json["authorizes_bounded_live_trial"], false);
        let rendered = json.to_string();
        assert!(!rendered.contains("sk-"));
        assert!(!rendered.contains("OPENAI_API_KEY=sk"));
    }

    #[test]
    fn bare_config_and_agent_actor_cannot_acknowledge() {
        let decision = draft_partial_mediation_authority_decision();
        // Wrong phrase (simulates boolean/env alone).
        let bad_phrase = OperatorAcknowledgementSubmission {
            actor: "operator-alice".into(),
            phrase: "true".into(),
            decision_body_sha256: decision.decision_body_sha256.clone(),
            residual_finding_sha256: decision.residual_finding_sha256.clone(),
            explicit_go: true,
        };
        assert!(validate_operator_acknowledgement(&decision, &bad_phrase).is_err());

        let agent_actor = OperatorAcknowledgementSubmission {
            actor: "agent".into(),
            phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
            decision_body_sha256: decision.decision_body_sha256.clone(),
            residual_finding_sha256: decision.residual_finding_sha256.clone(),
            explicit_go: true,
        };
        assert!(validate_operator_acknowledgement(&decision, &agent_actor)
            .unwrap_err()
            .contains("human"));

        let bot_actor = OperatorAcknowledgementSubmission {
            actor: "github-actions".into(),
            phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
            decision_body_sha256: decision.decision_body_sha256.clone(),
            residual_finding_sha256: decision.residual_finding_sha256.clone(),
            explicit_go: true,
        };
        assert!(validate_operator_acknowledgement(&decision, &bot_actor).is_err());
    }

    #[test]
    fn valid_human_acknowledgement_can_accept_without_agent_self_approve_helper() {
        let mut decision = draft_partial_mediation_authority_decision();
        assert!(
            agent_must_not_self_approve(&decision)
                || !decision.status.authorizes_bounded_live_trial()
        );
        let ok = OperatorAcknowledgementSubmission {
            actor: "operator-alice".into(),
            phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
            decision_body_sha256: decision.decision_body_sha256.clone(),
            residual_finding_sha256: decision.residual_finding_sha256.clone(),
            explicit_go: true,
        };
        apply_operator_acknowledgement_fixture_only(&mut decision, &ok).expect("human ack");
        assert_eq!(decision.status, AuthorityDecisionStatus::OperatorAccepted);
        assert!(decision.status.authorizes_bounded_live_trial());
        // Hash mismatch invalidates residual binding.
        let finding = evaluate_residual_admission_for_current_product();
        // Same finding should not invalidate.
        assert!(!invalidate_if_residual_changed(&mut decision, &finding));
    }

    #[test]
    fn hash_mismatch_and_reject_paths() {
        let mut decision = draft_partial_mediation_authority_decision();
        let bad_hash = OperatorAcknowledgementSubmission {
            actor: "operator-bob".into(),
            phrase: OPERATOR_RISK_ACCEPTANCE_PHRASE.into(),
            decision_body_sha256: "0".repeat(64),
            residual_finding_sha256: decision.residual_finding_sha256.clone(),
            explicit_go: true,
        };
        assert!(validate_operator_acknowledgement(&decision, &bad_hash)
            .unwrap_err()
            .contains("decision_body_sha256"));

        apply_operator_reject(&mut decision).unwrap();
        assert_eq!(decision.status, AuthorityDecisionStatus::OperatorRejected);
        assert!(!decision.status.authorizes_bounded_live_trial());
    }

    #[test]
    fn go_and_no_go_alternatives_are_recorded() {
        let decision = draft_partial_mediation_authority_decision();
        assert!(decision.go_alternative.starts_with("GO"));
        assert!(decision.no_go_alternative.starts_with("NO-GO"));
        assert!(!decision.rollback_and_kill.is_empty());
        assert!(!decision.post_trial_evidence_required.is_empty());
        assert!(decision
            .non_claims
            .iter()
            .any(|c| c.contains("self-approval")));
    }

    #[test]
    fn mutating_authority_fields_changes_canonical_decision_hash() {
        let base = draft_partial_mediation_authority_decision();
        let base_hash = base.decision_body_sha256.clone();
        let mut m = base.clone();
        m.trial_envelope.max_retries = 3;
        m.decision_body_sha256 = compute_decision_body_sha256(&m);
        assert_ne!(base_hash, m.decision_body_sha256);
        let mut m2 = base.clone();
        m2.trial_envelope.model = "mutated-model".into();
        m2.decision_body_sha256 = compute_decision_body_sha256(&m2);
        assert_ne!(base_hash, m2.decision_body_sha256);
        let mut m3 = base.clone();
        m3.go_alternative = "mutated-go".into();
        m3.decision_body_sha256 = compute_decision_body_sha256(&m3);
        assert_ne!(base_hash, m3.decision_body_sha256);
    }
}
